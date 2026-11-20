# xBNC plugin SDK

- `c/`: the single header `xbnc_plugin.h` and the official plugins, which are also the examples (`make` in `c/` builds them with clang and wasm-ld; `make bundle` also copies them to the catalogue built into xbncd, `crates/xbncd/catalog/`).
- `rust/`: the `xbnc-plugin` crate and the `stats` example.

The complete guide, with the interface, the manifest and the limits, is in [docs/PLUGINS.md](../docs/PLUGINS.md) (section “Writing plugins”).
