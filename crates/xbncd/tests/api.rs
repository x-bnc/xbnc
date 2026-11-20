//! REST API tests against a real bouncer instance (router called in-process).

mod common;
use axum::body::Body;
use axum::extract::connect_info::MockConnectInfo;
use axum::http::{header, Method, Request, StatusCode};
use axum::Router;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use std::net::SocketAddr;
use tower::ServiceExt;
use xbncd::db;

const PW: &str = "correct-horse-1";

struct T {
    _dir: tempfile::TempDir,
    running: xbncd::Running,
    app: Router,
}

/// Response pieces the tests look at.
struct Resp {
    status: StatusCode,
    body: Value,
    raw: String,
    headers: axum::http::HeaderMap,
}

impl T {
    async fn new() -> T {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("xbnc.toml");
        std::fs::write(
            &p,
            format!(
                r#"
                secret_key_file = "{d}/secret.key"
                [database]
                path = "{d}/xbnc.db"
                {pg}
                [[listen]]
                address = "127.0.0.1:0"
                [web]
                address = "127.0.0.1:0"
                [limits]
                login_max_failures = 3
                "#,
                d = dir.path().display(),
                pg = common::pg_test_database()
                    .map(|u| format!("url = \"{u}\""))
                    .unwrap_or_default(),
            ),
        )
        .unwrap();
        let cfg = xbncd::Config::load(&p).unwrap();
        let web = cfg.web.clone().unwrap();
        let running = xbncd::start(cfg, true).await.unwrap();
        let addr: SocketAddr = "127.0.0.1:40000".parse().unwrap();
        let app = xbncd::web::router(running.ctx.clone(), web).layer(MockConnectInfo(addr));
        T {
            _dir: dir,
            running,
            app,
        }
    }

    async fn req(
        &self,
        method: Method,
        path: &str,
        cookie: Option<&str>,
        csrf: Option<&str>,
        body: Option<Value>,
    ) -> Resp {
        let mut b = Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, "localhost");
        if let Some(c) = cookie {
            b = b.header(header::COOKIE, format!("xbnc_session={c}"));
        }
        if let Some(t) = csrf {
            b = b.header("x-csrf-token", t);
        }
        let req = match body {
            Some(v) => b
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(v.to_string()))
                .unwrap(),
            None => b.body(Body::empty()).unwrap(),
        };
        let resp = self.app.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let headers = resp.headers().clone();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let raw = String::from_utf8_lossy(&bytes).to_string();
        let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        Resp {
            status,
            body,
            raw,
            headers,
        }
    }

    /// Creates a user directly in the database.
    fn add_user(&self, name: &str, role: &str) -> i64 {
        let h = xbncd::crypto::hash_password(PW).unwrap();
        self.running
            .ctx
            .db
            .with(|c| db::create_user_with_role(c, name, &h, role))
            .unwrap()
    }

    async fn login(&self, name: &str) -> Session {
        self.login_with(name, PW).await
    }

    async fn login_with(&self, name: &str, password: &str) -> Session {
        let r = self
            .req(
                Method::POST,
                "/api/v1/auth/login",
                None,
                None,
                Some(json!({"username": name, "password": password})),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.raw);
        session_from(&r)
    }
}

struct Session {
    cookie: String,
    csrf: String,
}

fn session_from(r: &Resp) -> Session {
    let set = r.headers.get(header::SET_COOKIE).unwrap().to_str().unwrap();
    assert!(
        set.contains("HttpOnly") && set.contains("SameSite=Strict"),
        "{set}"
    );
    let cookie = set
        .split(';')
        .next()
        .unwrap()
        .split_once('=')
        .unwrap()
        .1
        .to_string();
    Session {
        cookie,
        csrf: r.body["csrf"].as_str().unwrap().to_string(),
    }
}

