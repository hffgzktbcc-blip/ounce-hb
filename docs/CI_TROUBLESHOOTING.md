# CI & Runner Troubleshooting Guide

Quick fixes for the most common `ounce-hb` runner, workflow, and supply-chain check failures.

---

## 1. Runner & Host Exit Codes (`1`–`5`)

| Exit code | Category | Common cause & fix |
| --- | --- | --- |
| `1` | Unrecognised / assertion failed | Either `--expect-hash` / `--expect-stdout` did not match, `repro` / `diff-trace` found a mismatch, or the backend exited without a recognised stop line (e.g. killed by OS `SIGKILL` / OOM). Inspect the printed stdout/stderr. |
| `2` | Refused | Bad CLI flags, guest arguments or dashed flags given without `--`, guest arguments exceeding 16 KiB (`ounce-core #418`), guest arguments passed to an `.nso`, invalid `--timeout` / `OUNCE_HB_TIMEOUT_SECS`, missing `--sdmc` directory, malformed NSO0/LZ4, or out-of-scope container (`NSP`, `XCI`, `NCA`, `META`). |
| `3` | Backend missing | `OUNCE_CORE_BIN` (or `OUNCE_RUNTIME_BIN` for `--backend runtime`) is unset or does not point to an executable file, and no executable binary was found on `PATH`. Run `ounce-hb doctor --json` to inspect resolved paths. |
| `4` | Timeout | The guest exceeded the wall-clock limit (default `60` s). Pass `--timeout <secs>` or set `OUNCE_HB_TIMEOUT_SECS` (up to `86400`), or check whether the guest is waiting for interactive input / stuck in an infinite loop. `ounce-hb` kills the entire child process group (`kill -- -pgid` on Unix, `taskkill /T` on Windows) and cleans up the staged directory. |
| `5` | Host I/O | The backend binary exists but could not be executed (missing `+x` bit on Unix, wrong architecture, or non-PE file on Windows), or staging under `TMPDIR`/`TEMP` failed. |

---

## 2. Guest & Engine Stop Exit Codes (`10`–`17`)

| Exit code | Category | Common cause & fix |
| --- | --- | --- |
| `10` | Unimplemented instruction | The guest reached an ARM64 opcode not yet implemented by the engine. Check `ounce-core`'s instruction coverage or compare engines with `ounce-hb run --auto`. |
| `11` | Memory fault | Invalid guest memory read/write. Use the `To reproduce:` command and `aarch64-none-elf-addr2line` (see [INTEGRATION.md](INTEGRATION.md#4-vs-code-vscodetasksjson--problemmatcher-zero-extension-crash-navigation)) to map the faulting PC. |
| `12` | CPU error | Guest hit `BRK` or a CPU exception. Inspect the `Ounce Core said:` detail line. |
| `13` | `svcBreak` | Guest deliberately aborted via `svcBreak` (e.g. `libnx` assertion or `romfsInit()` failure on an engine without embedded RomFS). See [DIAGNOSIS.md](DIAGNOSIS.md#embedded-romfs-and-romfsinit-stops). |
| `14` | Unhandled SVC | Guest invoked a Horizon OS SVC not implemented by the engine. |
| `15` | CPU halted | Engine reported `[VM] CPU Halted`. |
| `16` | Budget exhausted | Guest exceeded `OUNCE_NRO_BUDGET` instructions without exiting. Raise `OUNCE_NRO_BUDGET` for long-running programs. |
| `17` | Loader error | Engine could not load the staged `.nro` or `.nso` (e.g. corrupt header or segment bounds). |

For full stop-line matching rules, see [DIAGNOSIS.md](DIAGNOSIS.md).

---

## 3. Workflow & Supply-Chain Checks

### Action SHA Pin Check (`scripts/check_action_pins.py`)
Every non-local `uses:` reference under `.github/workflows/` and `.github/actions/` must be pinned to a full 40-character commit SHA with a trailing version comment (`# vX.Y.Z`):

```sh
python3 scripts/check_action_pins.py
```

To resolve a release tag to its commit SHA:
```sh
git ls-remote https://github.com/<owner>/<repo> refs/tags/<tag> "refs/tags/<tag>^{}"
```
Use the `^{}` commit hash when present (annotated tag).

### SPDX 2.3 SBOM & `SHA256SUMS` (`scripts/generate_sbom.py`)
Release builds generate a deterministic SPDX 2.3 JSON SBOM per target triple (`ounce-hb-<target>.spdx.json`), covered by `SHA256SUMS`. To test locally:

```sh
python3 scripts/generate_sbom.py --output /tmp/ounce-hb.spdx.json
python3 -m unittest discover -s scripts -p 'test_*.py' -v
```

### Guardrails & Binary Provenance (`scripts/guardrails.sh`)
`scripts/guardrails.sh` fails on:
- Any binary file not registered with its SHA-256, source, pinned toolchain, and build command in [PROVENANCE.md](PROVENANCE.md).
- Prohibited extensions (`.nsp`, `.xci`, `.nca`, `.keys`, `.firmware`) or forbidden key/decryption patterns.

Run both the self-test and repository scan locally before pushing:
```sh
scripts/guardrails.sh --self-test && scripts/guardrails.sh
```

### Markdown Links & Public Docs Check
- Broken relative Markdown links: run `python3 scripts/check_links.py`.
- Public docs impact (`public-docs.yml`): when a PR changes `src/` or `Cargo.toml`, update affected public docs (`README.md`, `CHANGELOG.md`, `docs/*.md`) in the same PR, or include a reasoned `Docs: none — <reason>` line in the PR description.
