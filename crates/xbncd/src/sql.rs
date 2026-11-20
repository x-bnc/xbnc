//! A small database layer over SQLite (rusqlite) and PostgreSQL (postgres).
//!
//! It keeps the shape of the rusqlite API that `db.rs` uses (`execute`,
//! `query_row`, `prepare` + `query_map`, `optional`, `last_insert_rowid`,
//! `transaction`) so the same queries run on both. Queries are written in
//! the SQLite dialect; for PostgreSQL they are translated once and cached
//! (`?N` placeholders, `INSERT OR IGNORE`, two-argument MAX/MIN, LIKE).

use anyhow::anyhow;
use std::collections::HashMap;
use std::fmt;

// ------------------------------------------------------------ values

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Int(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

impl From<i64> for Value {
    fn from(v: i64) -> Self {
        Value::Int(v)
    }
}
impl From<String> for Value {
    fn from(v: String) -> Self {
        Value::Text(v)
    }
}
impl From<&str> for Value {
    fn from(v: &str) -> Self {
        Value::Text(v.to_string())
    }
}

/// Something that can be bound as a query parameter.
pub trait Arg {
    fn value(&self) -> Value;
}

impl<T: Arg + ?Sized> Arg for &T {
    fn value(&self) -> Value {
        (**self).value()
    }
}
impl<T: Arg> Arg for Option<T> {
    fn value(&self) -> Value {
        self.as_ref().map(Arg::value).unwrap_or(Value::Null)
    }
}
macro_rules! int_arg {
    ($($t:ty),*) => {$(
        impl Arg for $t {
            fn value(&self) -> Value { Value::Int(*self as i64) }
        }
    )*};
}
int_arg!(i64, i32, i16, i8, u32, u16, u8, usize, u64);
impl Arg for bool {
    fn value(&self) -> Value {
        Value::Int(*self as i64)
    }
}
impl Arg for f64 {
    fn value(&self) -> Value {
        Value::Real(*self)
    }
}
impl Arg for str {
    fn value(&self) -> Value {
        Value::Text(self.to_string())
    }
}
impl Arg for String {
    fn value(&self) -> Value {
        Value::Text(self.clone())
    }
}
impl Arg for [u8] {
    fn value(&self) -> Value {
        Value::Blob(self.to_vec())
    }
}
impl Arg for Vec<u8> {
    fn value(&self) -> Value {
        Value::Blob(self.clone())
    }
}
impl Arg for Value {
    fn value(&self) -> Value {
        self.clone()
    }
}

/// Query parameters: `params![a, b]`, an array `[a, b]`, `()` for none, or
/// [`params_from_iter`].
pub trait Params {
    fn values(self) -> Vec<Value>;
}
impl Params for () {
    fn values(self) -> Vec<Value> {
        Vec::new()
    }
}
impl Params for &[&dyn Arg] {
    fn values(self) -> Vec<Value> {
        self.iter().map(|a| a.value()).collect()
    }
}
impl<T: Arg, const N: usize> Params for [T; N] {
    fn values(self) -> Vec<Value> {
        self.iter().map(Arg::value).collect()
    }
}
impl<T: Arg, const N: usize> Params for &[T; N] {
    fn values(self) -> Vec<Value> {
        self.iter().map(Arg::value).collect()
    }
}
pub struct FromIter(Vec<Value>);
impl Params for FromIter {
    fn values(self) -> Vec<Value> {
        self.0
    }
}
pub fn params_from_iter<I: IntoIterator<Item = T>, T: Arg>(i: I) -> FromIter {
    FromIter(i.into_iter().map(|v| v.value()).collect())
}

#[macro_export]
macro_rules! params {
    () => { &[] as &[&dyn $crate::sql::Arg] };
    ($($p:expr),+ $(,)?) => { &[$(&$p as &dyn $crate::sql::Arg),+] as &[&dyn $crate::sql::Arg] };
}

// ------------------------------------------------------------ errors

#[derive(Debug)]
pub enum SqlError {
    NoRows,
    /// A UNIQUE, PRIMARY KEY, FOREIGN KEY or CHECK rule refused the change.
    Constraint(String),
    Other(anyhow::Error),
}

pub type SqlResult<T> = std::result::Result<T, SqlError>;

impl fmt::Display for SqlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SqlError::NoRows => write!(f, "query returned no rows"),
            SqlError::Constraint(m) => write!(f, "constraint failed: {m}"),
            SqlError::Other(e) => write!(f, "{e:#}"),
        }
    }
}
impl std::error::Error for SqlError {}

