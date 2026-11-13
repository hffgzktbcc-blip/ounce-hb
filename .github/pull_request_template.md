## Summary
<!-- Explain what changed and why. Reference any relevant issue numbers. -->

## Motivation & Context
<!-- Why is this change necessary? What problem does it solve? -->

## Checks Run
<!-- State which commands you ran locally and their results. -->
- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets --locked -- -D warnings`
- [ ] `cargo test --workspace --locked`
- [ ] `scripts/guardrails.sh --self-test && scripts/guardrails.sh`

## Clean-Room & Rules Checklist
- [ ] **Clean room**: No code derived from, adapted from, or informed by other Switch emulators (Yuzu, Ryujinx, Suyu, etc.)
- [ ] **Homebrew only**: No keys, firmware, retail game assets, encrypted containers, or DRM circumvention
- [ ] **Provenance**: Any added binary file is built from committed source with a pinned toolchain and registered in `docs/PROVENANCE.md`
- [ ] **Safe Rust**: `#![forbid(unsafe_code)]` remains strictly enforced
- [ ] **Documentation**: Relative documentation links resolve and no claims exceed repository evidence

## AI Assistance Disclosure
<!-- Disclose any AI tools used (e.g., Claude, Copilot, Codex) and what they did, or state "None". -->

## Limitations & Edge Cases
<!-- State any untested edge cases, known limitations, or missing upstream features. -->
