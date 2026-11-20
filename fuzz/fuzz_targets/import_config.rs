//! Configuration files of other bouncers (uploaded in the panel).
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else { return };
    let db = xbncd::db::Db::open_in_memory().unwrap();
    let _ = db.with(|c| {
        if let Ok(plan) = xbncd::import::parse(text, None, c) {
            // Whatever was accepted can be applied without panicking.
            let sb = xbncd::crypto::SecretBox::from_key(&[1u8; 32]);
            let _ = xbncd::import::apply(c, &plan, &sb, 10);
        }
        Ok(())
    });
});
