# Third-party software

xBNC 1.0.0 is distributed under the Apache License 2.0 (see LICENSE). It is built
with the components below, each under its own license. Where a component offers
several licenses ("A OR B"), xBNC uses it under a permissive option compatible
with Apache-2.0. xBNC is written from scratch and includes no code from other bouncers.

Generated from `cargo metadata` and `npm ls --omit=dev` for the resolved dependency
graphs; regenerate it on every release. SQLite is compiled in through
`libsqlite3-sys` (bundled feature) and is in the public domain.

## Web panel (bundled into the binary)

Fonts are distributed by Fontsource under the SIL Open Font License 1.1:
Bricolage Grotesque, Atkinson Hyperlegible and JetBrains Mono.

Total: 13 npm packages.

| Package | Version | License | Source |
| --- | --- | --- | --- |
| @fontsource-variable/bricolage-grotesque | 5.3.0 | OFL-1.1 | git+https://github.com/fontsource/font-files.git |
| @fontsource-variable/jetbrains-mono | 5.3.0 | OFL-1.1 | git+https://github.com/fontsource/font-files.git |
| @fontsource/atkinson-hyperlegible | 5.3.0 | OFL-1.1 | git+https://github.com/fontsource/font-files.git |
| @types/react | 19.3.0 | MIT | https://github.com/DefinitelyTyped/DefinitelyTyped.git |
| cookie | 1.1.1 | MIT | jshttp/cookie |
| csstype | 3.2.3 | MIT | https://github.com/frenic/csstype |
| lucide-react | 1.70.0 | ISC | https://github.com/lucide-icons/lucide.git |
| react | 19.3.1 | MIT | https://github.com/react/react.git |
| react-dom | 19.3.1 | MIT | https://github.com/react/react.git |
| react-router | 7.18.5 | MIT | https://github.com/remix-run/react-router |
| react-router-dom | 7.18.5 | MIT | https://github.com/remix-run/react-router |
| scheduler | 0.28.0 | MIT | https://github.com/react/react.git |
| set-cookie-parser | 2.7.3 | MIT | nfriedly/set-cookie-parser |

## Rust crates

Total: 275 crates.

