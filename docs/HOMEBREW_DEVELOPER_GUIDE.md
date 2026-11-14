# Homebrew Developer Validation Guide

`ounce-hb` is a turn-key, clean-room homebrew test runner and diagnostic CLI. Homebrew authors can use it to validate unencrypted `.nro` builds locally in CI or on a workstation without needing console hardware, firmware, or keys.

---

## 1. Run & Diagnose Your `.nro` Locally

### Safe-Rust Verification (`--backend core`)
Use the default `ounce-core` backend when you want deterministic execution, bounded instruction/time budgets, and safe-Rust (`#![forbid(unsafe_code)]`) fault reporting:

```sh
ounce-hb run --backend core --timeout 10s path/to/your_homebrew.nro
```

### High-Speed Runtime Verification (`--backend runtime`)
If you have `ounce-runtime` installed, run the exact same `.nro` through the fast JIT / Apple Silicon NCE backend:

```sh
ounce-hb run --backend runtime --timeout 10s path/to/your_homebrew.nro
```

Both backends enforce identical clean exit (`0`) vs. non-zero fault exit codes and print clear diagnostic banners on abnormal termination.

---

## 2. Diagnostic Guarantees & Fault Categories

When a homebrew binary fails, `ounce-hb` preserves guest stdout/stderr and prints an actionable diagnostic banner with a reproducible command:

| Fault Condition | Behavior |
| :--- | :--- |
| **Clean Exit** | Returns exit code `0` with full guest stdout/stderr. |
| **CPU Breakpoint (`brk #0`)** | Cleanly refused with non-zero exit code and diagnostic banner. |
| **Undefined Instruction** | Cleanly refused with non-zero exit code and faulting PC/opcode context. |
| **Unmapped Memory Read/Write** | Cleanly refused with non-zero exit code and faulting address. |
| **Infinite Loop / Hang** | Bounded by `--timeout` (default watchdog), terminated cleanly without hanging CI. |

*(See [`docs/DIAGNOSIS.md`](DIAGNOSIS.md) for the complete exit-code table and protocol.)*

---

## 3. Integrating `ounce-hb` into Homebrew CI

Add a clean-room validation step to your GitHub Actions workflow to verify that your `.nro` builds cleanly, runs to completion without faulting, and produces expected output:

```yaml
- name: Verify homebrew NRO
  run: |
    ounce-hb run --backend core --timeout 15s build/app.nro
```

---

## 4. Clean-Room & Legal Boundary

- **Homebrew only**: `ounce-hb` only accepts unencrypted, open-source `.nro` binaries.
- **No keys, no firmware, no retail titles**: Protected retail containers and proprietary assets are strictly out of scope. See [`docs/CLEAN_ROOM.md`](CLEAN_ROOM.md).
