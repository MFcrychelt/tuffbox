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
}
