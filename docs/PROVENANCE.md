# Provenance register

Every binary file in this repository (test fixtures, images, anything that is not text) must
be built from committed source with a pinned toolchain and listed here. CI
(`scripts/guardrails.sh`) fails on any tracked or untracked binary file that is not registered.

Add one table row per file. The first column is the repo-relative path in backticks.

| Path | SHA-256 | Built from (source path and commit) | Toolchain (name and pinned version) | Build command | License of source |
|---|---|---|---|---|---|
| `docs/assets/family-banner.jpg` | 002389e90ad3b5622a112a29edab875bdfcbbf54934a66a5954aef216c420b90 | User-provided community infographic banner | N/A (Image) | Generated graphic | CC-BY-4.0 |
| `tests/fixtures/libnx_hello/libnx_hello.nro` | 9abd6a44cd7a57c9276e8a280a636842c442ee21bc8f6392ad56595b9e825e98 | Original clean-room source: `tests/fixtures/libnx_hello/main.c`, commit `dce0365` ([source](https://github.com/hffgzktbcc-blip/ounce-hb/tree/dce0365/tests/fixtures/libnx_hello)) | devkitA64 r30-1; GCC 16.1.0-1; binutils 2.46.0-1; newlib 4.6.0.20260123-4; libnx 4.12.0-1; elf2nro from switch-tools 1.13.1-1 | `DEVKITPRO=/opt/devkitpro make -C tests/fixtures/libnx_hello clean all` | MIT (fixture source); libnx ISC, newlib permissive notices, GCC runtime exception (linked runtime) |

The libnx fixture uses original MIT source committed alongside its build recipe; no external
sample source or emulator source was used. `check-toolchain.sh` requires the exact installed
package releases above. elf2nro has no version flag; its pinned package is switch-tools
1.13.1-1. The NRO contains no icon, NACP, RomFS, debug paths, or timestamp metadata.
Two clean builds on the pinned toolchain produced the SHA-256 above.

The program initializes and updates the framebuffer console, prints a fixed greeting,
echoes guest arguments (excluding argv[0], whose staged path varies), and returns zero.
It mirrors text to libnx's `debugDevice_SVC` stream so ounce-core captures it on host stdout.
The fixture license is also in `tests/fixtures/libnx_hello/LICENSE`.

Validation runs with `cargo test --locked --test libnx_fixture` even without a backend;
there is no CLI dry-run mode. Set `OUNCE_CORE_BIN` to a v0.8.1 binary to additionally
check successful execution with and without guest arguments. This is one fixture's result,
not a general libnx compatibility claim.

<!-- Entries go above this line. -->
