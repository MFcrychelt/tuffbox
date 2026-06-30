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

lazy_static::lazy_static! {
    static ref PROCESSES: Mutex<HashMap<u32, RunningProcess>> = Mutex::new(HashMap::new());
}

pub fn spawn_and_track(
    profile_id: String,
    mut cmd: Command,
    log_path: impl AsRef<Path>,
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

    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut child = cmd.spawn()?;
    let pid = child.id();

    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");

    let mut log_file_clone = log_file.try_clone()?;

    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines().flatten() {
            let _ = writeln!(log_file_clone, "{line}");
        }
    });

    let mut log_file_clone2 = log_file.try_clone()?;
    std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines().flatten() {
            let _ = writeln!(log_file_clone2, "{line}");
        }
    });

    let info = RunningProcess {
        profile_id: profile_id.clone(),
        log_path: log_path.clone(),
    };
    PROCESSES.lock().unwrap().insert(pid, info.clone());

    std::thread::spawn(move || {
        let _ = child.wait();
        PROCESSES.lock().unwrap().remove(&pid);
    });

    Ok(info)
}

pub fn list_running() -> Vec<RunningProcess> {
    PROCESSES
        .lock()
        .unwrap()
        .values()
        .cloned()
        .collect()
}

pub fn read_log_tail(path: &Path, limit: usize) -> std::io::Result<String> {
    if !path.exists() {
        return Ok(String::new());
    }
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let lines: Vec<String> = reader.lines().collect::<Result<_, _>>()?;
    let start = lines.len().saturating_sub(limit);
    Ok(lines[start..].join("\n"))
}
