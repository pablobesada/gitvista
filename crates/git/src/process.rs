use gitvista_core::Cancellation;
use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::thread;
use std::time::{Duration, Instant};

pub(crate) struct CommandOutput {
    pub status: ExitStatus,
    pub stdout: Vec<u8>,
    stderr: Vec<u8>,
}
impl CommandOutput {
    pub fn error_message(&self) -> String {
        let message = String::from_utf8_lossy(&self.stderr).trim().to_owned();
        if message.is_empty() {
            format!("Git failed with {}", self.status)
        } else {
            message
        }
    }
}

#[derive(Debug)]
pub(crate) enum ProcessError {
    Cancelled,
    Timeout,
    OutputLimit,
    Io(String),
}
impl std::fmt::Display for ProcessError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("Operation cancelled"),
            Self::Timeout => formatter.write_str("Git command exceeded its 30-second time limit"),
            Self::OutputLimit => {
                formatter.write_str("Git command exceeded its output memory limit")
            }
            Self::Io(error) => formatter.write_str(error),
        }
    }
}

fn collect(mut reader: impl Read, limit: usize, overflow: &AtomicBool) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 16 * 1024];
    loop {
        let read = reader.read(&mut chunk).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        if bytes.len() + read > limit {
            overflow.store(true, Ordering::Release);
            return Ok(bytes);
        }
        bytes.extend_from_slice(&chunk[..read]);
    }
    Ok(bytes)
}

pub(crate) fn execute(
    executable: &OsStr,
    cwd: &Path,
    args: &[OsString],
    stdout_limit: usize,
    timeout: Duration,
    control: &Cancellation,
) -> Result<CommandOutput, ProcessError> {
    if control.is_cancelled() {
        return Err(ProcessError::Cancelled);
    }
    let mut command = Command::new(executable);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    command
        .current_dir(cwd)
        .args([
            "--no-optional-locks",
            "--no-pager",
            "--literal-pathspecs",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "diff.external=",
            // Porcelain diff can refresh stat-only index entries even when
            // optional locks are disabled. Keep queries strictly read-only.
            "-c",
            "diff.autoRefreshIndex=false",
            "-c",
            "core.quotePath=true",
            "-c",
            "color.ui=false",
        ])
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        // Reading a partial clone must not silently fetch missing blobs.
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_PAGER", "cat")
        .env("LC_ALL", "C")
        // A GUI launched from a Git-aware terminal must not inherit another repository.
        .env_remove("GIT_DIR")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_OBJECT_DIRECTORY")
        .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
        .env_remove("GIT_EXTERNAL_DIFF")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().map_err(|error| {
        ProcessError::Io(format!(
            "Cannot start Git: {error}. Install Git or Xcode Command Line Tools."
        ))
    })?;
    let overflow = Arc::new(AtomicBool::new(false));
    let stdout = child.stdout.take().expect("piped stdout");
    let stderr = child.stderr.take().expect("piped stderr");
    let stdout_overflow = overflow.clone();
    let stderr_overflow = overflow.clone();
    let output_reader = thread::spawn(move || collect(stdout, stdout_limit, &stdout_overflow));
    let error_reader = thread::spawn(move || collect(stderr, 64 * 1024, &stderr_overflow));
    let start = Instant::now();
    let status = loop {
        let abort = if control.is_cancelled() {
            Some(ProcessError::Cancelled)
        } else if overflow.load(Ordering::Acquire) {
            Some(ProcessError::OutputLimit)
        } else if start.elapsed() > timeout {
            Some(ProcessError::Timeout)
        } else {
            None
        };
        if let Some(error) = abort {
            kill_tree(&mut child);
            let _ = child.wait();
            finish_aborted_readers(output_reader, error_reader);
            return Err(error);
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => thread::sleep(Duration::from_millis(2)),
            Err(error) => {
                kill_tree(&mut child);
                let _ = child.wait();
                finish_aborted_readers(output_reader, error_reader);
                return Err(ProcessError::Io(error.to_string()));
            }
        }
    };
    // Git may have exited while a submodule descendant still owns its pipes.
    // Keep cancellation/deadlines active until both readers have completed.
    while !output_reader.is_finished() || !error_reader.is_finished() {
        let abort = if control.is_cancelled() {
            Some(ProcessError::Cancelled)
        } else if overflow.load(Ordering::Acquire) {
            Some(ProcessError::OutputLimit)
        } else if start.elapsed() > timeout {
            Some(ProcessError::Timeout)
        } else {
            None
        };
        if let Some(error) = abort {
            kill_tree(&mut child);
            finish_aborted_readers(output_reader, error_reader);
            return Err(error);
        }
        thread::sleep(Duration::from_millis(2));
    }
    let stdout = output_reader
        .join()
        .map_err(|_| ProcessError::Io("Git stdout reader panicked".into()))?
        .map_err(ProcessError::Io)?;
    let stderr = error_reader
        .join()
        .map_err(|_| ProcessError::Io("Git stderr reader panicked".into()))?
        .map_err(ProcessError::Io)?;
    if control.is_cancelled() {
        return Err(ProcessError::Cancelled);
    }
    if overflow.load(Ordering::Acquire) {
        return Err(ProcessError::OutputLimit);
    }
    Ok(CommandOutput {
        status,
        stdout,
        stderr,
    })
}

