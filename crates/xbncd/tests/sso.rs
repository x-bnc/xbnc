//! Passkeys, OpenID Connect and LDAP sign-in against a real bouncer, with a
//! software authenticator, a fake identity provider and a fake directory.

mod common;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use base64::Engine;
use common::*;
use ring::rand::SystemRandom;
use ring::signature::{
    EcdsaKeyPair, KeyPair, ECDSA_P256_SHA256_ASN1_SIGNING, ECDSA_P256_SHA256_FIXED_SIGNING,
};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

async fn raw(
    addr: std::net::SocketAddr,
    host: &str,
    method: &str,
    path: &str,
    headers: &str,
    body: &str,
) -> (u16, String, String) {
    let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{headers}\r\n{body}",
        body.len()
    );
    s.write_all(req.as_bytes()).await.unwrap();
    let mut out = Vec::new();
    s.read_to_end(&mut out).await.unwrap();
    let text = String::from_utf8_lossy(&out).to_string();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let status = head.split(' ').nth(1).unwrap().parse().unwrap();
    (status, head.to_string(), body.to_string())
}

fn json_of(body: &str) -> Value {
    let start = body.find(['{', '[']).unwrap_or(0);
    let end = body.rfind(['}', ']']).map(|e| e + 1).unwrap_or(body.len());
    serde_json::from_str(&body[start..end]).unwrap_or(Value::Null)
}

fn cookie_of(head: &str) -> Option<String> {
    head.lines()
        .find(|l| {
            l.to_ascii_lowercase()
                .starts_with("set-cookie: xbnc_session=")
        })
        .map(|l| {
            l.split(';')
                .next()
                .unwrap()
                .split_once(": ")
                .unwrap()
                .1
                .to_string()
        })
}

fn header_of(head: &str, name: &str) -> Option<String> {
    head.lines()
        .find(|l| {
            l.to_ascii_lowercase()
                .starts_with(&format!("{}:", name.to_ascii_lowercase()))
        })
        .map(|l| l.split_once(": ").unwrap().1.trim().to_string())
}

fn cbor_bytes(v: &[u8]) -> Vec<u8> {
    let mut out = if v.len() < 24 {
        vec![0x40 | v.len() as u8]
    } else if v.len() < 256 {
        vec![0x58, v.len() as u8]
    } else {
        vec![0x59, (v.len() >> 8) as u8, v.len() as u8]
    };
    out.extend_from_slice(v);
    out
}

