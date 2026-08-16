use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunningProcess {
    /// Stable instance key (usually the project manifest path).
    pub id: String,
    pub profile_id: String,
    pub pid: u32,
    pub log_path: PathBuf,
    /// Unix epoch seconds when the process was spawned.
    pub started_at: u64,
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
    /// PIDs re-attached from disk that we poll (no Child handle).
    static ref ORPHAN_WATCHED: Mutex<std::collections::HashSet<u32>> =
        Mutex::new(std::collections::HashSet::new());
    /// Processes that exited while an orphan watcher was running.
    static ref EXITED_QUEUE: Mutex<Vec<RunningProcess>> = Mutex::new(Vec::new());
}

/// Best-effort: is OS process `pid` still alive?
pub fn pid_is_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    #[cfg(windows)]
    {
        // PROCESS_QUERY_LIMITED_INFORMATION + GetExitCodeProcess(STILL_ACTIVE).
        type Handle = *mut std::ffi::c_void;
        #[link(name = "kernel32")]
        extern "system" {
            fn OpenProcess(access: u32, inherit: i32, pid: u32) -> Handle;
            fn CloseHandle(h: Handle) -> i32;
            fn GetExitCodeProcess(h: Handle, code: *mut u32) -> i32;
        }
        const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
        const STILL_ACTIVE: u32 = 259;
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
            if h.is_null() {
                return false;
            }
            let mut code = 0u32;
            let ok = GetExitCodeProcess(h, &mut code);
            CloseHandle(h);
            ok != 0 && code == STILL_ACTIVE
        }
    }
    #[cfg(unix)]
    {
        // signal 0 = existence check; EPERM also means the process exists.
        let status = Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        matches!(status, Ok(s) if s.success())
            || Path::new(&format!("/proc/{pid}")).exists()
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = pid;
        true
    }
}

