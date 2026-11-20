//! Single sign-on: OpenID Connect (authorization code + PKCE) and LDAP
//! simple bind. Outside identities are linked to local users in the
//! `user_identities` table; a local account is never taken over by name
//! unless the administrator allows it with `link_existing`.

use anyhow::{bail, Context, Result};
use base64::engine::general_purpose::{STANDARD as B64, URL_SAFE_NO_PAD as B64URL};
use base64::Engine;
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::db::{self, Db, User};

fn default_role() -> String {
    "user".into()
}
fn default_scopes() -> String {
    "openid profile email".into()
}
fn default_username_claim() -> String {
    "preferred_username".into()
}
fn default_groups_claim() -> String {
    "groups".into()
}

fn check_role(r: &str) -> Result<()> {
    match crate::rbac::Role::parse(r) {
        Some(crate::rbac::Role::Superadmin) | None => {
            bail!("default_role must be a role other than superadmin (got {r:?})")
        }
        _ => Ok(()),
    }
}

fn http_ok(url: &str, what: &str) -> Result<()> {
    let u = crate::http::Url::parse(url).with_context(|| format!("{what} is not a valid URL"))?;
    if !u.tls {
        let local = u.host == "localhost"
            || u.host
                .parse::<std::net::IpAddr>()
                .map(|i| i.is_loopback())
                .unwrap_or(false);
        if !local {
            bail!("{what} must use https:// (plain http is only allowed on this machine)");
        }
    }
    Ok(())
}

// ------------------------------------------------------------ OpenID Connect

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OidcConfig {
    /// Name on the login button ("Google", "Keycloak"...).
    pub name: Option<String>,
    /// Issuer URL; `/.well-known/openid-configuration` is read from it.
    pub issuer: String,
    pub client_id: String,
    pub client_secret: String,
    #[serde(default = "default_scopes")]
    pub scopes: String,
    /// Claim used as the user name of accounts created automatically.
    #[serde(default = "default_username_claim")]
    pub username_claim: String,
    /// Create an xBNC user the first time someone signs in.
    #[serde(default)]
    pub auto_create: bool,
    /// Link to an existing local user with the same name (only for
    /// providers you control).
    #[serde(default)]
    pub link_existing: bool,
    #[serde(default = "default_role")]
    pub default_role: String,
    /// When set, only members of one of these groups may sign in.
    #[serde(default)]
    pub allowed_groups: Vec<String>,
    #[serde(default = "default_groups_claim")]
    pub groups_claim: String,
}

impl OidcConfig {
    pub fn display_name(&self) -> String {
        self.name.clone().unwrap_or_else(|| "SSO".into())
    }
    pub fn validate(&self) -> Result<()> {
        http_ok(&self.issuer, "oidc.issuer")?;
        if self.client_id.is_empty() || self.client_secret.is_empty() {
            bail!("oidc.client_id and oidc.client_secret are required");
        }
        if !self.scopes.split_whitespace().any(|s| s == "openid") {
            bail!("oidc.scopes must include \"openid\"");
        }
        check_role(&self.default_role)
    }
}

/// Discovery document and keys, cached for an hour.
#[derive(Default)]
pub struct OidcCache {
    inner: Mutex<Option<(Instant, Arc<Provider>)>>,
}

pub struct Provider {
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub issuer: String,
    pub jwks: Vec<Value>,
}

async fn get_json(url: &str, tls: Arc<rustls::ClientConfig>) -> Result<Value> {
    let r = crate::http::request("GET", url, None, tls, 512 * 1024).await?;
    if r.status != 200 {
        bail!("{url} answered {}", r.status);
    }
    serde_json::from_slice(&r.body).with_context(|| format!("{url} did not return JSON"))
}

