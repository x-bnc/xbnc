//! IRC lines from servers and clients: parsing must never panic, and what
//! is parsed must serialise back into something that parses the same way.
#![no_main]
use libfuzzer_sys::fuzz_target;
use xbnc_proto::Message;

fuzz_target!(|data: &[u8]| {
    let Ok(line) = std::str::from_utf8(data) else { return };
    if let Ok(m) = Message::parse(line) {
        let out = m.to_string();
        assert!(!out.contains('\r') && !out.contains('\n'), "{out:?}");
        let again = Message::parse(&out).expect("serialised line must parse");
        assert_eq!(again.command, m.command);
        assert_eq!(again.params, m.params);
    }
});
