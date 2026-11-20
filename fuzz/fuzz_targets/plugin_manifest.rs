//! Uploaded plugin files: section parsing, manifest validation and packing.
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = xbncd::plugins::read_manifest(data);
    let _ = xbncd::plugins::embed_manifest(
        data,
        br#"{"id":"fuzz","name":"Fuzz","version":"1"}"#,
    );
    if data.len() > 2 {
        let (a, b) = data.split_at(data.len() / 2);
        let _ = xbncd::plugins::embed_manifest(a, b);
    }
});
