//! Execution backends. The runner needs one operation: run a validated NRO with guest arguments
//! and get back the exit code and console output. Today that is a subprocess call to the
//! `ounce-core` CLI. A library backend can implement [`Backend`] later without changing callers.
//! See docs/OUNCE_CORE_API_NEEDS.md.
//!
//! Process control (docs/DIAGNOSIS.md, "Process control"):
//!
//! - On unix the child runs in its own process group, so the timeout kills everything it started,
//!   not only the direct child. On Windows the tree is killed with `taskkill /T`.
//! - Each of the child's stdout and stderr is kept up to [`MAX_CAPTURED_BYTES`]: the first half of
//!   that budget and the last half, with a marker line where bytes were dropped. The pipes are
//!   drained to the end either way, so a chatty guest cannot block on a full pipe.
//! - Waiting for the pipes to close is bounded by [`PIPE_CLOSE_GRACE`]. A process the child
//!   started that keeps the pipes open cannot hold ounce-hb hostage.
//! - On unix a small watch process (`sh`) holds a pipe from ounce-hb. When ounce-hb dies for any
//!   reason, including SIGTERM, SIGINT and SIGKILL, the pipe closes and the watch removes the
//!   staged directory and kills the child's process group. No signal handler is installed: that
//!   would need `unsafe`, which this crate forbids.

use std::collections::VecDeque;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::RunError;

/// Environment variable that names the ounce-core binary. Checked before `PATH`.
pub const CORE_BIN_ENV: &str = "OUNCE_CORE_BIN";

/// Wall-clock limit for one run. ounce-core is killed when it is exceeded.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

/// Largest number of bytes kept from each of ounce-core's stdout and stderr. The first
/// [`HEAD_BYTES`] and the last [`TAIL_BYTES`] of a longer stream are kept and the bytes between
/// them are dropped, with a marker line in their place (see [`truncation_marker`]). The end is kept
/// because ounce-core prints its stop line after the guest's last output.
pub const MAX_CAPTURED_BYTES: usize = 8 * 1024 * 1024;

/// Bytes kept from the start of a stream longer than [`MAX_CAPTURED_BYTES`].
pub const HEAD_BYTES: usize = MAX_CAPTURED_BYTES / 2;

/// Bytes kept from the end of a stream longer than [`MAX_CAPTURED_BYTES`].
pub const TAIL_BYTES: usize = MAX_CAPTURED_BYTES - HEAD_BYTES;

/// How long ounce-hb waits for the child's pipes to close after the child has been reaped, before
/// it kills whatever the child left behind and uses the output it has.
pub const PIPE_CLOSE_GRACE: Duration = Duration::from_secs(2);

/// How often the child is polled for exit while the timeout runs.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// Display flag that keeps ounce-core from opening a window or audio device.
const HEADLESS: &str = "--present=headless";

/// What one run produced.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunOutput {
    /// ounce-core's process exit status. `None` when the process was killed by a signal.
    pub exit_code: Option<i32>,
    /// What ounce-core wrote to stdout, decoded lossily. At most [`MAX_CAPTURED_BYTES`] of it,
    /// plus one marker line where bytes were dropped (`stdout_dropped` says how many).
    pub stdout: String,
    /// What ounce-core wrote to stderr, decoded lossily, kept like `stdout`.
    pub stderr: String,
    /// Bytes of stdout that were dropped to stay within [`MAX_CAPTURED_BYTES`]. `0` means the
    /// whole stream is in `stdout`.
    pub stdout_dropped: u64,
    /// Bytes of stderr that were dropped. `0` means the whole stream is in `stderr`.
    pub stderr_dropped: u64,
}

impl RunOutput {
    /// True when either stream was cut to fit [`MAX_CAPTURED_BYTES`].
    pub fn truncated(&self) -> bool {
        self.stdout_dropped > 0 || self.stderr_dropped > 0
    }
}

/// Runs one staged NRO. The program's bytes already passed `classify` and live in a private
/// directory that only ounce-hb writes to (see [`Staged`]).
pub trait Backend {
    fn run_nro(&self, program: &Staged, args: &[String]) -> Result<RunOutput, RunError>;
}

/// Runs the `ounce-core` CLI as a child process: `ounce-core run-nro <nro> --present=headless [args]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subprocess {
    bin: PathBuf,
    timeout: Duration,
}

