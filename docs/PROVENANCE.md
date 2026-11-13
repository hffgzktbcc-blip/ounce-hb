# Provenance register

Every binary file in this repository (test fixtures, images, anything that is not text) must
be built from committed source with a pinned toolchain and listed here. CI
(`scripts/guardrails.sh`) fails on any tracked or untracked binary file that is not registered.

Add one table row per file. The first column is the repo-relative path in backticks.

| Path | SHA-256 | Built from (source path and commit) | Toolchain (name and pinned version) | Build command | License of source |
|---|---|---|---|---|---|
| `docs/assets/family-banner.jpg` | 002389e90ad3b5622a112a29edab875bdfcbbf54934a66a5954aef216c420b90 | User-provided community infographic banner | N/A (Image) | Generated graphic | CC-BY-4.0 |

<!-- Entries go above this line. The register is currently empty: this repo has no binary files. -->
