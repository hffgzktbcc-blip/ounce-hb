#!/usr/bin/env python3
"""Compare the ounce-core CLI with ounce-hb on the ounce-core outside-program corpus.

Each corpus program in <ounce-core-dir>/tests/corpus/oracles.json whose NRO is present under
<builds-dir>/<build>/build/source/ is run twice:

  direct   $OUNCE_CORE_BIN run-nro <nro> --present=headless
  via-hb   <ounce-hb> run <nro>      (OUNCE_CORE_BIN is inherited, so ounce-hb runs the same binary)

The direct output is classified into a stop category by the rules in docs/DIAGNOSIS.md. The
category's exit code is read from the table in the same file. The via-hb run must exit with that
code, its stdout must match the direct stdout, and its stderr must begin with the direct stderr
(ounce-hb appends its explanation after the guest's own output). Console text is normalized first;
see NORMALIZE below. docs/CORPUS_PARITY.md says what this does and does not show.

Verdicts: match (both paths agree), differ (both ran and disagree), error (a run timed out or could
not start) and missing (no NRO on disk, so nothing was run).

Exit status: 0 when no program recorded as "match" in the baseline has stopped matching, 1 when one
has, 2 on a configuration error. A missing NRO counts only with --require-present.

Standard library only. Python 3.9 or newer.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import re
import signal
import subprocess
import sys
import time
from dataclasses import dataclass
from datetime import datetime, timezone
from pathlib import Path, PurePosixPath
from typing import Dict, List, Optional, Sequence, Tuple

SCHEMA_VERSION = 1
REPO = Path(__file__).resolve().parents[1]
DEFAULT_OUNCE_HB = REPO / "target" / "release" / ("ounce-hb" + (".exe" if os.name == "nt" else ""))
DEFAULT_BASELINE = REPO / "tests" / "parity" / "baseline.json"
DEFAULT_EXIT_CODES = REPO / "docs" / "DIAGNOSIS.md"
# Same limit as ounce-hb's own DEFAULT_TIMEOUT, so a slow program is not judged by two limits.
DEFAULT_TIMEOUT = 60.0
CORE_BIN_ENV = "OUNCE_CORE_BIN"
HEADLESS = "--present=headless"
VERDICTS = ("match", "differ", "error", "missing")
SAFE_NAME = re.compile(r"[A-Za-z0-9][A-Za-z0-9._-]*")

# Stop categories, by the key used in this file and the name used in the table in docs/DIAGNOSIS.md.
CATEGORY_NAMES = {
    "clean": "clean exit",
    "unrecognised": "unrecognised",
    "unimplemented": "unimplemented instruction",
    "memory_fault": "memory fault",
    "cpu_error": "CPU error",
    "break": "svcBreak",
    "unhandled_svc": "unhandled SVC",
    "cpu_halted": "CPU halted",
    "budget": "budget exhausted",
    "loader": "loader error",
}

# Stop lines that docs/DIAGNOSIS.md ("What is matched") says ounce-core prints. Kept in step with
# src/diagnose.rs by hand; tests/parity covers each one with a fixture line.
CLEAN_LINE = "[VM] Process exited gracefully (svcExitProcess)."
UNIMPL_RE = re.compile(r"\[VM Error\] Unimplemented instruction 0x([0-9A-Fa-f]+) at PC: 0x([0-9A-Fa-f]+)")
DEC_RE = re.compile(r"[0-9]+")

# Output that varies between two runs of the same binary on the same input, or that depends on the
# path the NRO was read from. These are masked before comparison and nothing else is:
#   - the instruction count, wall-clock time and rate of the VM summary, and the JIT cache counters
#     (the count varies on programs whose execution depends on host timing; the budget line still
#     states the exact budget, and the stop line is compared);
#   - the NRO path, which ounce-hb replaces with its private staged copy;
#   - the "Final X0" register at a budget stop, which changes with the path length and name the
#     loader sees (measured: the same bytes give X0 32, 64 or 80 depending on the path);
#   - the "Memory Buckets" line, whose sizes vary between runs of the same binary (measured: Code=140,
#     144, 148, 196 KB across runs).
NORMALIZE = (
    (re.compile(r"Executed [0-9]+ instructions in [^()\n]+ \([0-9.]+ MIPS\)"),
     "Executed <count> instructions in <elapsed> (<rate> MIPS)"),
    (re.compile(r"Basic-Block JIT Cache: [^\n]*"), "Basic-Block JIT Cache: <counters>"),
    (re.compile(r"Memory Buckets \([^\n]*"), "Memory Buckets (<sizes>)"),
    (re.compile(r"Final X0: [^\n]*"), "Final X0: <path-dependent register>"),
    (re.compile(r"\S*ounce-hb-run-[0-9]+-[0-9]+-[0-9]+/program\.nro"), "<NRO>"),
)


# ounce-core v0.8.0 opens the host audio device on every run. On a machine with no sound device it
# writes these lines to stderr during an otherwise clean run (docs/DIAGNOSIS.md, "Host audio noise").
# They are dropped from stderr before comparison, on both paths. The exit code is still compared.
AUDIO_NOISE = ("ALSA lib ", "[HostAudioSink] ")


class ConfigError(Exception):
    """A problem with the inputs that stops the run before any program is compared."""


@dataclass(frozen=True)
class Run:
    """One process run. `exit_code` is None when the process was killed by a signal."""

    status: str  # "ok", "timeout" or "spawn-error"
    exit_code: Optional[int]
    stdout: bytes
    stderr: bytes
    seconds: float
    detail: str = ""


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def text_of(data: bytes) -> str:
    """Console output as text, decoded lossily as ounce-hb decodes it."""
    return data.decode("utf-8", errors="replace")


def normalize(data: bytes, nro_arg: str, stream: str) -> str:
    """Console text with the NRO path and the run-to-run fields masked. See NORMALIZE.

    On stderr, host audio noise lines are dropped first (see AUDIO_NOISE).
    """
    text = text_of(data)
    if stream == "stderr":
        text = "\n".join(line for line in text.split("\n") if not line.startswith(AUDIO_NOISE))
    text = text.replace(nro_arg, "<NRO>")
    for pattern, replacement in NORMALIZE:
        text = pattern.sub(replacement, text)
    return text


def console_digest(text: str) -> str:
    return sha256_hex(text.encode("utf-8"))


def run_command(argv: Sequence[str], env: Dict[str, str], timeout: float) -> Run:
    """Runs one command. On timeout the whole process group is killed.

    ounce-hb runs ounce-core as a child. Killing only ounce-hb on timeout would leave ounce-core
    running as an orphan, so the run gets its own session and the group is killed.
    """
    started = time.monotonic()
    try:
        proc = subprocess.Popen(
            list(argv),
            stdin=subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=env,
            start_new_session=True,
        )
    except OSError as error:
        return Run("spawn-error", None, b"", b"", time.monotonic() - started, str(error))
    try:
        out, err = proc.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        os.killpg(proc.pid, signal.SIGKILL)
        out, err = proc.communicate()
        return Run("timeout", None, out, err, time.monotonic() - started,
                   f"no exit within {timeout:g}s")
    elapsed = time.monotonic() - started
    if proc.returncode < 0:
        return Run("ok", None, out, err, elapsed, f"killed by signal {-proc.returncode}")
    return Run("ok", proc.returncode, out, err, elapsed)


def load_exit_codes(path: Path) -> Dict[str, int]:
    """Category key to exit code, read from the 'Exit codes' table of docs/DIAGNOSIS.md."""
    try:
        text = path.read_text(encoding="utf-8")
    except OSError as error:
        raise ConfigError(f"cannot read exit code table {path}: {error}")
    section = text.split("\n## Exit codes", 1)
    if len(section) != 2:
        raise ConfigError(f"{path}: no '## Exit codes' section")
    by_name: Dict[str, int] = {}
    for line in section[1].split("\n## ", 1)[0].splitlines():
        match = re.match(r"^\|\s*`(\d+)`\s*\|\s*([^|]+?)\s*\|", line)
        if match:
            by_name[match.group(2)] = int(match.group(1))
    codes = {}
    for key, name in CATEGORY_NAMES.items():
        if name not in by_name:
            raise ConfigError(f"{path}: exit code table has no row for '{name}'")
        codes[key] = by_name[name]
    return codes


def _recognise(line: str) -> Optional[str]:
    """The category key of one trimmed output line, mirroring src/diagnose.rs recognise().

    None means the line is not a stop line, or it has the shape of one but its fields do not parse.
    """
    if line.startswith("[VM Error] Unimplemented instruction 0x"):
        m = UNIMPL_RE.fullmatch(line)
        return "unimplemented" if m and int(m.group(1), 16) <= 0xFFFFFFFF else None
    if line.startswith("[VM Error] Memory access violation: "):
        return "memory_fault"
    if line.startswith("[VM Error] "):
        return "cpu_error"
    if line.startswith("[VM] Hit svcBreak:"):
        return "break"
    if line.startswith("[VM] Stopping on unhandled SVC #0x"):
        m = re.match(r"[0-9A-Fa-f]+", line[len("[VM] Stopping on unhandled SVC #0x"):])
        return "unhandled_svc" if m and int(m.group(0), 16) <= 0xFFFF else None
    if line.startswith("[VM] CPU Halted: "):
        return "cpu_halted"
    if line.startswith("[VM] Execution budget reached ("):
        m = DEC_RE.match(line[len("[VM] Execution budget reached ("):])
        return "budget" if m else None
    if line.startswith("[Loader Error] "):
        return "loader"
    return None


def classify_core_output(run: Run) -> str:
    """The stop category of one direct ounce-core run, mirroring src/diagnose.rs from_output()."""
    lines = [line.strip() for line in (text_of(run.stdout) + "\n" + text_of(run.stderr)).split("\n")]
    clean = CLEAN_LINE in lines
    fault = next((k for k in (_recognise(line) for line in lines) if k), None)
    if run.exit_code == 0 and clean and fault is None:
        return "clean"
    if run.exit_code not in (None, 0) and not clean and fault is not None:
        return fault
    return "unrecognised"


def expected_hb_exit(direct: Run, codes: Dict[str, int]) -> Tuple[str, int]:
    """(category key, exit code) that ounce-hb must return for this direct run."""
    key = classify_core_output(direct)
    return key, codes[key]


def compare(direct: Run, via_hb: Run, codes: Dict[str, int], nro_arg: str) -> dict:
    """Verdict and reasons for one program. The result also carries the expected category."""
    failures = [f"{label} run {run.status}: {run.detail}"
                for label, run in (("direct", direct), ("via ounce-hb", via_hb)) if run.status != "ok"]
    if failures:
        return {"verdict": "error", "reasons": failures, "category": None, "expected_exit": None}

    key, expected = expected_hb_exit(direct, codes)
    reasons = []
    if via_hb.exit_code != expected:
        reasons.append(f"ounce-hb exit {via_hb.exit_code}, expected {expected} "
                       f"({CATEGORY_NAMES[key]}) for this ounce-core output")
    d_out, v_out = normalize(direct.stdout, nro_arg, "stdout"), normalize(via_hb.stdout, nro_arg, "stdout")
    if d_out != v_out:
        reasons.append("stdout differs")
    d_err, v_err = normalize(direct.stderr, nro_arg, "stderr"), normalize(via_hb.stderr, nro_arg, "stderr")
    if key == "clean":
        if d_err != v_err:
            reasons.append("stderr differs")
    elif not v_err.startswith(d_err):
        reasons.append("stderr differs before the ounce-hb explanation")
    return {"verdict": "differ" if reasons else "match", "reasons": reasons,
            "category": key, "expected_exit": expected}


def regressions(baseline: Dict[str, str], results: Dict[str, str], selected: Sequence[str],
                require_present: bool) -> List[str]:
    """Baseline matches that no longer match. Only programs in `selected` are checked."""
    out = []
    for pid in selected:
        if baseline.get(pid) != "match":
            continue
        now = results.get(pid)
        if now == "match":
            continue
        if now == "missing" and not require_present:
            continue
        out.append(f"{pid}: baseline match, now {now or 'absent from the corpus'}")
    return out


def _plain_relative(name: str) -> bool:
    parts = PurePosixPath(name).parts
    return not PurePosixPath(name).is_absolute() and all(p not in ("", ".", "..") for p in parts)


def load_corpus(core_dir: Path) -> List[dict]:
    path = core_dir / "tests" / "corpus" / "oracles.json"
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, ValueError) as error:
        raise ConfigError(f"cannot read corpus registry {path}: {error}")
    oracles = data.get("oracles") if isinstance(data, dict) else None
    if not isinstance(oracles, list) or not oracles:
        raise ConfigError(f"{path}: expected a nonempty 'oracles' list")
    programs, seen = [], set()
    for oracle in oracles:
        if not isinstance(oracle, dict):
            raise ConfigError(f"{path}: every oracle must be an object")
        pid, build, nro = oracle.get("id"), oracle.get("build"), oracle.get("nro")
        if not all(isinstance(v, str) and v for v in (pid, build, nro)):
            raise ConfigError(f"{path}: every oracle needs string id, build and nro")
        if pid in seen:
            raise ConfigError(f"{path}: duplicate oracle id {pid!r}")
        if not SAFE_NAME.fullmatch(build) or not _plain_relative(nro):
            raise ConfigError(f"{path}: oracle {pid!r} has an unsafe build or nro path")
        seen.add(pid)
        programs.append({"id": pid, "build": build, "nro": nro})
    return programs


def load_baseline(path: Path) -> Dict[str, str]:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except OSError:
        raise ConfigError(f"no baseline at {path}; generate one with --write-baseline on a real run")
    except ValueError as error:
        raise ConfigError(f"baseline {path} is not JSON: {error}")
    programs = data.get("programs") if isinstance(data, dict) else None
    if not isinstance(programs, dict) or not all(
        isinstance(k, str) and v in VERDICTS for k, v in programs.items()
    ):
        raise ConfigError(f"baseline {path} needs a 'programs' map of id to verdict")
    return programs


def git(repo: Path, *args: str) -> Optional[str]:
    try:
        out = subprocess.run(["git", "-C", str(repo), *args], capture_output=True, text=True, check=True)
    except (OSError, subprocess.CalledProcessError):
        return None
    return out.stdout.strip()


def collect_meta(core_dir: Path, core_bin: Path, hb_bin: Path, env: Dict[str, str], timeout: float) -> dict:
    dirty = git(core_dir, "status", "--porcelain", "--untracked-files=no")
    version = run_command([str(core_bin), "--version"], env, 30.0)
    return {
        "ounce_core": {
            "dir": str(core_dir),
            "commit": git(core_dir, "rev-parse", "HEAD"),
            "tag": git(core_dir, "describe", "--tags", "--exact-match", "HEAD"),
            "tracked_tree_dirty": None if dirty is None else bool(dirty),
            "binary_sha256": sha256_file(core_bin),
            "version_output": version.stdout.decode("utf-8", errors="replace").strip() or None,
        },
        "ounce_hb": {
            "binary_sha256": sha256_file(hb_bin),
            "repo_commit": git(REPO, "rev-parse", "HEAD"),
        },
        "platform": f"{platform.system()} {platform.machine()} {platform.release()}",
        "python": platform.python_version(),
        "timeout_seconds": timeout,
        "normalized_fields": [
            "the NRO path (as given, and the staged copy ounce-hb runs)",
            "the instruction count, wall-clock time and MIPS of the '[VM Summary] Executed' line",
            "the '[VM] Basic-Block JIT Cache' counters",
            "the 'Final X0' register value wherever printed (path-dependent on budget stops)",
            "the '[VM Summary]' memory bucket sizes (vary between runs)",
            "on stderr only: lines beginning 'ALSA lib ' or '[HostAudioSink] ' (host audio noise)",
        ],
    }


def run_summary(run: Run, nro_arg: str) -> dict:
    return {
        "status": run.status,
        "exit_code": run.exit_code,
        "detail": run.detail or None,
        "stdout_sha256": console_digest(normalize(run.stdout, nro_arg, "stdout")),
        "stderr_sha256": console_digest(normalize(run.stderr, nro_arg, "stderr")),
        "stdout_bytes": len(run.stdout),
        "stderr_bytes": len(run.stderr),
        "seconds": round(run.seconds, 3),
    }


def check_program(prog: dict, nro: Path, nro_rel: str, core_bin: Path, hb_bin: Path,
                  env: Dict[str, str], timeout: float, codes: Dict[str, int],
                  raw_dir: Optional[Path] = None) -> dict:
    entry = {"id": prog["id"], "build": prog["build"], "nro": nro_rel, "nro_sha256": None,
             "verdict": "missing", "category": None, "expected_exit": None, "reasons": [],
             "direct": None, "via_hb": None}
    if not nro.is_file():
        entry["reasons"].append("NRO not present under the builds directory; not run")
        return entry
    nro_arg = str(nro)
    entry["nro_sha256"] = sha256_file(nro)
    direct = run_command([str(core_bin), "run-nro", nro_arg, HEADLESS], env, timeout)
    via_hb = run_command([str(hb_bin), "run", nro_arg], env, timeout)
    result = compare(direct, via_hb, codes, nro_arg)
    entry.update(verdict=result["verdict"], reasons=result["reasons"], category=result["category"],
                 expected_exit=result["expected_exit"],
                 direct=run_summary(direct, nro_arg), via_hb=run_summary(via_hb, nro_arg))
    if raw_dir is not None and entry["verdict"] != "match":
        # Raw output of a run that did not match, for diagnosis. Not normalized, so it holds host paths.
        raw_dir.mkdir(parents=True, exist_ok=True)
        for label, run in (("direct", direct), ("via-hb", via_hb)):
            (raw_dir / f"{prog['id']}.{label}.stdout").write_bytes(run.stdout)
            (raw_dir / f"{prog['id']}.{label}.stderr").write_bytes(run.stderr)
    return entry


def render_summary(report: dict, regressed: List[str]) -> str:
    totals = report["summary"]
    core = report["ounce_core"]
    lines = [
        f"corpus parity: ounce-core {core.get('tag') or core.get('commit')} "
        f"({core.get('commit')}, tracked tree dirty: {core.get('tracked_tree_dirty')})",
        f"programs: {totals['total']}   match: {totals['match']}   differ: {totals['differ']}   "
        f"error: {totals['error']}   not evaluated (missing NRO): {totals['missing']}",
        "match means the two paths agree on exit category, stdout and stderr. It does not mean the "
        "program passes its first-frame oracle.",
        "direct categories: " + ", ".join(f"{k} {v}" for k, v in sorted(totals["categories"].items())),
        f"regressions against baseline: {len(regressed)}",
    ]
    lines += [f"  REGRESSED {line}" for line in regressed]
    for r in report["programs"]:
        if r["verdict"] in ("differ", "error") or (r["verdict"] == "missing" and report["policy"]["require_present"]):
            lines.append(f"  {r['id']}: {r['verdict']}: {'; '.join(r['reasons'])}")
    return "\n".join(lines) + "\n"


def run_parity(args: argparse.Namespace) -> int:
    core_bin_value = os.environ.get(CORE_BIN_ENV, "")
    if not core_bin_value:
        raise ConfigError(f"{CORE_BIN_ENV} must name the ounce-core binary under test")
    core_bin = Path(core_bin_value)
    if not core_bin.is_file():
        raise ConfigError(f"{CORE_BIN_ENV} points at {core_bin}, which is not a file")
    core_dir = Path(args.ounce_core_dir).resolve()
    if not (core_dir / "tests" / "corpus" / "oracles.json").is_file():
        raise ConfigError(f"{core_dir} is not an ounce-core checkout (no tests/corpus/oracles.json)")
    hb_bin = Path(args.ounce_hb)
    if not hb_bin.is_file():
        raise ConfigError(f"ounce-hb binary not found at {hb_bin}; build it with cargo build --release")
    codes = load_exit_codes(Path(args.exit_codes))
    builds_dir = Path(args.builds_dir).resolve() if args.builds_dir else core_dir / "tests" / "corpus" / "builds"

    programs = load_corpus(core_dir)
    if args.program:
        known = {p["id"] for p in programs}
        unknown = [pid for pid in args.program if pid not in known]
        if unknown:
            raise ConfigError(f"unknown corpus program(s): {', '.join(unknown)}")
        programs = [p for p in programs if p["id"] in set(args.program)]
    baseline = None if args.write_baseline else load_baseline(args.baseline)

    env = dict(os.environ)
    env[CORE_BIN_ENV] = str(core_bin)
    meta = collect_meta(core_dir, core_bin, hb_bin, env, args.timeout)
    started_at = datetime.now(timezone.utc).isoformat(timespec="seconds")

    report_path = Path(args.report)
    results = []
    for index, prog in enumerate(programs, 1):
        nro_rel = f"{prog['build']}/build/source/{prog['nro']}"
        nro = builds_dir / prog["build"] / "build" / "source" / prog["nro"]
        entry = check_program(prog, nro, nro_rel, core_bin, hb_bin, env, args.timeout, codes,
                              raw_dir=report_path.parent / "raw")
        results.append(entry)
        print(f"[{index}/{len(programs)}] {prog['id']}: {entry['verdict']}", file=sys.stderr, flush=True)

    counts = {v: sum(1 for r in results if r["verdict"] == v) for v in VERDICTS}
    categories: Dict[str, int] = {}
    for r in results:
        if r["category"]:
            categories[CATEGORY_NAMES[r["category"]]] = categories.get(CATEGORY_NAMES[r["category"]], 0) + 1
    report = {
        "schema_version": SCHEMA_VERSION,
        "started_at": started_at,
        "finished_at": datetime.now(timezone.utc).isoformat(timespec="seconds"),
        "compared": "stop category (from docs/DIAGNOSIS.md) and exit code, stdout and stderr of "
                    "'ounce-core run-nro --present=headless' versus 'ounce-hb run' on the same NRO",
        "not_compared": "first-frame oracles (frame hashes, pixel probes) and any frame or display "
                        "output that the CLI does not print",
        "policy": {"require_present": args.require_present, "builds_dir": str(builds_dir),
                   "baseline": None if args.write_baseline else str(args.baseline),
                   "exit_codes": str(args.exit_codes)},
        **meta,
        "summary": {"total": len(results), **counts, "categories": categories},
        "programs": results,
    }

    regressed: List[str] = []
    if baseline is not None:
        selected = [p["id"] for p in programs]
        regressed = regressions(baseline, {r["id"]: r["verdict"] for r in results}, selected,
                                args.require_present)

    report_path.parent.mkdir(parents=True, exist_ok=True)
    report_path.write_text(json.dumps(report, indent=2) + "\n", encoding="utf-8")
    summary_path = Path(args.summary) if args.summary else report_path.with_suffix(".txt")
    summary_path.parent.mkdir(parents=True, exist_ok=True)
    summary = render_summary(report, regressed)
    summary_path.write_text(summary, encoding="utf-8")
    sys.stdout.write(summary)
    print(f"report: {report_path}\nsummary: {summary_path}")

    if args.write_baseline:
        baseline_doc = {
            "schema_version": SCHEMA_VERSION,
            "note": "Written by scripts/corpus_parity.py --write-baseline. A program recorded as "
                    "match must keep matching; the other verdicts are recorded but not enforced.",
            "generated_at": report["finished_at"],
            "generated_from": {
                "ounce_core_commit": meta["ounce_core"]["commit"],
                "ounce_core_tag": meta["ounce_core"]["tag"],
                "ounce_core_binary_sha256": meta["ounce_core"]["binary_sha256"],
                "ounce_hb_binary_sha256": meta["ounce_hb"]["binary_sha256"],
                "platform": meta["platform"],
            },
            "programs": {r["id"]: r["verdict"] for r in results},
        }
        Path(args.write_baseline).parent.mkdir(parents=True, exist_ok=True)
        Path(args.write_baseline).write_text(json.dumps(baseline_doc, indent=2) + "\n", encoding="utf-8")
        print(f"baseline written: {args.write_baseline}")
        return 0

    return 1 if regressed else 0


def parse_args(argv: Optional[Sequence[str]] = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0],
                                     formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--ounce-core-dir", required=True, help="ounce-core checkout (corpus registry)")
    parser.add_argument("--builds-dir",
                        help="directory with <build>/build/source/<nro> trees; default: "
                             "<ounce-core-dir>/tests/corpus/builds")
    parser.add_argument("--ounce-hb", default=str(DEFAULT_OUNCE_HB), help="ounce-hb binary under test")
    parser.add_argument("--exit-codes", default=str(DEFAULT_EXIT_CODES),
                        help="file with the exit code table (default: %(default)s)")
    parser.add_argument("--baseline", type=Path, default=DEFAULT_BASELINE)
    parser.add_argument("--write-baseline", metavar="PATH",
                        help="write a new baseline from this run instead of checking against one")
    parser.add_argument("--report", default="corpus-parity-report.json", help="JSON report path")
    parser.add_argument("--summary", help="text summary path; default: the report path with .txt")
    parser.add_argument("--timeout", type=float, default=DEFAULT_TIMEOUT,
                        help="seconds allowed per run (default %(default)s)")
    parser.add_argument("--require-present", action="store_true",
                        help="count a missing NRO as a regression when the baseline recorded a match")
    parser.add_argument("--program", action="append", default=[], metavar="ID",
                        help="compare only this corpus program (repeatable)")
    return parser.parse_args(argv)


def main(argv: Optional[Sequence[str]] = None) -> int:
    args = parse_args(argv)
    try:
        return run_parity(args)
    except ConfigError as error:
        print(f"corpus_parity: {error}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
