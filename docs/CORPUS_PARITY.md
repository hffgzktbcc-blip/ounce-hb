# Corpus parity: ounce-core CLI versus ounce-hb

`scripts/corpus_parity.py` checks that `ounce-hb run` reports what `ounce-core run-nro` reports, for each
program in ounce-core's outside-program corpus. It is a subprocess comparison. It is not a frame or
pixel check, and a `match` is not a pass of the corpus oracles.

## What is compared

For each corpus program whose NRO is on disk, the script runs it two ways:

1. **direct**: `ounce-core run-nro <nro> --present=headless`, with `OUNCE_CORE_BIN` set to the binary
   under test.
2. **via ounce-hb**: `ounce-hb run <nro>`. ounce-hb runs the same `OUNCE_CORE_BIN`.

Both runs must agree on:

- **Stop category.** The direct output is classified by the rules in [DIAGNOSIS.md](DIAGNOSIS.md)
  (the stop lines it lists, matched only when the exit status fits). The category's exit code is read
  from the table in the same file. ounce-hb's exit code must equal it.
- **stdout**, after the normalization below.
- **stderr**, after the same normalization and with host audio noise removed. For a clean exit the two
  must be identical. For a stopped run, the direct stderr must be a prefix of ounce-hb's stderr, because
  ounce-hb appends its explanation after the guest's own output.

Verdicts:

| Verdict | Meaning |
|---|---|
| `match` | Both paths agree on category, stdout and stderr. |
| `differ` | Both ran and disagree on one of the above. The reasons say which. |
| `error` | A run did not finish within the time limit, or could not start. |
| `missing` | The NRO is not on disk, so nothing was run. |

## What is not compared

- **The first-frame oracles** in `tests/corpus/oracles.json`: frame hashes, font hashes and pixel
  probes. The `run-nro` CLI does not expose first-frame hash oracles or frame data, so the subprocess
  path cannot check a frame. Nothing here shows a frame matches its oracle, and this is not oracle parity.
- **Display, audio and input.** Only what the CLI prints on stdout and stderr is compared.
- **Anything after the instruction budget.** Most corpus programs stop on the default budget of 50
  million instructions (see Limits). The comparison covers only the output produced before that stop.

## Normalization

These fields are masked before comparison. Each one was checked against raw output:

| Field | Why |
|---|---|
| NRO path, as given and as ounce-hb's staged copy | ounce-hb runs a private copy under a temporary directory, and the CLI prints the path it loaded. |
| Instruction count, wall-clock time and rate on `[VM Summary] Executed` | The count varies between runs on programs whose execution depends on host timing. Measured: one applet gave 16556877 and 16557482 instructions on two direct runs of the same file and path. The budget line still gives the exact budget and is compared. |
| `Basic-Block JIT Cache` counters | Vary between runs of the same binary. |
| `[VM Summary] Memory Buckets` sizes | Vary between runs of the same binary. |
| `Final X0` register wherever printed | Depends on the path the loader saw. Measured: the same bytes gave X0 = 32, 64 and 80 for three path lengths and names. |
| On stderr only: lines starting `ALSA lib ` or `[HostAudioSink] ` | Host audio noise from ounce-core on machines with no sound device (docs/DIAGNOSIS.md, "Host audio noise"). Exit codes are still compared. |

Each masked field is listed in the report under `normalized_fields`.

## Running it

Needs an ounce-core checkout at `v0.8.1`, a built `ounce-core` binary, a built `ounce-hb` binary, and the
corpus NROs under `<builds-dir>/<build>/build/source/<nro>`. Those NROs are built from pinned upstream
sources with devkitA64; see ounce-core's `scripts/prepare_external_corpus.py`. They are not in the
repository or in the ounce-core release.

```sh
cargo build --release --locked   # builds target/release/ounce-hb
OUNCE_CORE_BIN=/path/to/ounce-core python3 scripts/corpus_parity.py \
  --ounce-core-dir /path/to/ounce-core \
  --builds-dir /path/to/builds \
  --report corpus-parity-report.json
```

Options that matter:

- `--write-baseline PATH` writes a new baseline from this run. Use it only on a real run.
- `--require-present` counts a missing NRO as a regression when the baseline recorded a match.
- `--program ID` (repeatable) compares only the named programs.
- `--timeout SECONDS` sets the limit for each run. It defaults to 60, the same as ounce-hb's default. When
  it is raised, set `OUNCE_HB_TIMEOUT_SECS` to the same value so both paths get the same limit.