impl From<rusqlite::Error> for SqlError {
    fn from(e: rusqlite::Error) -> Self {
        match e {
            rusqlite::Error::QueryReturnedNoRows => SqlError::NoRows,
            rusqlite::Error::SqliteFailure(f, m)
                if f.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                SqlError::Constraint(m.unwrap_or_default())
            }
            e => SqlError::Other(e.into()),
        }
    }
}
impl From<postgres::Error> for SqlError {
    fn from(e: postgres::Error) -> Self {
        if let Some(db) = e.as_db_error() {
            if db.code().code().starts_with("23") {
                return SqlError::Constraint(db.message().to_string());
            }
        }
        SqlError::Other(anyhow!("PostgreSQL: {}", pg_error_text(&e)))
    }
}
impl From<anyhow::Error> for SqlError {
    fn from(e: anyhow::Error) -> Self {
        SqlError::Other(e)
    }
}

fn pg_error_text(e: &postgres::Error) -> String {
    match e.as_db_error() {
        Some(d) => format!("{} ({})", d.message(), d.code().code()),
        None => e.to_string(),
    }
}

pub trait OptionalExtension<T> {
    fn optional(self) -> SqlResult<Option<T>>;
}
impl<T> OptionalExtension<T> for SqlResult<T> {
    fn optional(self) -> SqlResult<Option<T>> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(SqlError::NoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}

// ------------------------------------------------------------ rows

pub enum Row<'a> {
    Lite(&'a rusqlite::Row<'a>),
    Pg(&'a postgres::Row),
}

/// A column type that can be read from a row.
pub trait FromCol: Sized {
    fn from_value(v: Value) -> SqlResult<Self>;
}

fn bad_type(want: &str, v: &Value) -> SqlError {
    SqlError::Other(anyhow!("expected {want}, found {v:?}"))
}

impl FromCol for Value {
    fn from_value(v: Value) -> SqlResult<Self> {
        Ok(v)
    }
}
impl FromCol for i64 {
    fn from_value(v: Value) -> SqlResult<Self> {
        match v {
            Value::Int(i) => Ok(i),
            Value::Real(f) => Ok(f as i64),
            v => Err(bad_type("an integer", &v)),
        }
    }
}
macro_rules! int_col {
    ($($t:ty),*) => {$(
        impl FromCol for $t {
            fn from_value(v: Value) -> SqlResult<Self> {
                let i = i64::from_value(v)?;
                <$t>::try_from(i).map_err(|_| SqlError::Other(anyhow!("integer {i} out of range")))
            }
        }
    )*};
}
int_col!(i32, u32, u16, u8, usize, u64);
impl FromCol for bool {
    fn from_value(v: Value) -> SqlResult<Self> {
        Ok(i64::from_value(v)? != 0)
    }
}
impl FromCol for f64 {
    fn from_value(v: Value) -> SqlResult<Self> {
        match v {
            Value::Real(f) => Ok(f),
            Value::Int(i) => Ok(i as f64),
            v => Err(bad_type("a number", &v)),
        }
    }
}
impl FromCol for String {
    fn from_value(v: Value) -> SqlResult<Self> {
        match v {
            Value::Text(s) => Ok(s),
            Value::Blob(b) => String::from_utf8(b).map_err(|e| SqlError::Other(e.into())),
            v => Err(bad_type("text", &v)),
        }
    }
}
impl FromCol for Vec<u8> {
    fn from_value(v: Value) -> SqlResult<Self> {
        match v {
            Value::Blob(b) => Ok(b),
            Value::Text(s) => Ok(s.into_bytes()),
            v => Err(bad_type("bytes", &v)),
        }
    }
}
impl<T: FromCol> FromCol for Option<T> {
    fn from_value(v: Value) -> SqlResult<Self> {
        match v {
            Value::Null => Ok(None),
            v => T::from_value(v).map(Some),
        }
    }
}

fn lite_value(r: &rusqlite::Row, i: usize) -> SqlResult<Value> {
    use rusqlite::types::ValueRef;
    Ok(match r.get_ref(i)? {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(n) => Value::Int(n),
        ValueRef::Real(f) => Value::Real(f),
        ValueRef::Text(t) => Value::Text(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Blob(b) => Value::Blob(b.to_vec()),
    })
}

fn pg_value(r: &postgres::Row, i: usize) -> SqlResult<Value> {
    use postgres::types::Type;
    let col = r
        .columns()
        .get(i)
        .ok_or_else(|| SqlError::Other(anyhow!("no column {i}")))?;
    let t = col.type_();
    let v = if *t == Type::INT8 {
        r.try_get::<_, Option<i64>>(i)?.map(Value::Int)
    } else if *t == Type::INT4 {
        r.try_get::<_, Option<i32>>(i)?
            .map(|v| Value::Int(v as i64))
    } else if *t == Type::INT2 {
        r.try_get::<_, Option<i16>>(i)?
            .map(|v| Value::Int(v as i64))
    } else if *t == Type::BOOL {
        r.try_get::<_, Option<bool>>(i)?
            .map(|v| Value::Int(v as i64))
    } else if *t == Type::FLOAT8 {
        r.try_get::<_, Option<f64>>(i)?.map(Value::Real)
    } else if *t == Type::FLOAT4 {
        r.try_get::<_, Option<f32>>(i)?
            .map(|v| Value::Real(v as f64))
    } else if *t == Type::BYTEA {
        r.try_get::<_, Option<Vec<u8>>>(i)?.map(Value::Blob)
    } else if *t == Type::NUMERIC {
        r.try_get::<_, Option<PgNumeric>>(i)?.map(|n| n.0)
    } else {
        r.try_get::<_, Option<String>>(i)?.map(Value::Text)
    };
    Ok(v.unwrap_or(Value::Null))
}

/// NUMERIC (what SUM of BIGINT gives), read as an integer or a float.
struct PgNumeric(Value);
impl<'a> postgres::types::FromSql<'a> for PgNumeric {
    fn from_sql(
        _: &postgres::types::Type,
        raw: &'a [u8],
    ) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        // Binary NUMERIC: ndigits, weight, sign, dscale, then base-10000 digits.
        if raw.len() < 8 {
            return Err("bad numeric".into());
        }
        let n = u16::from_be_bytes([raw[0], raw[1]]) as usize;
        let weight = i16::from_be_bytes([raw[2], raw[3]]) as i32;
        let sign = u16::from_be_bytes([raw[4], raw[5]]);
        let mut v: f64 = 0.0;
        for k in 0..n {
            let d = u16::from_be_bytes([raw[8 + 2 * k], raw[9 + 2 * k]]) as f64;
            v += d * 10000f64.powi(weight - k as i32);
        }
        if sign == 0x4000 {
            v = -v;
        }
        Ok(PgNumeric(if v.fract() == 0.0 && v.abs() < 9.0e18 {
            Value::Int(v as i64)
        } else {
            Value::Real(v)
        }))
    }
    fn accepts(t: &postgres::types::Type) -> bool {
        *t == postgres::types::Type::NUMERIC
    }
}

impl Row<'_> {
    pub fn get<T: FromCol>(&self, i: usize) -> SqlResult<T> {
        let v = match self {
            Row::Lite(r) => lite_value(r, i)?,
            Row::Pg(r) => pg_value(r, i)?,
        };
        T::from_value(v)
    }
    pub fn len(&self) -> usize {
        match self {
            Row::Lite(r) => r.as_ref().column_count(),
            Row::Pg(r) => r.len(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// ------------------------------------------------------------ PostgreSQL binding

#[derive(Debug)]
struct PgArg(Value);

impl postgres::types::ToSql for PgArg {
    fn to_sql(
        &self,
        ty: &postgres::types::Type,
        out: &mut bytes::BytesMut,
    ) -> Result<postgres::types::IsNull, Box<dyn std::error::Error + Sync + Send>> {
        use postgres::types::Type;
        let text_like = |t: &Type| {
            *t == Type::TEXT
                || *t == Type::VARCHAR
                || *t == Type::BPCHAR
                || *t == Type::NAME
                || *t == Type::UNKNOWN
                || t.name() == "citext"
        };
        match &self.0 {
            Value::Null => Ok(postgres::types::IsNull::Yes),
            Value::Int(i) => {
                if *ty == Type::INT8 {
                    i.to_sql(ty, out)
                } else if *ty == Type::INT4 {
                    i32::try_from(*i)?.to_sql(ty, out)
                } else if *ty == Type::INT2 {
                    i16::try_from(*i)?.to_sql(ty, out)
                } else if *ty == Type::BOOL {
                    (*i != 0).to_sql(ty, out)
                } else if *ty == Type::FLOAT8 {
                    (*i as f64).to_sql(ty, out)
                } else if text_like(ty) {
                    out.extend_from_slice(i.to_string().as_bytes());
                    Ok(postgres::types::IsNull::No)
                } else {
                    Err(format!("cannot bind an integer to {ty}").into())
                }
            }
            Value::Real(f) => {
                if *ty == Type::FLOAT8 {
                    f.to_sql(ty, out)
                } else if *ty == Type::FLOAT4 {
                    (*f as f32).to_sql(ty, out)
                } else if *ty == Type::INT8 {
                    (*f as i64).to_sql(ty, out)
                } else {
                    Err(format!("cannot bind a number to {ty}").into())
                }
            }
            Value::Text(s) => {
                if *ty == Type::INT8 {
                    s.parse::<i64>()?.to_sql(ty, out)
                } else {
                    out.extend_from_slice(s.as_bytes());
                    Ok(postgres::types::IsNull::No)
                }
            }
            Value::Blob(b) => {
                out.extend_from_slice(b);
                Ok(postgres::types::IsNull::No)
            }
        }
    }
    fn accepts(_: &postgres::types::Type) -> bool {
        true
    }
    postgres::types::to_sql_checked!();
}

/// Rewrites a query from the SQLite dialect used in `db.rs`.
pub fn pg_sql(sql: &str) -> String {
    let mut out = String::with_capacity(sql.len() + 16);
    let mut next = 1;
    let mut chars = sql.chars().peekable();
    let mut quote = false;
    while let Some(c) = chars.next() {
        if c == '\'' {
            quote = !quote;
            out.push(c);
            continue;
        }
        if c == '?' && !quote {
            let mut n = String::new();
            while let Some(d) = chars.peek().filter(|d| d.is_ascii_digit()) {
                n.push(*d);
                chars.next();
            }
            if n.is_empty() {
                out.push_str(&format!("${next}"));
                next += 1;
            } else {
                out.push('$');
                out.push_str(&n);
            }
            continue;
        }
        out.push(c);
    }
    let mut s = out;
    if s.contains("INSERT OR IGNORE INTO") {
        s = s.replace("INSERT OR IGNORE INTO", "INSERT INTO");
        match s.find(" RETURNING ") {
            Some(i) => s.insert_str(i, " ON CONFLICT DO NOTHING"),
            None => s.push_str(" ON CONFLICT DO NOTHING"),
        }
    }
    s = s
        .replace(" LIKE ", " ILIKE ")
        .replace(" COLLATE NOCASE", "")
        .replace("IFNULL(", "COALESCE(")
        .replace(" AS INTEGER)", " AS BIGINT)");
    // Two-argument MAX/MIN are GREATEST/LEAST.
    for (from, to) in [("MAX(", "GREATEST("), ("MIN(", "LEAST(")] {
        let mut i = 0;
        while let Some(p) = s[i..].find(from).map(|p| p + i) {
            let start = p + from.len();
            let mut depth = 0;
            let mut commas = 0;
            let mut end = None;
            for (k, ch) in s[start..].char_indices() {
                match ch {
                    '(' => depth += 1,
                    ')' if depth == 0 => {
                        end = Some(start + k);
                        break;
                    }
                    ')' => depth -= 1,
                    ',' if depth == 0 => commas += 1,
                    _ => {}
                }
            }
            if end.is_some() && commas == 1 {
                s.replace_range(p..start, to);
                i = p + to.len();
            } else {
                i = start;
            }
        }
    }
    s
}

// ------------------------------------------------------------ connection

pub enum Conn {
    Lite(rusqlite::Connection),
    Pg(PgWorker),
}

type PgCache = HashMap<String, postgres::Statement>;
type PgJob = Box<dyn FnOnce(&mut postgres::Client, &mut PgCache) + Send>;

/// A PostgreSQL connection owned by its own thread. The `postgres` crate
/// drives a private runtime, which must not run inside Tokio's, so every
/// query is handed to that thread and its answer awaited.
pub struct PgWorker {
    tx: std::sync::mpsc::Sender<PgJob>,
}

impl PgWorker {
    pub fn connect(
        url: &str,
        tls: std::sync::Arc<rustls::ClientConfig>,
    ) -> anyhow::Result<PgWorker> {
        let cfg: postgres::Config = url
            .parse()
            .map_err(|e| anyhow!("invalid database.url: {e}"))?;
        let (tx, rx) = std::sync::mpsc::channel::<PgJob>();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("xbnc-postgres".into())
            .spawn(move || {
                let mut client = match cfg.connect(RustlsConnect(tls)) {
                    Ok(c) => {
                        let _ = ready_tx.send(Ok(()));
                        c
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(anyhow!(
                            "cannot connect to PostgreSQL: {}",
                            pg_error_text(&e)
                        )));
                        return;
                    }
                };
                let mut cache = PgCache::new();
                while let Ok(job) = rx.recv() {
                    job(&mut client, &mut cache);
                }
            })?;
        ready_rx
            .recv()
            .map_err(|_| anyhow!("the PostgreSQL thread stopped"))??;
        Ok(PgWorker { tx })
    }

    fn run<R: Send + 'static>(
        &self,
        f: impl FnOnce(&mut postgres::Client, &mut PgCache) -> R + Send + 'static,
    ) -> SqlResult<R> {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        self.tx
            .send(Box::new(move |c, cache| {
                let _ = tx.send(f(c, cache));
            }))
            .map_err(|_| SqlError::Other(anyhow!("the PostgreSQL connection is closed")))?;
        rx.recv()
            .map_err(|_| SqlError::Other(anyhow!("the PostgreSQL connection is closed")))
    }
}

impl Conn {
    pub fn is_postgres(&self) -> bool {
        matches!(self, Conn::Pg(_))
    }

    /// The SQLite connection (for SQLite-only features: backups, PRAGMAs).
    pub fn lite(&self) -> Option<&rusqlite::Connection> {
        match self {
            Conn::Lite(c) => Some(c),
            _ => None,
        }
    }

    fn pg_run<R: Send + 'static>(
        &self,
        sql: &str,
        f: impl FnOnce(&mut postgres::Client, &postgres::Statement) -> Result<R, postgres::Error>
            + Send
            + 'static,
    ) -> SqlResult<R> {
        let Conn::Pg(w) = self else { unreachable!() };
        let sql = sql.to_string();
        w.run(move |client, cache| {
            let st = match cache.get(&sql) {
                Some(s) => s.clone(),
                None => {
                    let s = client.prepare(&pg_sql(&sql))?;
                    if cache.len() > 2000 {
                        cache.clear();
                    }
                    cache.insert(sql.clone(), s.clone());
                    s
                }
            };
            f(client, &st)
        })?
        .map_err(SqlError::from)
    }

    pub fn execute<P: Params>(&self, sql: &str, p: P) -> SqlResult<usize> {
        let vals = p.values();
        match self {
            Conn::Lite(c) => {
                let mut st = c.prepare_cached(sql)?;
                Ok(st.execute(rusqlite::params_from_iter(vals.iter().map(lite_arg)))?)
            }
            Conn::Pg(_) => {
                let args: Vec<PgArg> = vals.into_iter().map(PgArg).collect();
                self.pg_run(sql, move |c, st| {
                    let refs: Vec<&(dyn postgres::types::ToSql + Sync)> = args
                        .iter()
                        .map(|a| a as &(dyn postgres::types::ToSql + Sync))
                        .collect();
                    c.execute(st, &refs).map(|n| n as usize)
                })
            }
        }
    }

    pub fn query_row<P: Params, T, F: FnOnce(&Row) -> SqlResult<T>>(
        &self,
        sql: &str,
        p: P,
        f: F,
    ) -> SqlResult<T> {
        let vals = p.values();
        match self {
            Conn::Lite(c) => {
                let mut st = c.prepare_cached(sql)?;
                let mut rows = st.query(rusqlite::params_from_iter(vals.iter().map(lite_arg)))?;
                match rows.next()? {
                    Some(r) => f(&Row::Lite(r)),
                    None => Err(SqlError::NoRows),
                }
            }
            Conn::Pg(_) => {
                let rows = self.pg_query(sql, vals)?;
                match rows.first() {
                    Some(r) => f(&Row::Pg(r)),
                    None => Err(SqlError::NoRows),
                }
            }
        }
    }

    fn pg_query(&self, sql: &str, vals: Vec<Value>) -> SqlResult<Vec<postgres::Row>> {
        let args: Vec<PgArg> = vals.into_iter().map(PgArg).collect();
        self.pg_run(sql, move |c, st| {
            let refs: Vec<&(dyn postgres::types::ToSql + Sync)> = args
                .iter()
                .map(|a| a as &(dyn postgres::types::ToSql + Sync))
                .collect();
            c.query(st, &refs)
        })
    }

    pub fn prepare(&self, sql: &str) -> SqlResult<Stmt<'_>> {
        Ok(Stmt {
            conn: self,
            sql: sql.to_string(),
        })
    }