fn registry_path() -> PathBuf {
    dirs::data_local_dir()
        .or_else(dirs::config_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("TuffBox")
        .join("running-instances.json")
}

fn persist_registry(map: &HashMap<u32, RunningProcess>) {
    let path = registry_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let list: Vec<&RunningProcess> = map.values().collect();
    if let Ok(json) = serde_json::to_string_pretty(&list) {
        let _ = std::fs::write(path, json);
    }
}

fn load_registry() -> Vec<RunningProcess> {
    let path = registry_path();
    let Ok(raw) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

fn prune_dead_locked(map: &mut HashMap<u32, RunningProcess>) -> Vec<RunningProcess> {
    let dead: Vec<u32> = map
        .keys()
        .copied()
        .filter(|pid| !pid_is_alive(*pid))
        .collect();
    let mut exited = Vec::with_capacity(dead.len());
    for pid in dead {
        if let Some(proc) = map.remove(&pid) {
            exited.push(proc);
        }
    }
    exited
}

fn ensure_orphan_watcher(proc: RunningProcess) {
    {
        let mut watched = ORPHAN_WATCHED.lock().unwrap_or_else(|e| e.into_inner());
        if !watched.insert(proc.pid) {
            return;
        }
    }
    std::thread::spawn(move || {
        while pid_is_alive(proc.pid) {
            std::thread::sleep(std::time::Duration::from_millis(1500));
        }
        let removed = {
            let mut map = PROCESSES.lock().unwrap_or_else(|e| e.into_inner());
            let removed = map.remove(&proc.pid);
            if removed.is_some() {
                persist_registry(&map);
            }
            removed
        };
        {
            let mut watched = ORPHAN_WATCHED.lock().unwrap_or_else(|e| e.into_inner());
            watched.remove(&proc.pid);
        }
        // Only queue if we were first to notice — avoids duplicate process-exited.
        if let Some(proc) = removed {
            EXITED_QUEUE
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(proc);
        }
    });
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
    instance_id: String,
    profile_id: String,
    cmd: Command,
    log_path: impl AsRef<Path>,
) -> std::io::Result<RunningProcess> {
    spawn_and_track_with_cleanup(
        instance_id,
        profile_id,
        cmd,
        log_path,
        Vec::new(),
        None,
        false,
    )
}

pub fn spawn_and_track_with_cleanup(
    instance_id: String,
    profile_id: String,
    mut cmd: Command,
    log_path: impl AsRef<Path>,
    cleanup_paths: Vec<PathBuf>,
    on_exit: Option<OnExit>,
    show_console: bool,
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

    // On Windows: hide the game window by default; for server runs open a
    // real console so stdout/stderr (and stdin for commands) are visible.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        if show_console {
            cmd.creation_flags(0x00000010); // CREATE_NEW_CONSOLE
        } else {
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
    }

    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    if show_console {
        // Keep stdin attached so the OS console can send server commands.
        cmd.stdin(Stdio::inherit());
    }

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

    // Stabilize briefly so an instantly-crashing JVM doesn't look "running".
    let stabilize_until = std::time::Instant::now() + std::time::Duration::from_millis(800);
    loop {
        match child.try_wait()? {
            Some(status) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!(
                        "Minecraft exited immediately (code {:?}). Check logs/tuffbox-console.log and logs/latest.log.",
                        status.code()
                    ),
                ));
            }
            None => {
                if std::time::Instant::now() >= stabilize_until {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
        }
    }

    let started_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let info = RunningProcess {
        id: instance_id,
        profile_id,
        pid,
        log_path: log_path.clone(),
        started_at,
    };
    {
        let mut map = PROCESSES.lock().unwrap_or_else(|e| e.into_inner());
        map.insert(pid, info.clone());
        persist_registry(&map);
    }

    std::thread::spawn(move || {
        let started = std::time::Instant::now();
        let exit = child.wait();
        let duration_secs = started.elapsed().as_secs();
        {
            let mut map = PROCESSES.lock().unwrap_or_else(|e| e.into_inner());
            map.remove(&pid);
            persist_registry(&map);
        }
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

/// Alive processes, plus any that exited since the last call (orphan watchers / prune).
pub fn list_running_detailed() -> (Vec<RunningProcess>, Vec<RunningProcess>) {
    let mut map = PROCESSES.lock().unwrap_or_else(|e| e.into_inner());
    // Re-attach processes that outlived a previous launcher session.
    for proc in load_registry() {
        if pid_is_alive(proc.pid) {
            map.entry(proc.pid).or_insert_with(|| {
                ensure_orphan_watcher(proc.clone());
                proc
            });
        }
    }
    let mut exited = prune_dead_locked(&mut map);
    {
        let mut queued = EXITED_QUEUE.lock().unwrap_or_else(|e| e.into_inner());
        exited.append(&mut queued);
    }
    if !exited.is_empty() {
        persist_registry(&map);
    }
    (map.values().cloned().collect(), exited)
}

pub fn list_running() -> Vec<RunningProcess> {
    list_running_detailed().0
}

fn instance_key(id: &str) -> String {
    id.replace('\\', "/")
        .trim_end_matches('/')
        .to_ascii_lowercase()
}

/// True if any tracked (and still-alive) process belongs to `instance_id`.
pub fn is_instance_running(instance_id: &str) -> bool {
    let key = instance_key(instance_id);
    list_running()
        .into_iter()
        .any(|p| instance_key(&p.id) == key)
}

/// Force-kill every tracked process for `instance_id`. The wait thread still
/// runs `on_exit` afterward (playtime / crash classification / UI events).
pub fn kill_instance(instance_id: &str) -> std::io::Result<usize> {
    let key = instance_key(instance_id);
    let pids: Vec<u32> = list_running()
        .into_iter()
        .filter(|p| instance_key(&p.id) == key)
        .map(|p| p.pid)
        .collect();
    for pid in &pids {
        kill_pid(*pid)?;
    }
    // Optimistic prune — wait thread will also remove + persist.
    {
        let mut map = PROCESSES.lock().unwrap_or_else(|e| e.into_inner());
        for pid in &pids {
            map.remove(pid);
        }
        persist_registry(&map);
    }
    Ok(pids.len())
}

fn kill_pid(pid: u32) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let status = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .creation_flags(0x08000000) // CREATE_NO_WINDOW
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("taskkill exited with {status}"),
            ))
        }
    }
    #[cfg(not(windows))]
    {
        let status = Command::new("kill")
            .args(["-TERM", &pid.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()?;
        if status.success() {
            Ok(())
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("kill exited with {status}"),
            ))
        }
    }
}

pub fn read_log_tail(path: &Path, limit: usize) -> std::io::Result<String> {
    if !path.exists() {
        return Ok(String::new());
    }
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let lines: Vec<String> = read_lines_lossy(reader).collect();
    let start = lines.len().saturating_sub(limit);
    Ok(format_minecraft_log_for_display(&lines[start..].join("\n")))
}

