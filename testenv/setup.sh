#!/usr/bin/env bash
# Creates a test user with two networks (Ergo and InspIRCd) in the testenv.
set -euo pipefail
cd "$(dirname "$0")"

run() { docker compose exec -T "$@"; }

echo "==> Creating user 'test' (password: testpassword1)"
run -e XBNC_PASSWORD=testpassword1 xbnc xbncd user add test --admin || true

echo "==> Adding networks"
run xbnc xbncd network add test ergo --host ergo --port 6667 --no-tls --nick xbnctest || true
run xbnc xbncd network add test inspircd --host inspircd --port 6667 --no-tls --nick xbnctest || true

echo "==> Saving channels"
run xbnc xbncd channel add test ergo '#xbnc' || true
run xbnc xbncd channel add test inspircd '#xbnc' || true

echo "==> Reloading xBNC"
docker compose kill -s HUP xbnc >/dev/null

cat <<'EOF'

Ready. Connect an IRC client to 127.0.0.1 port 6667 (no TLS):
  server password  test/ergo@laptop:testpassword1
  server password  test/inspircd@laptop:testpassword1

Web panel: http://localhost:8080  (user: test, password: testpassword1)

To talk to the bouncer user from "outside", connect a second client
directly to Ergo (127.0.0.1:16667) or InspIRCd (127.0.0.1:26667) and join #xbnc.

Logs: docker compose logs -f xbnc
EOF
