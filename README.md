# ounce-hb

A runner for **Nintendo Switch homebrew**: unencrypted NRO (and libnx-produced NSO) programs
that you built yourself from source with [devkitPro](https://devkitpro.org/) and
[libnx](https://github.com/switchbrew/libnx).

ounce-hb builds on [ounce-core](https://github.com/hffgzktbcc-blip/ounce-core) (CPU engines, MMU,
and verification tooling) and aims for transparent execution, strict clean-room provenance,
informative diagnostics, and reproducible results.

> **Status: Milestone M0**. The CLI validates input formats and refuses all out-of-scope
> containers. It executes validated NROs via an `ounce-core` subprocess runner in headless mode.
> See [Usage](#usage), [docs/LIMITS.md](docs/LIMITS.md), and
> [docs/OUNCE_CORE_API_NEEDS.md](docs/OUNCE_CORE_API_NEEDS.md).

---

## Scope & Non-Goals

### In Scope
- Unencrypted homebrew **NRO** files (`NRO0` magic at offset `0x10`).
- Unencrypted homebrew **NSO** files (`NSO0` magic at offset `0x00`) built from source with devkitPro/libnx (format classification supported; execution planned).
- Clean-room implementation derived solely from public specifications, libnx headers, and Arm manuals.
- Clear error reporting, diagnostic exit codes, and automated verification guardrails.

### Strictly Out of Scope (Now and Always)
- **Retail games, commercial software, and proprietary game dumps.**
- **Console keys, key derivation, or key handling** of any kind. None are accepted, stored, or processed.
- **Console firmware or system files.**
- **Encrypted retail containers**: NSP / PFS0 packages, XCI game card images, and NCA content archives.
- **Circumvention of technical protection measures** or DRM.
- **Reverse-engineered code from existing Switch emulators** (e.g., Yuzu, Ryujinx, Suyu).

Issues, pull requests, or discussions requesting retail software support, keys, firmware, or circumvention are **closed and locked immediately without discussion**.

---

## Usage

### Prerequisites
1. **Rust stable toolchain** (see [rust-toolchain.toml](rust-toolchain.toml)).
2. **ounce-core CLI binary**: ounce-hb executes validated homebrew via `ounce-core run-nro <app.nro> --present=headless`.
   - Obtain a pre-built binary from the GitHub releases of [hffgzktbcc-blip/ounce-core](https://github.com/hffgzktbcc-blip/ounce-core/releases) (v0.8.0), or build from source from the [ounce-core repository](https://github.com/hffgzktbcc-blip/ounce-core).
   - Ensure `ounce-core` is available on your `PATH`, or specify its path directly via the `OUNCE_CORE_BIN` environment variable (e.g., `export OUNCE_CORE_BIN=/path/to/ounce-core`).

### Running a Homebrew Program
```sh
# Run a homebrew NRO
cargo run -- run path/to/app.nro

# Run with guest arguments (separated by --)
cargo run -- run path/to/app.nro -- arg1 arg2
```

### Subprocess Runner Behavior
- **Timeout**: Each execution is subject to a 60-second wall-clock limit. If exceeded, the child process is terminated and exit code 3 is returned.
- **Guest Arguments**: Guest arguments starting with `-` are refused because upstream `ounce-core` silently discards them. Pass positional arguments only.
- **Headless Mode**: ounce-hb runs `ounce-core` with `--present=headless` so no GUI window or host audio device is required.
- **Output Forwarding**: Program stdout and stderr are forwarded directly to your console.

### Exit Codes
| Exit Code | Meaning |
|---|---|
| `0` | Guest program executed successfully and exited with 0 |
| `1` | Guest program exited with 1 (or was terminated by a signal) |
| `2` | Bad CLI invocation, file unreadable, or out-of-scope input (e.g. NSP, XCI, NCA, NPDM, or unrecognized magic) |
| `3` | Runner error (missing `ounce-core` binary, execution timeout, unsupported argument starting with `-`, or unsupported NSO kind) |

---

## Development & Verification

Every change must pass our test suite, formatting standards, and legal guardrails:

```sh
# Check formatting
cargo fmt --all -- --check

# Run linter
cargo clippy --workspace --all-targets -- -D warnings

# Run unit and integration tests
cargo test --workspace

# Run provenance and legal guardrail checks
scripts/guardrails.sh --self-test
scripts/guardrails.sh
```

---

## Project Documentation

- **Contributing**: [CONTRIBUTING.md](CONTRIBUTING.md) — clean-room development rules, workflow, and claiming tasks.
- **Code of Conduct**: [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) — community standards and respectful participation.
- **Security Policy**: [SECURITY.md](SECURITY.md) — private vulnerability reporting route via GitHub Advisories.
- **Clean-Room Policy**: [docs/CLEAN_ROOM.md](docs/CLEAN_ROOM.md) — allowed sources, prohibited sources, and tainted contributor rules.
- **Project Vision**: [docs/VISION.md](docs/VISION.md) — purpose, pillars, and milestone roadmap.
- **Honest Limits**: [docs/LIMITS.md](docs/LIMITS.md) — verified current capabilities and explicit permanent limits.
- **Add a Program**: [docs/ADD_A_PROGRAM.md](docs/ADD_A_PROGRAM.md) — building and testing a homebrew binary with devkitPro.
- **Upstream Core Needs**: [docs/OUNCE_CORE_API_NEEDS.md](docs/OUNCE_CORE_API_NEEDS.md) — library API requirements for future in-process ounce-core integration.
- **Provenance Register**: [docs/PROVENANCE.md](docs/PROVENANCE.md) — tracking of all binary fixtures and their toolchains.
- **Wiki**: [docs/wiki/Home.md](docs/wiki/Home.md) — repository wiki pages (Home, Getting started, FAQ, Clean-room policy, Architecture, Roadmap).
- **Credits**: [CREDITS.md](CREDITS.md) — contributors, upstream projects, and toolchains.
- **Changelog**: [CHANGELOG.md](CHANGELOG.md) — version history and milestone tracking.

---

## Legal & Disclaimers

ounce-hb is an independent open-source project and is not affiliated with, endorsed by, or sponsored by Nintendo. "Nintendo" and "Nintendo Switch" are registered trademarks of Nintendo of America Inc. and Nintendo Co., Ltd. These terms are used strictly for compatibility identification and descriptive purposes.

This project is licensed under the [MIT License](LICENSE).