impl OidcCache {
    pub async fn provider(
        &self,
        cfg: &OidcConfig,
        tls: Arc<rustls::ClientConfig>,
        refresh: bool,
    ) -> Result<Arc<Provider>> {
        if !refresh {
            if let Some((at, p)) = &*self.inner.lock().unwrap_or_else(|e| e.into_inner()) {
                if at.elapsed() < Duration::from_secs(3600) {
                    return Ok(p.clone());
                }
            }
        }
        let base = cfg.issuer.trim_end_matches('/');
        let d = get_json(
            &format!("{base}/.well-known/openid-configuration"),
            tls.clone(),
        )
        .await?;
        let s = |k: &str| -> Result<String> {
            d[k].as_str()
                .map(str::to_string)
                .with_context(|| format!("the provider's configuration has no {k}"))
        };
        let issuer = s("issuer")?;
        if issuer.trim_end_matches('/') != base {
            bail!("the provider says its issuer is {issuer}, not {base}");
        }
        let (auth, token, jwks_uri) = (
            s("authorization_endpoint")?,
            s("token_endpoint")?,
            s("jwks_uri")?,
        );
        for (u, w) in [
            (&auth, "authorization_endpoint"),
            (&token, "token_endpoint"),
            (&jwks_uri, "jwks_uri"),
        ] {
            http_ok(u, w)?;
        }
        let keys = get_json(&jwks_uri, tls).await?;
        let p = Arc::new(Provider {
            authorization_endpoint: auth,
            token_endpoint: token,
            issuer,
            jwks: keys["keys"].as_array().cloned().unwrap_or_default(),
        });
        *self.inner.lock().unwrap_or_else(|e| e.into_inner()) = Some((Instant::now(), p.clone()));
        Ok(p)
    }
}

/// PKCE S256 challenge of a verifier.
pub fn pkce_challenge(verifier: &str) -> String {
    use sha2::{Digest, Sha256};
    B64URL.encode(Sha256::digest(verifier.as_bytes()))
}

/// Percent-encodes everything but unreserved characters.
pub fn url_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

pub fn form_encode(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", url_encode(k), url_encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

pub fn authorize_url(
    p: &Provider,
    cfg: &OidcConfig,
    redirect: &str,
    state: &str,
    nonce: &str,
    verifier: &str,
) -> String {
    let q = form_encode(&[
        ("response_type", "code"),
        ("client_id", &cfg.client_id),
        ("redirect_uri", redirect),
        ("scope", &cfg.scopes),
        ("state", state),
        ("nonce", nonce),
        ("code_challenge", &pkce_challenge(verifier)),
        ("code_challenge_method", "S256"),
    ]);
    let sep = if p.authorization_endpoint.contains('?') {
        '&'
    } else {
        '?'
    };
    format!("{}{sep}{q}", p.authorization_endpoint)
}

/// Exchanges the code and returns the verified ID token claims.
pub async fn exchange_code(
    cache: &OidcCache,
    cfg: &OidcConfig,
    tls: Arc<rustls::ClientConfig>,
    code: &str,
    redirect: &str,
    verifier: &str,
    nonce: &str,
) -> Result<Value> {
    let p = cache.provider(cfg, tls.clone(), false).await?;
    let body = form_encode(&[
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect),
        ("code_verifier", verifier),
        ("client_id", &cfg.client_id),
        ("client_secret", &cfg.client_secret),
    ]);
    let r = crate::http::request_opts(
        "POST",
        &p.token_endpoint,
        Some(("application/x-www-form-urlencoded", body.as_bytes())),
        tls.clone(),
        512 * 1024,
        Duration::from_secs(15),
        false,
        &[],
    )
    .await?;
    let v: Value = serde_json::from_slice(&r.body).unwrap_or(Value::Null);
    if r.status != 200 {
        bail!(
            "the provider refused the sign-in: {}",
            v["error_description"]
                .as_str()
                .or(v["error"].as_str())
                .unwrap_or("unknown error")
        );
    }
    let token = v["id_token"]
        .as_str()
        .context("the provider returned no id_token")?;
    let now = db::now_ms() / 1000;
    match verify_id_token(token, &p, cfg, nonce, now) {
        Ok(c) => Ok(c),
        Err(e) if e.to_string().contains("unknown key") => {
            // Keys rotated: read them again once.
            let p = cache.provider(cfg, tls, true).await?;
            verify_id_token(token, &p, cfg, nonce, now)
        }
        Err(e) => Err(e),
    }
}

