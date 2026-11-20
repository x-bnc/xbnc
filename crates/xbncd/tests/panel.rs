//! The panel as an IRC client (chat over WebSocket), mentions, per-channel
//! history and notification settings, against a real bouncer and the
//! scripted IRC server.

mod common;
use common::*;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message as Ws};

/// A minimal HTTP/1.1 client for the panel API.
struct Http {
    addr: std::net::SocketAddr,
    cookie: String,
    csrf: String,
}

async fn raw(
    addr: std::net::SocketAddr,
    method: &str,
    path: &str,
    headers: &str,
    body: &str,
) -> (u16, String, String) {
    let mut s = tokio::net::TcpStream::connect(addr).await.unwrap();
    let req = format!(
        "{method} {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{headers}\r\n{body}",
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

impl Http {
    async fn login(addr: std::net::SocketAddr) -> Http {
        let body = json!({"username": "alice", "password": PASSWORD}).to_string();
        let (status, head, body) = raw(addr, "POST", "/api/v1/auth/login", "", &body).await;
        assert_eq!(status, 200, "{head}{body}");
        let cookie = head
            .lines()
            .find(|l| {
                l.to_ascii_lowercase()
                    .starts_with("set-cookie: xbnc_session=")
            })
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .split_once(": ")
            .unwrap()
            .1
            .to_string();
        let csrf = serde_json::from_str::<Value>(&body).unwrap()["csrf"]
            .as_str()
            .unwrap()
            .to_string();
        Http { addr, cookie, csrf }
    }

    async fn call(&self, method: &str, path: &str, body: Value) -> (u16, Value) {
        let headers = format!("Cookie: {}\r\nX-CSRF-Token: {}\r\n", self.cookie, self.csrf);
        let b = if body.is_null() {
            String::new()
        } else {
            body.to_string()
        };
        let (status, _, body) = raw(self.addr, method, path, &headers, &b).await;
        // Chunked bodies: take the JSON between the first { or [ and the end.
        let start = body.find(['{', '[']).unwrap_or(0);
        let end = body.rfind(['}', ']']).map(|e| e + 1).unwrap_or(body.len());
        (
            status,
            serde_json::from_str(&body[start..end]).unwrap_or(Value::Null),
        )
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn panel_chat_mentions_and_channel_history() {
    let mut h = Harness::start().await;
    let web = h.running.as_ref().unwrap().web_addr.unwrap();
    let http = Http::login(web).await;
    let (_, nets) = http.call("GET", "/api/v1/networks", Value::Null).await;
    let nid = nets[0]["id"].as_i64().unwrap();

    // The chat: the panel attaches as one more client of the user.
    let mut req = format!("ws://{web}/api/v1/networks/{nid}/chat")
        .into_client_request()
        .unwrap();
    req.headers_mut()
        .insert("Cookie", http.cookie.parse().unwrap());
    req.headers_mut()
        .insert("Origin", format!("http://{web}").parse().unwrap());
    let (mut ws, _) = tokio_tungstenite::connect_async(req).await.unwrap();
    async fn until(
        ws: &mut (impl StreamExt<Item = Result<Ws, tokio_tungstenite::tungstenite::Error>> + Unpin),
        needle: &str,
    ) -> String {
        loop {
            let m = tokio::time::timeout(std::time::Duration::from_secs(5), ws.next())
                .await
                .unwrap_or_else(|_| panic!("never got {needle}"))
                .unwrap()
                .unwrap();
            if let Ws::Text(t) = m {
                let v: Value = serde_json::from_str(&t).unwrap();
                let line = v["line"].as_str().unwrap_or("").to_string();
                if line.contains(needle) || t.contains(needle) {
                    return line;
                }
            }
        }
    }
    until(&mut ws, " 366 ").await;
    // Messages from the network arrive live, with their time.
    h.ircd
        .send(":bob!b@fake PRIVMSG #test :alice: are you there?");
    let l = until(&mut ws, "are you there?").await;
    assert!(l.starts_with("@") && l.contains("time="), "{l}");
    // Writing from the panel goes to the network as the user.
    ws.send(Ws::Text(
        json!({"line": "PRIVMSG #test :yes, from the panel"})
            .to_string()
            .into(),
    ))
    .await
    .unwrap();
    h.ircd.expect("PRIVMSG #test :yes, from the panel").await;
    until(&mut ws, "yes, from the panel").await; // echo-message
                                                 // Commands outside the allowed list are refused.
    ws.send(Ws::Text(json!({"line": "QUIT :bye"}).to_string().into()))
        .await
        .unwrap();
    until(&mut ws, "not available in the panel chat").await;
    // Reading in the panel moves the read marker for every device.
    let ts = xbncd::util::format_server_time(xbncd::db::now_ms());
    ws.send(Ws::Text(
        json!({"line": format!("MARKREAD #test timestamp={ts}")})
            .to_string()
            .into(),
    ))
    .await
    .unwrap();
    until(&mut ws, "MARKREAD #test timestamp=").await;
    // The panel counts as a connected client.
    let (_, st) = http
        .call("GET", &format!("/api/v1/networks/{nid}"), Value::Null)
        .await;
    assert!(
        st.to_string().contains("(panel)") || st.to_string().contains("\"web\""),
        "{st}"
    );

    // Mentions.
    let (_, m) = http.call("GET", "/api/v1/mentions", Value::Null).await;
    assert_eq!(m["mentions"][0]["text"], "alice: are you there?", "{m}");
    assert_eq!(m["mentions"][0]["network"], "fake");
    assert_eq!(m["mentions"].as_array().unwrap().len(), 1);

    // A channel that is not stored.
    let (s, _) = http
        .call(
            "PUT",
            &format!("/api/v1/networks/{nid}/channels/%23test/history"),
            json!({"retention_days": -1}),
        )
        .await;
    assert_eq!(s, 200);
    let (_, ch) = http
        .call(
            "GET",
            &format!("/api/v1/networks/{nid}/channels"),
            Value::Null,
        )
        .await;
    assert_eq!(ch[0]["retention_days"], -1, "{ch}");
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    h.ircd.send(":bob!b@fake PRIVMSG #test :this is not kept");
    until(&mut ws, "this is not kept").await;
    let (_, hist) = http
        .call(
            "GET",
            &format!("/api/v1/networks/{nid}/history?target=%23test&q=kept"),
            Value::Null,
        )
        .await;
    assert_eq!(hist["messages"].as_array().unwrap().len(), 0, "{hist}");
    let (s, _) = http
        .call(
            "PUT",
            &format!("/api/v1/networks/{nid}/channels/%23nowhere/history"),
            json!({"retention_days": 5}),
        )
        .await;
    assert_eq!(s, 404);

    // Notification settings: validated, token masked, test reports errors.
    let (s, v) = http
        .call(
            "PUT",
            "/api/v1/me/notifications",
            json!({"enabled": true, "webhook_url": "http://plain"}),
        )
        .await;
    assert_eq!(s, 400, "{v}");
    let (s, v) = http
        .call("PUT", "/api/v1/me/notifications", json!({"enabled": true, "telegram_token": "123:secretsecret", "telegram_chat": "42", "push": false}))
        .await;
    assert_eq!(s, 200, "{v}");
    assert_eq!(v["settings"]["telegram_token"], "…cret");
    let (_, v) = http
        .call("GET", "/api/v1/me/notifications", Value::Null)
        .await;
    assert_eq!(v["settings"]["telegram_token"], "…cret");
    assert_eq!(v["email_available"], false);
    // Saving with the masked token keeps it.
    let (s, _) = http
        .call("PUT", "/api/v1/me/notifications", json!({"enabled": true, "telegram_token": "…cret", "telegram_chat": "42", "push": false}))
        .await;
    assert_eq!(s, 200);
    let (_, k) = http.call("GET", "/api/v1/push/key", Value::Null).await;
    assert_eq!(k["public_key"].as_str().unwrap().len(), 87, "{k}");
    let (s, _) = http
        .call("POST", "/api/v1/me/push", json!({"endpoint": "https://push.example.org/x", "keys": {"p256dh": "bad", "auth": "bad"}}))
        .await;
    assert_eq!(s, 400);
    drop(ws);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn developer_mode_installs_changed_plugins_and_streams_logs() {
    let h = Harness::start().await;
    let web = h.running.as_ref().unwrap().web_addr.unwrap();
    let http = Http::login(web).await;
    let (st, dev) = http
        .call("PUT", "/api/v1/plugins/dev", json!({"enabled": true}))
        .await;
    assert_eq!(st, 200, "{dev}");
    assert_eq!(dev["enabled"], true);
    let folder = std::path::PathBuf::from(dev["folder"].as_str().unwrap());

    let mut req = format!("ws://{web}/api/v1/plugins/logs")
        .into_client_request()
        .unwrap();
    req.headers_mut()
        .insert("Cookie", http.cookie.parse().unwrap());
    req.headers_mut()
        .insert("Origin", format!("http://{web}").parse().unwrap());
    let (mut ws, _) = tokio_tungstenite::connect_async(req).await.unwrap();

    std::fs::write(
        folder.join("dice.wasm"),
        include_bytes!("../../../sdk/c/examples/dice.wasm"),
    )
    .unwrap();
    let line = loop {
        let m = tokio::time::timeout(std::time::Duration::from_secs(10), ws.next())
            .await
            .expect("a log line")
            .unwrap()
            .unwrap();
        if let Ws::Text(t) = m {
            break serde_json::from_str::<Value>(&t).unwrap();
        }
    };
    assert_eq!(line["level"], "info", "{line}");
    assert!(
        line["text"].as_str().unwrap().starts_with("installed"),
        "{line}"
    );
    let (_, list) = http.call("GET", "/api/v1/plugins", Value::Null).await;
    let p = list["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == line["plugin"])
        .unwrap()
        .clone();
    assert_eq!(p["installed_by"], "developer mode");
    // It is turned on in the networks of whoever enabled developer mode.
    let (_, nets) = http.call("GET", "/api/v1/networks", Value::Null).await;
    let nid = nets[0]["id"].as_i64().unwrap();
    let (_, np) = http
        .call(
            "GET",
            &format!("/api/v1/networks/{nid}/plugins"),
            Value::Null,
        )
        .await;
    let dice = np["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "dice")
        .unwrap()
        .clone();
    assert_eq!(dice["enabled"], true, "{np}");

    // A manifest next to the file is packed in by itself.
    let mut manifest: Value =
        serde_json::from_str(include_str!("../../../sdk/c/examples/dice.json")).unwrap();
    manifest["version"] = json!("9.9.9");
    std::fs::write(folder.join("dice.json"), manifest.to_string()).unwrap();
    let line = loop {
        let m = tokio::time::timeout(std::time::Duration::from_secs(10), ws.next())
            .await
            .expect("a log line")
            .unwrap()
            .unwrap();
        if let Ws::Text(t) = m {
            break serde_json::from_str::<Value>(&t).unwrap();
        }
    };
    assert!(
        line["text"]
            .as_str()
            .unwrap()
            .starts_with("installed 9.9.9"),
        "{line}"
    );

    // A broken file is reported, not installed.
    std::fs::write(folder.join("broken.wasm"), b"not wasm").unwrap();
    let line = loop {
        let m = tokio::time::timeout(std::time::Duration::from_secs(10), ws.next())
            .await
            .expect("a log line")
            .unwrap()
            .unwrap();
        if let Ws::Text(t) = m {
            break serde_json::from_str::<Value>(&t).unwrap();
        }
    };
    assert_eq!(line["level"], "error", "{line}");
    assert_eq!(line["plugin"], "broken.wasm");

    // A WebSocket from another site is refused.
    let mut req = format!("ws://{web}/api/v1/plugins/logs")
        .into_client_request()
        .unwrap();
    req.headers_mut()
        .insert("Cookie", http.cookie.parse().unwrap());
    req.headers_mut()
        .insert("Origin", "https://evil.example".parse().unwrap());
    assert!(tokio_tungstenite::connect_async(req).await.is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn plugin_catalog_one_click_install() {
    let h = Harness::start().await;
    let web = h.running.as_ref().unwrap().web_addr.unwrap();
    let http = Http::login(web).await;
    let (st, cat) = http
        .call("GET", "/api/v1/plugins/catalog", Value::Null)
        .await;
    assert_eq!(st, 200, "{cat}");
    let ids: Vec<&str> = cat["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["id"].as_str().unwrap())
        .collect();
    for id in ["seen", "remind", "tell", "poll", "chanstats", "antiflood"] {
        assert!(ids.contains(&id), "{id} missing from {ids:?}");
    }
    let (st, r) = http
        .call("POST", "/api/v1/plugins/catalog/remind", Value::Null)
        .await;
    assert_eq!(st, 200, "{r}");
    assert_eq!(r["id"], "remind");
    let (_, cat) = http
        .call("GET", "/api/v1/plugins/catalog", Value::Null)
        .await;
    let remind = cat["plugins"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == "remind")
        .unwrap()
        .clone();
    assert_eq!(remind["installed_version"], "1.1.0");
    let (st, _) = http
        .call("POST", "/api/v1/plugins/catalog/nope", Value::Null)
        .await;
    assert_eq!(st, 404);
}
