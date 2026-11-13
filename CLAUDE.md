# Claude Code

@AGENTS.md

## Fast Verification
```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
scripts/guardrails.sh --self-test && scripts/guardrails.sh
```