/// Checks an ID token's signature (RS256 or ES256) and claims.
pub fn verify_id_token(
    token: &str,
    p: &Provider,
    cfg: &OidcConfig,
    nonce: &str,
    now: i64,
) -> Result<Value> {
    use ring::signature;
    let mut parts = token.split('.');
    let (Some(h), Some(c), Some(s), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        bail!("malformed ID token");
    };
    let header: Value = serde_json::from_slice(&B64URL.decode(h)?)?;
    let claims: Value = serde_json::from_slice(&B64URL.decode(c)?)?;
    let sig = B64URL.decode(s)?;
    let alg = header["alg"].as_str().unwrap_or("");
    let kid = header["kid"].as_str();
    let key = p
        .jwks
        .iter()
        .find(|k| {
            kid.is_none_or(|kid| k["kid"].as_str() == Some(kid))
                && k["use"].as_str().is_none_or(|u| u == "sig")
                && match alg {
                    "RS256" => k["kty"] == "RSA",
                    "ES256" => k["kty"] == "EC" && k["crv"] == "P-256",
                    _ => false,
                }
        })
        .with_context(|| format!("unknown key or unsupported algorithm ({alg})"))?;
    let msg = format!("{h}.{c}");
    let field = |n: &str| -> Result<Vec<u8>> {
        Ok(B64URL.decode(
            key[n]
                .as_str()
                .with_context(|| format!("key without {n}"))?,
        )?)
    };
    let ok = match alg {
        "RS256" => {
            let der = crate::webauthn::rsa_der(&field("n")?, &field("e")?);
            signature::UnparsedPublicKey::new(&signature::RSA_PKCS1_2048_8192_SHA256, der)
                .verify(msg.as_bytes(), &sig)
                .is_ok()
        }
        _ => {
            let mut pt = vec![4];
            pt.extend(field("x")?);
            pt.extend(field("y")?);
            signature::UnparsedPublicKey::new(&signature::ECDSA_P256_SHA256_FIXED, pt)
                .verify(msg.as_bytes(), &sig)
                .is_ok()
        }
    };
    if !ok {
        bail!("the ID token signature is not valid");
    }
    if claims["iss"].as_str().map(|i| i.trim_end_matches('/'))
        != Some(p.issuer.trim_end_matches('/'))
    {
        bail!("the ID token is from another issuer");
    }
    let aud_ok = match &claims["aud"] {
        Value::String(a) => a == &cfg.client_id,
        Value::Array(a) => a.iter().any(|x| x == cfg.client_id.as_str()),
        _ => false,
    };
    if !aud_ok {
        bail!("the ID token is for another application");
    }
    let exp = claims["exp"]
        .as_i64()
        .context("the ID token has no expiry")?;
    if exp + 60 < now {
        bail!("the ID token has expired");
    }
    if claims["nonce"].as_str() != Some(nonce) {
        bail!("the ID token nonce does not match");
    }
    if claims["sub"].as_str().is_none_or(str::is_empty) {
        bail!("the ID token has no subject");
    }
    if !cfg.allowed_groups.is_empty() {
        let groups: Vec<&str> = match &claims[cfg.groups_claim.as_str()] {
            Value::Array(a) => a.iter().filter_map(Value::as_str).collect(),
            Value::String(s) => vec![s.as_str()],
            _ => vec![],
        };
        if !cfg
            .allowed_groups
            .iter()
            .any(|g| groups.contains(&g.as_str()))
        {
            bail!("your account is not in a group allowed to use this bouncer");
        }
    }
    Ok(claims)
}

/// A local user name from a claim value: allowed characters only.
pub fn username_from(v: &str) -> Option<String> {
    let base = v.split('@').next().unwrap_or(v);
    let s: String = base
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        .take(32)
        .collect();
    (!s.is_empty()).then_some(s)
}

