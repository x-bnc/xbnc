# Contributing to xBNC

## Building and testing

```bash
cargo build
cargo test --all
cargo clippy --all-targets -- -D warnings
cargo fmt --all
```

Requirements: Rust 1.89 or later and a C compiler (SQLite is built with the project).

### Web panel

```bash
cd web
npm ci
npm run dev          # Vite on http://localhost:5173, with /api sent to xbncd on 127.0.0.1:8080
npm run build        # checks types and generates web/dist
npm run i18n:check   # every text of the panel is in locales/en.json
```

`web/dist` is embedded in the binary at build time, so **commit `web/dist` together with the panel changes** (CI checks that it matches the sources).

Texts are written in English: in the panel inside `t("...")`, `tk("...")` or `tx("...")`, and in the server in the `irc` section of `locales/en.json`, used with `self.t("key", &[])` or `tr!(self, "key", ...)`. Add the Spanish translation to `locales/es.json` (the tests check that it is complete). The guide for translators is [TRANSLATING.md](TRANSLATING.md).

When you add an API route, document it in `docs/openapi.json`: a test fails if it is missing.

## Documentation

The documentation lives in the wiki, a separate project (`xbnc-wiki`, next to this folder), so anyone can read it without installing xBNC. The files of `docs/` (English) and `docs/es/` (Spanish) are generated from the wiki; do not edit them by hand. After changing the wiki (or anything the wiki reads from the source code: modules, plugins, `*status` commands, language files), regenerate them:

```bash
python3 ../xbnc-wiki/build.py --app . --markdown docs
```

`docs/STATUS.md` and `docs/openapi.json` are written by hand.

## Project rules

These rules come from the requirements document and are applied in review:

1. No simulated features or demo code as the final implementation. If something cannot be done properly yet, it is documented in `docs/STATUS.md` instead of being faked.
2. IRCv3 capabilities that are not implemented are not announced.
3. Every new feature comes with tests; changes to network behaviour, with a test in `crates/xbncd/tests/e2e.rs`.
4. Zero compiler and clippy warnings.
5. Never log passwords, tokens or credentials; use `crypto::Secret` and `auth::redact_line`.
6. Database migrations are added at the end of `MIGRATIONS` in `db.rs` (and of `PG_MIGRATIONS` for PostgreSQL); an existing one is never modified.
7. Do not copy code from other projects. Implement from the specifications (RFCs, IRCv3, Modern IRC).
8. New dependencies only if necessary and with a license allowed in `deny.toml`. Regenerate `THIRD_PARTY_LICENSES.md` when dependencies change.
9. Every visible change is noted in `CHANGELOG.md`.

## Layout

```
crates/xbnc-proto   IRC protocol without I/O
crates/xbncd        service, API, CLI and end-to-end tests
web/                panel (React + TypeScript + Vite); web/dist is embedded in the binary
locales/            translations: en.json (base), es.json and any other language
sdk/                plugin SDKs for C and Rust, and the official plugins
packaging/          example configuration, systemd unit, Nginx and monitoring
scripts/            installation, update, backup, restore and publishing on GitHub
docker/, Dockerfile, docker-compose.yml
testenv/            environment with Ergo and InspIRCd, and the real-network test plan
docs/               documentation (generated from the wiki) and the OpenAPI specification
```

## Commits and branches

One change per *pull request*, describing the problem, the solution and how it was tested. CI must pass.
