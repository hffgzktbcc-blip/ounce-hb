# Getting Started

This guide explains how to install prerequisites, build ounce-hb from source, and run your first
Nintendo Switch homebrew executable.

---

## Prerequisites

1. **Rust Stable Toolchain**:
   Ensure you have a recent stable Rust toolchain installed via `rustup`.
   ```sh
   rustup update stable
   ```

2. **ounce-core Binary**:
   ounce-hb delegates ARM64 emulation to `ounce-core`.
   - Download the v0.8.0 release from [hffgzktbcc-blip/ounce-core](https://github.com/hffgzktbcc-blip/ounce-core/releases),
     or build it from source.
   - Place the binary on your system `PATH` or point to it via the `OUNCE_CORE_BIN` environment variable:
     ```sh
     export OUNCE_CORE_BIN=/path/to/ounce-core
     ```

3. **devkitPro & libnx** (for building your own homebrew):
   - Follow setup instructions at [devkitpro.org](https://devkitpro.org/).
   - Ensure `devkitA64` and `libnx` are installed.

---

## Building ounce-hb

Clone the repository and build using Cargo:
```sh
cargo build --release
```
The compiled binary will be located at `target/release/ounce-hb`.

---

## Running a Homebrew Application

To execute a homebrew `.nro` file:
```sh
# Using cargo
cargo run -- run path/to/application.nro

# Or using the built release binary
./target/release/ounce-hb run path/to/application.nro
```

### Passing Guest Arguments
To pass command-line arguments to the guest homebrew application, separate them with `--`:
```sh
cargo run -- run path/to/application.nro -- input_file.bin output_file.bin
```
*(Note: Arguments starting with `-` are not supported and will be refused by ounce-hb).*

---

## Understanding Exit Codes

ounce-hb communicates results via standard process exit codes:

| Code | Meaning |
|---|---|
| `0` | Guest program ran to completion and exited with status 0 |
| `1` | Guest program returned exit code 1 or was terminated by a signal |
| `2` | Bad command invocation, file unreadable, or out-of-scope input (NSP, XCI, NCA, NPDM) |
| `3` | Runner error (missing `ounce-core` binary, 60s timeout, unsupported argument, or NSO kind) |

---

## Running Verification Checks

Verify that your environment meets all quality and legal guardrails:
```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
scripts/guardrails.sh --self-test
scripts/guardrails.sh
```

---

## Related Documentation
- [FAQ](FAQ.md)
- [Architecture](Architecture.md)
- [Add a Program](../ADD_A_PROGRAM.md)
- [Limits & Capabilities](../LIMITS.md)
