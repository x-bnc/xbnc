//! History search text: the FTS query built from it must always be valid
//! FTS5 syntax (no SQL or FTS errors from user input).
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(q) = std::str::from_utf8(data) else { return };
    if let Some(fts) = xbncd::db::fts_query(q) {
        let c = rusqlite_conn();
        c.query_row(
            "SELECT count(*) FROM t WHERE t MATCH ?1",
            [&fts],
            |r| r.get::<_, i64>(0),
        )
        .expect("generated FTS query must be valid");
    }
});

fn rusqlite_conn() -> rusqlite::Connection {
    let c = rusqlite::Connection::open_in_memory().unwrap();
    c.execute_batch("CREATE VIRTUAL TABLE t USING fts5(text, tokenize='unicode61 remove_diacritics 2'); INSERT INTO t VALUES ('hola mundo');")
        .unwrap();
    c
}
