use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
};

#[derive(Debug, Clone)]
pub struct RunningProcess {
    pub profile_id: String,
    pub log_path: PathBuf,
}

/// Outcome of a spawned process exiting, handed to [`OnExit`] callbacks.
#[derive(Debug, Clone, Copy)]
pub struct ProcessExit {
    pub code: Option<i32>,
    /// Wall-clock seconds the process was alive (best-effort).
    pub duration_secs: u64,
}

/// Callback invoked once the spawned process exits. Used by the launcher to
/// detect JVM crashes and surface a categorized error instead of letting the
/// game die silently.
pub type OnExit = Box<dyn FnOnce(ProcessExit) + Send + 'static>;

lazy_static::lazy_static! {
    static ref PROCESSES: Mutex<HashMap<u32, RunningProcess>> = Mutex::new(HashMap::new());
}

/// Reads newline-delimited output from a reader, tolerating non-UTF-8 bytes.
///
/// Minecraft/Java processes occasionally emit output that isn't valid UTF-8
/// (e.g. platform-native paths or garbled native crash output). Using
/// `BufRead::lines()` there would drop/terminate the stream on the first
/// invalid byte sequence (it maps `InvalidData` to an `Err`, which
/// `.flatten()`/`?` silently swallows), losing the rest of the log forever.
/// This reads raw bytes and lossily decodes each line instead so the log
/// capture never stalls or truncates on non-UTF-8 output.
pub fn read_lines_lossy(mut reader: impl BufRead) -> impl Iterator<Item = String> {
    std::iter::from_fn(move || {
        let mut buf = Vec::new();
        match reader.read_until(b'\n', &mut buf) {
            Ok(0) => None,
            Ok(_) => {
                while matches!(buf.last(), Some(b'\n') | Some(b'\r')) {
                    buf.pop();
                }
                Some(String::from_utf8_lossy(&buf).into_owned())
            }
            Err(_) => None,
        }
    })
}

pub fn spawn_and_track(
    profile_id: String,
    cmd: Command,
    log_path: impl AsRef<Path>,
) -> std::io::Result<RunningProcess> {
    spawn_and_track_with_cleanup(profile_id, cmd, log_path, Vec::new(), None)
}

pub fn spawn_and_track_with_cleanup(
    profile_id: String,
    mut cmd: Command,
    log_path: impl AsRef<Path>,
    cleanup_paths: Vec<PathBuf>,
    on_exit: Option<OnExit>,
) -> std::io::Result<RunningProcess> {
    let log_path = log_path.as_ref().to_path_buf();
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let log_file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&log_path)?;

    // On Windows, hide the child console window (e.g. the launched game).
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn()?;
    let pid = child.id();

    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");

    let mut log_file_clone = log_file.try_clone()?;

    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in read_lines_lossy(reader) {
            let _ = writeln!(log_file_clone, "{line}");
        }
    });

    let mut log_file_clone2 = log_file.try_clone()?;
    std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in read_lines_lossy(reader) {
            let _ = writeln!(log_file_clone2, "{line}");
        }
    });

    let info = RunningProcess {
        profile_id: profile_id.clone(),
        log_path: log_path.clone(),
    };
    PROCESSES
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(pid, info.clone());

    std::thread::spawn(move || {
        let started = std::time::Instant::now();
        let exit = child.wait();
        let duration_secs = started.elapsed().as_secs();
        PROCESSES
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&pid);
        for path in cleanup_paths {
            let _ = std::fs::remove_file(path);
        }
        if let Some(cb) = on_exit {
            cb(ProcessExit {
                code: exit.ok().and_then(|s| s.code()),
                duration_secs,
            });
        }
    });

    Ok(info)
}

pub fn list_running() -> Vec<RunningProcess> {
    PROCESSES.lock().unwrap().values().cloned().collect()
}

pub fn read_log_tail(path: &Path, limit: usize) -> std::io::Result<String> {
    if !path.exists() {
        return Ok(String::new());
    }
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let lines: Vec<String> = read_lines_lossy(reader).collect();
    let start = lines.len().saturating_sub(limit);
    Ok(lines[start..].join("\n"))
}