impl Subprocess {
    pub fn new(bin: PathBuf, timeout: Duration) -> Self {
        Self { bin, timeout }
    }
}

impl Backend for Subprocess {
    fn run_nro(&self, program: &Staged, args: &[String]) -> Result<RunOutput, RunError> {
        // ounce-core drops every argument that starts with '-' without saying so. Refuse instead.
        if let Some(bad) = args.iter().find(|a| a.starts_with('-')) {
            return Err(RunError::UnsupportedArg(bad.clone()));
        }
        let mut cmd = Command::new(&self.bin);
        cmd.arg("run-nro")
            .arg(program.path())
            .arg(HEADLESS)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Its own process group, so that a kill reaches everything the child starts. Ctrl-C from a
        // terminal then no longer reaches the child directly; the death watch below covers that.
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }

        let mut child = spawn_cmd(&mut cmd, &self.bin)?;
        let child_id = child.id();
        #[cfg(unix)]
        let watch = DeathWatch::start(child_id, program.dir());
        let pipes = Pipes::start(&mut child);

        let deadline = Instant::now() + self.timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) if Instant::now() >= deadline => {
                    kill_tree(&mut child);
                    let _ = child.wait();
                    break Err(RunError::Timeout(self.timeout));
                }
                Ok(None) => thread::sleep(POLL_INTERVAL),
                Err(e) => {
                    kill_tree(&mut child);
                    let _ = child.wait();
                    break Err(RunError::Io(format!("waiting for ounce-core failed: {e}")));
                }
            }
        };
        // The child is reaped. Tell the watch that ounce-hb is still here and will clean up itself.
        #[cfg(unix)]
        drop(watch);

        // Normally both pipes close the moment the child exits. If something the child started
        // still holds them, kill the child's group once more and settle for what has arrived.
        if !pipes.wait_closed(PIPE_CLOSE_GRACE) {
            #[cfg(unix)]
            kill_group(child_id);
            #[cfg(not(unix))]
            let _ = child_id;
            pipes.wait_closed(PIPE_CLOSE_GRACE);
        }
        let (stdout, stderr) = pipes.snapshot();

        let status = status?;
        Ok(RunOutput {
            exit_code: status.code(),
            stdout: stdout.text,
            stderr: stderr.text,
            stdout_dropped: stdout.dropped,
            stderr_dropped: stderr.dropped,
        })
    }
}

fn spawn_cmd(cmd: &mut Command, bin: &Path) -> Result<Child, RunError> {
    #[cfg(target_os = "linux")]
    {
        for _ in 0..10 {
            match cmd.spawn() {
                Ok(child) => return Ok(child),
                Err(e) if e.raw_os_error() == Some(26) => {
                    thread::yield_now();
                    continue;
                }
                Err(e) => return Err(RunError::Io(format!("cannot start {}: {e}", bin.display()))),
            }
        }
    }
    cmd.spawn()
        .map_err(|e| RunError::Io(format!("cannot start {}: {e}", bin.display())))
}

/// Kills the child and everything it started. On unix the child leads its own process group, so
/// the group is killed; on Windows `taskkill /T` walks the tree. The direct child is killed as well
/// in case the group or tree kill was unavailable.
#[cfg(unix)]
fn kill_tree(child: &mut Child) {
    kill_group(child.id());
    let _ = child.kill();
}

