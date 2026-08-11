//! Overlay GL-hook support: localhost JSON proxy + DLL inject into the game process.
//!
//! Env for the JVM (set before spawn):
//! - `TUFFBOX_OVERLAY_IPC=127.0.0.1:{port}`
//! - `TUFFBOX_OVERLAY_SESSION=<path to overlay-session.json>`

use once_cell::sync::Lazy;
use serde::Serialize;
use serde_json::json;
use std::io::{Read, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU16, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

static IPC_PORT: AtomicU16 = AtomicU16::new(0);
static IPC_RUNNING: AtomicBool = AtomicBool::new(false);
static SESSION_PATH: Lazy<Mutex<Option<PathBuf>>> = Lazy::new(|| Mutex::new(None));

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct OverlaySession {
    username: String,
    uuid: String,
    api_base: String,
    anon_key: String,
    #[serde(default)]
    write_secret: String,
    #[serde(default)]
    pack_name: String,
}

fn load_session() -> Result<OverlaySession, String> {
    let path = SESSION_PATH
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "overlay session path not set".to_string())?;
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("read session: {e}"))?;
    serde_json::from_str(&raw).map_err(|e| format!("parse session: {e}"))
}

fn http_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())
}

fn edge_url(base: &str, name: &str) -> String {
    let base = base.trim_end_matches('/');
    format!("{base}/functions/v1/{name}")
}

fn rest_url(base: &str, path_and_query: &str) -> String {
    let base = base.trim_end_matches('/');
    format!("{base}/rest/v1/{path_and_query}")
}

fn handle_session() -> Result<serde_json::Value, String> {
    let s = load_session()?;
    Ok(json!({
        "username": s.username,
        "uuid": s.uuid,
        "packName": s.pack_name,
        "apiBase": s.api_base,
    }))
}

fn handle_youtube_feed() -> Result<serde_json::Value, String> {
    let s = load_session()?;
    let client = http_client()?;
    let url = rest_url(
        &s.api_base,
        "youtube_feed?select=video_id,title,thumbnail_url,channel_name,source,lang,view_count,published_at&order=view_count.desc&limit=40",
    );
    let resp = client
        .get(&url)
        .header("apikey", &s.anon_key)
        .header("Authorization", format!("Bearer {}", s.anon_key))
        .send()
        .map_err(|e| e.to_string())?;
    let status = resp.status();
    let body: serde_json::Value = resp.json().map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("youtube_feed {status}: {body}"));
    }
    // Normalize for the hook UI.
    let items = body
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|row| {
            json!({
                "id": row.get("video_id").cloned().unwrap_or(json!("")),
                "title": row.get("title").cloned().unwrap_or(json!("")),
                "thumbnail_url": row.get("thumbnail_url").cloned().unwrap_or(json!("")),
                "channel": row.get("channel_name").cloned().unwrap_or(json!("")),
            })
        })
        .collect::<Vec<_>>();
    Ok(json!({ "items": items }))
}

fn handle_friends() -> Result<serde_json::Value, String> {
    let s = load_session()?;
    let client = http_client()?;
    let url = edge_url(&s.api_base, "overlay-friends");
    let resp = client
        .post(&url)
        .header("apikey", &s.anon_key)
        .header("Authorization", format!("Bearer {}", s.anon_key))
        .header("Content-Type", "application/json")
        .json(&json!({
            "action": "list",
            "playerKey": s.uuid,
            "username": s.username,
            "writeSecret": s.write_secret,
        }))
        .send()
        .map_err(|e| e.to_string())?;
    let status = resp.status();
    let body: serde_json::Value = resp.json().unwrap_or_else(|_| json!({ "error": "bad json" }));
    if !status.is_success() {
        return Err(format!("friends {status}: {body}"));
    }
    Ok(body)
}

fn handle_chat_poll(peer: Option<&str>) -> Result<serde_json::Value, String> {
    let s = load_session()?;
    let client = http_client()?;
    let url = edge_url(&s.api_base, "overlay-chat-poll");
    let mut payload = json!({
        "playerKey": s.uuid,
        "username": s.username,
        "writeSecret": s.write_secret,
    });
    if let Some(p) = peer.filter(|p| !p.is_empty()) {
        payload["peerKey"] = json!(p);
    }
    let resp = client
        .post(&url)
        .header("apikey", &s.anon_key)
        .header("Authorization", format!("Bearer {}", s.anon_key))
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .map_err(|e| e.to_string())?;
    let status = resp.status();
    let body: serde_json::Value = resp.json().unwrap_or_else(|_| json!({ "error": "bad json" }));
    if !status.is_success() {
        return Err(format!("chat-poll {status}: {body}"));
    }
    Ok(body)
}

fn handle_youtube_resolve(id: &str) -> Result<serde_json::Value, String> {
    let id = id.trim();
    if id.is_empty() {
        return Err("id required".into());
    }
    // Hook plays via libmpv with a YouTube URL; ytdl inside mpv resolves streams.
    Ok(json!({
        "id": id,
        "url": format!("https://www.youtube.com/watch?v={id}"),
    }))
}