impl T {
    async fn get(&self, s: &Session, path: &str) -> Resp {
        self.req(Method::GET, path, Some(&s.cookie), None, None)
            .await
    }
    async fn send(&self, s: &Session, m: Method, path: &str, body: Value) -> Resp {
        self.req(m, path, Some(&s.cookie), Some(&s.csrf), Some(body))
            .await
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn first_run_setup_with_one_time_code() {
    let t = T::new().await;
    let r = t.req(Method::GET, "/api/v1/setup", None, None, None).await;
    assert_eq!(r.body["needed"], true);
    let token = t
        .running
        .ctx
        .setup_token
        .lock()
        .unwrap()
        .clone()
        .expect("setup code generated");

    let bad = t
        .req(
            Method::POST,
            "/api/v1/setup",
            None,
            None,
            Some(json!({"token": "AAAA-BBBB-CCCC", "username": "boss", "password": PW})),
        )
        .await;
    assert_eq!(bad.status, StatusCode::UNAUTHORIZED);
    let weak = t
        .req(
            Method::POST,
            "/api/v1/setup",
            None,
            None,
            Some(json!({"token": token, "username": "boss", "password": "short"})),
        )
        .await;
    assert_eq!(weak.status, StatusCode::BAD_REQUEST);

    let ok = t
        .req(
            Method::POST,
            "/api/v1/setup",
            None,
            None,
            Some(json!({"token": token.to_lowercase(), "username": "boss", "password": PW})),
        )
        .await;
    assert_eq!(ok.status, StatusCode::OK, "{}", ok.raw);
    assert_eq!(ok.body["user"]["role"], "superadmin");
    let s = session_from(&ok);
    assert_eq!(
        t.get(&s, "/api/v1/auth/me").await.body["user"]["username"],
        "boss"
    );

    let again = t
        .req(
            Method::POST,
            "/api/v1/setup",
            None,
            None,
            Some(json!({"token": token, "username": "x", "password": PW})),
        )
        .await;
    assert_eq!(again.status, StatusCode::CONFLICT, "setup works only once");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn login_sessions_csrf_and_security_headers() {
    let t = T::new().await;
    t.add_user("alice", "user");
    let r = t
        .req(Method::GET, "/api/v1/auth/me", None, None, None)
        .await;
    assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    assert!(r.headers.get(header::CONTENT_SECURITY_POLICY).is_some());
    assert_eq!(r.headers.get(header::X_FRAME_OPTIONS).unwrap(), "DENY");
    assert_eq!(r.headers.get(header::CACHE_CONTROL).unwrap(), "no-store");

    let bad = t
        .req(
            Method::POST,
            "/api/v1/auth/login",
            None,
            None,
            Some(json!({"username": "alice", "password": "nope"})),
        )
        .await;
    assert_eq!(bad.body["error"], "bad_credentials");

    let s = t.login("alice").await;
    let me = t.get(&s, "/api/v1/auth/me").await;
    assert_eq!(me.body["permissions"]["manage_users"], false);
    assert_eq!(me.body["permissions"]["manage_own_networks"], true);

    // Mutations need the CSRF token...
    let no_csrf = t
        .req(
            Method::PUT,
            "/api/v1/me/password",
            Some(&s.cookie),
            None,
            Some(json!({"current_password": PW, "new_password": "another-password"})),
        )
        .await;
    assert_eq!(no_csrf.body["error"], "csrf");
    // ...and are refused from another origin.
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/v1/auth/logout")
        .header(header::HOST, "localhost")
        .header(header::ORIGIN, "https://evil.example")
        .header(header::COOKIE, format!("xbnc_session={}", s.cookie))
        .header("x-csrf-token", &s.csrf)
        .body(Body::empty())
        .unwrap();
    assert_eq!(
        t.app.clone().oneshot(req).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );

    let sessions = t.get(&s, "/api/v1/sessions").await;
    assert_eq!(sessions.body.as_array().unwrap().len(), 1);
    assert_eq!(sessions.body[0]["current"], true);

    let out = t
        .send(&s, Method::POST, "/api/v1/auth/logout", json!({}))
        .await;
    assert_eq!(out.status, StatusCode::OK);
    assert_eq!(
        t.get(&s, "/api/v1/auth/me").await.status,
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn brute_force_blocks_web_logins() {
    let t = T::new().await;
    t.add_user("alice", "user");
    for _ in 0..3 {
        t.req(
            Method::POST,
            "/api/v1/auth/login",
            None,
            None,
            Some(json!({"username": "alice", "password": "bad"})),
        )
        .await;
    }
    let r = t
        .req(
            Method::POST,
            "/api/v1/auth/login",
            None,
            None,
            Some(json!({"username": "alice", "password": PW})),
        )
        .await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);

    // An admin sees the block and lifts it.
    t.add_user("root", "superadmin");
    t.running
        .ctx
        .limiter
        .record_success("127.0.0.1".parse().unwrap());
    let ctx = &t.running.ctx;
    ctx.limiter.unblock("127.0.0.1".parse().unwrap());
    let admin = t.login("root").await;
    let sec = t.get(&admin, "/api/v1/security?failures=true").await;
    assert!(sec.body["events"].as_array().unwrap().len() >= 3);
    assert!(sec.body["events"][0]["success"] == false);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn networks_channels_and_history() {
    let t = T::new().await;
    let uid = t.add_user("alice", "user");
    let s = t.login("alice").await;

    let created = t
        .send(&s, Method::POST, "/api/v1/networks", json!({
            "name": "libera", "host": "127.0.0.1", "port": 1, "tls": false, "nick": "alice",
            "sasl_username": "alice", "sasl_password": "sasl-secret", "channels": ["#xbnc-dev", "#xbnc"]
        }))
        .await;
    assert_eq!(created.status, StatusCode::CREATED, "{}", created.raw);
    let nid = created.body["id"].as_i64().unwrap();
    assert_eq!(created.body["has_sasl_password"], true);
    assert!(
        !created.raw.contains("sasl-secret"),
        "secrets are never returned"
    );
    let stored = t
        .running
        .ctx
        .db
        .with(|c| db::get_network_by_id(c, nid))
        .unwrap()
        .unwrap();
    assert!(
        stored.sasl_password_enc.unwrap().starts_with("v1:"),
        "stored encrypted"
    );

    let bad = t
        .send(
            &s,
            Method::POST,
            "/api/v1/networks",
            json!({"name": "bad name", "host": "x", "port": 6697, "nick": "a"}),
        )
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);

    // Commands on connect: validated, stored encrypted, readable by the owner.
    let oc = format!("/api/v1/networks/{nid}/on-connect");
    assert_eq!(t.get(&s, &oc).await.body["commands"], "");
    let bad_oc = t
        .send(&s, Method::PUT, &oc, json!({"commands": "/msg nickserv"}))
        .await;
    assert_eq!(bad_oc.status, StatusCode::BAD_REQUEST, "{}", bad_oc.raw);
    let text = "/msg NickServ IDENTIFY oc-secret";
    let put = t
        .send(&s, Method::PUT, &oc, json!({"commands": text}))
        .await;
    assert_eq!(put.status, StatusCode::OK, "{}", put.raw);
    assert_eq!(t.get(&s, &oc).await.body["commands"], text);
    let enc = t
        .running
        .ctx
        .db
        .with(move |c| db::get_on_connect(c, nid))
        .unwrap()
        .unwrap();
    assert!(!enc.contains("oc-secret"), "stored encrypted");

    // The actor is started and reports its state.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let one = t.get(&s, &format!("/api/v1/networks/{nid}")).await;
    assert!(one.body["status"]["state"].is_string(), "{}", one.raw);

    let upd = t
        .send(
            &s,
            Method::PATCH,
            &format!("/api/v1/networks/{nid}"),
            json!({"nick": "alice2", "sasl_password": ""}),
        )
        .await;
    assert_eq!(
        upd.status,
        StatusCode::BAD_REQUEST,
        "clearing only the SASL password leaves an incomplete SASL setup"
    );
    let upd = t
        .send(
            &s,
            Method::PATCH,
            &format!("/api/v1/networks/{nid}"),
            json!({"nick": "alice2", "sasl_username": "", "sasl_password": ""}),
        )
        .await;
    assert_eq!(upd.status, StatusCode::OK, "{}", upd.raw);
    assert_eq!(upd.body["nick"], "alice2");
    assert_eq!(upd.body["has_sasl_password"], false);

    let ch = t.get(&s, &format!("/api/v1/networks/{nid}/channels")).await;
    assert_eq!(ch.body.as_array().unwrap().len(), 2);
    let add = t
        .send(
            &s,
            Method::POST,
            &format!("/api/v1/networks/{nid}/channels"),
            json!({"name": "#new", "key": "k"}),
        )
        .await;
    assert_eq!(add.status, StatusCode::OK);
    let bad_ch = t
        .send(
            &s,
            Method::POST,
            &format!("/api/v1/networks/{nid}/channels"),
            json!({"name": "nohash"}),
        )
        .await;
    assert_eq!(bad_ch.status, StatusCode::BAD_REQUEST);
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let del = t
        .req(
            Method::DELETE,
            &format!("/api/v1/networks/{nid}/channels/%23xbnc-dev"),
            Some(&s.cookie),
            Some(&s.csrf),
            None,
        )
        .await;
    assert_eq!(del.status, StatusCode::OK);
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let names: Vec<String> = t
        .running
        .ctx
        .db
        .with(|c| db::list_channels(c, nid))
        .unwrap()
        .into_iter()
        .map(|c| c.name)
        .collect();
    assert_eq!(
        names,
        vec!["#xbnc", "#new"],
        "saved channels keep their join order"
    );

    // History: search, paging and export.
    t.running
        .ctx
        .db
        .with(|c| {
            for i in 0..5 {
                db::insert_message(
                    c,
                    &db::NewMessage {
                        network_id: nid,
                        target: "#xbnc".into(),
                        target_cf: "#xbnc".into(),
                        ts: 1_700_000_000_000 + i,
                        sender: "bob!b@h".into(),
                        command: "PRIVMSG".into(),
                        text: format!("hola {i}"),
                        outgoing: false,
                        mention: false,
                    },
                )?;
            }
            Ok(())
        })
        .unwrap();
    let conv = t
        .get(&s, &format!("/api/v1/networks/{nid}/conversations"))
        .await;
    assert_eq!(conv.body[0]["messages"], 5);
    let h = t
        .get(
            &s,
            &format!("/api/v1/networks/{nid}/history?target=%23xbnc&q=hola&limit=2"),
        )
        .await;
    assert_eq!(h.body["messages"].as_array().unwrap().len(), 2);
    assert_eq!(h.body["more"], true);
    assert_eq!(h.body["messages"][0]["nick"], "bob");
    let exp = t
        .get(
            &s,
            &format!("/api/v1/networks/{nid}/history/export?target=%23xbnc"),
        )
        .await;
    assert!(
        exp.raw.contains("<bob> hola 0") && exp.raw.contains("<bob> hola 4"),
        "{}",
        exp.raw
    );
    assert!(exp
        .headers
        .get(header::CONTENT_DISPOSITION)
        .unwrap()
        .to_str()
        .unwrap()
        .contains("attachment"));

    // Another user cannot see it at all.
    t.add_user("mallory", "user");
    let m = t.login("mallory").await;
    assert_eq!(
        t.get(&m, &format!("/api/v1/networks/{nid}")).await.status,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        t.get(&m, "/api/v1/networks")
            .await
            .body
            .as_array()
            .unwrap()
            .len(),
        0
    );

    // A limited user cannot create networks but an admin can create one for them.
    let lim = t.add_user("lim", "limited");
    let l = t.login("lim").await;
    let r = t
        .send(
            &l,
            Method::POST,
            "/api/v1/networks",
            json!({"name": "x", "host": "127.0.0.1", "port": 1, "nick": "lim"}),
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    t.add_user("root", "superadmin");
    let root = t.login("root").await;
    let r = t
        .send(&root, Method::POST, "/api/v1/networks", json!({"user_id": lim, "name": "x", "host": "127.0.0.1", "port": 1, "nick": "lim", "tls": false}))
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.raw);
    assert_eq!(
        t.get(&l, "/api/v1/networks")
            .await
            .body
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        t.get(&root, "/api/v1/networks?all=true")
            .await
            .body
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let d = t
        .req(
            Method::DELETE,
            &format!("/api/v1/networks/{nid}"),
            Some(&s.cookie),
            Some(&s.csrf),
            None,
        )
        .await;
    assert_eq!(d.status, StatusCode::OK);
    assert!(t
        .running
        .ctx
        .db
        .with(|c| db::get_network_by_id(c, nid))
        .unwrap()
        .is_none());
    let _ = uid;
    let audit = t.get(&root, "/api/v1/audit").await;
    assert!(audit.raw.contains("network.create") && audit.raw.contains("network.delete"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn user_management_and_rbac() {
    let t = T::new().await;
    t.add_user("root", "superadmin");
    let admin_id = t.add_user("adm", "admin");
    t.add_user("mod", "moderator");
    let root = t.login("root").await;
    let adm = t.login("adm").await;
    let md = t.login("mod").await;

    // Admin creates a user but cannot create another admin.
    let r = t
        .send(
            &adm,
            Method::POST,
            "/api/v1/users",
            json!({"username": "bob", "password": PW, "role": "user", "max_clients": 2}),
        )
        .await;
    assert_eq!(r.status, StatusCode::CREATED, "{}", r.raw);
    let bob = r.body["id"].as_i64().unwrap();
    assert_eq!(r.body["max_clients"], 2);
    let r = t
        .send(
            &adm,
            Method::POST,
            "/api/v1/users",
            json!({"username": "eve", "password": PW, "role": "admin"}),
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);

    // Moderator can list and suspend users but not change roles.
    assert_eq!(t.get(&md, "/api/v1/users").await.status, StatusCode::OK);
    let r = t
        .send(
            &md,
            Method::PATCH,
            &format!("/api/v1/users/{bob}"),
            json!({"enabled": false}),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.raw);
    let r = t
        .send(
            &md,
            Method::PATCH,
            &format!("/api/v1/users/{bob}"),
            json!({"role": "moderator"}),
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    let r = t
        .send(
            &md,
            Method::PATCH,
            &format!("/api/v1/users/{admin_id}"),
            json!({"enabled": false}),
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);

    // A suspended user cannot log in.
    let r = t
        .req(
            Method::POST,
            "/api/v1/auth/login",
            None,
            None,
            Some(json!({"username": "bob", "password": PW})),
        )
        .await;
    assert_eq!(r.body["error"], "account_disabled");

    // Limits can be cleared with null.
    let r = t
        .send(
            &root,
            Method::PATCH,
            &format!("/api/v1/users/{bob}"),
            json!({"enabled": true, "max_clients": null}),
        )
        .await;
    assert_eq!(r.body["max_clients"], Value::Null);

    // Last superadmin cannot be demoted or deleted; nobody can demote themselves.
    let me = t.get(&root, "/api/v1/auth/me").await.body["user"]["id"]
        .as_i64()
        .unwrap();
    let r = t
        .send(
            &root,
            Method::PATCH,
            &format!("/api/v1/users/{me}"),
            json!({"role": "user"}),
        )
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    let r = t
        .req(
            Method::DELETE,
            &format!("/api/v1/users/{me}"),
            Some(&root.cookie),
            Some(&root.csrf),
            None,
        )
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);

    // Password reset logs the user out everywhere.
    let b = t.login("bob").await;
    let r = t
        .send(
            &adm,
            Method::POST,
            &format!("/api/v1/users/{bob}/password"),
            json!({"password": "brand-new-password"}),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(
        t.get(&b, "/api/v1/auth/me").await.status,
        StatusCode::UNAUTHORIZED
    );

    let r = t
        .req(
            Method::DELETE,
            &format!("/api/v1/users/{bob}"),
            Some(&adm.cookie),
            Some(&adm.csrf),
            None,
        )
        .await;
    assert_eq!(r.status, StatusCode::OK);
    assert_eq!(
        t.get(&root, "/api/v1/users")
            .await
            .body
            .as_array()
            .unwrap()
            .len(),
        3
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn settings_apply_live() {
    let t = T::new().await;
    t.add_user("root", "superadmin");
    t.add_user("adm", "admin");
    let root = t.login("root").await;
    let adm = t.login("adm").await;

    let cur = t.get(&adm, "/api/v1/settings").await;
    assert_eq!(cur.body["editable"], false);
    let mut s = cur.body["current"].clone();
    s["history"]["retention_days"] = json!(7);
    s["limits"]["login_max_failures"] = json!(1);
    assert_eq!(
        t.send(&adm, Method::PUT, "/api/v1/settings", s.clone())
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    let r = t
        .send(&root, Method::PUT, "/api/v1/settings", s.clone())
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.raw);
    assert_eq!(t.running.ctx.settings.get().history.retention_days, 7);

    // The new login threshold applies immediately: one failure blocks.
    t.req(
        Method::POST,
        "/api/v1/auth/login",
        None,
        None,
        Some(json!({"username": "x", "password": "y"})),
    )
    .await;
    let r = t
        .req(
            Method::POST,
            "/api/v1/auth/login",
            None,
            None,
            Some(json!({"username": "root", "password": PW})),
        )
        .await;
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);

    let mut bad = s.clone();
    bad["limits"]["client_queue"] = json!(1);
    assert_eq!(
        t.send(&root, Method::PUT, "/api/v1/settings", bad)
            .await
            .status,
        StatusCode::BAD_REQUEST
    );

    let sys = t.get(&root, "/api/v1/system").await;
    assert_eq!(sys.body["database"]["schema_version"], 11);
    let dash = t.get(&root, "/api/v1/dashboard").await;
    assert_eq!(dash.body["scope"], "global");
    assert!(dash.body["system"]["users"].as_i64().unwrap() >= 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_factor_authentication() {
    let t = T::new().await;
    t.add_user("alice", "user");
    let s = t.login("alice").await;
    let setup = t
        .send(&s, Method::POST, "/api/v1/me/totp/setup", json!({}))
        .await;
    let secret = setup.body["secret"].as_str().unwrap().to_string();
    assert!(setup.body["qr_svg"].as_str().unwrap().contains("<svg"));
    assert!(setup.body["uri"]
        .as_str()
        .unwrap()
        .starts_with("otpauth://totp/"));

    let wrong = t
        .send(
            &s,
            Method::POST,
            "/api/v1/me/totp/enable",
            json!({"code": "000000"}),
        )
        .await;
    assert_eq!(wrong.status, StatusCode::BAD_REQUEST);
    let code = xbncd::totp::code_at(&secret, xbncd::totp::now_unix()).unwrap();
    let ok = t
        .send(
            &s,
            Method::POST,
            "/api/v1/me/totp/enable",
            json!({"code": code}),
        )
        .await;
    assert_eq!(ok.status, StatusCode::OK, "{}", ok.raw);

    let r = t
        .req(
            Method::POST,
            "/api/v1/auth/login",
            None,
            None,
            Some(json!({"username": "alice", "password": PW})),
        )
        .await;
    assert_eq!(r.body["error"], "totp_required");
    let r = t
        .req(
            Method::POST,
            "/api/v1/auth/login",
            None,
            None,
            Some(json!({"username": "alice", "password": PW, "totp_code": "123456"})),
        )
        .await;
    assert!(
        r.body["error"] == "bad_totp" || r.status == StatusCode::OK,
        "a random code is almost surely wrong"
    );
    let code = xbncd::totp::code_at(&secret, xbncd::totp::now_unix()).unwrap();
    let r = t
        .req(
            Method::POST,
            "/api/v1/auth/login",
            None,
            None,
            Some(json!({"username": "alice", "password": PW, "totp_code": code})),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.raw);
    let s2 = session_from(&r);
    let off = t
        .send(
            &s2,
            Method::POST,
            "/api/v1/me/totp/disable",
            json!({"password": PW}),
        )
        .await;
    assert_eq!(off.status, StatusCode::OK);
    t.login("alice").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn panel_and_openapi_are_served() {
    let t = T::new().await;
    let index = t.req(Method::GET, "/", None, None, None).await;
    assert_eq!(index.status, StatusCode::OK);
    assert!(index.raw.to_lowercase().contains("<!doctype html"));
    let deep = t.req(Method::GET, "/networks/12", None, None, None).await;
    assert_eq!(
        deep.status,
        StatusCode::OK,
        "SPA routes fall back to index.html"
    );
    let missing = t.req(Method::GET, "/api/v1/nope", None, None, None).await;
    assert_eq!(missing.status, StatusCode::NOT_FOUND);
    let spec = t
        .req(Method::GET, "/api/v1/openapi.json", None, None, None)
        .await;
    assert!(spec.body["openapi"].as_str().unwrap().starts_with("3."));
    assert_eq!(
        t.req(Method::GET, "/api/v1/health", None, None, None)
            .await
            .body["status"],
        "ok"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn modules_certificates_notes_and_language() {
    let t = T::new().await;
    t.add_user("alice", "user");
    t.add_user("lim", "limited");
    let s = t.login("alice").await;
    let created = t
        .send(
            &s,
            Method::POST,
            "/api/v1/networks",
            json!({
                "name": "net", "host": "127.0.0.1", "port": 1, "tls": false, "nick": "alice"
            }),
        )
        .await;
    let nid = created.body["id"].as_i64().unwrap();

    // Catalog and effective state.
    let cat = t.get(&s, "/api/v1/modules").await;
    assert_eq!(cat.status, StatusCode::OK);
    let ids: Vec<&str> = cat.body["modules"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&"keepnick") && ids.contains(&"autoop") && ids.contains(&"log"));
    let mods = t.get(&s, &format!("/api/v1/networks/{nid}/modules")).await;
    let away = mods.body["modules"]
        .as_array()
        .unwrap()
        .iter()
        .find(|m| m["id"] == "away")
        .unwrap()
        .clone();
    assert_eq!(away["enabled"], true);
    assert_eq!(away["config"]["delay"], 60);

    // Updates are validated.
    let url = format!("/api/v1/networks/{nid}/modules/autoop");
    let bad = t
        .send(
            &s,
            Method::PUT,
            &url,
            json!({"enabled": true, "config": {"entries": "o *"}}),
        )
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST, "{}", bad.raw);
    let unknown = t
        .send(
            &s,
            Method::PUT,
            &format!("/api/v1/networks/{nid}/modules/nope"),
            json!({"enabled": true}),
        )
        .await;
    assert_eq!(unknown.status, StatusCode::BAD_REQUEST);
    let ok = t
        .send(
            &s,
            Method::PUT,
            &url,
            json!({"enabled": true, "config": {"entries": "o bob"}}),
        )
        .await;
    assert_eq!(ok.status, StatusCode::OK, "{}", ok.raw);
    assert_eq!(ok.body["config"]["entries"], "o bob");
    assert_eq!(ok.body["config"]["delay"], 3, "defaults filled in");

    // Client certificate: generate, read fingerprints, never expose the key.
    let cert_url = format!("/api/v1/networks/{nid}/cert");
    assert_eq!(t.get(&s, &cert_url).await.body["present"], false);
    let gen = t
        .send(&s, Method::POST, &format!("{cert_url}/generate"), json!({}))
        .await;
    assert_eq!(gen.status, StatusCode::OK, "{}", gen.raw);
    assert_eq!(
        gen.body["fingerprints"]["sha512"].as_str().unwrap().len(),
        128
    );
    assert!(
        !gen.raw.contains("PRIVATE KEY"),
        "private key never returned"
    );
    let (_, key_enc) = t
        .running
        .ctx
        .db
        .with(move |c| db::get_client_cert(c, nid))
        .unwrap()
        .unwrap();
    assert!(key_enc.starts_with("v1:"), "key stored encrypted");
    let junk = t
        .send(
            &s,
            Method::PUT,
            &cert_url,
            json!({"certificate": "x", "private_key": "y"}),
        )
        .await;
    assert_eq!(junk.status, StatusCode::BAD_REQUEST);
    let del = t.send(&s, Method::DELETE, &cert_url, json!({})).await;
    assert_eq!(del.body["present"], false);

    // Notes.
    let put = t
        .send(
            &s,
            Method::PUT,
            "/api/v1/notes/todo",
            json!({"text": "buy milk"}),
        )
        .await;
    assert_eq!(put.status, StatusCode::OK, "{}", put.raw);
    let bad_key = t
        .send(&s, Method::PUT, "/api/v1/notes/a%20b", json!({"text": "x"}))
        .await;
    assert_eq!(bad_key.status, StatusCode::BAD_REQUEST);
    assert_eq!(
        t.get(&s, "/api/v1/notes").await.body["notes"][0]["text"],
        "buy milk"
    );
    assert_eq!(
        t.send(&s, Method::DELETE, "/api/v1/notes/todo", json!({}))
            .await
            .status,
        StatusCode::OK
    );

    // Language.
    let lang = t
        .send(
            &s,
            Method::PUT,
            "/api/v1/me/language",
            json!({"language": "es"}),
        )
        .await;
    assert_eq!(lang.body["effective"], "es");
    assert_eq!(t.get(&s, "/api/v1/auth/me").await.body["language"], "es");
    let bad_lang = t
        .send(
            &s,
            Method::PUT,
            "/api/v1/me/language",
            json!({"language": "xx"}),
        )
        .await;
    assert_eq!(bad_lang.status, StatusCode::BAD_REQUEST);

    // A limited user cannot change modules of someone else's network.
    let l = t.login("lim").await;
    let denied = t
        .send(&l, Method::PUT, &url, json!({"enabled": false}))
        .await;
    assert!(denied.status == StatusCode::FORBIDDEN || denied.status == StatusCode::NOT_FOUND);
}

impl T {
    async fn bearer(&self, token: &str, m: Method, path: &str, body: Option<Value>) -> Resp {
        let mut b = Request::builder()
            .method(m)
            .uri(path)
            .header(header::HOST, "localhost")
            .header(header::AUTHORIZATION, format!("Bearer {token}"));
        let req = match body {
            Some(v) => b
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(v.to_string()))
                .unwrap(),
            None => {
                b = b.header("x-test", "1");
                b.body(Body::empty()).unwrap()
            }
        };
        let resp = self.app.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let headers = resp.headers().clone();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let raw = String::from_utf8_lossy(&bytes).to_string();
        let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        Resp {
            status,
            body,
            raw,
            headers,
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn api_tokens_metrics_and_alerts() {
    let t = T::new().await;
    t.add_user("root", "superadmin");
    t.add_user("bob", "user");
    let root = t.login("root").await;
    let bob = t.login("bob").await;

    // Create one token of each scope.
    let mut tok = std::collections::HashMap::new();
    for scope in ["read", "write", "metrics"] {
        let r = t
            .send(
                &root,
                Method::POST,
                "/api/v1/tokens",
                json!({"name": format!("t-{scope}"), "scope": scope, "expires_days": 30}),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.raw);
        let secret = r.body["token"].as_str().unwrap().to_string();
        assert!(secret.starts_with("xbnc_") && secret.len() > 40);
        tok.insert(scope, (secret, r.body["id"].as_i64().unwrap()));
    }
    let bad = t
        .send(
            &root,
            Method::POST,
            "/api/v1/tokens",
            json!({"name": "x", "scope": "root"}),
        )
        .await;
    assert_eq!(bad.status, StatusCode::BAD_REQUEST);
    // The list never shows the secret.
    let list = t.get(&root, "/api/v1/tokens").await;
    assert_eq!(list.body["tokens"].as_array().unwrap().len(), 3);
    assert!(!list.raw.contains(&tok["read"].0));
    // Tokens are per user.
    assert_eq!(
        t.get(&bob, "/api/v1/tokens").await.body["tokens"]
            .as_array()
            .unwrap()
            .len(),
        0
    );

    // Read token: GET works, writes are refused, no CSRF needed.
    let (read, _) = &tok["read"];
    assert_eq!(
        t.bearer(read, Method::GET, "/api/v1/auth/me", None)
            .await
            .status,
        StatusCode::OK
    );
    let r = t
        .bearer(
            read,
            Method::PUT,
            "/api/v1/notes/a",
            Some(json!({"text": "b"})),
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "{}", r.raw);
    assert_eq!(r.body["error"], "token_read_only");
    // Write token can change things.
    let (write, _) = &tok["write"];
    let r = t
        .bearer(
            write,
            Method::PUT,
            "/api/v1/notes/hello",
            Some(json!({"text": "world"})),
        )
        .await;
    assert!(r.status.is_success(), "{}", r.raw);
    // But not account security, which needs a real session.
    let r = t
        .bearer(
            write,
            Method::PUT,
            "/api/v1/me/password",
            Some(json!({"current_password": PW, "new_password": "another-password-9"})),
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN, "{}", r.raw);
    assert_eq!(r.body["error"], "session_required");
    let r = t
        .bearer(
            write,
            Method::POST,
            "/api/v1/tokens",
            Some(json!({"name": "y", "scope": "write"})),
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    // Metrics token: only /metrics.
    let (met, _) = &tok["metrics"];
    assert_eq!(
        t.bearer(met, Method::GET, "/api/v1/auth/me", None)
            .await
            .status,
        StatusCode::FORBIDDEN
    );
    let r = t.bearer(met, Method::GET, "/api/v1/metrics", None).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.raw);
    assert!(r.headers[header::CONTENT_TYPE]
        .to_str()
        .unwrap()
        .starts_with("text/plain; version=0.0.4"));
    for series in [
        "xbnc_info{version=",
        "xbnc_users 2",
        "xbnc_failed_logins_total",
        "xbnc_alerts_firing{alert=\"network_down\"} 0",
        "# TYPE xbnc_reconnects_total counter",
    ] {
        assert!(r.raw.contains(series), "{series} missing in\n{}", r.raw);
    }
    // A plain user cannot read metrics, even with a session.
    assert_eq!(
        t.get(&bob, "/api/v1/metrics").await.status,
        StatusCode::FORBIDDEN
    );

    // Wrong tokens fail and count as failed logins (3 → blocked).
    for _ in 0..3 {
        let r = t
            .bearer("xbnc_nope", Method::GET, "/api/v1/auth/me", None)
            .await;
        assert_eq!(r.status, StatusCode::UNAUTHORIZED);
    }
    assert_eq!(
        t.bearer(read, Method::GET, "/api/v1/auth/me", None)
            .await
            .status,
        StatusCode::TOO_MANY_REQUESTS
    );
    t.running.ctx.limiter.unblock("127.0.0.1".parse().unwrap());

    // Revoking works immediately.
    let id = tok["read"].1;
    let r = t
        .send(
            &root,
            Method::DELETE,
            &format!("/api/v1/tokens/{id}"),
            json!({}),
        )
        .await;
    assert!(r.status.is_success(), "{}", r.raw);
    assert_eq!(
        t.bearer(read, Method::GET, "/api/v1/auth/me", None)
            .await
            .status,
        StatusCode::UNAUTHORIZED
    );
    // Bob cannot delete root's tokens.
    let wid = tok["write"].1;
    let r = t
        .send(
            &bob,
            Method::DELETE,
            &format!("/api/v1/tokens/{wid}"),
            json!({}),
        )
        .await;
    assert_eq!(r.status, StatusCode::NOT_FOUND);

    // Alerts: a tiny database limit fires right away.
    let mut s = t.get(&root, "/api/v1/settings").await.body["current"].clone();
    assert_eq!(s["alerts"]["notify_irc"], true);
    s["alerts"]["database_mb"] = json!(0);
    s["alerts"]["webhook_url"] = json!("ftp://nope");
    assert_eq!(
        t.send(&root, Method::PUT, "/api/v1/settings", s.clone())
            .await
            .status,
        StatusCode::BAD_REQUEST
    );
    s["alerts"]["webhook_url"] = json!("");
    s["alerts"]["failed_logins_10min"] = json!(1);
    assert!(t
        .send(&root, Method::PUT, "/api/v1/settings", s.clone())
        .await
        .status
        .is_success());
    xbncd::alerts::tick(&t.running.ctx).await;
    let a = t.get(&root, "/api/v1/alerts").await;
    assert_eq!(a.body["alerts"][0]["kind"], "failed_logins", "{}", a.raw);
    assert_eq!(
        t.get(&bob, "/api/v1/alerts").await.status,
        StatusCode::FORBIDDEN
    );
    let m = t.get(&root, "/api/v1/metrics").await;
    assert!(
        m.raw
            .contains("xbnc_alerts_firing{alert=\"failed_logins\"} 1"),
        "{}",
        m.raw
    );
    // Turning the rule off resolves it.
    s["alerts"]["failed_logins_10min"] = json!(0);
    assert!(t
        .send(&root, Method::PUT, "/api/v1/settings", s)
        .await
        .status
        .is_success());
    xbncd::alerts::tick(&t.running.ctx).await;
    assert_eq!(
        t.get(&root, "/api/v1/alerts").await.body["alerts"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    let audit = t.running.ctx.db.with(|c| db::recent_audit(c, 50)).unwrap();
    assert!(audit
        .iter()
        .any(|e| e.2 == "alert" && e.3.starts_with("resolved")));
    assert!(audit.iter().any(|e| e.2 == "token.create"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn import_from_another_bouncer() {
    let t = T::new().await;
    t.add_user("root", "superadmin");
    t.add_user("adm", "admin");
    t.add_user("taken", "user");
    let root = t.login("root").await;
    let adm = t.login("adm").await;
    let config = r#"
<User carol>
    Pass = plain#carol-password-1
    Nick = carol
    LoadModule = keepnick
    <Network oftc>
        Server = irc.oftc.net +6697
        <Chan #xbnc>
        </Chan>
    </Network>
</User>
<User taken>
    Pass = plain#whatever
</User>
<User dave>
    Pass = sha256#5baf7bc4af0e74a5155fa16eea25573bad825c3fb5d34169871a226d9fc18cde#somesalt#
</User>
"#;
    // Only a superadmin can import.
    let r = t
        .send(
            &adm,
            Method::POST,
            "/api/v1/import/preview",
            json!({ "config": config }),
        )
        .await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    let r = t
        .send(
            &root,
            Method::POST,
            "/api/v1/import/preview",
            json!({ "config": "nonsense" }),
        )
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    let r = t
        .send(
            &root,
            Method::POST,
            "/api/v1/import/preview",
            json!({ "config": config }),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.raw);
    let plan = &r.body["plan"];
    assert_eq!(plan["users"][0]["name"], "carol");
    assert_eq!(plan["users"][0]["networks"][0]["channels"][0], "#xbnc");
    assert_eq!(plan["users"][1]["skip"]["code"], "user_exists");
    assert!(plan["warnings"]
        .as_array()
        .unwrap()
        .iter()
        .any(|w| w["code"] == "config_only"));
    // The preview never leaks secrets.
    assert!(!r.raw.contains("carol-password-1"));
    // Nothing was created yet.
    assert!(t
        .running
        .ctx
        .db
        .with(|c| db::get_user(c, "carol"))
        .unwrap()
        .is_none());

    let r = t
        .send(
            &root,
            Method::POST,
            "/api/v1/import/apply",
            json!({ "config": config }),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.raw);
    assert_eq!(r.body["report"]["users_created"], json!(["carol", "dave"]));
    assert_eq!(r.body["report"]["networks_created"], 1);
    // Carol can log in with her old password, and her network is running.
    t.login_with("carol", "carol-password-1").await;
    assert!(t.running.networks().iter().any(|h| h.name == "oftc"));
    // An imported salted SHA-256 hash works once and is then replaced by Argon2id.
    let old = t
        .running
        .ctx
        .db
        .with(|c| db::get_user(c, "dave"))
        .unwrap()
        .unwrap();
    assert!(old.password_hash.starts_with("legacy-sha256$"));
    t.login_with("dave", "test").await;
    let new = t
        .running
        .ctx
        .db
        .with(|c| db::get_user(c, "dave"))
        .unwrap()
        .unwrap();
    assert!(
        new.password_hash.starts_with("$argon2id$"),
        "{}",
        new.password_hash
    );
    t.login_with("dave", "test").await;
}

impl T {
    async fn upload(&self, s: &Session, path: &str, bytes: &[u8]) -> Resp {
        let req = Request::builder()
            .method(Method::POST)
            .uri(path)
            .header(header::HOST, "localhost")
            .header(header::COOKIE, format!("xbnc_session={}", s.cookie))
            .header("x-csrf-token", &s.csrf)
            .header(header::CONTENT_TYPE, "application/wasm")
            .body(Body::from(bytes.to_vec()))
            .unwrap();
        let resp = self.app.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let headers = resp.headers().clone();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let raw = String::from_utf8_lossy(&bytes).to_string();
        let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        Resp {
            status,
            body,
            raw,
            headers,
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn plugin_management() {
    const DICE: &[u8] = include_bytes!("../../../sdk/c/examples/dice.wasm");
    let t = T::new().await;
    t.add_user("root", "superadmin");
    let bob_id = t.add_user("bob", "user");
    t.add_user("adm", "admin");
    let root = t.login("root").await;
    let bob = t.login("bob").await;
    let adm = t.login("adm").await;

    // The official plugins are installed on the first start, off everywhere.
    let official = t.get(&root, "/api/v1/plugins").await.body["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap().to_string())
        .collect::<Vec<_>>();
    assert_eq!(official.len(), 18, "{official:?}");
    assert!(official.contains(&"trivia".to_string()) && official.contains(&"dice".to_string()));
    // Start from an empty server for the rest of the test; removed official
    // plugins stay removed until the next xBNC version.
    for id in &official {
        let r = t
            .send(
                &root,
                Method::DELETE,
                &format!("/api/v1/plugins/{id}"),
                json!({}),
            )
            .await;
        assert_eq!(r.status, StatusCode::OK, "{}", r.raw);
    }

    // Only a superadmin installs plugins.
    assert_eq!(
        t.upload(&adm, "/api/v1/plugins/inspect", DICE).await.status,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        t.upload(&bob, "/api/v1/plugins", DICE).await.status,
        StatusCode::FORBIDDEN
    );
    let r = t
        .upload(&root, "/api/v1/plugins/inspect", b"not wasm")
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);
    let r = t.upload(&root, "/api/v1/plugins/inspect", DICE).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.raw);
    assert_eq!(r.body["manifest"]["id"], "dice");
    assert_eq!(r.body["manifest"]["permissions"], json!(["send"]));
    assert!(r.body["installed_version"].is_null());
    // Inspecting installs nothing.
    assert_eq!(
        t.get(&root, "/api/v1/plugins").await.body["plugins"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    let r = t.upload(&root, "/api/v1/plugins", DICE).await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.raw);
    // What the API installed can run on the engine the networks use.
    let loaded = t.running.ctx.plugins.get("dice").unwrap();
    xbncd::plugins::PluginInstance::new(&t.running.ctx.plugins.engine, loaded).unwrap();
    let list = t.get(&bob, "/api/v1/plugins").await;
    assert_eq!(list.body["plugins"][0]["id"], "dice");
    assert_eq!(list.body["plugins"][0]["loaded"], true);
    assert_eq!(list.body["can_manage"], false);

    // Bob enables it on his network with his own settings.
    let nid = t
        .running
        .ctx
        .db
        .with(|c| {
            db::create_network(
                c,
                bob_id,
                &db::NewNetwork {
                    name: "net".into(),
                    host: "127.0.0.1".into(),
                    port: 1,
                    tls: false,
                    nick: "bob".into(),
                    username: "bob".into(),
                    realname: String::new(),
                    server_password_enc: None,
                    sasl_username: None,
                    sasl_password_enc: None,
                },
                10,
            )
        })
        .unwrap();
    let path = format!("/api/v1/networks/{nid}/plugins/dice");
    let r = t
        .send(
            &bob,
            Method::PUT,
            &path,
            json!({"enabled": true, "config": {"max_dice": 99}}),
        )
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST, "{}", r.raw);
    let r = t
        .send(
            &bob,
            Method::PUT,
            &path,
            json!({"enabled": true, "config": {"max_dice": 3}}),
        )
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.raw);
    let np = t
        .get(&bob, &format!("/api/v1/networks/{nid}/plugins"))
        .await;
    assert_eq!(np.body["plugins"][0]["enabled"], true);
    assert_eq!(np.body["plugins"][0]["config"]["max_dice"], 3);
    assert_eq!(np.body["plugins"][0]["config"]["trigger"], "!roll");

    // Turned off on the server: no longer offered to networks.
    let r = t
        .send(
            &root,
            Method::PATCH,
            "/api/v1/plugins/dice",
            json!({"enabled": false}),
        )
        .await;
    assert!(r.status.is_success(), "{}", r.raw);
    let np = t
        .get(&bob, &format!("/api/v1/networks/{nid}/plugins"))
        .await;
    assert_eq!(np.body["plugins"].as_array().unwrap().len(), 0);
    let r = t
        .send(&bob, Method::PUT, &path, json!({"enabled": true}))
        .await;
    assert_eq!(r.status, StatusCode::BAD_REQUEST);

    // Removing deletes the file and every network's settings.
    let r = t
        .send(&root, Method::DELETE, "/api/v1/plugins/dice", json!({}))
        .await;
    assert!(r.status.is_success(), "{}", r.raw);
    assert!(!t.running.ctx.plugins.path_of("dice").exists());
    let rows = t
        .running
        .ctx
        .db
        .with(|c| db::plugin_config_rows(c, nid))
        .unwrap();
    assert!(rows.is_empty());
    let audit = t.running.ctx.db.with(|c| db::recent_audit(c, 50)).unwrap();
    assert!(audit.iter().any(|e| e.2 == "plugin.install"));
    assert!(audit.iter().any(|e| e.2 == "plugin.remove"));
}

/// Profile photos: only real images are stored, only the user (and those
/// who can see users) can fetch one, and it is served so that a browser
/// never treats it as a page.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn profile_photos() {
    let t = T::new().await;
    let ana = t.add_user("ana", "user");
    t.add_user("bob", "user");
    t.add_user("boss", "admin");
    let s = t.login("ana").await;
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR".to_vec();
    png.extend_from_slice(&256u32.to_be_bytes());
    png.extend_from_slice(&256u32.to_be_bytes());
    png.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
    let put = |ct: &'static str, body: Vec<u8>, csrf: Option<String>| {
        let app = t.app.clone();
        let cookie = s.cookie.clone();
        async move {
            let mut b = Request::builder()
                .method(Method::PUT)
                .uri("/api/v1/me/avatar")
                .header(header::HOST, "localhost")
                .header(header::COOKIE, format!("xbnc_session={cookie}"))
                .header(header::CONTENT_TYPE, ct);
            if let Some(c) = csrf {
                b = b.header("x-csrf-token", c);
            }
            let r = app.oneshot(b.body(Body::from(body)).unwrap()).await.unwrap();
            let st = r.status();
            let bytes = r.into_body().collect().await.unwrap().to_bytes();
            (st, serde_json::from_slice::<Value>(&bytes).unwrap_or(Value::Null))
        }
    };
    // Without the CSRF token nothing is stored.
    let (st, _) = put("image/png", png.clone(), None).await;
    assert_eq!(st, StatusCode::FORBIDDEN);
    // Not an image, or not the type it claims.
    let (st, b) = put("image/png", b"<svg onload=alert(1)>".to_vec(), Some(s.csrf.clone())).await;
    assert_eq!(st, StatusCode::BAD_REQUEST, "{b}");
    let (st, _) = put("image/jpeg", png.clone(), Some(s.csrf.clone())).await;
    assert_eq!(st, StatusCode::BAD_REQUEST);
    let (st, b) = put("image/png", png.clone(), Some(s.csrf.clone())).await;
    assert_eq!(st, StatusCode::OK, "{b}");
    let url = b["avatar"].as_str().unwrap().to_string();
    assert!(url.starts_with(&format!("/api/v1/users/{ana}/avatar?v=")), "{url}");
    let me = t.req(Method::GET, "/api/v1/auth/me", Some(&s.cookie), None, None).await;
    assert_eq!(me.body["user"]["avatar"], url.as_str());
    let got = t.req(Method::GET, &url, Some(&s.cookie), None, None).await;
    assert_eq!(got.status, StatusCode::OK);
    assert_eq!(got.headers[header::CONTENT_TYPE], "image/png");
    assert_eq!(got.headers[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
    assert!(got.headers[header::CONTENT_SECURITY_POLICY].to_str().unwrap().contains("sandbox"));
    // Another user cannot fetch it; an administrator can, and sees it listed.
    let bob = t.login("bob").await;
    let r = t.req(Method::GET, &url, Some(&bob.cookie), None, None).await;
    assert_eq!(r.status, StatusCode::FORBIDDEN);
    let boss = t.login("boss").await;
    assert_eq!(t.req(Method::GET, &url, Some(&boss.cookie), None, None).await.status, StatusCode::OK);
    let users = t.req(Method::GET, "/api/v1/users", Some(&boss.cookie), None, None).await;
    let row = users.body.as_array().unwrap().iter().find(|u| u["id"] == ana).unwrap().clone();
    assert_eq!(row["avatar"], url.as_str());
    // The administrator removes it.
    let r = t
        .req(Method::DELETE, &format!("/api/v1/users/{ana}/avatar"), Some(&boss.cookie), Some(&boss.csrf), None)
        .await;
    assert_eq!(r.status, StatusCode::OK, "{}", r.raw);
    assert_eq!(t.req(Method::GET, &url, Some(&s.cookie), None, None).await.status, StatusCode::NOT_FOUND);
    let me = t.req(Method::GET, "/api/v1/auth/me", Some(&s.cookie), None, None).await;
    assert!(me.body["user"]["avatar"].is_null());
}
