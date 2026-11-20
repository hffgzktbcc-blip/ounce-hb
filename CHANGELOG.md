# Changelog

All notable changes to ounce-hb will be documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

---

## [0.2.0] - 2026-11-20

### Added
- **Engine & mount flags (`--cpu`, `--sdmc`)**: `ounce-hb run`, `repro`, `bisect`, and `diff-trace` accept `--cpu <interp|tier3|tier4>` and `--sdmc <dir>`, forwarding `--cpu=<interpreter|tier3|tier4>` and `--sdmc=<dir>` to the backend and recording both in `ounce.repro.v1` bundles (#68).
- **Per-run timeout & machine-readable output**: `ounce-hb run --timeout <secs>` overrides `OUNCE_HB_TIMEOUT_SECS` (exit `4` on timeout), and `ounce-hb run --json` prints one `ounce.hb.run.v1` object on stdout (#45).
- **Doctor toolchain checks**: `ounce-hb doctor` (text and `--json`) reports `DEVKITPRO`, `devkitA64`, and `cargo-nx` availability without altering its exit code (#45).
- **Framebuffer hash & frame-count parity**: `ounce.run.v1` parsing extracts `frames_presented` and `framebuffer_hash`; `ounce.repro.v1` and `ounce-hb repro` record and verify both per engine, and `ounce-hb run --auto` escalates and reports `Runtime Divergence` (writing `ounce-repro.json`) when both engines emit differing non-null `framebuffer_hash` values (#50, #69).
- **CI assertions & bisection**: `ounce-hb run --expect-hash <hash>` and `--expect-stdout <substring>` assert golden outputs directly on `run` (exiting `1` on mismatch), and `ounce-hb bisect` wraps `repro` for `git bisect run` (mapping infrastructure/refusal exits `2`–`5` to `125`) (#70).
- **Differential tracing & CI summaries**: `ounce-hb diff-trace` compares `ounce.trace.v1` (`OUNCE_TRACE_SUMMARY`) summaries between `ounce-runtime` and `ounce-core` (`0` on agreement, `1` with first differing step/field), and `run --auto`, `repro`, and `diff-trace` append a Markdown divergence table to `$GITHUB_STEP_SUMMARY` when set (#71).
- **Embedded RomFS detection**: `ounce_hb::has_embedded_romfs` detects trailing `ASET` asset sections with non-zero embedded RomFS data in NRO images, with missing-RomFS `svcBreak` diagnostics documented in `docs/DIAGNOSIS.md` and `docs/LIMITS.md` (#71).
- **Dashed guest argument forwarding (`--`)**: `Subprocess::run_nro` forwards `-- <guest args>` to the backend when any guest argument after `--` starts with `-` (e.g. `ounce-hb run app.nro -- --max-frames 5`), while continuing to refuse unexpected or dashed arguments given without `--` with exit code `2` (#72, closes #64).
- **Supply-chain hardening (Q3)**: Added `scripts/check_action_pins.py` (enforced in `ci.yml`, verifying all 26 GitHub Actions `uses:` references are pinned to 40-char commit SHAs with `# vX.Y.Z` comments) and pure-stdlib SPDX 2.3 SBOM generation (`scripts/generate_sbom.py`, wired into `release.yml` and covered by `SHA256SUMS`) (#72).
- **Developer integration docs**: Added `docs/INTEGRATION.md` with copy-paste snippets for `cargo-nx`, `devkitA64` Makefiles, GitHub Actions (`setup-ounce`), and VS Code `tasks.json` + `problemMatcher` (#70).

### Changed
- **JSON parser hardening**: Replaced the flat substring scan in `src/report_v1.rs` (`ReportV1::parse` and `ReproManifest::parse`) with a zero-dependency, `#![forbid(unsafe_code)]` recursive-descent JSON parser supporting signed `i32` `exit_code`, nested objects, string escapes, string arrays, booleans, and `null` (#69).
- **CI ergonomics**: Added job timeouts (#65), `help wanted` self-assignment in the claim bot (#66), and a docs-only fast path in `ci.yml` (#67).

---

## [0.0.1] - 2026-11-13

First tagged release: milestone M0. It runs unencrypted Switch homebrew through the released `ounce-core`
command-line tool and explains how each run stopped. It is a thin, tested layer, not an emulator of its own.
Tested and verified against ounce-core v0.8.1 (and backward-compatible with v0.8.0).

### Added
- `ounce-hb run <app.nro> [-- guest args...]`: classify the file, then run it through the `ounce-core` binary
  (found through `OUNCE_CORE_BIN`, then `PATH`) in headless mode, with a wall-clock timeout (60 s by default,
  `OUNCE_HB_TIMEOUT_SECS` to change it) and the program's output forwarded.
- Plain-words explanation of how a run stopped, with the command to reproduce it and a stable, documented exit
  code per category ([docs/DIAGNOSIS.md](docs/DIAGNOSIS.md)). Backed by real runs against ounce-core v0.8.1 on
  Linux, macOS and Windows (18 end-to-end tests per OS, none skipped), except exit code 15 (unreachable from a
  homebrew program; the proof is in the document) and exit code 1, which is covered only by killing a run.
- Hardened subprocess runner: process group termination on timeout (`kill -- -pgid` on Unix, `taskkill /T` on Windows),
  grace periods preventing leaked child hangs, 8 MiB output capping per stream protecting host memory RSS,
  parent death watch, robust stop classification distinguishing guest debug strings on stdout from runner errors on
  stderr, and graceful handling of closed stdout/stderr pipes without panicking (exit 101).
- Format classification of `NRO0` and `NSO0`, and refusal of retail and system formats (`PFS0`, `HEAD`, `NCA0/2/3`,
  `META`). NSO files are recognised but not executed.
- Guest-argument checks: arguments starting with `-` and argument sets over 16 KiB are refused before ounce-core
  starts. ounce-core v0.8.1 enforces its own 62,855-byte limit; ounce-hb maintains the conservative 16 KiB limit.
- Clean-room and provenance guardrails (`scripts/guardrails.sh`), a binary provenance register,
  `#![forbid(unsafe_code)]`.
- The Ounce family overview table in README documenting roles, licences, and relationships between `ounce-core`,
  `ounce-hb`, and `ounce-runtime`.
- CI on Linux, macOS and Windows, licence and vulnerability checks, Dependabot, draft-only releases with provenance
  attestation, wiki publishing from `docs/wiki/`, and scope policy automation that flags and labels out-of-scope requests
  (closing/locking behind `POLICY_AUTOCLOSE=true`).
- Community files, issue forms, a roadmap and a wiki.

### Known limits
- No in-process `ounce-core` library backend (parked, see [issue #8](https://github.com/hffgzktbcc-blip/ounce-hb/issues/8)).
- No recorded corpus parity yet ([issue #7](https://github.com/hffgzktbcc-blip/ounce-hb/issues/7)).
- No NSO execution, graphics or input beyond what the ounce-core CLI provides in headless mode.
