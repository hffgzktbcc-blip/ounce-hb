//! Execution backends. The runner needs one operation: run a validated NRO with guest arguments
//! and get back the exit code and console output. Today that is a subprocess call to the
//! `ounce-core` CLI. A library backend can implement [`Backend`] later without changing callers.
//! See docs/OUNCE_CORE_API_NEEDS.md.

use std::ffi::OsString;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use crate::RunError;

/// Environment variable that names the ounce-core binary. Checked before `PATH`.
pub const CORE_BIN_ENV: &str = "OUNCE_CORE_BIN";

/// Wall-clock limit for one run. ounce-core is killed when it is exceeded.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

/// How often the child is polled for exit while the timeout runs.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// Display flag that keeps ounce-core from opening a window or audio device.
const HEADLESS: &str = "--present=headless";

/// What one run produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunOutput {
    /// ounce-core's process exit status. `None` when the process was killed by a signal.
    pub exit_code: Option<i32>,
    /// Everything ounce-core wrote to stdout, decoded lossily.
    pub stdout: String,
    /// Everything ounce-core wrote to stderr, decoded lossily.
    pub stderr: String,
}

/// Runs one NRO file. `nro` is a file on disk that already passed `classify`.
pub trait Backend {
    fn run_nro(&self, nro: &Path, args: &[String]) -> Result<RunOutput, RunError>;
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
    fn run_nro(&self, nro: &Path, args: &[String]) -> Result<RunOutput, RunError> {
        // ounce-core drops every argument that starts with '-' without saying so. Refuse instead.
        if let Some(bad) = args.iter().find(|a| a.starts_with('-')) {
            return Err(RunError::UnsupportedArg(bad.clone()));
        }
        let mut cmd = Command::new(&self.bin);
        cmd.arg("run-nro")
            .arg(nro)
            .arg(HEADLESS)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = spawn_cmd(&mut cmd, &self.bin)?;

        // Drain both pipes on their own threads so a chatty guest cannot block on a full pipe.
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let out_reader = thread::spawn(move || read_all(stdout));
        let err_reader = thread::spawn(move || read_all(stderr));

        let deadline = Instant::now() + self.timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = out_reader.join();
                    let _ = err_reader.join();
                    return Err(RunError::Timeout(self.timeout));
                }
                Ok(None) => thread::sleep(POLL_INTERVAL),
                Err(e) => {
                    let _ = child.kill();
                    return Err(RunError::Io(format!("waiting for ounce-core failed: {e}")));
                }
            }
        };

        Ok(RunOutput {
            exit_code: status.code(),
            stdout: String::from_utf8_lossy(&out_reader.join().unwrap_or_default()).into_owned(),
            stderr: String::from_utf8_lossy(&err_reader.join().unwrap_or_default()).into_owned(),
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

fn read_all<R: Read>(pipe: Option<R>) -> Vec<u8> {
    let mut buf = Vec::new();
    if let Some(mut pipe) = pipe {
        let _ = pipe.read_to_end(&mut buf);
    }
    buf
}

/// Finds the ounce-core binary: `OUNCE_CORE_BIN` if set (it must be a file), else the first
/// `ounce-core` on `PATH`. Takes the variables as arguments so tests need not touch the environment.
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
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join(&name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Err(RunError::CoreNotFound(format!(
        "{CORE_BIN_ENV} is not set and no ounce-core is on PATH"
    )))
}

/// [`find_core`] using the real environment.
pub fn locate_core() -> Result<PathBuf, RunError> {
    find_core(std::env::var_os(CORE_BIN_ENV), std::env::var_os("PATH"))
}

/// A private copy of the NRO bytes that were validated. Removed when dropped.
///
/// The copy is what runs, so the program executed is exactly the bytes `classify` accepted, even if
/// the original file changes afterwards.
pub(crate) struct Staged {
    dir: PathBuf,
    file: PathBuf,
}

impl Staged {
    pub(crate) fn create(bytes: &[u8]) -> Result<Self, RunError> {
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

    pub(crate) fn path(&self) -> &Path {
        &self.file
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
        fn run_nro(&self, nro: &Path, _args: &[String]) -> Result<RunOutput, RunError> {
            Ok(RunOutput {
                exit_code: Some(0),
                stdout: fs::read_to_string(nro).unwrap(),
                stderr: String::new(),
            })
        }
    }

    #[test]
    fn override_must_be_a_file() {
        let missing = OsString::from("/nonexistent/ounce-core-for-test");
        let e = find_core(Some(missing), None).unwrap_err();
        assert!(matches!(&e, RunError::CoreNotFound(m) if m.contains(CORE_BIN_ENV)));
    }

    #[test]
    fn override_wins_over_path_and_empty_override_is_ignored() {
        let dir = std::env::temp_dir().join(unique_name("find"));
        fs::create_dir_all(&dir).unwrap();
        let on_path = dir.join(format!("ounce-core{}", std::env::consts::EXE_SUFFIX));
        fs::write(&on_path, b"").unwrap();
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

    #[test]
    fn staged_copy_matches_bytes_and_is_removed() {
        let bytes = b"NRO-bytes-for-test".to_vec();
        let staged = Staged::create(&bytes).unwrap();
        let path = staged.path().to_path_buf();
        assert_eq!(fs::read(&path).unwrap(), bytes);
        let out = Recorder.run_nro(&path, &[]).unwrap();
        assert_eq!(out.stdout.as_bytes(), bytes.as_slice());
        drop(staged);
        assert!(!path.exists());
    }

    #[test]
    fn refuses_guest_args_that_ounce_core_would_drop() {
        let bin = Subprocess::new(PathBuf::from("/nonexistent/ounce-core"), DEFAULT_TIMEOUT);
        let e = bin
            .run_nro(Path::new("x.nro"), &["--gui".to_string()])
            .unwrap_err();
        assert_eq!(e, RunError::UnsupportedArg("--gui".to_string()));
    }

    #[test]
    fn missing_binary_is_an_io_error() {
        let bin = Subprocess::new(PathBuf::from("/nonexistent/ounce-core"), DEFAULT_TIMEOUT);
        assert!(matches!(
            bin.run_nro(Path::new("x.nro"), &[]),
            Err(RunError::Io(_))
        ));
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
        let out = runner
            .run_nro(Path::new("p.nro"), &["a".to_string()])
            .unwrap();
        assert_eq!(out.exit_code, Some(7));
        assert!(
            out.stdout
                .contains("out:run-nro p.nro --present=headless a"),
            "{}",
            out.stdout
        );
        assert_eq!(out.stderr.trim(), "err");
        let _ = fs::remove_dir_all(bin.parent().unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn kills_a_run_that_exceeds_the_timeout() {
        // `exec` replaces the shell so that killing the child also closes its pipes.
        let bin = fake_core("exec sleep 30");
        let runner = Subprocess::new(bin.clone(), Duration::from_millis(200));
        let started = Instant::now();
        let e = runner.run_nro(Path::new("p.nro"), &[]).unwrap_err();
        assert_eq!(e, RunError::Timeout(Duration::from_millis(200)));
        assert!(started.elapsed() < Duration::from_secs(20));
        let _ = fs::remove_dir_all(bin.parent().unwrap());
    }
}