// ------------------------------------------------------------ LDAP

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LdapConfig {
    /// `ldaps://host:636`, or `ldap://host:389` with `starttls = true`.
    pub url: String,
    #[serde(default)]
    pub starttls: bool,
    /// DN to bind as; `{username}` is replaced by the login name.
    pub bind_dn: String,
    /// PEM file with the CA of the LDAP server (default: system CAs).
    pub ca_file: Option<PathBuf>,
    #[serde(default)]
    pub auto_create: bool,
    #[serde(default)]
    pub link_existing: bool,
    #[serde(default = "default_role")]
    pub default_role: String,
}

struct LdapTarget {
    host: String,
    port: u16,
    tls: bool,
}

impl LdapConfig {
    fn target(&self) -> Result<LdapTarget> {
        let (tls, rest, port) = if let Some(r) = self.url.strip_prefix("ldaps://") {
            (true, r, 636)
        } else if let Some(r) = self.url.strip_prefix("ldap://") {
            (false, r, 389)
        } else {
            bail!("ldap.url must start with ldaps:// or ldap://");
        };
        let rest = rest.trim_end_matches('/');
        let (host, port) = match rest.rsplit_once(':') {
            Some((h, p)) if !h.ends_with(']') || rest.starts_with('[') => (
                h.trim_matches(['[', ']']).to_string(),
                p.parse().context("invalid ldap.url port")?,
            ),
            _ => (rest.trim_matches(['[', ']']).to_string(), port),
        };
        if host.is_empty() {
            bail!("ldap.url has no host");
        }
        Ok(LdapTarget { host, port, tls })
    }

    pub fn validate(&self) -> Result<()> {
        let t = self.target()?;
        if !t.tls && !self.starttls {
            let local = t.host == "localhost"
                || t.host
                    .parse::<std::net::IpAddr>()
                    .map(|i| i.is_loopback())
                    .unwrap_or(false);
            if !local {
                bail!("ldap: use ldaps:// or starttls = true; passwords must not travel in plain text");
            }
        }
        if t.tls && self.starttls {
            bail!("ldap: starttls is only for ldap:// addresses");
        }
        if !self.bind_dn.contains("{username}") {
            bail!("ldap.bind_dn must contain {{username}}");
        }
        if let Some(f) = &self.ca_file {
            if !f.exists() {
                bail!("ldap.ca_file {} does not exist", f.display());
            }
        }
        check_role(&self.default_role)
    }

    fn tls_config(&self, default: Arc<rustls::ClientConfig>) -> Result<Arc<rustls::ClientConfig>> {
        let Some(f) = &self.ca_file else {
            return Ok(default);
        };
        use rustls_pki_types::pem::PemObject;
        let mut roots = rustls::RootCertStore::empty();
        for c in rustls_pki_types::CertificateDer::pem_file_iter(f)
            .context("cannot read ldap.ca_file")?
        {
            roots.add(c?)?;
        }
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        Ok(Arc::new(
            rustls::ClientConfig::builder_with_provider(provider)
                .with_protocol_versions(&[&rustls::version::TLS13, &rustls::version::TLS12])?
                .with_root_certificates(roots)
                .with_no_client_auth(),
        ))
    }

    pub fn dn_for(&self, username: &str) -> String {
        self.bind_dn.replace("{username}", username)
    }
}

fn ber_len(n: usize) -> Vec<u8> {
    if n < 128 {
        vec![n as u8]
    } else if n < 256 {
        vec![0x81, n as u8]
    } else {
        vec![0x82, (n >> 8) as u8, n as u8]
    }
}

fn tlv(tag: u8, body: &[u8]) -> Vec<u8> {
    let mut v = vec![tag];
    v.extend(ber_len(body.len()));
    v.extend_from_slice(body);
    v
}