| Crate | Version | License | Source |
| --- | --- | --- | --- |
| adler2 | 2.0.1 | 0BSD OR MIT OR Apache-2.0 | https://github.com/oyvindln/adler2 |
| aead | 0.5.2 | MIT OR Apache-2.0 | https://github.com/RustCrypto/traits |
| aho-corasick | 1.1.5 | Unlicense OR MIT | https://github.com/BurntSushi/aho-corasick |
| anstream | 1.0.0 | MIT OR Apache-2.0 | https://github.com/rust-cli/anstyle.git |
| anstyle | 1.0.14 | MIT OR Apache-2.0 | https://github.com/rust-cli/anstyle.git |
| anstyle-parse | 1.0.0 | MIT OR Apache-2.0 | https://github.com/rust-cli/anstyle.git |
| anstyle-query | 1.1.5 | MIT OR Apache-2.0 | https://github.com/rust-cli/anstyle.git |
| anstyle-wincon | 3.0.11 | MIT OR Apache-2.0 | https://github.com/rust-cli/anstyle.git |
| anyhow | 1.0.104 | MIT OR Apache-2.0 | https://github.com/dtolnay/anyhow |
| argon2 | 0.5.3 | MIT OR Apache-2.0 | https://github.com/RustCrypto/password-hashes/tree/master/argon2 |
| async-trait | 0.1.92 | MIT OR Apache-2.0 | https://github.com/dtolnay/async-trait |
| atomic-waker | 1.1.2 | Apache-2.0 OR MIT | https://github.com/smol-rs/atomic-waker |
| autocfg | 1.5.1 | Apache-2.0 OR MIT | https://github.com/cuviper/autocfg |
| axum | 0.8.9 | MIT | https://github.com/tokio-rs/axum |
| axum-core | 0.5.6 | MIT | https://github.com/tokio-rs/axum |
| axum-macros | 0.5.1 | MIT | https://github.com/tokio-rs/axum |
| base64 | 0.22.1 | MIT OR Apache-2.0 | https://github.com/marshallpierce/rust-base64 |
| base64ct | 1.8.3 | Apache-2.0 OR MIT | https://github.com/RustCrypto/formats |
| bitflags | 2.13.2 | MIT OR Apache-2.0 | https://github.com/bitflags/bitflags |
| blake2 | 0.10.6 | MIT OR Apache-2.0 | https://github.com/RustCrypto/hashes |
| block-buffer | 0.10.4 | MIT OR Apache-2.0 | https://github.com/RustCrypto/utils |
| block-buffer | 0.12.1 | MIT OR Apache-2.0 | https://github.com/RustCrypto/utils |
| blowfish | 0.9.1 | MIT OR Apache-2.0 | https://github.com/RustCrypto/block-ciphers |
| bumpalo | 3.20.3 | MIT OR Apache-2.0 | https://github.com/fitzgen/bumpalo |
| byteorder | 1.5.0 | Unlicense OR MIT | https://github.com/BurntSushi/byteorder |
| bytes | 1.12.1 | MIT | https://github.com/tokio-rs/bytes |
| cc | 1.11.1 | MIT OR Apache-2.0 | https://github.com/rust-lang/cc-rs |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 | https://github.com/rust-lang/cfg-if |
| chacha20 | 0.10.2 | MIT OR Apache-2.0 | https://github.com/RustCrypto/stream-ciphers |
| chacha20 | 0.9.1 | Apache-2.0 OR MIT | https://github.com/RustCrypto/stream-ciphers |
| chacha20poly1305 | 0.10.1 | Apache-2.0 OR MIT | https://github.com/RustCrypto/AEADs/tree/master/chacha20poly1305 |
| cipher | 0.4.4 | MIT OR Apache-2.0 | https://github.com/RustCrypto/traits |
| clap | 4.6.7 | MIT OR Apache-2.0 | https://github.com/clap-rs/clap |
| clap_builder | 4.6.7 | MIT OR Apache-2.0 | https://github.com/clap-rs/clap |
| clap_derive | 4.6.7 | MIT OR Apache-2.0 | https://github.com/clap-rs/clap |
| clap_lex | 1.1.1 | MIT OR Apache-2.0 | https://github.com/clap-rs/clap |
| cmov | 0.5.4 | Apache-2.0 OR MIT | https://github.com/RustCrypto/utils |
| colorchoice | 1.0.5 | MIT OR Apache-2.0 | https://github.com/rust-cli/anstyle.git |
| const-oid | 0.10.2 | Apache-2.0 OR MIT | https://github.com/RustCrypto/formats |
| core-foundation | 0.10.1 | MIT OR Apache-2.0 | https://github.com/servo/core-foundation-rs |
| core-foundation-sys | 0.8.7 | MIT OR Apache-2.0 | https://github.com/servo/core-foundation-rs |
| cpufeatures | 0.2.17 | MIT OR Apache-2.0 | https://github.com/RustCrypto/utils |
| cpufeatures | 0.3.1 | MIT OR Apache-2.0 | https://github.com/RustCrypto/utils |
| crc32fast | 1.5.5 | MIT OR Apache-2.0 | https://github.com/srijs/rust-crc32fast |
| crypto-common | 0.1.7 | MIT OR Apache-2.0 | https://github.com/RustCrypto/traits |
| crypto-common | 0.2.2 | MIT OR Apache-2.0 | https://github.com/RustCrypto/traits |
| ctutils | 0.4.8 | Apache-2.0 OR MIT | https://github.com/RustCrypto/utils |
| data-encoding | 2.11.1 | MIT | https://github.com/ia0/data-encoding |
| deranged | 0.5.9 | MIT OR Apache-2.0 | https://github.com/jhpratt/deranged |
| digest | 0.10.7 | MIT OR Apache-2.0 | https://github.com/RustCrypto/traits |
| digest | 0.11.3 | MIT OR Apache-2.0 | https://github.com/RustCrypto/traits |
| equivalent | 1.0.3 | Apache-2.0 OR MIT | https://github.com/indexmap-rs/equivalent |
| errno | 0.3.14 | MIT OR Apache-2.0 | https://github.com/lambda-fairy/rust-errno |
| fallible-iterator | 0.2.0 | MIT/Apache-2.0 | https://github.com/sfackler/rust-fallible-iterator |
| fallible-iterator | 0.3.0 | MIT/Apache-2.0 | https://github.com/sfackler/rust-fallible-iterator |
| fallible-streaming-iterator | 0.1.9 | MIT/Apache-2.0 | https://github.com/sfackler/fallible-streaming-iterator |
| filetime | 0.2.29 | MIT/Apache-2.0 | https://github.com/alexcrichton/filetime |
| find-msvc-tools | 0.1.15 | MIT OR Apache-2.0 | https://github.com/rust-lang/cc-rs |
| flate2 | 1.1.11 | MIT OR Apache-2.0 | https://github.com/rust-lang/flate2-rs |
| foldhash | 0.1.5 | Zlib | https://github.com/orlp/foldhash |
| foldhash | 0.2.0 | Zlib | https://github.com/orlp/foldhash |
| form_urlencoded | 1.2.2 | MIT OR Apache-2.0 | https://github.com/servo/rust-url |
| futures-channel | 0.3.34 | MIT OR Apache-2.0 | https://github.com/rust-lang/futures-rs |
| futures-core | 0.3.34 | MIT OR Apache-2.0 | https://github.com/rust-lang/futures-rs |
| futures-sink | 0.3.34 | MIT OR Apache-2.0 | https://github.com/rust-lang/futures-rs |
| futures-task | 0.3.34 | MIT OR Apache-2.0 | https://github.com/rust-lang/futures-rs |
| futures-util | 0.3.34 | MIT OR Apache-2.0 | https://github.com/rust-lang/futures-rs |
| generic-array | 0.14.7 | MIT | https://github.com/fizyk20/generic-array.git |
| getrandom | 0.2.17 | MIT OR Apache-2.0 | https://github.com/rust-random/getrandom |
| getrandom | 0.3.4 | MIT OR Apache-2.0 | https://github.com/rust-random/getrandom |
| getrandom | 0.4.3 | MIT OR Apache-2.0 | https://github.com/rust-random/getrandom |
| hashbrown | 0.15.5 | MIT OR Apache-2.0 | https://github.com/rust-lang/hashbrown |
| hashbrown | 0.17.1 | MIT OR Apache-2.0 | https://github.com/rust-lang/hashbrown |
| hashlink | 0.12.2 | MIT OR Apache-2.0 | https://github.com/djc/hashlink |
| heck | 0.5.0 | MIT OR Apache-2.0 | https://github.com/withoutboats/heck |
| hmac | 0.12.1 | MIT OR Apache-2.0 | https://github.com/RustCrypto/MACs |
| hmac | 0.13.0 | MIT OR Apache-2.0 | https://github.com/RustCrypto/MACs |
| http | 1.5.0 | MIT OR Apache-2.0 | https://github.com/hyperium/http |
| http-body | 1.1.0 | MIT | https://github.com/hyperium/http-body |
| http-body-util | 0.1.5 | MIT | https://github.com/hyperium/http-body |
| httparse | 1.10.1 | MIT OR Apache-2.0 | https://github.com/seanmonstar/httparse |
| httpdate | 1.0.3 | MIT OR Apache-2.0 | https://github.com/pyfisch/httpdate |
| hybrid-array | 0.4.15 | MIT OR Apache-2.0 | https://github.com/RustCrypto/hybrid-array |
| hyper | 1.12.0 | MIT | https://github.com/hyperium/hyper |
| hyper-util | 0.1.21 | MIT | https://github.com/hyperium/hyper-util |
| indexmap | 2.14.2 | Apache-2.0 OR MIT | https://github.com/indexmap-rs/indexmap |
| inout | 0.1.4 | MIT OR Apache-2.0 | https://github.com/RustCrypto/utils |
| is_terminal_polyfill | 1.70.2 | MIT OR Apache-2.0 | https://github.com/polyfill-rs/is_terminal_polyfill |
| itoa | 1.0.18 | MIT OR Apache-2.0 | https://github.com/dtolnay/itoa |
| jiff | 0.2.42 | Unlicense OR MIT | https://github.com/BurntSushi/jiff |
| jiff-core | 0.1.4 | Unlicense OR MIT | https://github.com/BurntSushi/jiff |
| jiff-static | 0.2.42 | Unlicense OR MIT | https://github.com/BurntSushi/jiff |
| jiff-tzdb | 0.1.9 | Unlicense OR MIT | https://github.com/BurntSushi/jiff |
| jiff-tzdb-platform | 0.1.3 | Unlicense OR MIT | https://github.com/BurntSushi/jiff |
| js-sys | 0.3.107 | MIT OR Apache-2.0 | https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/js-sys |
| lazy_static | 1.5.1 | MIT OR Apache-2.0 | https://github.com/rust-lang-nursery/lazy-static.rs |
| libc | 0.2.191 | MIT OR Apache-2.0 | https://github.com/rust-lang/libc |
| libm | 0.2.16 | MIT | https://github.com/rust-lang/compiler-builtins |
| libredox | 0.1.25 | MIT | https://gitlab.redox-os.org/redox-os/libredox.git |
| libsqlite3-sys | 0.39.1 | MIT | https://github.com/rusqlite/rusqlite |
| lock_api | 0.4.14 | MIT OR Apache-2.0 | https://github.com/Amanieu/parking_lot |
| log | 0.4.35 | MIT OR Apache-2.0 | https://github.com/rust-lang/log |
| matchers | 0.2.0 | MIT | https://github.com/hawkw/matchers |
| matchit | 0.8.4 | MIT AND BSD-3-Clause | https://github.com/ibraheemdev/matchit |
| md-5 | 0.11.0 | MIT OR Apache-2.0 | https://github.com/RustCrypto/hashes |
| memchr | 2.8.3 | Unlicense OR MIT | https://github.com/BurntSushi/memchr |
| mime | 0.3.18 | MIT OR Apache-2.0 | https://github.com/hyperium/mime |
| mime_guess | 2.0.5 | MIT | https://github.com/abonander/mime_guess |
| miniz_oxide | 0.9.1 | MIT OR Zlib OR Apache-2.0 | https://github.com/Frommi/miniz_oxide/tree/master/miniz_oxide |
| mio | 1.2.4 | MIT | https://github.com/tokio-rs/mio |
| nu-ansi-term | 0.50.3 | MIT | https://github.com/nushell/nu-ansi-term |
| num-bigint | 0.4.8 | MIT OR Apache-2.0 | https://github.com/rust-num/num-bigint |
| num-conv | 0.2.2 | MIT OR Apache-2.0 | https://github.com/jhpratt/num-conv |
| num-integer | 0.1.47 | MIT OR Apache-2.0 | https://github.com/rust-num/num-integer |
| num-traits | 0.2.19 | MIT OR Apache-2.0 | https://github.com/rust-num/num-traits |
| objc2-core-foundation | 0.3.2 | Zlib OR Apache-2.0 OR MIT | https://github.com/madsmtm/objc2 |
| objc2-system-configuration | 0.3.2 | Zlib OR Apache-2.0 OR MIT | https://github.com/madsmtm/objc2 |
| once_cell | 1.21.4 | MIT OR Apache-2.0 | https://github.com/matklad/once_cell |
| once_cell_polyfill | 1.70.2 | MIT OR Apache-2.0 | https://github.com/polyfill-rs/once_cell_polyfill |
| opaque-debug | 0.3.1 | MIT OR Apache-2.0 | https://github.com/RustCrypto/utils |
| openssl-probe | 0.2.2 | MIT OR Apache-2.0 | https://github.com/rustls/openssl-probe |
| parking_lot | 0.12.5 | MIT OR Apache-2.0 | https://github.com/Amanieu/parking_lot |
| parking_lot_core | 0.9.12 | MIT OR Apache-2.0 | https://github.com/Amanieu/parking_lot |
| password-hash | 0.5.0 | MIT OR Apache-2.0 | https://github.com/RustCrypto/traits/tree/master/password-hash |
| pem | 3.0.6 | MIT | https://github.com/jcreekmore/pem-rs.git |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 | https://github.com/servo/rust-url/ |
| phf | 0.13.1 | MIT | https://github.com/rust-phf/rust-phf |
| phf_shared | 0.13.1 | MIT | https://github.com/rust-phf/rust-phf |
| pin-project-lite | 0.2.17 | Apache-2.0 OR MIT | https://github.com/taiki-e/pin-project-lite |
| pkg-config | 0.3.34 | MIT OR Apache-2.0 | https://github.com/rust-lang/pkg-config-rs |
| poly1305 | 0.8.0 | Apache-2.0 OR MIT | https://github.com/RustCrypto/universal-hashes |
| portable-atomic | 1.15.0 | Apache-2.0 OR MIT | https://github.com/taiki-e/portable-atomic |
| portable-atomic-util | 0.2.8 | Apache-2.0 OR MIT | https://github.com/taiki-e/portable-atomic-util |
| postgres | 0.19.14 | MIT OR Apache-2.0 | https://github.com/rust-postgres/rust-postgres |
| postgres-protocol | 0.6.12 | MIT OR Apache-2.0 | https://github.com/rust-postgres/rust-postgres |
| postgres-types | 0.2.14 | MIT OR Apache-2.0 | https://github.com/rust-postgres/rust-postgres |
| powerfmt | 0.2.1 | MIT OR Apache-2.0 | https://github.com/jhpratt/powerfmt |
| ppv-lite86 | 0.2.21 | MIT OR Apache-2.0 | https://github.com/cryptocorrosion/cryptocorrosion |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 | https://github.com/dtolnay/proc-macro2 |
| qrcode | 0.14.1 | MIT OR Apache-2.0 | https://github.com/kennytm/qrcode-rust |
| quote | 1.0.47 | MIT OR Apache-2.0 | https://github.com/dtolnay/quote |
| r-efi | 5.3.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later | https://github.com/r-efi/r-efi |
| r-efi | 6.0.0 | MIT OR Apache-2.0 OR LGPL-2.1-or-later | https://github.com/r-efi/r-efi |
| rand | 0.10.3 | MIT OR Apache-2.0 | https://github.com/rust-random/rand |
| rand | 0.9.5 | MIT OR Apache-2.0 | https://github.com/rust-random/rand |
| rand_chacha | 0.9.0 | MIT OR Apache-2.0 | https://github.com/rust-random/rand |
| rand_core | 0.10.1 | MIT OR Apache-2.0 | https://github.com/rust-random/rand_core |
| rand_core | 0.6.4 | MIT OR Apache-2.0 | https://github.com/rust-random/rand |
| rand_core | 0.9.5 | MIT OR Apache-2.0 | https://github.com/rust-random/rand |
| rcgen | 0.13.2 | MIT OR Apache-2.0 | https://github.com/rustls/rcgen |
| redox_syscall | 0.5.18 | MIT | https://gitlab.redox-os.org/redox-os/syscall |
| regex-automata | 0.4.18 | MIT OR Apache-2.0 | https://github.com/rust-lang/regex |
| regex-syntax | 0.8.11 | MIT OR Apache-2.0 | https://github.com/rust-lang/regex |
| ring | 0.17.14 | Apache-2.0 AND ISC | https://github.com/briansmith/ring |
| rpassword | 7.5.4 | Apache-2.0 | https://github.com/conradkleinespel/rpassword |
| rsqlite-vfs | 0.2.0 | MIT | https://github.com/Spxg/sqlite-wasm-rs |
| rtoolbox | 0.0.6 | Apache-2.0 | https://github.com/conradkleinespel/rtoolbox |
| rusqlite | 0.41.1 | MIT | https://github.com/rusqlite/rusqlite |
| rust-embed | 8.13.0 | MIT | https://pyrossh.dev/repos/rust-embed |
| rust-embed-impl | 8.13.0 | MIT | https://pyrossh.dev/repos/rust-embed |
| rust-embed-utils | 8.13.0 | MIT | https://pyrossh.dev/repos/rust-embed |
| rustls | 0.23.46 | Apache-2.0 OR ISC OR MIT | https://github.com/rustls/rustls |
| rustls-native-certs | 0.8.5 | Apache-2.0 OR ISC OR MIT | https://github.com/rustls/rustls-native-certs |
| rustls-pki-types | 1.15.1 | MIT OR Apache-2.0 | https://github.com/rustls/pki-types |
| rustls-webpki | 0.103.16 | ISC | https://github.com/rustls/webpki |
| rustversion | 1.0.23 | MIT OR Apache-2.0 | https://github.com/dtolnay/rustversion |
| ryu | 1.0.23 | Apache-2.0 OR BSL-1.0 | https://github.com/dtolnay/ryu |
| same-file | 1.0.6 | Unlicense/MIT | https://github.com/BurntSushi/same-file |
| schannel | 0.1.30 | MIT | https://github.com/steffengy/schannel-rs |
| scopeguard | 1.2.0 | MIT OR Apache-2.0 | https://github.com/bluss/scopeguard |
| security-framework | 3.7.0 | MIT OR Apache-2.0 | https://github.com/kornelski/rust-security-framework |
| security-framework-sys | 2.17.0 | MIT OR Apache-2.0 | https://github.com/kornelski/rust-security-framework |
| serde | 1.0.229 | MIT OR Apache-2.0 | https://github.com/serde-rs/serde |
| serde_core | 1.0.229 | MIT OR Apache-2.0 | https://github.com/serde-rs/serde |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 | https://github.com/serde-rs/serde |
| serde_json | 1.0.154 | MIT OR Apache-2.0 | https://github.com/serde-rs/json |
| serde_path_to_error | 0.1.20 | MIT OR Apache-2.0 | https://github.com/dtolnay/path-to-error |
| serde_spanned | 1.1.3 | MIT OR Apache-2.0 | https://github.com/toml-rs/toml |
| serde_urlencoded | 0.7.1 | MIT/Apache-2.0 | https://github.com/nox/serde_urlencoded |
| sha1 | 0.10.7 | MIT OR Apache-2.0 | https://github.com/RustCrypto/hashes |
| sha2 | 0.10.9 | MIT OR Apache-2.0 | https://github.com/RustCrypto/hashes |
| sha2 | 0.11.1 | MIT OR Apache-2.0 | https://github.com/RustCrypto/hashes |
| sharded-slab | 0.1.7 | MIT | https://github.com/hawkw/sharded-slab |
| shlex | 2.0.1 | MIT OR Apache-2.0 | https://github.com/comex/rust-shlex |
| signal-hook-registry | 1.4.8 | MIT OR Apache-2.0 | https://github.com/vorner/signal-hook |
| simd-adler32 | 0.3.10 | MIT | https://github.com/mcountryman/simd-adler32 |
| siphasher | 1.0.4 | MIT OR Apache-2.0 | https://github.com/jedisct1/rust-siphash |
| slab | 0.4.12 | MIT | https://github.com/tokio-rs/slab |
| smallvec | 1.16.5 | MIT OR Apache-2.0 | https://github.com/servo/rust-smallvec |
| socket2 | 0.6.5 | MIT OR Apache-2.0 | https://github.com/rust-lang/socket2 |
| spin | 0.9.9 | MIT | https://github.com/mvdnes/spin-rs.git |
| sqlite-wasm-rs | 0.6.2 | MIT | https://github.com/Spxg/sqlite-wasm-rs |
| string-interner | 0.19.0 | MIT/Apache-2.0 | https://github.com/robbepop/string-interner |
| stringprep | 0.1.5 | MIT/Apache-2.0 | https://github.com/sfackler/rust-stringprep |
| strsim | 0.11.1 | MIT | https://github.com/rapidfuzz/strsim-rs |
| subtle | 2.6.1 | BSD-3-Clause | https://github.com/dalek-cryptography/subtle |
| syn | 2.0.119 | MIT OR Apache-2.0 | https://github.com/dtolnay/syn |
| syn | 3.0.9 | MIT OR Apache-2.0 | https://github.com/dtolnay/syn |
| sync_wrapper | 1.0.2 | Apache-2.0 | https://github.com/Actyx/sync_wrapper |
| tar | 0.4.46 | MIT OR Apache-2.0 | https://github.com/composefs/tar-rs |
| thiserror | 2.0.21 | MIT OR Apache-2.0 | https://github.com/dtolnay/thiserror |
| thiserror-impl | 2.0.21 | MIT OR Apache-2.0 | https://github.com/dtolnay/thiserror |
| thread_local | 1.1.10 | MIT OR Apache-2.0 | https://github.com/Amanieu/thread_local-rs |
| time | 0.3.55 | MIT OR Apache-2.0 | https://github.com/time-rs/time |
| time-core | 0.1.9 | MIT OR Apache-2.0 | https://github.com/time-rs/time |
| time-macros | 0.2.32 | MIT OR Apache-2.0 | https://github.com/time-rs/time |
| tinyvec | 1.13.3 | Zlib OR Apache-2.0 OR MIT | https://github.com/Lokathor/tinyvec |
| tokio | 1.54.1 | MIT | https://github.com/tokio-rs/tokio |
| tokio-macros | 2.7.2 | MIT | https://github.com/tokio-rs/tokio |
| tokio-postgres | 0.7.18 | MIT OR Apache-2.0 | https://github.com/rust-postgres/rust-postgres |
| tokio-rustls | 0.26.6 | MIT OR Apache-2.0 | https://github.com/rustls/tokio-rustls |
| tokio-tungstenite | 0.29.0 | MIT | https://github.com/snapview/tokio-tungstenite |
| tokio-util | 0.7.20 | MIT | https://github.com/tokio-rs/tokio |
| toml | 1.2.1+spec-1.1.0 | MIT OR Apache-2.0 | https://github.com/toml-rs/toml |
| toml_datetime | 1.1.3+spec-1.1.0 | MIT OR Apache-2.0 | https://github.com/toml-rs/toml |
| toml_parser | 1.1.5+spec-1.1.0 | MIT OR Apache-2.0 | https://github.com/toml-rs/toml |
| toml_writer | 1.2.0+spec-1.1.0 | MIT OR Apache-2.0 | https://github.com/toml-rs/toml |
| tower | 0.5.3 | MIT | https://github.com/tower-rs/tower |
| tower-layer | 0.3.3 | MIT | https://github.com/tower-rs/tower |
| tower-service | 0.3.3 | MIT | https://github.com/tower-rs/tower |
| tracing | 0.1.44 | MIT | https://github.com/tokio-rs/tracing |
| tracing-attributes | 0.1.31 | MIT | https://github.com/tokio-rs/tracing |
| tracing-core | 0.1.36 | MIT | https://github.com/tokio-rs/tracing |
| tracing-log | 0.2.0 | MIT | https://github.com/tokio-rs/tracing |
| tracing-subscriber | 0.3.23 | MIT | https://github.com/tokio-rs/tracing |
| tungstenite | 0.29.0 | MIT OR Apache-2.0 | https://github.com/snapview/tungstenite-rs |
| typenum | 1.20.1 | MIT OR Apache-2.0 | https://github.com/paholg/typenum |
| unicase | 2.10.0 | MIT OR Apache-2.0 | https://github.com/seanmonstar/unicase |
| unicode-bidi | 0.3.18 | MIT OR Apache-2.0 | https://github.com/servo/unicode-bidi |
| unicode-ident | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 | https://github.com/dtolnay/unicode-ident |
| unicode-normalization | 0.1.26 | (MIT OR Apache-2.0) AND Unicode-3.0 | https://github.com/unicode-rs/unicode-normalization |
| unicode-properties | 0.1.5 | (MIT OR Apache-2.0) AND Unicode-3.0 | https://github.com/unicode-rs/unicode-properties |
| universal-hash | 0.5.1 | MIT OR Apache-2.0 | https://github.com/RustCrypto/traits |
| untrusted | 0.9.0 | ISC | https://github.com/briansmith/untrusted |
| utf8parse | 0.2.2 | Apache-2.0 OR MIT | https://github.com/alacritty/vte |
| valuable | 0.1.1 | MIT | https://github.com/tokio-rs/valuable |
| vcpkg | 0.2.15 | MIT/Apache-2.0 | https://github.com/mcgoo/vcpkg-rs |
| version_check | 0.9.5 | MIT/Apache-2.0 | https://github.com/SergioBenitez/version_check |
| walkdir | 2.5.0 | Unlicense/MIT | https://github.com/BurntSushi/walkdir |
| wasi | 0.11.1+wasi-snapshot-preview1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | https://github.com/bytecodealliance/wasi |
| wasi | 0.14.7+wasi-0.2.4 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | https://github.com/bytecodealliance/wasi-rs |
| wasip2 | 1.0.4+wasi-0.2.12 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | https://github.com/bytecodealliance/wasi-rs |
| wasite | 1.0.2 | Apache-2.0 OR BSL-1.0 OR MIT | https://github.com/ardaku/wasite |
| wasm-bindgen | 0.2.130 | MIT OR Apache-2.0 | https://github.com/wasm-bindgen/wasm-bindgen |
| wasm-bindgen-macro | 0.2.130 | MIT OR Apache-2.0 | https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/macro |
| wasm-bindgen-macro-support | 0.2.130 | MIT OR Apache-2.0 | https://github.com/wasm-bindgen/wasm-bindgen/tree/main/crates/macro-support |
| wasm-bindgen-shared | 0.2.130 | MIT OR Apache-2.0 | https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/shared |
| wasmi | 2.0.0 | MIT/Apache-2.0 | https://github.com/wasmi-labs/wasmi |
| wasmi_collections | 2.0.0 | MIT/Apache-2.0 | https://github.com/wasmi-labs/wasmi |
| wasmi_core | 2.0.0 | MIT/Apache-2.0 | https://github.com/wasmi-labs/wasmi |
| wasmi_ir | 2.0.0 | MIT/Apache-2.0 | https://github.com/wasmi-labs/wasmi |
| wasmparser | 0.228.0 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | https://github.com/bytecodealliance/wasm-tools/tree/main/crates/wasmparser |
| web-sys | 0.3.107 | MIT OR Apache-2.0 | https://github.com/wasm-bindgen/wasm-bindgen/tree/master/crates/web-sys |
| whoami | 2.1.3 | Apache-2.0 OR BSL-1.0 OR MIT | https://github.com/ardaku/whoami |
| winapi-util | 0.1.11 | Unlicense OR MIT | https://github.com/BurntSushi/winapi-util |
| windows-link | 0.2.1 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| windows-sys | 0.52.0 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| windows-targets | 0.52.6 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| windows_aarch64_gnullvm | 0.52.6 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| windows_aarch64_msvc | 0.52.6 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| windows_i686_gnu | 0.52.6 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| windows_i686_gnullvm | 0.52.6 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| windows_i686_msvc | 0.52.6 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| windows_x86_64_gnu | 0.52.6 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| windows_x86_64_gnullvm | 0.52.6 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| windows_x86_64_msvc | 0.52.6 | MIT OR Apache-2.0 | https://github.com/microsoft/windows-rs |
| winnow | 1.0.5 | MIT | https://github.com/winnow-rs/winnow |
| wit-bindgen | 0.57.1 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | https://github.com/bytecodealliance/wit-bindgen |
| yasna | 0.5.2 | MIT OR Apache-2.0 | https://github.com/qnighy/yasna.rs |
| zerocopy | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT | https://github.com/google/zerocopy |
| zerocopy-derive | 0.8.63 | BSD-2-Clause OR Apache-2.0 OR MIT | https://github.com/google/zerocopy |
| zeroize | 1.9.1 | Apache-2.0 OR MIT | https://github.com/RustCrypto/utils |
| zlib-rs | 0.6.8 | Zlib | https://github.com/trifectatechfoundation/zlib-rs |
| zmij | 1.0.23 | MIT | https://github.com/dtolnay/zmij |
