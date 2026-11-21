# Security policy

## Versions with security fixes

| Version | Fixes |
| --- | --- |
| 1.0.x | Yes |
| 0.x | No: update to 1.0 |

## How to report a vulnerability

Do not open a public issue. Use the repository's private advisory on GitHub
(**Security → Report a vulnerability**, https://github.com/x-bnc/xbnc/security/advisories/new).
If you cannot use it, open an issue asking for a private contact, without
any detail of the problem.

If you can, include:

- the version (`xbncd --version` or the panel footer);
- which part is affected (IRC, panel, API, plugins, installer…);
- the steps to reproduce it or a proof of concept;
- what someone exploiting it could achieve.

## What you can expect

- An answer within **3 days** at most.
- An initial assessment within **7 days**: if it is confirmed, its severity
  and a plan.
- A fix published within **90 days** at most, sooner if it is serious.
  We keep you informed during that time.
- Credit in the release notes, unless you prefer not to appear.

We ask you not to publish the details until a fixed version is out or 90
days have passed, whichever comes first.

## Scope

Everything in this repository: the `xbncd` server, the panel, the API, the
SDK and the official plugins, the installer and the packages. IRC networks,
clients and third-party plugins are out of scope (tell their authors).

## Good practice

Test on your own installation, with no denial of service against other
people's servers, no access to other people's data and no social
engineering.

## What xBNC does for you

The threat model is in [docs/THREAT_MODEL.md](docs/THREAT_MODEL.md) (also in
the xBNC wiki, section “Threat model”), and the security measures for users
and administrators in [docs/USERS_AND_SECURITY.md](docs/USERS_AND_SECURITY.md).
Releases can be signed with Ed25519 and the panel only installs signed
packages.
