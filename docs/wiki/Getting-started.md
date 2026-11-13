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

Every outcome has its own exit code, so scripts can tell categories apart. The full, authoritative list with
what each category means is [DIAGNOSIS.md](../DIAGNOSIS.md); this table is the same contract.

| Code | Meaning |
|---|---|
| `0` | Guest program exited normally (svcExitProcess) |
| `1` | ounce-core stopped in a way ounce-hb does not recognise (raw output is printed unchanged) |
| `2` | Bad CLI invocation, file unreadable, out-of-scope input (e.g. NSP, XCI, NCA, NPDM, or unrecognized magic), a guest argument starting with `-`, guest arguments longer than 16 KiB together ([ounce-core #418](https://github.com/hffgzktbcc-blip/ounce-core/issues/418)), a malformed `OUNCE_HB_TIMEOUT_SECS`, or an unsupported NSO kind |
| `3` | ounce-core binary missing or not found |
| `4` | Execution timeout (ounce-core killed after 60 seconds, or `OUNCE_HB_TIMEOUT_SECS`) |
| `5` | ounce-hb could not start, wait on, or stage for ounce-core (host I/O error) |
| `10` | Unimplemented instruction in ounce-core |
| `11` | Memory access fault |
| `12` | Other CPU error reported by ounce-core |
| `13` | Guest called svcBreak |
| `14` | Guest called a system service ounce-core does not provide |
| `15` | CPU halted |
| `16` | Instruction budget exhausted before the guest exited |
| `17` | ounce-core could not load the program (loader error) |

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
