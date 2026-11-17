# Quick start

ounce-hb runs an unencrypted homebrew `.nro` through an engine binary and
turns the stop into a stable exit code. It does not contain the engine.
`ounce-core` is MIT. `ounce-runtime` is GPL-3.0-or-later and stays a separate
process.

The commands on this page are on `main` after v0.0.2 (14 Nov 2026). The
published v0.0.2 archive has `run` only: no `doctor`, no `repro`, no `--auto`.

## Build

```sh
git clone https://github.com/hffgzktbcc-blip/ounce-hb.git
cd ounce-hb
cargo build --release --locked
```

Put `ounce-core` on `PATH`, or:

```sh
export OUNCE_CORE_BIN=/path/to/ounce-core
```

Optional runtime binary: `OUNCE_RUNTIME_BIN` or an `ounce-runtime` executable
on `PATH`.

## Run

```sh
./target/release/ounce-hb run path/to/app.nro
./target/release/ounce-hb run path/to/app.nro --backend core -- arg1 arg2
```

There is no `--timeout` flag and no `--json` flag on `run`. A word in that
position, including `--timeout`, is refused with exit 2. The wall clock limit
is 60 seconds, or `OUNCE_HB_TIMEOUT_SECS` (a number from 1 to 86400). Guest
arguments go after `--` and must not start with `-`.

```sh
OUNCE_HB_TIMEOUT_SECS=15 ./target/release/ounce-hb run path/to/app.nro
```

## Doctor

```sh
./target/release/ounce-hb doctor
./target/release/ounce-hb doctor --json
```

Text output names the host, whether AArch64 hardware replay is available
(Linux or macOS on aarch64), and the resolved `ounce-core` and `ounce-runtime`
binaries. JSON is `"schema": "ounce.doctor.v1"`. Exit 0 when at least one
engine is found, exit 3 when neither is. This does not check `DEVKITPRO`,
`devkitA64`, or `cargo-nx`.

## Auto

```sh
./target/release/ounce-hb run --auto path/to/app.nro
```

`--auto` or `--backend auto` runs `ounce-runtime` when that binary exists, and
otherwise runs `ounce-core`. If the runtime report has `silent_fallback` or
the diagnosed exit is 10 through 15, ounce-hb runs ounce-core as well. The
same diagnosis is a guest bug. A different diagnosis is a runtime divergence,
and ounce-hb writes `ounce-repro.json` (`ounce.repro.v1`).

The two engines do not yet label every fault with the same `stop_kind`. Read
the bundle before treating a divergence line as an engine bug.

## Repro

```sh
./target/release/ounce-hb repro ounce-repro.json
./target/release/ounce-hb repro --backend core path/to/directory
```

A directory is every `*.repro.json` and `ounce-repro.json` inside it, sorted.
The selected backend (default `core`) must reproduce the recorded `stop_kind`
and `exit_code`. Exit 0 if all match, 1 if any do not, 2 if the input is
missing or not a valid manifest. When the manifest's `core` section records
`framebuffer_hash` and/or `frames_presented` (ounce-core builds after v0.9.0 write them for runs that
present frames; v0.9.0 itself does not), those must match too; a missing actual hash counts as a mismatch. Manifests
without them are checked as before. `run --auto` does not compare hashes yet.

## GitHub Action

`setup-ounce` in this repository installs release archives and then runs
`ounce-hb doctor --json`.

```yaml
- uses: hffgzktbcc-blip/ounce-hb/.github/actions/setup-ounce@<commit>
  with:
    ounce-hb-version: v0.0.2
    ounce-core-version: v0.8.2
    install-runtime: 'false'
```

Do not point this at the published v0.0.2 ounce-hb archive and expect the
doctor step to pass: that archive has no `doctor` subcommand. Use a commit
you have built, or wait until v0.1.0 is tagged. On `main` the action installs
`linux-x86_64`, macOS arm64, and Windows x86_64. It checks checksums for
ounce-hb and ounce-core. It does not check the ounce-runtime checksum, it has
no linux-arm64 archive, and the release workflow still triggers on every `v*`
tag. Those items are open in pull request 40. Do not publish a floating
`setup-ounce@v1` tag until that trigger change is on `main`.

`install-runtime: 'true'` downloads the GPL-3.0-or-later `ounce-runtime`
archive as a separate binary. ounce-hb does not link it.