fn kill_tree(child: &mut std::process::Child) {
    #[cfg(unix)]
    {
        // This child was assigned its own group before exec. Killing that group
        // closes pipes held by Git's submodule children before joining readers.
        unsafe {
            libc::kill(-(child.id() as i32), libc::SIGKILL);
        }
    }
    let _ = child.kill();
}

type ReaderThread = thread::JoinHandle<Result<Vec<u8>, String>>;
fn finish_aborted_readers(stdout: ReaderThread, stderr: ReaderThread) {
    let deadline = Instant::now() + Duration::from_millis(250);
    while (!stdout.is_finished() || !stderr.is_finished()) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(2));
    }
    // Normally SIGKILL of our process group closes both pipes immediately. A
    // deliberately detached process must not extend a cancellation indefinitely.
    // Dropping its reader handle relinquishes the join rather than blocking UI.
    if stdout.is_finished() {
        let _ = stdout.join();
    }
    if stderr.is_finished() {
        let _ = stderr.join();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    #[test]
    fn output_reader_has_a_hard_memory_bound() {
        let overflow = AtomicBool::new(false);
        let result = collect(Cursor::new(vec![42u8; 1024]), 32, &overflow).unwrap();
        assert!(result.len() <= 32);
        assert!(overflow.load(Ordering::Acquire));
    }
    #[test]
    fn already_cancelled_does_not_spawn() {
        let control = Cancellation::new();
        control.cancel();
        assert!(matches!(
            execute(
                OsStr::new("nonexistent"),
                Path::new("."),
                &[],
                1,
                Duration::from_secs(1),
                &control
            ),
            Err(ProcessError::Cancelled)
        ));
    }
    #[cfg(unix)]
    #[test]
    fn timeout_terminates_and_reaps_processes() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let script = directory.path().join("fake-git");
        fs::write(&script, "#!/usr/bin/python3\nimport time\ntime.sleep(10)\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let start = Instant::now();
        let result = execute(
            script.as_os_str(),
            directory.path(),
            &[],
            1024,
            Duration::from_millis(20),
            &Cancellation::new(),
        );
        assert!(matches!(result, Err(ProcessError::Timeout)));
        assert!(start.elapsed() < Duration::from_secs(1));
    }
    #[cfg(unix)]
    #[test]
    fn timeout_applies_to_pipes_held_after_the_parent_has_exited() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let script = directory.path().join("fake-git");
        fs::write(&script, "#!/usr/bin/python3\nimport os, time\nif os.fork() == 0:\n    time.sleep(10)\nelse:\n    os._exit(0)\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let start = Instant::now();
        let result = execute(
            script.as_os_str(),
            directory.path(),
            &[],
            1024,
            Duration::from_millis(100),
            &Cancellation::new(),
        );
        assert!(matches!(result, Err(ProcessError::Timeout)));
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}
