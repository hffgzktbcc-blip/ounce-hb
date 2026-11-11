# ounce-hb

A runner for **Nintendo Switch homebrew**: NRO and NSO programs that you built yourself from
source with [devkitPro](https://devkitpro.org/) and [libnx](https://github.com/switchbrew/libnx).
It builds on [ounce-core](https://github.com/hffgzktbcc-blip/ounce-core) (CPU engines, MMU,
verification tooling) and aims for correct output, good diagnostics and verifiable results.

> Status: **milestone M0 (scaffold)**. The CLI validates input and refuses everything out of
> scope. It does **not** execute programs yet; see [docs/OUNCE_CORE_API_NEEDS.md](docs/OUNCE_CORE_API_NEEDS.md).

## Scope

In scope:
- Homebrew NRO (and libnx-produced NSO) built from source with devkitPro/libnx.

Out of scope, now and in the future:
- Retail games, keys, firmware or system files. None are shipped, requested or accepted.
- NSP, XCI and NCA files, and any decryption.
- Circumvention of any technical protection.

Issues and requests about retail software, keys or piracy are closed and locked.

## Usage

```sh
cargo run -- run path/to/app.nro
```

Anything that is not an unencrypted homebrew NRO/NSO is rejected with a message and exit code 2.
A valid NRO currently exits with code 3 and states which ounce-core API is missing.

## Development

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
scripts/guardrails.sh        # unregistered binaries, key/decryption code
```

See [CONTRIBUTING.md](CONTRIBUTING.md) (clean-room rule, provenance, AI disclosure) and
[SECURITY.md](SECURITY.md).

## Legal

ounce-hb is not affiliated with, endorsed by or sponsored by Nintendo. "Nintendo" and "Switch"
are trademarks of their owners and are used here only to describe compatibility with
homebrew software. MIT licensed, see [LICENSE](LICENSE).