#[cfg(windows)]
fn kill_tree(child: &mut Child) {
    let _ = Command::new("taskkill")
        .args(["/PID", &child.id().to_string(), "/T", "/F"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = child.kill();
}

#[cfg(not(any(unix, windows)))]
fn kill_tree(child: &mut Child) {
    let _ = child.kill();
}

/// Sends SIGKILL to the process group `pgid` with the `kill` command (`kill -s KILL -- -<pgid>`).
/// std has no process-group kill and this crate forbids `unsafe`, so the command does it. `kill`
/// is looked up on PATH first and at `/bin/kill` if that fails.
#[cfg(unix)]
fn kill_group(pgid: u32) {
    let target = format!("-{pgid}");
    for kill in ["kill", "/bin/kill"] {
        let ran = Command::new(kill)
            .args(["-s", "KILL", "--", &target])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        if ran.is_ok() {
            return;
        }
    }
}

/// The shell script of the death watch. `$1` is the child's process group, `$2` the staged
/// directory. It ignores the signals a terminal or job runner sends, then waits for one line on
/// stdin. ounce-hb writes `ok` when it has reaped the child and will clean up itself. If instead the
/// pipe closes with no line, ounce-hb died: the staged directory is removed and the group killed.
/// The directory goes first because the watch is in that group and the kill ends it too.
#[cfg(unix)]
const WATCH_SCRIPT: &str = "trap '' INT TERM HUP PIPE\n\
    if ! read -r line; then rm -rf -- \"$2\"; kill -s KILL -- \"-$1\" 2>/dev/null; fi\n";

/// A `sh` process that cleans up if ounce-hb dies before the child is reaped. See [`WATCH_SCRIPT`].
/// Dropping it tells the watch to stand down and waits, bounded, for it to exit.
#[cfg(unix)]
struct DeathWatch {
    child: Option<Child>,
}

#[cfg(unix)]
impl DeathWatch {
    /// How long to wait for the watch to exit after it was told to stand down.
    const EXIT_GRACE: Duration = Duration::from_secs(1);

    /// Starts the watch for the child's group `pgid` and the staged directory `dir`. It joins the
    /// child's process group, so a kill aimed at ounce-hb's own group does not take it along; if
    /// that is refused (the child is already gone), it runs in ounce-hb's group. When `sh` cannot
    /// be started at all, the run goes ahead without a watch.
    fn start(pgid: u32, dir: &Path) -> Self {
        let child = Self::spawn(pgid, dir, true)
            .or_else(|_| Self::spawn(pgid, dir, false))
            .ok();
        Self { child }
    }

    fn spawn(pgid: u32, dir: &Path, join_group: bool) -> io::Result<Child> {
        use std::os::unix::process::CommandExt;
        let mut cmd = Command::new("sh");
        cmd.arg("-c")
            .arg(WATCH_SCRIPT)
            .arg("ounce-hb-watch")
            .arg(pgid.to_string())
            .arg(dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if join_group {
            cmd.process_group(i32::try_from(pgid).map_err(|_| io::ErrorKind::InvalidInput)?);
        }
        cmd.spawn()
    }
}

#[cfg(unix)]
impl Drop for DeathWatch {
    fn drop(&mut self) {
        use std::io::Write;
        let Some(mut watch) = self.child.take() else {
            return;
        };
        if let Some(mut stdin) = watch.stdin.take() {
            let _ = stdin.write_all(b"ok\n");
            drop(stdin);
        }
        let deadline = Instant::now() + Self::EXIT_GRACE;
        loop {
            match watch.try_wait() {
                Ok(Some(_)) => return,
                Ok(None) if Instant::now() < deadline => thread::sleep(POLL_INTERVAL),
                _ => {
                    let _ = watch.kill();
                    let _ = watch.wait();
                    return;
                }
            }
        }
    }
}

/// The marker line put where bytes of `stream` (`stdout` or `stderr`) were dropped.
pub fn truncation_marker(stream: &str, dropped: u64) -> String {
    format!(
        "[ounce-hb: {dropped} bytes of ounce-core's {stream} were dropped here to stay within \
         {MAX_CAPTURED_BYTES} bytes; the first {HEAD_BYTES} and the last {TAIL_BYTES} bytes are kept]\n"
    )
}

/// What was kept of one stream: its first [`HEAD_BYTES`], its last [`TAIL_BYTES`], and how many
/// bytes between them were dropped.
#[derive(Debug, Default)]
struct Capture {
    head: Vec<u8>,
    tail: VecDeque<u8>,
    dropped: u64,
}

/// A stream as text, with the marker in place of the dropped bytes.
#[derive(Debug, Default, PartialEq, Eq)]
struct Rendered {
    text: String,
    dropped: u64,
}

impl Capture {
    fn push(&mut self, mut chunk: &[u8]) {
        if self.head.len() < HEAD_BYTES {
            let n = chunk.len().min(HEAD_BYTES - self.head.len());
            self.head.extend_from_slice(&chunk[..n]);
            chunk = &chunk[n..];
        }
        if chunk.is_empty() {
            return;
        }
        if chunk.len() >= TAIL_BYTES {
            self.dropped += (self.tail.len() + chunk.len() - TAIL_BYTES) as u64;
            self.tail.clear();
            self.tail.extend(&chunk[chunk.len() - TAIL_BYTES..]);
        } else {
            self.tail.extend(chunk);
            if self.tail.len() > TAIL_BYTES {
                let excess = self.tail.len() - TAIL_BYTES;
                self.tail.drain(..excess);
                self.dropped += excess as u64;
            }
        }
    }

    fn render(&self, stream: &str) -> Rendered {
        let mut text = String::from_utf8_lossy(&self.head).into_owned();
        if self.dropped > 0 {
            if !text.is_empty() && !text.ends_with('\n') {
                text.push('\n');
            }
            text.push_str(&truncation_marker(stream, self.dropped));
        }
        if !self.tail.is_empty() {
            let tail: Vec<u8> = self.tail.iter().copied().collect();
            text.push_str(&String::from_utf8_lossy(&tail));
        }
        Rendered {
            text,
            dropped: self.dropped,
        }
    }
}

type Shared = Arc<Mutex<Capture>>;

/// The child's stdout and stderr, drained on their own threads into bounded captures.
struct Pipes {
    stdout: Shared,
    stderr: Shared,
    readers: Vec<JoinHandle<()>>,
}

impl Pipes {
    fn start(child: &mut Child) -> Self {
        let stdout: Shared = Arc::default();
        let stderr: Shared = Arc::default();
        let mut readers = Vec::new();
        if let Some(pipe) = child.stdout.take() {
            let into = Arc::clone(&stdout);
            readers.push(thread::spawn(move || drain(pipe, &into)));
        }
        if let Some(pipe) = child.stderr.take() {
            let into = Arc::clone(&stderr);
            readers.push(thread::spawn(move || drain(pipe, &into)));
        }
        Self {
            stdout,
            stderr,
            readers,
        }
    }

    /// Waits up to `grace` for both pipes to reach end of file. False when one is still open; its
    /// reader thread then keeps draining in the background and is not joined.
    fn wait_closed(&self, grace: Duration) -> bool {
        let deadline = Instant::now() + grace;
        loop {
            if self.readers.iter().all(JoinHandle::is_finished) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            thread::sleep(POLL_INTERVAL);
        }
    }

    /// The output so far, as text.
    fn snapshot(&self) -> (Rendered, Rendered) {
        let render = |shared: &Shared, name: &str| {
            shared
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .render(name)
        };
        (
            render(&self.stdout, "stdout"),
            render(&self.stderr, "stderr"),
        )
    }
}

/// Reads `pipe` to the end into `into`. Everything past the cap is counted and dropped, so the
/// writer never blocks on a full pipe.
fn drain<R: Read>(mut pipe: R, into: &Shared) {
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        match pipe.read(&mut buf) {
            Ok(0) => return,
            Ok(n) => into
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(&buf[..n]),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => return,
        }
    }
}

/// Finds the ounce-core binary: `OUNCE_CORE_BIN` if set (it must be a file), else the first
/// executable `ounce-core` on `PATH`. Empty `PATH` entries are skipped rather than read as the
/// current directory, and a file without execute permission is passed over. Takes the variables as
/// arguments so tests need not touch the environment.
pub fn find_core(
    override_bin: Option<OsString>,
    path: Option<OsString>,
) -> Result<PathBuf, RunError> {
    if let Some(value) = override_bin.filter(|v| !v.is_empty()) {
        let bin = PathBuf::from(value);
        return if bin.is_file() {
            Ok(bin)
        } else {
            Err(RunError::CoreNotFound(format!(
                "{CORE_BIN_ENV} points at {}, which is not a file",
                bin.display()
            )))
        };
    }
    let name = format!("ounce-core{}", std::env::consts::EXE_SUFFIX);
    if let Some(path) = path {
        for dir in std::env::split_paths(&path).filter(|d| !d.as_os_str().is_empty()) {
            let candidate = dir.join(&name);
            if is_executable_file(&candidate) {
                return Ok(candidate);
            }
        }
    }
    Err(RunError::CoreNotFound(format!(
        "{CORE_BIN_ENV} is not set and no ounce-core is on PATH"
    )))
}

/// True for a regular file that the current user may execute. On unix that is any execute bit;
/// Windows has no such bit, so any regular file counts.
#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

/// [`find_core`] using the real environment.
pub fn locate_core() -> Result<PathBuf, RunError> {
    find_core(std::env::var_os(CORE_BIN_ENV), std::env::var_os("PATH"))
}

/// A private copy of the NRO bytes that were validated. Removed when dropped.
///
/// The copy is what runs, so the program executed is exactly the bytes `classify` accepted, even if
/// the original file changes afterwards. The directory is created by ounce-hb with mode `0700` on
/// unix and holds nothing but the copy, which is why the death watch may remove it whole.
#[derive(Debug)]
pub struct Staged {
    dir: PathBuf,
    file: PathBuf,
}

impl Staged {
    pub fn create(bytes: &[u8]) -> Result<Self, RunError> {
        let dir = std::env::temp_dir().join(unique_name("run"));
        #[cfg(unix)]
        let builder = {
            use std::os::unix::fs::DirBuilderExt;
            let mut b = fs::DirBuilder::new();
            b.mode(0o700);
            b
        };
        #[cfg(not(unix))]
        let builder = fs::DirBuilder::new();
        builder
            .create(&dir)
            .map_err(|e| RunError::Io(format!("cannot create {}: {e}", dir.display())))?;
        let staged = Staged {
            file: dir.join("program.nro"),
            dir,
        };
        fs::write(&staged.file, bytes)
            .map_err(|e| RunError::Io(format!("cannot write {}: {e}", staged.file.display())))?;
        Ok(staged)
    }

    /// The private copy of the program.
    pub fn path(&self) -> &Path {
        &self.file
    }

    /// The private directory that holds the copy and nothing else.
    pub fn dir(&self) -> &Path {
        &self.dir
    }
}

impl Drop for Staged {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// A name unique within this process and across processes, under the temp directory.
pub(crate) fn unique_name(tag: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("ounce-hb-{tag}-{}-{nanos}-{n}", std::process::id())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Recorder;

    impl Backend for Recorder {
        fn run_nro(&self, program: &Staged, _args: &[String]) -> Result<RunOutput, RunError> {
            Ok(RunOutput {
                exit_code: Some(0),
                stdout: fs::read_to_string(program.path()).unwrap(),
                ..RunOutput::default()
            })
        }
    }

    /// Writes `ounce-core` into a fresh directory with the given unix mode (ignored elsewhere).
    fn core_in_dir(mode: u32) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(unique_name("find"));
        fs::create_dir_all(&dir).unwrap();
        let bin = dir.join(format!("ounce-core{}", std::env::consts::EXE_SUFFIX));
        fs::write(&bin, b"").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&bin, fs::Permissions::from_mode(mode)).unwrap();
        }
        #[cfg(not(unix))]
        let _ = mode;
        (dir, bin)
    }

    #[test]
    fn override_must_be_a_file() {
        let missing = OsString::from("/nonexistent/ounce-core-for-test");
        let e = find_core(Some(missing), None).unwrap_err();
        assert!(matches!(&e, RunError::CoreNotFound(m) if m.contains(CORE_BIN_ENV)));
    }

    #[test]
    fn override_wins_over_path_and_empty_override_is_ignored() {
        let (dir, on_path) = core_in_dir(0o755);
        let path = std::env::join_paths([&dir]).unwrap();

        assert_eq!(find_core(None, Some(path.clone())).unwrap(), on_path);
        assert_eq!(
            find_core(Some(OsString::new()), Some(path.clone())).unwrap(),
            on_path
        );
        assert_eq!(
            find_core(Some(on_path.clone().into_os_string()), None).unwrap(),
            on_path
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn nothing_found_is_a_clear_error() {
        let e = find_core(None, Some(OsString::new())).unwrap_err();
        assert!(matches!(&e, RunError::CoreNotFound(m) if m.contains("PATH")));
    }

    /// Finding 21: a non-executable `ounce-core` earlier on PATH used to be taken (and then failed to
    /// start with exit 5) instead of falling through to the executable one.
    #[cfg(unix)]
    #[test]
    fn path_lookup_skips_files_without_execute_permission() {
        let (plain_dir, _plain) = core_in_dir(0o644);
        let (exec_dir, exec) = core_in_dir(0o755);
        let path = std::env::join_paths([&plain_dir, &exec_dir]).unwrap();
        assert_eq!(find_core(None, Some(path)).unwrap(), exec);
        let only_plain = std::env::join_paths([&plain_dir]).unwrap();
        assert!(matches!(
            find_core(None, Some(only_plain)),
            Err(RunError::CoreNotFound(_))
        ));
        let _ = fs::remove_dir_all(&plain_dir);
        let _ = fs::remove_dir_all(&exec_dir);
    }

    #[test]
    fn staged_copy_matches_bytes_and_is_removed() {
        let bytes = b"NRO-bytes-for-test".to_vec();
        let staged = Staged::create(&bytes).unwrap();
        let path = staged.path().to_path_buf();
        assert_eq!(staged.dir(), path.parent().unwrap());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        let out = Recorder.run_nro(&staged, &[]).unwrap();
        assert_eq!(out.stdout.as_bytes(), bytes.as_slice());
        drop(staged);
        assert!(!path.exists());
    }

    #[test]
    fn refuses_guest_args_that_ounce_core_would_drop() {
        let bin = Subprocess::new(PathBuf::from("/nonexistent/ounce-core"), DEFAULT_TIMEOUT);
        let staged = Staged::create(b"x").unwrap();
        let e = bin.run_nro(&staged, &["--gui".to_string()]).unwrap_err();
        assert_eq!(e, RunError::UnsupportedArg("--gui".to_string()));
    }

    #[test]
    fn missing_binary_is_an_io_error() {
        let bin = Subprocess::new(PathBuf::from("/nonexistent/ounce-core"), DEFAULT_TIMEOUT);
        let staged = Staged::create(b"x").unwrap();
        assert!(matches!(bin.run_nro(&staged, &[]), Err(RunError::Io(_))));
    }

    /// Finding 5: the capture keeps the first and last parts of a long stream and counts the rest.
    #[test]
    fn capture_keeps_head_and_tail_and_counts_the_dropped_bytes() {
        let mut c = Capture::default();
        c.push(b"short");
        assert_eq!(c.render("stdout").text, "short");
        assert_eq!(c.dropped, 0);

        let mut c = Capture::default();
        let chunk = vec![b'a'; 1_000_003];
        let mut total = 0usize;
        while total < MAX_CAPTURED_BYTES + 3 * chunk.len() {
            c.push(&chunk);
            total += chunk.len();
        }
        let stop = b"\n[VM] Execution budget reached (7 instructions).\n";
        c.push(stop);
        total += stop.len();
        assert_eq!(c.head.len(), HEAD_BYTES);
        assert_eq!(c.tail.len(), TAIL_BYTES);
        assert_eq!(c.dropped as usize, total - MAX_CAPTURED_BYTES);
        let r = c.render("stdout");
        assert_eq!(r.dropped, c.dropped);
        assert!(r.text.contains(&truncation_marker("stdout", c.dropped)));
        assert!(
            r.text
                .ends_with("[VM] Execution budget reached (7 instructions).\n")
        );
        assert!(r.text.len() < MAX_CAPTURED_BYTES + 512);

        // One chunk larger than the tail replaces it and counts everything that fell out.
        let mut c = Capture::default();
        c.push(&vec![b'h'; HEAD_BYTES]);
        c.push(b"old-tail");
        c.push(&vec![b't'; TAIL_BYTES + 10]);
        assert_eq!(c.tail.len(), TAIL_BYTES);
        assert_eq!(c.dropped, 8 + 10);
    }

    /// Writes an executable `sh` script. The file handle is closed and synced before it is run.
    #[cfg(unix)]
    fn fake_core(body: &str) -> PathBuf {
        use std::io::Write;
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(unique_name("fake"));
        fs::create_dir_all(&dir).unwrap();
        let bin = dir.join("ounce-core");
        let mut file = fs::File::create(&bin).unwrap();
        file.write_all(format!("#!/bin/sh\n{body}\n").as_bytes())
            .unwrap();
        file.sync_all().unwrap();
        drop(file);
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).unwrap();
        bin
    }

    #[cfg(unix)]
    #[test]
    fn captures_exit_code_and_both_streams() {
        let bin = fake_core("echo \"out:$*\"; echo err >&2; exit 7");
        let runner = Subprocess::new(bin.clone(), Duration::from_secs(30));
        let staged = Staged::create(b"x").unwrap();
        let out = runner.run_nro(&staged, &["a".to_string()]).unwrap();
        assert_eq!(out.exit_code, Some(7));
        assert!(
            out.stdout.contains(&format!(
                "out:run-nro {} --present=headless a",
                staged.path().display()
            )),
            "{}",
            out.stdout
        );
        assert_eq!(out.stderr.trim(), "err");
        assert!(!out.truncated());
        let _ = fs::remove_dir_all(bin.parent().unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn kills_a_run_that_exceeds_the_timeout() {
        // `exec` replaces the shell so that killing the child also closes its pipes.
        let bin = fake_core("exec sleep 30");
        let runner = Subprocess::new(bin.clone(), Duration::from_millis(200));
        let staged = Staged::create(b"x").unwrap();
        let started = Instant::now();
        let e = runner.run_nro(&staged, &[]).unwrap_err();
        assert_eq!(e, RunError::Timeout(Duration::from_millis(200)));
        assert!(started.elapsed() < Duration::from_secs(20));
        let _ = fs::remove_dir_all(bin.parent().unwrap());
    }

    /// Finding 4: a process the child started, holding the pipes, used to keep ounce-hb waiting
    /// until it exited on its own. Now it is killed with the group and the wait is bounded.
    #[cfg(unix)]
    #[test]
    fn timeout_kills_the_grandchild_and_does_not_wait_for_it() {
        let token = unique_name("grandchild");
        // Two commands, so that sh does not exec into `sleep` and keeps the token in its command line.
        let bin = fake_core(&format!("sh -c 'sleep 30; true' {token} &\nexec sleep 30"));
        let runner = Subprocess::new(bin.clone(), Duration::from_millis(300));
        let staged = Staged::create(b"x").unwrap();
        let started = Instant::now();
        let e = runner.run_nro(&staged, &[]).unwrap_err();
        assert_eq!(e, RunError::Timeout(Duration::from_millis(300)));
        let elapsed = started.elapsed();
        assert!(elapsed < Duration::from_secs(10), "took {elapsed:?}");
        // SIGKILL delivery is asynchronous; give it a moment.
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let ps = Command::new("ps")
                .args(["-axo", "command="])
                .output()
                .unwrap();
            let alive = String::from_utf8_lossy(&ps.stdout)
                .lines()
                .any(|l| l.contains(&token));
            if !alive {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "grandchild {token} still running"
            );
            thread::sleep(POLL_INTERVAL);
        }
        let _ = fs::remove_dir_all(bin.parent().unwrap());
    }

    /// Finding 5: a stream longer than the cap is cut in the middle, the cut is marked, and the
    /// stop line at the end survives.
    #[cfg(unix)]
    #[test]
    fn long_output_is_capped_with_a_marker_and_keeps_the_end() {
        let total = MAX_CAPTURED_BYTES * 2 + 12345;
        let bin = fake_core(&format!(
            "head -c {total} /dev/zero | tr '\\0' x\necho\necho '[VM] Execution budget reached (7 instructions).'\nexit 1"
        ));
        let runner = Subprocess::new(bin.clone(), Duration::from_secs(60));
        let staged = Staged::create(b"x").unwrap();
        let out = runner.run_nro(&staged, &[]).unwrap();
        assert_eq!(out.exit_code, Some(1));
        assert!(out.truncated());
        assert_eq!(out.stderr_dropped, 0);
        let stop = "\n[VM] Execution budget reached (7 instructions).\n";
        assert_eq!(
            out.stdout_dropped as usize,
            total + stop.len() - MAX_CAPTURED_BYTES
        );
        assert!(
            out.stdout.len() < MAX_CAPTURED_BYTES + 512,
            "{}",
            out.stdout.len()
        );
        assert!(
            out.stdout
                .contains(&truncation_marker("stdout", out.stdout_dropped))
        );
        assert!(
            out.stdout.ends_with(stop),
            "{}",
            &out.stdout[out.stdout.len() - 100..]
        );
        let _ = fs::remove_dir_all(bin.parent().unwrap());
    }
}