    /// Several statements without parameters.
    pub fn execute_batch(&self, sql: &str) -> SqlResult<()> {
        match self {
            Conn::Lite(c) => Ok(c.execute_batch(sql)?),
            Conn::Pg(w) => {
                let sql = sql.to_string();
                w.run(move |c, _| c.batch_execute(&sql))?
                    .map_err(SqlError::from)
            }
        }
    }

    /// Id of the row just inserted.
    pub fn last_insert_rowid(&self) -> i64 {
        match self {
            Conn::Lite(c) => c.last_insert_rowid(),
            Conn::Pg(_) => self
                .query_row("SELECT lastval()", (), |r| r.get(0))
                .unwrap_or(0),
        }
    }

    pub fn transaction(&mut self) -> SqlResult<Tx<'_>> {
        self.execute_batch("BEGIN")?;
        Ok(Tx {
            conn: self,
            done: false,
        })
    }
}

fn lite_arg(v: &Value) -> rusqlite::types::Value {
    match v {
        Value::Null => rusqlite::types::Value::Null,
        Value::Int(i) => rusqlite::types::Value::Integer(*i),
        Value::Real(f) => rusqlite::types::Value::Real(*f),
        Value::Text(s) => rusqlite::types::Value::Text(s.clone()),
        Value::Blob(b) => rusqlite::types::Value::Blob(b.clone()),
    }
}

