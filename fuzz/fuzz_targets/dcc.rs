//! DCC offers from IRC: parsing never panics and rendering round-trips.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(text) = std::str::from_utf8(data) else { return };
    if let Some(d) = xbncd::dcc::parse(text) {
        let r = d.render();
        assert!(!r.contains(['\r', '\n', '\0']));
        assert_eq!(xbncd::dcc::parse(&r), Some(d));
    }
});
