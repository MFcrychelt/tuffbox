//! Optional libmpv integration (dynamic LoadLibrary). Fail-soft if DLL missing.

use crate::gl;
use once_cell::sync::OnceCell;
use parking_lot::Mutex;
use std::ffi::{c_char, c_int, c_void, CString};

type MpvHandle = *mut c_void;

struct MpvApi {
    create: unsafe extern "C" fn() -> MpvHandle,
    initialize: unsafe extern "C" fn(MpvHandle) -> c_int,
    terminate: unsafe extern "C" fn(MpvHandle),
    set_option_string: unsafe extern "C" fn(MpvHandle, *const c_char, *const c_char) -> c_int,
    command_string: unsafe extern "C" fn(MpvHandle, *const c_char) -> c_int,
    _lib: libloading::Library,
}

static API: OnceCell<Option<MpvApi>> = OnceCell::new();
static PLAYER: Mutex<Option<Player>> = Mutex::new(None);

struct Player {
    handle: usize,
    tex: u32,
    width: i32,
    height: i32,
}

fn load_api() -> Option<&'static MpvApi> {
    API.get_or_init(|| unsafe { try_load() }).as_ref()
}

unsafe fn try_load() -> Option<MpvApi> {
    for name in ["mpv-2.dll", "mpv-1.dll", "libmpv-2.dll", "mpv.dll"] {
        let Ok(lib) = libloading::Library::new(name) else {
            continue;
        };
        let create = match lib.get::<unsafe extern "C" fn() -> MpvHandle>(b"mpv_create\0") {
            Ok(s) => *s,
            Err(_) => continue,
        };
        let initialize =
            match lib.get::<unsafe extern "C" fn(MpvHandle) -> c_int>(b"mpv_initialize\0") {
                Ok(s) => *s,
                Err(_) => continue,
            };
        let terminate = match lib.get::<unsafe extern "C" fn(MpvHandle)>(b"mpv_terminate_destroy\0")
        {
            Ok(s) => *s,
            Err(_) => continue,
        };
        let set_option_string = match lib.get::<unsafe extern "C" fn(
            MpvHandle,
            *const c_char,
            *const c_char,
        ) -> c_int>(b"mpv_set_option_string\0")
        {
            Ok(s) => *s,
            Err(_) => continue,
        };
        let command_string = match lib
            .get::<unsafe extern "C" fn(MpvHandle, *const c_char) -> c_int>(b"mpv_command_string\0")
        {
            Ok(s) => *s,
            Err(_) => continue,
        };
        return Some(MpvApi {
            create,
            initialize,
            terminate,
            set_option_string,
            command_string,
            _lib: lib,
        });
    }
    None
}

pub fn play_url(url: &str) -> Result<(), String> {
    let api = load_api()
        .ok_or_else(|| "libmpv not found (place mpv-2.dll next to the hook)".to_string())?;
    shutdown();
    unsafe {
        let handle = (api.create)();
        if handle.is_null() {
            return Err("mpv_create failed".into());
        }
        let vo = CString::new("null").unwrap();
        let key = CString::new("vo").unwrap();
        let _ = (api.set_option_string)(handle, key.as_ptr(), vo.as_ptr());
        let ytdl = CString::new("yes").unwrap();
        let ytdl_key = CString::new("ytdl").unwrap();
        let _ = (api.set_option_string)(handle, ytdl_key.as_ptr(), ytdl.as_ptr());
        if (api.initialize)(handle) < 0 {
            (api.terminate)(handle);
            return Err("mpv_initialize failed".into());
        }
        let cmd = CString::new(format!("loadfile {} replace", url.replace('\\', "/"))).unwrap();
        let _ = (api.command_string)(handle, cmd.as_ptr());

        let placeholder = vec![30u8, 30, 40, 255].repeat(16 * 16);
        let tex = gl::create_rgba_texture(16, 16, &placeholder);

        *PLAYER.lock() = Some(Player {
            handle: handle as usize,
            tex,
            width: 16,
            height: 16,
        });
    }
    Ok(())
}

pub fn toggle_pause() {
    let Some(api) = load_api() else {
        return;
    };
    let g = PLAYER.lock();
    if let Some(p) = g.as_ref() {
        unsafe {
            let cmd = CString::new("cycle pause").unwrap();
            let _ = (api.command_string)(p.handle as MpvHandle, cmd.as_ptr());
        }
    }
}

pub fn stop() {
    shutdown();
}

pub fn texture() -> Option<(u32, i32, i32)> {
    let g = PLAYER.lock();
    g.as_ref()
        .map(|p| (p.tex, p.width.max(1) * 40, p.height.max(1) * 22))
}

pub fn tick() {}

pub fn shutdown() {
    let api = load_api();
    let mut g = PLAYER.lock();
    if let Some(p) = g.take() {
        gl::delete_texture(p.tex);
        if let Some(api) = api {
            unsafe {
                (api.terminate)(p.handle as MpvHandle);
            }
        }
    }
}
