<a id="performance"></a>

# Performance

*Part of the xBNC 1.0.0 documentation. The same content, with search, is published as the xBNC wiki, a website of its own. These files are generated from the wiki; to change them, change the wiki.*

Load test (`cargo run --release --example loadtest -- --users 1000 --clients 1 --rate 2 --seconds 60`): a simulated IRC server sends messages to every network at once (the worst case) and each client measures how long it takes to receive them, going through the history, modules and plugins. Machine: 2 vCPU Xeon at 2.1 GHz, 7 GB of RAM, network disk, default configuration (SQLite).

| Users and networks | Clients | Messages/s delivered | CPU (of one core) | Memory | Latency p50 / p95 / p99 | Every network back after an outage |
| --- | --- | --- | --- | --- | --- | --- |
| 500 | 500 | 492 | 5 % | 52 MB | 21 / 43 / 46 ms | 6.2 s |
| 500 | 500 | 4,847 | 43 % | 56 MB | 46 / 64 / 72 ms | 6.2 s |
| 1,000 | 1,000 | 1,951 | 17 % | 88 MB | 50 / 80 / 95 ms | 6.2 s |
| 1,000 | 2,000 | 1,968 | 11 % | 114 MB | 44 / 66 / 78 ms | 6.2 s |
| 2,000 | 2,000 | 1,968 | 19 % | 153 MB | 75 / 141 / 155 ms | 6.3 s |

- No message was lost in any test.
- Latency grows with the size of the burst: with 2,000 networks receiving the same message at once, the last one is delivered about 150 ms after the first. With real traffic, spread over time, it is a few milliseconds.
- Memory: about 45 KB per connected network and about 30 KB more per client, not counting the history (on disk).
- Client logins (Argon2id) go one at a time per CPU: 2,000 clients signing in together take about 27 s on 2 cores, without a memory spike.
- Disk: about 150 bytes per stored message, including the search index.

**Sizing:** a small server (1–2 vCPU, 1 GB of RAM) serves hundreds of users with several networks and thousands of messages per second. With many users, check `limits.max_connections`, the service's `LimitNOFILE` (65,536 by default) and the history retention.

### How it is measured

The test (`crates/xbncd/examples/loadtest.rs`) starts, in a single process:

- a simulated IRC server with a channel `#xbnc`;
- xBNC with N users, each one with its network connected to that server and joined to `#xbnc`;
- one or more IRC clients per user connected to xBNC.

The server sends `--rate` messages per second to the channel, **to every network at the same time** (the worst case: simultaneous bursts). Each message carries the time it was sent, and each client measures how long it takes to arrive: IRC server → xBNC (stored in the history, modules, plugins) → client. At the end the server cuts every connection and the test measures how long it takes for every network to come back.

Memory and CPU are those of the whole process, so they include the simulated server and the test clients (xBNC alone uses a little less). The build is `--release` with the default configuration (history in SQLite with WAL).

### Problems found and fixed with this test

| Problem | Before | After |
| --- | --- | --- |
| Memory after many simultaneous logins: glibc kept in each thread the 19 MB of every Argon2id calculation. | 1,680 MB with 500 clients | 55 MB (`util::tune_allocator`, a fixed `mmap` threshold) |
| Hundreds of Argon2id calculations at once (also useful against floods of login attempts). | No limit | One per CPU (`crypto::with_hash_slot`) |
| One SQLite transaction per stored message. | p50 70 ms and 13 % CPU with 500 networks | p50 21 ms and 5 % CPU: a single writer thread stores everything waiting in one transaction (`Db::insert_message`) |

### Still to do

- Repeat the test on dedicated hardware and with several networks per user.
- Measure with real IRC servers far away (the test uses a local one, without network latency).

To repeat it: `cargo run --release --example loadtest -- --users 2000 --clients 1 --rate 1 --seconds 60`. The options are `--users`, `--clients` (per user), `--rate` (messages per second to the channel) and `--seconds`.