The unit tests run without any NRO or devkitA64 toolchain:

```sh
python3 -m unittest discover -s tests/parity -v
```

## Baseline and exit status

`tests/parity/baseline.json` records each program's verdict from one real run. A program recorded as
`match` must keep matching. The script exits `1` when a recorded match stops matching, and `2` on a
configuration error (missing `OUNCE_CORE_BIN`, no baseline, an unreadable exit-code table). The other
verdicts are recorded but not enforced. A `missing` NRO counts only with `--require-present`.

## Nightly

`.github/workflows/nightly-parity.yml` runs daily and on demand. It checks out ounce-core at `v0.8.1`,
downloads its Linux release binary, and verifies the sha256 against both a pinned checksum (matching
`ounce-core-e2e.yml`) and the release's `SHA256SUMS`. It builds `ounce-hb`, runs the unit tests and the
comparison, and uploads the report. The job does not build the
corpus NROs, so every program without a prebuilt NRO is reported as `missing` and the job stays green.
Adding a corpus build step to the job is the next step.

## Evidence

Recorded on 2026-11-13T15:06:57+00:00 on macOS arm64, using the checksum-verified ounce-core
`v0.8.1` macOS release and its clean tagged corpus registry
(`0ae63588136efcc9b8c005022707b9e9d5077629`). ounce-hb was built in release mode from `main` at
`431c51ae39a36d51fc9922f0e8f1299386b4e4d8` (including PR #26 and PR #28). Both paths used the
default 50-million-instruction budget and a 60-second wall-clock limit per run.

| Verdict | Programs |
|---|---:|
| `match` | 44 |
| `differ` | 0 |
| `error` | 2 |
| `missing` | 4 |
| **Total** | **50** |

The 44 matches comprise 42 budget-exhausted stops, one memory fault and one `svcBreak`.
None of these matches establishes that a first frame was rendered correctly.

Every non-match is listed here:

| Program | Verdict | Reason |
|---|---|---|
| `deko-console` | `error` | direct run timeout: no exit within 60s; via ounce-hb run timeout: no exit within 60s. |
| `deko-examples-menu` | `missing` | NRO not present under the builds directory; not run. |
| `sdl2-simple` | `missing` | NRO not present under the builds directory; not run. |
| `nx-hbmenu` | `missing` | NRO not present under the builds directory; not run. |
| `hybrid-app` | `missing` | NRO not present under the builds directory; not run. |
| `deko-basic` | `error` | direct run timeout: no exit within 60s; via ounce-hb run timeout: no exit within 60s. |

`deko-console` still times out on both paths with the process-group cleanup fix present; this
run does not establish that the earlier timeout is fixed. `deko-basic` also exceeds this run's
60-second limit. The four missing NROs were absent from the supplied builds directory and were not
executed. These are recorded observations, not passing programs.

Binary SHA-256 values (the archive checksum is separately pinned in `ounce-core-e2e.yml`):

- ounce-core: `5d36eebca7c494b2d836f87e693dd07a277c3e13965182642680996ff517cb5a`.
- ounce-hb: `3650046bb4c7146b211fe18614448952e818b49f0deea6368536fb77efca3278`.

The machine-readable verdicts and generation metadata are in
[`tests/parity/baseline.json`](../tests/parity/baseline.json). The full local report records per-program
NRO hashes, normalized console hashes, timings, and reasons; generate it with `--report` as above.
Raw stdout/stderr for non-matches are written beside the report and can contain local paths; they
are not committed.

## Limits

- **Instruction budget.** Both paths stop at the default budget of 50 million instructions unless
  `OUNCE_NRO_BUDGET` is set. For budget-exhausted runs, the compared output is only what the program
  printed in its first 50 million instructions; the stop does not establish whether a frame was
  presented. Raising `OUNCE_NRO_BUDGET` for both paths would compare more output, at a higher run time
  per program.
- **Not built here.** The programs listed as `missing` in the evidence table above have no NRO and
  were not evaluated.
- **Platform.** The baseline comes from one macOS arm64 run. The nightly runs on Linux x86_64. Verdicts
  have not yet been compared across platforms.
- **Run time.** A run that takes longer than the limit is `error`, not `differ`. The evidence table
  records every timeout explicitly.