/// Strip common CSI ANSI sequences (colors/cursor) from JVM console output.
fn strip_ansi_codes(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' && chars.peek() == Some(&'[') {
            chars.next();
            while let Some(n) = chars.next() {
                if n.is_ascii_alphabetic() {
                    break;
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

fn format_log4j_timestamp(raw: &str) -> String {
    // LegacyXMLLayout uses millis since epoch; fall back to the raw attribute.
    if let Ok(ms) = raw.parse::<u64>() {
        let secs = ms / 1000;
        let h = (secs / 3600) % 24;
        let m = (secs / 60) % 60;
        let s = secs % 60;
        return format!("{h:02}:{m:02}:{s:02}");
    }
    raw.to_string()
}

fn decode_cdata(message: &str) -> String {
    let trimmed = message.trim();
    if let Some(inner) = trimmed
        .strip_prefix("<![CDATA[")
        .and_then(|s| s.strip_suffix("]]>"))
    {
        return inner.to_string();
    }
    trimmed
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

/// Turn Minecraft `LegacyXMLLayout` console dumps into human PatternLayout-like
/// lines (`[HH:mm:ss] [thread/LEVEL]: message`). Plain `latest.log` text is
/// returned unchanged (aside from ANSI stripping).
pub fn format_minecraft_log_for_display(raw: &str) -> String {
    let stripped = strip_ansi_codes(raw);
    if !stripped.contains("<log4j:event") && !stripped.contains("<Event ") {
        return stripped;
    }

    let mut out = String::with_capacity(stripped.len() / 2);
    let mut rest = stripped.as_str();
    while let Some(start) = rest
        .find("<log4j:event")
        .or_else(|| rest.find("<Event "))
    {
        if start > 0 {
            let prefix = rest[..start].trim();
            if !prefix.is_empty() {
                if !out.is_empty() {
                    out.push('\n');
                }
                out.push_str(prefix);
            }
        }
        rest = &rest[start..];
        let end_tag = if rest.starts_with("<log4j:event") {
            "</log4j:event>"
        } else {
            "</Event>"
        };
        let Some(end) = rest.find(end_tag) else {
            break;
        };
        let event = &rest[..end + end_tag.len()];
        rest = &rest[end + end_tag.len()..];

        let level = attr(event, "level").unwrap_or("INFO");
        let thread = attr(event, "thread").unwrap_or("?");
        let ts = attr(event, "timestamp")
            .map(format_log4j_timestamp)
            .unwrap_or_else(|| "--:--:--".into());
        let message = message_body(event);
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&format!("[{ts}] [{thread}/{level}]: {message}"));
    }
    let trailing = rest.trim();
    if !trailing.is_empty() {
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(trailing);
    }
    if out.is_empty() {
        stripped
    } else {
        out
    }
}

fn attr<'a>(event: &'a str, name: &str) -> Option<&'a str> {
    let key = format!("{name}=\"");
    let idx = event.find(&key)?;
    let start = idx + key.len();
    let end = event[start..].find('"')? + start;
    Some(&event[start..end])
}

fn message_body(event: &str) -> String {
    for (open, close) in [
        ("<log4j:message>", "</log4j:message>"),
        ("<Message>", "</Message>"),
    ] {
        if let Some(i) = event.find(open) {
            let start = i + open.len();
            if let Some(rel) = event[start..].find(close) {
                let mut msg = decode_cdata(&event[start..start + rel]);
                // Append throwable if present (stack traces).
                for (t_open, t_close) in [
                    ("<log4j:Throwable>", "</log4j:Throwable>"),
                    ("<Throwable>", "</Throwable>"),
                ] {
                    if let Some(ti) = event.find(t_open) {
                        let ts = ti + t_open.len();
                        if let Some(tr) = event[ts..].find(t_close) {
                            let thr = decode_cdata(&event[ts..ts + tr]);
                            if !thr.is_empty() {
                                msg.push('\n');
                                msg.push_str(&thr);
                            }
                        }
                    }
                }
                return msg;
            }
        }
    }
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_legacy_xml_layout_events() {
        let raw = r#"<log4j:event logger="net.minecraft.client.main.Main" timestamp="1710000000000" level="INFO" thread="main">
<log4j:message><![CDATA[Launching game]]></log4j:message>
</log4j:event>
<log4j:event logger="log4j" timestamp="1710000001000" level="WARN" thread="Worker-1">
<log4j:message><![CDATA[Something happened]]></log4j:message>
</log4j:event>"#;
        let formatted = format_minecraft_log_for_display(raw);
        assert!(!formatted.contains("log4j:event"), "{formatted}");
        assert!(!formatted.contains("timestamp="), "{formatted}");
        assert!(formatted.contains("Launching game"), "{formatted}");
        assert!(formatted.contains("[main/INFO]"), "{formatted}");
        assert!(formatted.contains("Something happened"), "{formatted}");
    }

    #[test]
    fn leaves_pattern_layout_untouched() {
        let raw = "[12:00:01] [main/INFO]: Hello\n[12:00:02] [Render/WARN]: Slow";
        assert_eq!(format_minecraft_log_for_display(raw), raw);
    }

    #[test]
    fn current_pid_is_alive() {
        assert!(pid_is_alive(std::process::id()));
        assert!(!pid_is_alive(0));
        // Extremely unlikely to be a live PID on a developer machine.
        assert!(!pid_is_alive(u32::MAX - 7));
    }

    #[test]
    fn instance_key_normalizes_slashes_and_case() {
        assert_eq!(
            instance_key(r"C:\Dev\Pack\tuffbox.json"),
            instance_key("c:/dev/pack/tuffbox.json/")
        );
    }

    #[test]
    fn process_exit_carries_code_and_duration() {
        let exit = ProcessExit {
            code: Some(1),
            duration_secs: 3,
        };
        assert_eq!(exit.code, Some(1));
        assert_eq!(exit.duration_secs, 3);
    }
}
