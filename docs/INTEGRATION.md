# Homebrew CI, Build & Editor Integration Snippets

Copy-paste configuration templates for running unencrypted Nintendo Switch homebrew through `ounce-hb` in local `cargo-nx` / `devkitA64` builds, GitHub Actions CI, `git bisect`, and VS Code.

> **Nominative-Use & Clean-Room Notice:** "Nintendo" and "Nintendo Switch" are registered trademarks of Nintendo Co., Ltd. and Nintendo of America Inc.; "devkitPro", "devkitA64", and "libnx" belong to their respective open-source projects. These names are used strictly for compatibility identification (nominative use). **Never** redistribute `devkitPro`/`libnx` binaries, console keys, firmware, or retail assets in repositories or CI artifacts.

> **Version note:** The [published v0.2.0 release](https://github.com/hffgzktbcc-blip/ounce-hb/releases/tag/v0.2.0) includes `run`, `doctor`, `repro`, `--auto`, `bisect` and the newer `--timeout`, `--json`, `--cpu`, `--sdmc`, `--expect-hash` and `--expect-stdout` options. For future releases, check the [latest release notes](https://github.com/hffgzktbcc-blip/ounce-hb/releases/latest) rather than assuming every change on `main` is published.

---

## 1. `cargo-nx` (`.cargo/config.toml`)

Configure Cargo to run compiled `.nro` binaries with `ounce-hb` when targeting `aarch64-nintendo-switch-freestanding`:

```toml
# .cargo/config.toml
[target.aarch64-nintendo-switch-freestanding]
runner = "ounce-hb run --backend core --timeout 30"
```

Notes:
- Keep runner options before the `.nro` path. `ounce-hb` forwards guest arguments (including dashed flags) verbatim after `--` and refuses unexpected arguments given without `--` (exit 2).
- Add `--expect-stdout "PASS"` or `--expect-hash fnv1a64:...` if you want `cargo run` / `cargo nx run` to assert expected output automatically.

---

## 2. `devkitA64` `Makefile` `test` Target

Add a `test` target alongside your standard `devkitPro` / `libnx` `Makefile` rules to execute the compiled `$(TARGET).nro` headlessly and verify stdout, framebuffer hashes, and saved `ounce.repro.v1` bundles:

```makefile
OUNCE_HB     ?= ounce-hb
OUNCE_CPU    ?= tier4
TEST_TIMEOUT ?= 30

.PHONY: test test-gfx test-repro

# Console/text homebrew test: assert exit 0 and expected marker on stdout
test: $(TARGET).nro
	$(OUNCE_HB) run --cpu $(OUNCE_CPU) --timeout $(TEST_TIMEOUT) \
		--expect-stdout "PASS" $(TARGET).nro

# Graphical homebrew test: assert exact pre-HUD framebuffer_hash (fnv1a64)
test-gfx: $(TARGET).nro
	$(OUNCE_HB) run --cpu $(OUNCE_CPU) --timeout $(TEST_TIMEOUT) \
		--expect-hash fnv1a64:3a00ccf6b6f28ea1 $(TARGET).nro

# Replay committed regression manifests
test-repro:
	$(OUNCE_HB) repro tests/repro
```

To hunt down a regression in your homebrew repository using `git bisect`:

```sh
git bisect start HEAD <known-good-commit>
git bisect run ounce-hb bisect tests/repro/regression.repro.json
```

`ounce-hb bisect` wraps `ounce-hb repro` and maps infrastructure or refusal exits (`2`–`5`, such as an unbuilt commit or timeout) to exit code `125` (`git bisect skip`), while passing (`0`) and regression mismatches (`1`) drive `git bisect` cleanly.

---

## 3. GitHub Actions Starter Workflow (`setup-ounce`)

Use the composite `setup-ounce` action from `hffgzktbcc-blip/ounce-hb` (pin a release tag or commit SHA in production workflows):

```yaml
# .github/workflows/homebrew-test.yml
name: Homebrew CI

on:
  push:
    branches: [main]
  pull_request:

jobs:
  test-nro:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      # Install devkitA64 / cargo-nx in your build step without committing toolchain binaries.
      # Example build output: build/app.nro
      - name: Build homebrew NRO
        run: make

      # Pin a commit SHA for production workflows.
      - name: Install Ounce tools
        uses: hffgzktbcc-blip/ounce-hb/.github/actions/setup-ounce@1ccdfc95259d50e6ca7b6ff660da20138994231a
        with:
          ounce-hb-version: v0.2.0
          ounce-core-version: v0.10.0
          install-runtime: 'false'

      - name: Run homebrew NRO headlessly
        run: |
          OUNCE_HB_TIMEOUT_SECS=30 ounce-hb run --backend core build/app.nro
```

These example tags are intentionally pinned to the releases available on 20 November 2026; they are not a substitute for checking [the latest HB release](https://github.com/hffgzktbcc-blip/ounce-hb/releases/latest) and [Core release](https://github.com/hffgzktbcc-blip/ounce-core/releases/latest). If a new command only exists on `main`, install a reviewed source commit explicitly and set `install-hb: 'false'`; do not present unreleased code as a tagged feature.

---

## 4. VS Code `.vscode/tasks.json` + `problemMatcher` (Zero-Extension Crash Navigation)

This lightweight `.vscode/tasks.json` bundle runs `ounce-hb` and attaches a `problemMatcher` that turns symbolicated crash lines (`file:line` or `file:line:col`) into clickable links in VS Code's terminal and Problems panel without any custom extension.

Because `ounce-hb` reports raw guest fault addresses (`at address 0x...` or `Break PC: 0x...`) when an `.elf` is not supplied, the `Run & symbolicate crashes` task resolves `0x...` addresses against your unstripped `.elf` using `aarch64-none-elf-addr2line` when `$DEVKITPRO` is available:

```json
{
  "version": "2.0.0",
  "tasks": [
    {
      "label": "ounce-hb: run NRO",
      "type": "shell",
      "command": "ounce-hb run --cpu tier4 --timeout 30 build/app.nro",
      "group": {
        "kind": "test",
        "isDefault": true
      },
      "problemMatcher": {
        "owner": "ounce-hb",
        "fileLocation": ["autoDetect", "${workspaceFolder}"],
        "pattern": {
          "regexp": "^(?:.*\\bat\\s+|.*->\\s*)?([^\\s:()\"']+\\.(?:c|cc|cpp|cxx|h|hpp|rs|s|S)):(\\d+)(?::(\\d+))?(?::?\\s*(.*))?$",
          "file": 1,
          "line": 2,
          "column": 3,
          "message": 4
        }
      }
    },
    {
      "label": "ounce-hb: run NRO and symbolicate crashes",
      "type": "shell",
      "command": "set -o pipefail; ounce-hb run --cpu tier4 --timeout 30 build/app.nro 2>&1 | awk -v elf='build/app.elf' -v a2l=\"${DEVKITPRO}/devkitA64/bin/aarch64-none-elf-addr2line\" '{ print; addr = \"\"; if (match($0, /at address 0x[0-9a-fA-F]+/)) addr = substr($0, RSTART + 11, RLENGTH - 11); else if (match($0, /Break PC: 0x[0-9a-fA-F]+/)) addr = substr($0, RSTART + 10, RLENGTH - 10); if (addr != \"\") { cmd = a2l \" -e \" elf \" -f -p \" addr; if ((cmd | getline loc) > 0 && loc != \"\" && loc !~ /\\?\\?:0/) print \"  at \" loc \": \" $0; close(cmd) } }'",
      "problemMatcher": {
        "owner": "ounce-hb",
        "fileLocation": ["autoDetect", "${workspaceFolder}"],
        "pattern": {
          "regexp": "^(?:.*\\bat\\s+|.*->\\s*)?([^\\s:()\"']+\\.(?:c|cc|cpp|cxx|h|hpp|rs|s|S)):(\\d+)(?::(\\d+))?(?::?\\s*(.*))?$",
          "file": 1,
          "line": 2,
          "column": 3,
          "message": 4
        }
      }
    },
    {
      "label": "ounce-hb: replay repro bundles",
      "type": "shell",
      "command": "ounce-hb repro tests/repro",
      "problemMatcher": []
    }
  ]
}
```

How clicking a crash line works in VS Code:
1. Guest panic/assert lines formatted as `src/main.rs:42:5: panic` or `at source/main.c:88` match immediately.
2. Fault diagnostics (`at address 0x0000000080000080` or `Break PC: 0x000000008000008C`) are resolved by `aarch64-none-elf-addr2line` into `  at source/main.c:88: ...`, which populates the **Problems** panel and Ctrl/Cmd-clicks directly to the faulting source line.
