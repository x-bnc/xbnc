//! PostgreSQL-only checks (run with XBNC_TEST_POSTGRES=postgres://user@host:port).

mod common;
use xbncd::db::{self, Db, NewMessage, NewNetwork};

#[test]
fn sqlite_database_moves_to_postgres() {
    let Some(url) = common::pg_test_database() else {
        eprintln!("XBNC_TEST_POSTGRES is not set; skipped");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let lite = Db::open(&dir.path().join("xbnc.db")).unwrap();
    lite.with(|c| {
        let uid = db::create_user(c, "Ana", "hash", true)?;
        let nid = db::create_network(
            c,
            uid,
            &NewNetwork {
                name: "undernet".into(),
                host: "irc.undernet.org".into(),
                port: 6667,
                tls: false,
                nick: "ana".into(),
                username: "ana".into(),
                realname: "Ana".into(),
                server_password_enc: Some("enc".into()),
                sasl_username: None,
                sasl_password_enc: None,
            },
            10,
        )?;
        db::upsert_channel(c, nid, "#XBNC", "#xbnc", None, false)?;
        for i in 0..12 {
            db::insert_message(
                c,
                &NewMessage {
                    network_id: nid,
                    target: "#XBNC".into(),
                    target_cf: "#xbnc".into(),
                    ts: 1000 + i,
                    sender: "bob!b@h".into(),
                    command: "PRIVMSG".into(),
                    text: format!("hola número {i}"),
                    outgoing: false,
                    mention: i == 3,
                },
            )?;
        }
        Ok(())
    })
    .unwrap();
    let pg = Db::open_postgres(&url, xbncd::server::tls_client_config().unwrap()).unwrap();
    db::copy_sqlite_to_postgres(&lite, &pg, |_, _| {}).unwrap();
    pg.with(|c| {
        let u = db::get_user(c, "ana")?.expect("user names stay case-insensitive");
        assert_eq!(u.username, "Ana");
        let nets = db::list_networks(c, u.id)?;
        assert_eq!(nets[0].name, "undernet");
        let n: i64 = c.query_row("SELECT COUNT(*) FROM messages", (), |r| r.get(0))?;
        assert_eq!(n, 12);
        let found = db::search_messages(
            c,
            nets[0].id,
            &db::HistoryFilter {
                text: Some("NÚMERO 1"),
                limit: 50,
                ..Default::default()
            },
        )?;
        assert_eq!(found.len(), 3, "1, 10 and 11: case-insensitive words");
        // New rows get new ids after the copied ones.
        let id = db::create_user(c, "bob", "hash", false)?;
        assert!(id > u.id);
        Ok(())
    })
    .unwrap();
    // Copying into a database that is not empty is refused.
    assert!(db::copy_sqlite_to_postgres(&lite, &pg, |_, _| {}).is_err());
}
