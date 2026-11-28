# Security Policy

## Supported Versions

Security fixes land on the latest `0.21.x` and subsequent `0.x` release line on `main`. Earlier tags do not receive backported patches.

## Security Model

Finaltree computes working-tree fingerprints locally using `git` commands with zero network transport. Its `fingerprint` and `verify` doors execute no user commands, while `stamp` executes the explicit gate command passed by the caller and binds the pre-run tree SHA-256 digest, command argv, exit code, and every output line under a tamper-evident `evidence-digest` header so altered or truncated logs are refused on `verify`. Its JSON reader enforces a `4000` nesting bound, and fingerprint validation refuses duplicate member paths or pairs across record member lists. Never commit real repository paths, internal commit histories, or private gate logs into test fixtures.

## Reporting a Vulnerability

Do not open a public issue for a suspected security vulnerability. Report vulnerabilities privately through [GitHub Security Advisories](https://github.com/david-engelmann/finaltree/security/advisories/new) with synthetic inputs only, naming the affected version and the relevant section of `CONTRACT.md`. Every report receives an initial review within seven days, and confirmed fixes ship with a regression pin in the test suite before public disclosure.