pub struct Stmt<'c> {
    conn: &'c Conn,
    sql: String,
}

impl Stmt<'_> {
    pub fn query_map<P: Params, T, F: FnMut(&Row) -> SqlResult<T>>(
        &mut self,
        p: P,
        mut f: F,
    ) -> SqlResult<std::vec::IntoIter<SqlResult<T>>> {
        let vals = p.values();
        let out: Vec<SqlResult<T>> = match self.conn {
            Conn::Lite(c) => {
                let mut st = c.prepare_cached(&self.sql)?;
                let mut rows = st.query(rusqlite::params_from_iter(vals.iter().map(lite_arg)))?;
                let mut out = Vec::new();
                while let Some(r) = rows.next()? {
                    out.push(f(&Row::Lite(r)));
                }
                out
            }
            Conn::Pg(_) => self
                .conn
                .pg_query(&self.sql, vals)?
                .iter()
                .map(|r| f(&Row::Pg(r)))
                .collect(),
        };
        Ok(out.into_iter())
    }

    pub fn query_row<P: Params, T, F: FnOnce(&Row) -> SqlResult<T>>(
        &mut self,
        p: P,
        f: F,
    ) -> SqlResult<T> {
        self.conn.query_row(&self.sql, p, f)
    }

    pub fn exists<P: Params>(&mut self, p: P) -> SqlResult<bool> {
        Ok(self.query_map(p, |_| Ok(()))?.next().is_some())
    }

    pub fn execute<P: Params>(&mut self, p: P) -> SqlResult<usize> {
        self.conn.execute(&self.sql, p)
    }
}