fn ldap_message(id: u8, op: Vec<u8>) -> Vec<u8> {
    let mut body = vec![0x02, 0x01, id];
    body.extend(op);
    tlv(0x30, &body)
}

pub fn bind_request(id: u8, dn: &str, password: &str) -> Vec<u8> {
    let mut b = vec![0x02, 0x01, 0x03];
    b.extend(tlv(0x04, dn.as_bytes()));
    b.extend(tlv(0x80, password.as_bytes()));
    ldap_message(id, tlv(0x60, &b))
}

fn starttls_request(id: u8) -> Vec<u8> {
    ldap_message(id, tlv(0x77, &tlv(0x80, b"1.3.6.1.4.1.1466.20037")))
}

async fn read_tlv<S: AsyncRead + Unpin>(s: &mut S) -> Result<(u8, Vec<u8>)> {
    let tag = s.read_u8().await?;
    let first = s.read_u8().await?;
    let len = if first < 0x80 {
        first as usize
    } else {
        let n = (first & 0x7f) as usize;
        if n == 0 || n > 3 {
            bail!("unsupported LDAP length");
        }
        let mut l = 0usize;
        for _ in 0..n {
            l = (l << 8) | s.read_u8().await? as usize;
        }
        l
    };
    if len > 64 * 1024 {
        bail!("LDAP answer too large");
    }
    let mut body = vec![0; len];
    s.read_exact(&mut body).await?;
    Ok((tag, body))
}

/// The result code of an LDAP response (BindResponse / ExtendedResponse).
pub fn result_code(msg: &[u8]) -> Result<u8> {
    // messageID
    if msg.first() != Some(&0x02) {
        bail!("bad LDAP response");
    }
    let id_len = *msg.get(1).context("bad LDAP response")? as usize;
    let op = msg.get(2 + id_len..).context("bad LDAP response")?;
    // [APPLICATION n] { ENUMERATED resultCode, ... }
    let mut i = 1;
    let l = *op.get(i).context("bad LDAP response")?;
    i += if l & 0x80 != 0 {
        1 + (l & 0x7f) as usize
    } else {
        1
    };
    if op.get(i) != Some(&0x0a) || op.get(i + 1) != Some(&1) {
        bail!("bad LDAP response");
    }
    op.get(i + 2).copied().context("bad LDAP response")
}

async fn ldap_exchange<S: AsyncRead + AsyncWrite + Unpin>(s: &mut S, req: &[u8]) -> Result<u8> {
    s.write_all(req).await?;
    s.flush().await?;
    let (tag, body) = read_tlv(s).await?;
    if tag != 0x30 {
        bail!("bad LDAP response");
    }
    result_code(&body)
}

/// Tries a simple bind. Ok(true): the password is right; Ok(false): it is
/// wrong; Err: the server could not be asked.
pub async fn ldap_bind(
    cfg: &LdapConfig,
    tls: Arc<rustls::ClientConfig>,
    username: &str,
    password: &str,
) -> Result<bool> {
    // An empty password would be an anonymous bind, which "succeeds".
    if password.is_empty() || db::validate_name("username", username).is_err() {
        return Ok(false);
    }
    let t = cfg.target()?;
    let tls = cfg.tls_config(tls)?;
    let dn = cfg.dn_for(username);
    let fut = async {
        let mut tcp = tokio::net::TcpStream::connect((t.host.as_str(), t.port)).await?;
        let connector = tokio_rustls::TlsConnector::from(tls);
        let name = rustls_pki_types::ServerName::try_from(t.host.clone())?;
        let code = if t.tls {
            let mut s = connector.connect(name, tcp).await?;
            ldap_exchange(&mut s, &bind_request(1, &dn, password)).await?
        } else if cfg.starttls {
            if ldap_exchange(&mut tcp, &starttls_request(1)).await? != 0 {
                bail!("the LDAP server refused STARTTLS");
            }
            let mut s = connector.connect(name, tcp).await?;
            ldap_exchange(&mut s, &bind_request(2, &dn, password)).await?
        } else {
            ldap_exchange(&mut tcp, &bind_request(1, &dn, password)).await?
        };
        Ok::<u8, anyhow::Error>(code)
    };
    let code = tokio::time::timeout(Duration::from_secs(10), fut)
        .await
        .context("the LDAP server did not answer in time")??;
    match code {
        0 => Ok(true),
        49 | 32 | 50 | 53 => Ok(false),
        c => bail!("the LDAP server answered with code {c}"),
    }
}

