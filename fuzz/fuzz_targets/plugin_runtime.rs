//! Arbitrary WebAssembly as a plugin: validation, instantiation and a call
//! must fail cleanly, never crash or hang the host.
#![no_main]
use libfuzzer_sys::fuzz_target;
use std::sync::Arc;

fuzz_target!(|data: &[u8]| {
    let manifest = br#"{"id":"fuzz","name":"Fuzz","version":"1","permissions":["send","raw"]}"#;
    let Ok(wasm) = xbncd::plugins::embed_manifest(data, manifest) else { return };
    let engine = xbncd::plugins::engine();
    let Ok(p) = xbncd::plugins::Plugin::compile(&engine, &wasm) else { return };
    if let Ok(mut i) = xbncd::plugins::PluginInstance::new(&engine, Arc::new(p)) {
        let _ = i.call(&serde_json::json!({"event": "message", "text": "!x"}));
    }
});