fn cbor_text(s: &str) -> Vec<u8> {
    let mut out = vec![0x60 | s.len() as u8];
    out.extend_from_slice(s.as_bytes());
    out
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn passkey_register_and_sign_in() {
    let h = Harness::start().await;
    let web = h.running.as_ref().unwrap().web_addr.unwrap();
    let host = format!("localhost:{}", web.port());
    let origin = format!("http://{host}");
    let (st, head, body) = raw(
        web,
        &host,
        "POST",
        "/api/v1/auth/login",
        "",
        &json!({"username":"alice","password":PASSWORD}).to_string(),
    )
    .await;
    assert_eq!(st, 200, "{body}");
    let cookie = cookie_of(&head).unwrap();
    let csrf = json_of(&body)["csrf"].as_str().unwrap().to_string();
    let authed = format!("Cookie: {cookie}\r\nX-CSRF-Token: {csrf}\r\nOrigin: {origin}\r\n");

    let (st, _, body) = raw(
        web,
        &host,
        "POST",
        "/api/v1/me/passkeys/options",
        &authed,
        "",
    )
    .await;
    assert_eq!(st, 200, "{body}");
    let opts = json_of(&body);
    assert_eq!(opts["publicKey"]["rp"]["id"], "localhost");
    let challenge = B64URL
        .decode(opts["publicKey"]["challenge"].as_str().unwrap())
        .unwrap();

    // The software authenticator.
    let rng = SystemRandom::new();
    let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &rng).unwrap();
    let kp =
        EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, pkcs8.as_ref(), &rng).unwrap();
    let pk = kp.public_key().as_ref();
    let mut cose = vec![0xa5, 0x01, 0x02, 0x03, 0x26, 0x20, 0x01, 0x21];
    cose.extend(cbor_bytes(&pk[1..33]));
    cose.push(0x22);
    cose.extend(cbor_bytes(&pk[33..65]));
    let cred_id = [7u8; 20];
    let mut auth = Sha256::digest(b"localhost").to_vec();
    auth.push(0x01 | 0x04 | 0x40);
    auth.extend_from_slice(&0u32.to_be_bytes());
    auth.extend_from_slice(&[0u8; 16]);
    auth.extend_from_slice(&(cred_id.len() as u16).to_be_bytes());
    auth.extend_from_slice(&cred_id);
    auth.extend_from_slice(&cose);
    let mut att = vec![0xa3];
    att.extend(cbor_text("fmt"));
    att.extend(cbor_text("none"));
    att.extend(cbor_text("attStmt"));
    att.push(0xa0);
    att.extend(cbor_text("authData"));
    att.extend(cbor_bytes(&auth));
    let cd =
        json!({"type":"webauthn.create","challenge":B64URL.encode(&challenge),"origin":origin})
            .to_string();
    let reg = json!({"id": opts["id"], "label": "Test key", "client_data_json": B64URL.encode(&cd), "attestation_object": B64URL.encode(&att)});
    let (st, _, body) = raw(
        web,
        &host,
        "POST",
        "/api/v1/me/passkeys",
        &authed,
        &reg.to_string(),
    )
    .await;
    assert_eq!(st, 200, "{body}");
    // The same ceremony cannot be used twice.
    let (st, _, _) = raw(
        web,
        &host,
        "POST",
        "/api/v1/me/passkeys",
        &authed,
        &reg.to_string(),
    )
    .await;
    assert_eq!(st, 400);
    let (_, _, body) = raw(web, &host, "GET", "/api/v1/me/passkeys", &authed, "").await;
    assert_eq!(json_of(&body)[0]["label"], "Test key");

    // Sign in without a password.
    let sign_in = |count: u32| {
        let (rng, kp, origin, host) = (&rng, &kp, origin.clone(), host.clone());
        async move {
            let (st, _, body) = raw(
                web,
                &host,
                "POST",
                "/api/v1/auth/passkey/options",
                &format!("Origin: {origin}\r\n"),
                "",
            )
            .await;
            assert_eq!(st, 200, "{body}");
            let o = json_of(&body);
            let ch = o["publicKey"]["challenge"].as_str().unwrap().to_string();
            let cd = json!({"type":"webauthn.get","challenge":ch,"origin":origin}).to_string();
            let mut ad = Sha256::digest(b"localhost").to_vec();
            ad.push(0x05);
            ad.extend_from_slice(&count.to_be_bytes());
            let mut msg = ad.clone();
            msg.extend_from_slice(&Sha256::digest(cd.as_bytes()));
            let sig = kp.sign(rng, &msg).unwrap();
            let req = json!({"id": o["id"], "credential_id": B64URL.encode(cred_id), "client_data_json": B64URL.encode(&cd), "authenticator_data": B64URL.encode(&ad), "signature": B64URL.encode(sig.as_ref())});
            raw(
                web,
                &host,
                "POST",
                "/api/v1/auth/passkey",
                &format!("Origin: {origin}\r\n"),
                &req.to_string(),
            )
            .await
        }
    };
    let (st, head, body) = sign_in(1).await;
    assert_eq!(st, 200, "{body}");
    assert!(cookie_of(&head).is_some());
    assert_eq!(json_of(&body)["user"]["username"], "alice");
    // A counter that does not go forward means a cloned key.
    let (st, _, _) = sign_in(1).await;
    assert_eq!(st, 401);

    // Requests from another site are refused.
    let (st, _, _) = raw(
        web,
        &host,
        "POST",
        "/api/v1/auth/passkey/options",
        "Origin: https://evil.example\r\n",
        "",
    )
    .await;
    assert_eq!(st, 403);
}