/// A transaction: rolled back when dropped without `commit`.
pub struct Tx<'c> {
    conn: &'c mut Conn,
    done: bool,
}

impl Tx<'_> {
    pub fn commit(mut self) -> SqlResult<()> {
        self.done = true;
        self.conn.execute_batch("COMMIT")
    }
}

impl std::ops::Deref for Tx<'_> {
    type Target = Conn;
    fn deref(&self) -> &Conn {
        self.conn
    }
}

impl Drop for Tx<'_> {
    fn drop(&mut self) {
        if !self.done {
            let _ = self.conn.execute_batch("ROLLBACK");
        }
    }
}

// ------------------------------------------------------------ PostgreSQL TLS

/// TLS for PostgreSQL connections with rustls (system CAs, or `ca_file`).
#[derive(Clone)]
pub struct RustlsConnect(pub std::sync::Arc<rustls::ClientConfig>);

impl<S> tokio_postgres::tls::MakeTlsConnect<S> for RustlsConnect
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    type Stream = RustlsStream<S>;
    type TlsConnect = RustlsConnector;
    type Error = rustls::pki_types::InvalidDnsNameError;

    fn make_tls_connect(&mut self, domain: &str) -> Result<RustlsConnector, Self::Error> {
        let name = rustls::pki_types::ServerName::try_from(domain.to_string())?;
        Ok(RustlsConnector {
            config: self.0.clone(),
            name,
        })
    }
}