// ------------------------------------------------------------ users

pub enum Provision {
    User(Box<User>),
    /// No linked account and none may be created.
    NotLinked,
    /// A local account with that name exists and is not linked.
    NameTaken(String),
}

/// Finds (or, when allowed, creates or links) the local user of an outside
/// identity.
pub async fn provision(
    db: &Db,
    provider: &str,
    subject: &str,
    username: Option<String>,
    auto_create: bool,
    link_existing: bool,
    role: &str,
) -> Result<Provision> {
    let (p, s, role) = (provider.to_string(), subject.to_string(), role.to_string());
    let hash = if auto_create {
        let pw = B64.encode(
            ring::rand::generate::<[u8; 32]>(&ring::rand::SystemRandom::new())
                .map_err(|_| anyhow::anyhow!("no randomness"))?
                .expose(),
        );
        Some(tokio::task::spawn_blocking(move || crate::crypto::hash_password(&pw)).await??)
    } else {
        None
    };
    db.call(move |c| {
        if let Some(uid) = db::identity_user(c, &p, &s)? {
            return Ok(match db::get_user_by_id(c, uid)? {
                Some(u) => Provision::User(Box::new(u)),
                None => Provision::NotLinked,
            });
        }
        let Some(name) = username else {
            return Ok(Provision::NotLinked);
        };
        if let Some(u) = db::get_user(c, &name)? {
            if link_existing {
                db::link_identity(c, &p, &s, u.id)?;
                return Ok(Provision::User(Box::new(u)));
            }
            return Ok(Provision::NameTaken(name));
        }
        let Some(hash) = hash else {
            return Ok(Provision::NotLinked);
        };
        let id = db::create_user_with_role(c, &name, &hash, &role)?;
        db::link_identity(c, &p, &s, id)?;
        Ok(match db::get_user_by_id(c, id)? {
            Some(u) => Provision::User(Box::new(u)),
            None => Provision::NotLinked,
        })
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::rand::SystemRandom;
    use ring::signature::{EcdsaKeyPair, KeyPair, ECDSA_P256_SHA256_FIXED_SIGNING};

    fn cfg() -> OidcConfig {
        toml::from_str(
            r#"issuer = "https://id.example.org"
client_id = "xbnc"
client_secret = "s"
allowed_groups = ["irc"]"#,
        )
        .unwrap()
    }

    #[test]
    fn id_token_checks() {
        let rng = SystemRandom::new();
        let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &rng).unwrap();
        let kp = EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8.as_ref(), &rng)
            .unwrap();
        let pk = kp.public_key().as_ref();
        let p = Provider {
            authorization_endpoint: "https://id.example.org/auth".into(),
            token_endpoint: "https://id.example.org/token".into(),
            issuer: "https://id.example.org".into(),
            jwks: vec![
                serde_json::json!({"kty":"EC","crv":"P-256","kid":"k1","x":B64URL.encode(&pk[1..33]),"y":B64URL.encode(&pk[33..])}),
            ],
        };
        let make = |claims: Value| {
            let h = B64URL.encode(r#"{"alg":"ES256","kid":"k1"}"#);
            let c = B64URL.encode(claims.to_string());
            let sig = kp.sign(&rng, format!("{h}.{c}").as_bytes()).unwrap();
            format!("{h}.{c}.{}", B64URL.encode(sig.as_ref()))
        };
        let good = serde_json::json!({"iss":"https://id.example.org","aud":"xbnc","exp":2000,"nonce":"n1","sub":"42","groups":["irc"],"preferred_username":"ana"});
        let c = verify_id_token(&make(good.clone()), &p, &cfg(), "n1", 1000).unwrap();
        assert_eq!(c["sub"], "42");
        for (k, v) in [("aud", "other"), ("iss", "https://evil"), ("nonce", "n2")] {
            let mut bad = good.clone();
            bad[k] = Value::String(v.into());
            assert!(
                verify_id_token(&make(bad), &p, &cfg(), "n1", 1000).is_err(),
                "{k}"
            );
        }
        assert!(
            verify_id_token(&make(good.clone()), &p, &cfg(), "n1", 5000).is_err(),
            "expired"
        );
        let mut nogroup = good.clone();
        nogroup["groups"] = serde_json::json!(["other"]);
        assert!(verify_id_token(&make(nogroup), &p, &cfg(), "n1", 1000).is_err());
        let mut t = make(good);
        t.replace_range(t.len() - 4.., "AAAA");
        assert!(
            verify_id_token(&t, &p, &cfg(), "n1", 1000).is_err(),
            "bad signature"
        );
        assert_eq!(
            username_from("Ana.Pérez@example.org").as_deref(),
            Some("Ana.Prez")
        );
    }

    #[test]
    fn ldap_encoding_and_config() {
        let r = bind_request(1, "uid=ana,dc=x", "pw");
        assert_eq!(&r[..2], &[0x30, (r.len() - 2) as u8]);
        // BindResponse success / invalid credentials.
        assert_eq!(
            result_code(&[0x02, 0x01, 0x01, 0x61, 0x07, 0x0a, 0x01, 0x00, 0x04, 0x00, 0x04, 0x00])
                .unwrap(),
            0
        );
        assert_eq!(
            result_code(&[0x02, 0x01, 0x01, 0x61, 0x07, 0x0a, 0x01, 0x31, 0x04, 0x00, 0x04, 0x00])
                .unwrap(),
            49
        );
        let c: LdapConfig =
            toml::from_str("url = \"ldap://ldap.example.org\"\nbind_dn = \"uid={username},dc=x\"")
                .unwrap();
        assert!(c.validate().is_err(), "plain LDAP to another machine");
        let c: LdapConfig =
            toml::from_str("url = \"ldaps://ldap.example.org\"\nbind_dn = \"uid={username},dc=x\"")
                .unwrap();
        c.validate().unwrap();
        assert_eq!(c.target().unwrap().port, 636);
    }

    /// A tiny LDAP server that accepts one password.
    #[tokio::test]
    async fn ldap_bind_against_a_fake_server() {
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = l.local_addr().unwrap().port();
        tokio::spawn(async move {
            loop {
                let (mut s, _) = l.accept().await.unwrap();
                tokio::spawn(async move {
                    let (_, body) = read_tlv(&mut s).await.unwrap();
                    let ok = body.windows(6).any(|w| w == b"secret");
                    let code = if ok { 0 } else { 49 };
                    s.write_all(&[
                        0x30, 0x0c, 0x02, 0x01, 0x01, 0x61, 0x07, 0x0a, 0x01, code, 0x04, 0x00,
                        0x04, 0x00,
                    ])
                    .await
                    .unwrap();
                });
            }
        });
        let c: LdapConfig = toml::from_str(&format!(
            "url = \"ldap://127.0.0.1:{port}\"\nbind_dn = \"uid={{username}},dc=x\""
        ))
        .unwrap();
        c.validate().unwrap();
        let tls = crate::server::tls_client_config().unwrap();
        assert!(ldap_bind(&c, tls.clone(), "ana", "secret").await.unwrap());
        assert!(!ldap_bind(&c, tls.clone(), "ana", "wrong").await.unwrap());
        assert!(
            !ldap_bind(&c, tls.clone(), "ana", "").await.unwrap(),
            "anonymous bind refused"
        );
        assert!(
            !ldap_bind(&c, tls, "a,b=c", "secret").await.unwrap(),
            "odd names refused"
        );
    }
}