/// A fake OpenID Connect provider.
struct Idp {
    base: String,
    nonce: Arc<Mutex<String>>,
    claims: Arc<Mutex<Value>>,
}

async fn fake_idp() -> Idp {
    use axum::routing::{get, post};
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://127.0.0.1:{}", l.local_addr().unwrap().port());
    let rng = SystemRandom::new();
    let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, &rng).unwrap();
    let kp = Arc::new(
        EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_FIXED_SIGNING, pkcs8.as_ref(), &rng).unwrap(),
    );
    let pk = kp.public_key().as_ref().to_vec();
    let nonce = Arc::new(Mutex::new(String::new()));
    let claims = Arc::new(Mutex::new(json!({})));
    let disc = json!({
        "issuer": base,
        "authorization_endpoint": format!("{base}/auth"),
        "token_endpoint": format!("{base}/token"),
        "jwks_uri": format!("{base}/jwks"),
    });
    let jwks = json!({"keys":[{"kty":"EC","crv":"P-256","kid":"k1","use":"sig","x":B64URL.encode(&pk[1..33]),"y":B64URL.encode(&pk[33..])}]});
    let (n2, c2, b2) = (nonce.clone(), claims.clone(), base.clone());
    let app = axum::Router::new()
        .route("/.well-known/openid-configuration", get(move || async move { axum::Json(disc) }))
        .route("/jwks", get(move || async move { axum::Json(jwks) }))
        .route(
            "/token",
            post(move |body: String| {
                let (kp, n, c, base) = (kp.clone(), n2.clone(), c2.clone(), b2.clone());
                async move {
                    assert!(body.contains("code=good-code"), "{body}");
                    assert!(body.contains("code_verifier="), "{body}");
                    let mut claims = c.lock().unwrap().clone();
                    claims["iss"] = json!(base);
                    claims["aud"] = json!("xbnc");
                    claims["exp"] = json!(4_000_000_000u64);
                    claims["nonce"] = json!(n.lock().unwrap().clone());
                    let hh = B64URL.encode(r#"{"alg":"ES256","kid":"k1"}"#);
                    let cc = B64URL.encode(claims.to_string());
                    let sig = kp.sign(&SystemRandom::new(), format!("{hh}.{cc}").as_bytes()).unwrap();
                    axum::Json(json!({"id_token": format!("{hh}.{cc}.{}", B64URL.encode(sig.as_ref())), "token_type":"Bearer"}))
                }
            }),
        );
    tokio::spawn(async move { axum::serve(l, app).await.unwrap() });
    Idp {
        base,
        nonce,
        claims,
    }
}