fn route(method: &str, path: &str, query: &str) -> (u16, serde_json::Value) {
    if method != "GET" && method != "POST" {
        return (405, json!({ "error": "method not allowed" }));
    }
    let path = path.split('?').next().unwrap_or(path);
    let result = match path {
        "/health" => Ok(json!({ "ok": true })),
        "/session" => handle_session(),
        "/youtube-feed" => handle_youtube_feed(),
        "/friends" => handle_friends(),
        "/chat" | "/chat/poll" => {
            let peer = query
                .split('&')
                .find_map(|p| p.strip_prefix("peer="))
                .map(|s| s.to_string());
            handle_chat_poll(peer.as_deref())
        }
        "/youtube-resolve" => {
            let id = query
                .split('&')
                .find_map(|p| p.strip_prefix("id="))
                .unwrap_or("");
            handle_youtube_resolve(id)
        }
        _ => Err(format!("not found: {path}")),
    };
    match result {
        Ok(v) => (200, v),
        Err(e) => (500, json!({ "error": e })),
    }
}

fn serve_connection(mut stream: std::net::TcpStream) {
    let mut buf = [0u8; 8192];
    let n = match stream.read(&mut buf) {
        Ok(n) if n > 0 => n,
        _ => return,
    };
    let req = String::from_utf8_lossy(&buf[..n]);
    let mut lines = req.lines();
    let first = lines.next().unwrap_or("");
    let mut parts = first.split_whitespace();
    let method = parts.next().unwrap_or("GET");
    let target = parts.next().unwrap_or("/");
    let (path, query) = match target.split_once('?') {
        Some((p, q)) => (p, q),
        None => (target, ""),
    };
    let (status, body) = route(method, path, query);
    let body_bytes = serde_json::to_vec(&body).unwrap_or_else(|_| b"{}".to_vec());
    let reason = if status == 200 { "OK" } else { "Error" };
    let _ = write!(
        stream,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nAccess-Control-Allow-Origin: *\r\n\r\n",
        body_bytes.len()
    );
    let _ = stream.write_all(&body_bytes);
}

/// Start (or reuse) the overlay IPC server. Returns `127.0.0.1:port`.
pub fn ensure_ipc_server(session_path: &Path) -> Result<String, String> {
    {
        let mut g = SESSION_PATH.lock().map_err(|e| e.to_string())?;
        *g = Some(session_path.to_path_buf());
    }

    let existing = IPC_PORT.load(Ordering::SeqCst);
    if existing != 0 && IPC_RUNNING.load(Ordering::SeqCst) {
        return Ok(format!("127.0.0.1:{existing}"));
    }

    let listener = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| format!("bind overlay ipc: {e}"))?;
    listener
        .set_nonblocking(false)
        .map_err(|e| e.to_string())?;
    let port = listener
        .local_addr()
        .map_err(|e| e.to_string())?
        .port();
    IPC_PORT.store(port, Ordering::SeqCst);
    IPC_RUNNING.store(true, Ordering::SeqCst);

    thread::Builder::new()
        .name("tuffbox-overlay-ipc".into())
        .spawn(move || {
            for conn in listener.incoming() {
                if !IPC_RUNNING.load(Ordering::SeqCst) {
                    break;
                }
                if let Ok(stream) = conn {
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
                    let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));
                    // One request per connection; keep it simple for the hook.
                    serve_connection(stream);
                }
            }
            IPC_RUNNING.store(false, Ordering::SeqCst);
        })
        .map_err(|e| format!("spawn ipc thread: {e}"))?;

    Ok(format!("127.0.0.1:{port}"))
}

pub fn stop_ipc_server() {
    let port = IPC_PORT.load(Ordering::SeqCst);
    IPC_RUNNING.store(false, Ordering::SeqCst);
    if port != 0 {
        let _ = std::net::TcpStream::connect_timeout(
            &SocketAddr::from(([127, 0, 0, 1], port)),
            Duration::from_millis(200),
        );
    }
    IPC_PORT.store(0, Ordering::SeqCst);
}

pub fn ipc_endpoint() -> Option<String> {
    let port = IPC_PORT.load(Ordering::SeqCst);
    if port == 0 {
        None
    } else {
        Some(format!("127.0.0.1:{port}"))
    }
}

