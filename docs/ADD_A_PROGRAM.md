# Adding & Testing a Homebrew Program

This guide explains how to build a clean-room Nintendo Switch homebrew program from source and
run it using ounce-hb.

---

## Prerequisites

1. **Rust Toolchain**: Stable Rust installed via `rustup` (see [../rust-toolchain.toml](../rust-toolchain.toml)).
2. **devkitPro & libnx**:
   - Install devkitPro following the instructions at [devkitpro.org](https://devkitpro.org/).
   - Ensure `devkitA64` and `libnx` are installed and the `DEVKITPRO` environment variable is set.
3. **ounce-core Binary**:
   - Download the v0.8.0 release binary from [hffgzktbcc-blip/ounce-core](https://github.com/hffgzktbcc-blip/ounce-core/releases)
     or compile it from source.
   - Add it to your `PATH` or set `export OUNCE_CORE_BIN=/path/to/ounce-core`.

---

## Step 1: Write a Deterministic Homebrew Program

Homebrew programs that print deterministic output to standard I/O make ideal test cases.

Here is a minimal example (`main.c`):
```c
#include <stdio.h>
#include <switch.h>

int main(int argc, char* argv[]) {
    consoleInit(NULL);
    printf("Hello from ounce-hb!\n");
    for (int i = 1; i < argc; i++) {
        printf("arg[%d]: %s\n", i, argv[i]);
    }
    consoleUpdate(NULL);
    consoleExit(NULL);
    return 0;
}
```

---

## Step 2: Build the NRO with devkitPro

Use devkitPro's standard application template:
```sh
# Using the standard devkitPro application Makefile:
make
```
This produces an unencrypted homebrew executable: `my_app.nro`.

Verify that the file begins with the unencrypted homebrew NRO format:
```sh
# Offset 0x10 must contain 'NRO0'
xxd -s 0x10 -l 4 my_app.nro
# Output: 00000010: 4e52 4f30  NRO0
```

---

## Step 3: Run with ounce-hb

Run the compiled NRO using the ounce-hb CLI:
```sh
cargo run -- run my_app.nro
```

To pass arguments to the guest program, use `--` followed by positional arguments:
```sh
cargo run -- run my_app.nro -- test alpha
```

> **Note**: Guest arguments starting with `-` are currently rejected by ounce-hb because upstream
> `ounce-core` silently discards them.

---

## Step 4: Adding Test Fixtures to the Repository

If you contribute a sample homebrew binary as an automated test fixture:

1. **Commit the Source**: The source code and build Makefile must be committed to the repository.
2. **Pinned Toolchain**: The build must specify the exact version of devkitPro/libnx used.
3. **Register in Provenance**: Add an entry for the binary in [PROVENANCE.md](PROVENANCE.md) detailing:
   - File path
   - SHA-256 hash
   - Source path and commit
   - Toolchain version
   - Exact build command
   - License of the source code
4. **Pass Guardrails**:
   ```sh
   scripts/guardrails.sh
   ```
   The script will fail if any binary is present that is not registered in `PROVENANCE.md`.

---

## Rules & Clean-Room Reminder

- Never commit retail game dumps, keys, or proprietary Nintendo SDK code.
- Test programs must use permissively licensed open-source code (e.g. MIT, BSD, ISC, zlib).
- See [../CONTRIBUTING.md](../CONTRIBUTING.md) and [CLEAN_ROOM.md](CLEAN_ROOM.md) for full policies.