pub struct RustlsConnector {
    config: std::sync::Arc<rustls::ClientConfig>,
    name: rustls::pki_types::ServerName<'static>,
}

impl<S> tokio_postgres::tls::TlsConnect<S> for RustlsConnector
where
    S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
{
    type Stream = RustlsStream<S>;
    type Error = std::io::Error;
    type Future = std::pin::Pin<
        Box<dyn std::future::Future<Output = std::io::Result<RustlsStream<S>>> + Send>,
    >;

    fn connect(self, stream: S) -> Self::Future {
        Box::pin(async move {
            let s = tokio_rustls::TlsConnector::from(self.config)
                .connect(self.name, stream)
                .await?;
            Ok(RustlsStream(Box::new(s)))
        })
    }
}

pub struct RustlsStream<S>(Box<tokio_rustls::client::TlsStream<S>>);

impl<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin> tokio::io::AsyncRead
    for RustlsStream<S>
{
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut *self.0).poll_read(cx, buf)
    }
}

impl<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin> tokio::io::AsyncWrite
    for RustlsStream<S>
{
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut *self.0).poll_write(cx, buf)
    }
    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut *self.0).poll_flush(cx)
    }
    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut *self.0).poll_shutdown(cx)
    }
}

impl<S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin> tokio_postgres::tls::TlsStream
    for RustlsStream<S>
{
    fn channel_binding(&self) -> tokio_postgres::tls::ChannelBinding {
        tokio_postgres::tls::ChannelBinding::none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialect_translation() {
        assert_eq!(
            pg_sql("SELECT a FROM t WHERE x = ?1 AND y LIKE ? AND z = '?'"),
            "SELECT a FROM t WHERE x = $1 AND y ILIKE $1 AND z = '?'"
        );
        assert_eq!(
            pg_sql("INSERT OR IGNORE INTO t (a) VALUES (?1)"),
            "INSERT INTO t (a) VALUES ($1) ON CONFLICT DO NOTHING"
        );
        assert_eq!(
            pg_sql("UPDATE t SET ts = MAX(ts, excluded.ts), n = MAX(n)"),
            "UPDATE t SET ts = GREATEST(ts, excluded.ts), n = MAX(n)"
        );
        assert_eq!(
            pg_sql("SELECT MAX(COALESCE(a, 0), b)"),
            "SELECT GREATEST(COALESCE(a, 0), b)"
        );
    }

    #[test]
    fn sqlite_layer() {
        let mut c = Conn::Lite(rusqlite::Connection::open_in_memory().unwrap());
        c.execute_batch(
            "CREATE TABLE t (id INTEGER PRIMARY KEY, name TEXT UNIQUE, b BLOB, n INTEGER)",
        )
        .unwrap();
        c.execute(
            "INSERT INTO t (name, b, n) VALUES (?1, ?2, ?3)",
            crate::params!["a", vec![1u8, 2], None::<i64>],
        )
        .unwrap();
        assert_eq!(c.last_insert_rowid(), 1);
        assert!(matches!(
            c.execute("INSERT INTO t (name) VALUES (?1)", ["a"]),
            Err(SqlError::Constraint(_))
        ));
        let (n, b, x): (String, Vec<u8>, Option<i64>) = c
            .query_row("SELECT name, b, n FROM t WHERE id = ?1", [1], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?))
            })
            .unwrap();
        assert_eq!((n.as_str(), b, x), ("a", vec![1, 2], None));
        assert!(c
            .query_row("SELECT id FROM t WHERE id = 9", (), |r| r.get::<i64>(0))
            .optional()
            .unwrap()
            .is_none());
        {
            let t = c.transaction().unwrap();
            t.execute("INSERT INTO t (name) VALUES ('b')", ()).unwrap();
        }
        let n: i64 = c
            .query_row("SELECT COUNT(*) FROM t", (), |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1, "rolled back");
        let rows: Vec<String> = c
            .prepare("SELECT name FROM t")
            .unwrap()
            .query_map((), |r| r.get(0))
            .unwrap()
            .collect::<SqlResult<_>>()
            .unwrap();
        assert_eq!(rows, vec!["a"]);
    }
}