/// Locate `tuffbox_overlay_hook.dll` next to the exe / resources / env.
pub fn find_hook_dll() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("TUFFBOX_OVERLAY_HOOK_DLL") {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            return Some(pb);
        }
    }
    let mut roots = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            roots.push(dir.to_path_buf());
            roots.push(dir.join("resources"));
            roots.push(dir.join("overlay-hook"));
            roots.push(dir.join("binaries"));
        }
    }
    // Dev: workspace bridges/overlay-hook target
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    roots.push(manifest_dir.join("binaries"));
    roots.push(
        manifest_dir
            .join("../../../target/release")
            .canonicalize()
            .unwrap_or_else(|_| manifest_dir.join("../../../target/release")),
    );
    roots.push(
        manifest_dir
            .join("../../../target/debug")
            .canonicalize()
            .unwrap_or_else(|_| manifest_dir.join("../../../target/debug")),
    );
    roots.push(manifest_dir.join("../../../bridges/overlay-hook/target/release"));
    roots.push(manifest_dir.join("../../../bridges/overlay-hook/target/debug"));

    for root in roots {
        for name in [
            "tuffbox_overlay_hook.dll",
            "tuffbox_overlay_hook.dll.dll",
            "overlay_hook.dll",
        ] {
            let p = root.join(name);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

#[cfg(windows)]
pub fn inject_hook_dll(pid: u32) -> Result<String, String> {
    let dll = find_hook_dll().ok_or_else(|| {
        "tuffbox_overlay_hook.dll not found — build bridges/overlay-hook (cdylib)".to_string()
    })?;
    inject_dll_loadlibrary(pid, &dll)?;
    Ok(format!("injected {}", dll.display()))
}

#[cfg(not(windows))]
pub fn inject_hook_dll(_pid: u32) -> Result<String, String> {
    Err("overlay GL hook inject is Windows-only".into())
}

#[cfg(windows)]
fn inject_dll_loadlibrary(pid: u32, dll: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::{PCSTR, PCWSTR};
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Diagnostics::Debug::WriteProcessMemory;
    use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
    use windows::Win32::System::Memory::{
        VirtualAllocEx, VirtualFreeEx, MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE,
    };
    use windows::Win32::System::Threading::{
        OpenProcess, WaitForSingleObject, PROCESS_CREATE_THREAD, PROCESS_QUERY_INFORMATION,
        PROCESS_VM_OPERATION, PROCESS_VM_READ, PROCESS_VM_WRITE, INFINITE,
    };

    type CreateRemoteThreadFn = unsafe extern "system" fn(
        windows::Win32::Foundation::HANDLE,
        *const core::ffi::c_void,
        usize,
        Option<unsafe extern "system" fn(*mut core::ffi::c_void) -> u32>,
        *mut core::ffi::c_void,
        u32,
        *mut u32,
    ) -> windows::Win32::Foundation::HANDLE;

    let wide: Vec<u16> = dll
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let bytes = wide.len() * 2;

    unsafe {
        let process = OpenProcess(
            PROCESS_CREATE_THREAD
                | PROCESS_QUERY_INFORMATION
                | PROCESS_VM_OPERATION
                | PROCESS_VM_WRITE
                | PROCESS_VM_READ,
            false,
            pid,
        )
        .map_err(|e| format!("OpenProcess: {e}"))?;

        let remote = VirtualAllocEx(
            process,
            None,
            bytes,
            MEM_COMMIT | MEM_RESERVE,
            PAGE_READWRITE,
        );
        if remote.is_null() {
            let _ = CloseHandle(process);
            return Err("VirtualAllocEx failed".into());
        }

        let mut written = 0usize;
        if WriteProcessMemory(
            process,
            remote,
            wide.as_ptr() as *const _,
            bytes,
            Some(&mut written),
        )
        .is_err()
            || written != bytes
        {
            let _ = VirtualFreeEx(process, remote, 0, MEM_RELEASE);
            let _ = CloseHandle(process);
            return Err("WriteProcessMemory failed".into());
        }

        let k32 = GetModuleHandleW(PCWSTR(windows::core::w!("kernel32.dll").as_ptr()))
            .map_err(|e| format!("GetModuleHandleW: {e}"))?;
        let load_library = GetProcAddress(k32, PCSTR(b"LoadLibraryW\0".as_ptr()))
            .ok_or_else(|| "LoadLibraryW not found".to_string())?;
        let create_remote = GetProcAddress(k32, PCSTR(b"CreateRemoteThread\0".as_ptr()))
            .ok_or_else(|| "CreateRemoteThread not found".to_string())?;
        let create_remote: CreateRemoteThreadFn = std::mem::transmute(create_remote);

        let thread = create_remote(
            process,
            std::ptr::null(),
            0,
            Some(std::mem::transmute(load_library)),
            remote,
            0,
            std::ptr::null_mut(),
        );
        if thread.is_invalid() {
            let _ = VirtualFreeEx(process, remote, 0, MEM_RELEASE);
            let _ = CloseHandle(process);
            return Err("CreateRemoteThread failed".into());
        }

        let _ = WaitForSingleObject(thread, INFINITE);
        let _ = CloseHandle(thread);
        let _ = VirtualFreeEx(process, remote, 0, MEM_RELEASE);
        let _ = CloseHandle(process);
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayHookLaunchInfo {
    pub ipc: String,
    pub session_path: String,
    pub hook_dll: Option<String>,
}