fn query_param(url: &str, name: &str) -> String {
    let q = url.split_once('?').unwrap().1;
    q.split('&')
        .find_map(|kv| kv.strip_prefix(&format!("{name}=")))
        .unwrap()
        .replace("%2D", "-")
        .replace("%5F", "_")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn openid_connect_sign_in() {
    let idp = fake_idp().await;
    let h = Harness::start_all(
        None,
        &[],
        &[],
        r#"public_url = "http://xbnc.test""#,
        &format!("[oidc]\nname = \"Fake\"\nissuer = \"{}\"\nclient_id = \"xbnc\"\nclient_secret = \"s3cret\"\nauto_create = true\n", idp.base),
    )
    .await;
    let web = h.running.as_ref().unwrap().web_addr.unwrap();
    let host = web.to_string();
    let (_, _, body) = raw(web, &host, "GET", "/api/v1/auth/methods", "", "").await;
    assert_eq!(json_of(&body)["oidc"]["name"], "Fake");

    let login = |claims: Value| {
        let (idp_nonce, idp_claims, host) = (idp.nonce.clone(), idp.claims.clone(), host.clone());
        async move {
            let (st, head, _) = raw(web, &host, "GET", "/api/v1/auth/oidc/start", "", "").await;
            assert_eq!(st, 302);
            let loc = header_of(&head, "location").unwrap();
            assert!(loc.contains("code_challenge_method=S256"));
            assert!(loc.contains(
                "redirect_uri=http%3A%2F%2Fxbnc.test%2Fapi%2Fv1%2Fauth%2Foidc%2Fcallback"
            ));
            *idp_nonce.lock().unwrap() = query_param(&loc, "nonce");
            *idp_claims.lock().unwrap() = claims;
            let state = query_param(&loc, "state");
            raw(
                web,
                &host,
                "GET",
                &format!("/api/v1/auth/oidc/callback?code=good-code&state={state}"),
                "",
                "",
            )
            .await
        }
    };
    // A new user is created and signed in.
    let (st, head, body) = login(json!({"sub":"u-1","preferred_username":"bob"})).await;
    assert_eq!(st, 200);
    assert!(body.contains("url=/\""), "{body}");
    let cookie = cookie_of(&head).expect("signed in");
    let (_, _, me) = raw(
        web,
        &host,
        "GET",
        "/api/v1/auth/me",
        &format!("Cookie: {cookie}\r\n"),
        "",
    )
    .await;
    assert_eq!(json_of(&me)["user"]["username"], "bob");
    // The same subject signs in to the same user again.
    let (_, head, _) = login(json!({"sub":"u-1","preferred_username":"renamed"})).await;
    let cookie = cookie_of(&head).unwrap();
    let (_, _, me) = raw(
        web,
        &host,
        "GET",
        "/api/v1/auth/me",
        &format!("Cookie: {cookie}\r\n"),
        "",
    )
    .await;
    assert_eq!(json_of(&me)["user"]["username"], "bob");
    // An existing local account is not taken over by name.
    let (st, head, body) = login(json!({"sub":"u-2","preferred_username":"alice"})).await;
    assert_eq!(st, 200);
    assert!(cookie_of(&head).is_none());
    assert!(body.contains("sso_error="), "{body}");
    // A state cannot be replayed or invented.
    let (_, head, body) = raw(
        web,
        &host,
        "GET",
        "/api/v1/auth/oidc/callback?code=good-code&state=made-up",
        "",
        "",
    )
    .await;
    assert!(cookie_of(&head).is_none() && body.contains("sso_error="));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ldap_sign_in_creates_a_linked_user() {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = l.local_addr().unwrap().port();
    tokio::spawn(async move {
        loop {
            let (mut s, _) = l.accept().await.unwrap();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 512];
                let n = s.read(&mut buf).await.unwrap();
                let req = &buf[..n];
                let ok = req.windows(16).any(|w| w == b"uid=carol,dc=x\x80\x07")
                    && req.windows(7).any(|w| w == b"dirpass");
                let code = if ok { 0 } else { 49 };
                s.write_all(&[
                    0x30, 0x0c, 0x02, 0x01, 0x01, 0x61, 0x07, 0x0a, 0x01, code, 0x04, 0x00, 0x04,
                    0x00,
                ])
                .await
                .unwrap();
            });
        }
    });
    let h = Harness::start_all(
        None,
        &[],
        &[],
        "",
        &format!("[ldap]\nurl = \"ldap://127.0.0.1:{port}\"\nbind_dn = \"uid={{username}},dc=x\"\nauto_create = true\n"),
    )
    .await;
    let web = h.running.as_ref().unwrap().web_addr.unwrap();
    let host = web.to_string();
    let login = |u: &str, p: &str| json!({"username": u, "password": p}).to_string();
    let (st, _, body) = raw(
        web,
        &host,
        "POST",
        "/api/v1/auth/login",
        "",
        &login("carol", "dirpass"),
    )
    .await;
    assert_eq!(st, 200, "{body}");
    assert_eq!(json_of(&body)["user"]["username"], "carol");
    let (st, _, _) = raw(
        web,
        &host,
        "POST",
        "/api/v1/auth/login",
        "",
        &login("carol", "wrong"),
    )
    .await;
    assert_eq!(st, 401);
    // Local passwords keep working.
    let (st, _, _) = raw(
        web,
        &host,
        "POST",
        "/api/v1/auth/login",
        "",
        &login("alice", PASSWORD),
    )
    .await;
    assert_eq!(st, 200);
}
