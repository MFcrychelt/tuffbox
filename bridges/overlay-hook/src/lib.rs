//! TuffBox overlay GL hook — injected into Minecraft (Windows).

#![allow(non_snake_case)]

#[cfg(windows)]
mod gl;
#[cfg(windows)]
mod ipc;
#[cfg(windows)]
mod lru;
#[cfg(windows)]
mod mpv;
#[cfg(windows)]
mod ui;

#[cfg(windows)]
mod win {
    use super::*;
    use once_cell::sync::OnceCell;
    use parking_lot::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};
    use windows::core::PCSTR;
    use windows::Win32::Foundation::{BOOL, HINSTANCE, LPARAM, LRESULT, TRUE, WPARAM};
    use windows::Win32::Graphics::Gdi::HDC;
    use windows::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
    use windows::Win32::System::SystemServices::{DLL_PROCESS_ATTACH, DLL_PROCESS_DETACH};
    use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_ESCAPE, VK_F8};
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, SetWindowsHookExW, UnhookWindowsHookEx, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT,
        WH_KEYBOARD_LL, WM_KEYDOWN,
    };

    static OVERLAY_OPEN: AtomicBool = AtomicBool::new(false);
    static HOOK_READY: AtomicBool = AtomicBool::new(false);
    static INPUT_HOOK: Mutex<Option<usize>> = Mutex::new(None);
    static UI_STATE: OnceCell<Mutex<ui::UiState>> = OnceCell::new();
    static ORIGINAL_SWAP: OnceCell<WglSwapBuffersFn> = OnceCell::new();
    static MH_ENABLED: AtomicBool = AtomicBool::new(false);

    type WglSwapBuffersFn = unsafe extern "system" fn(HDC) -> BOOL;

    fn ui_state() -> &'static Mutex<ui::UiState> {
        UI_STATE.get_or_init(|| Mutex::new(ui::UiState::new()))
    }

    unsafe extern "system" fn hooked_swap_buffers(hdc: HDC) -> BOOL {
        poll_hotkeys();
        if OVERLAY_OPEN.load(Ordering::SeqCst) {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                if let Some(mut st) = ui_state().try_lock() {
                    st.tick_and_draw(hdc);
                }
            }));
        } else if let Some(mut st) = ui_state().try_lock() {
            st.tick_background();
        }
        if let Some(orig) = ORIGINAL_SWAP.get() {
            orig(hdc)
        } else {
            TRUE
        }
    }

    fn poll_hotkeys() {
        static F8_WAS: AtomicBool = AtomicBool::new(false);
        let down = unsafe { GetAsyncKeyState(VK_F8.0 as i32) } < 0;
        let was = F8_WAS.load(Ordering::SeqCst);
        if down && !was {
            let open = !OVERLAY_OPEN.load(Ordering::SeqCst);
            OVERLAY_OPEN.store(open, Ordering::SeqCst);
            if open {
                if let Some(mut st) = ui_state().try_lock() {
                    st.on_open();
                }
                install_input_hook();
            } else {
                remove_input_hook();
            }
        }
        F8_WAS.store(down, Ordering::SeqCst);

        if OVERLAY_OPEN.load(Ordering::SeqCst)
            && unsafe { GetAsyncKeyState(VK_ESCAPE.0 as i32) } < 0
        {
            OVERLAY_OPEN.store(false, Ordering::SeqCst);
            remove_input_hook();
        }
    }

    unsafe extern "system" fn low_level_keyboard(
        code: i32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        if code == HC_ACTION as i32 && OVERLAY_OPEN.load(Ordering::SeqCst) {
            if wparam.0 as u32 == WM_KEYDOWN {
                let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
                if kb.vkCode != VK_F8.0 as u32 && kb.vkCode != VK_ESCAPE.0 as u32 {
                    return LRESULT(1);
                }
            }
        }
        CallNextHookEx(HHOOK::default(), code, wparam, lparam)
    }

    fn install_input_hook() {
        let mut g = INPUT_HOOK.lock();
        if g.is_some() {
            return;
        }
        unsafe {
            if let Ok(h) = SetWindowsHookExW(WH_KEYBOARD_LL, Some(low_level_keyboard), None, 0) {
                *g = Some(h.0 as usize);
            }
        }
    }

    fn remove_input_hook() {
        let mut g = INPUT_HOOK.lock();
        if let Some(raw) = g.take() {
            unsafe {
                let _ = UnhookWindowsHookEx(HHOOK(raw as *mut _));
            }
        }
    }

    fn install_gl_hook() -> Result<(), String> {
        unsafe {
            let opengl = GetModuleHandleA(PCSTR::from_raw(b"opengl32.dll\0".as_ptr()))
                .map_err(|e| format!("opengl32: {e}"))?;
            let sym = GetProcAddress(opengl, PCSTR::from_raw(b"wglSwapBuffers\0".as_ptr()))
                .ok_or_else(|| "wglSwapBuffers missing".to_string())?;
            let target: WglSwapBuffersFn = std::mem::transmute(sym);
            let _ = ORIGINAL_SWAP.set(target);

            minhook::MinHook::create_hook(target as _, hooked_swap_buffers as _)
                .map_err(|e| format!("MinHook create: {e:?}"))?;
            minhook::MinHook::enable_hook(target as _)
                .map_err(|e| format!("MinHook enable: {e:?}"))?;
            MH_ENABLED.store(true, Ordering::SeqCst);
        }
        HOOK_READY.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn bootstrap() {
        std::thread::spawn(|| {
            for _ in 0..50 {
                if unsafe { GetModuleHandleA(PCSTR::from_raw(b"opengl32.dll\0".as_ptr())).is_ok() }
                {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            match install_gl_hook() {
                Ok(()) => {
                    let _ = ipc::log_debug("tuffbox overlay hook ready (F8)");
                }
                Err(e) => {
                    let _ = ipc::log_debug(&format!("overlay hook failed: {e}"));
                }
            }
        });
    }

    #[no_mangle]
    pub unsafe extern "system" fn DllMain(
        _hinst: HINSTANCE,
        reason: u32,
        _reserved: *mut core::ffi::c_void,
    ) -> BOOL {
        match reason {
            DLL_PROCESS_ATTACH => bootstrap(),
            DLL_PROCESS_DETACH => {
                remove_input_hook();
                if MH_ENABLED.load(Ordering::SeqCst) {
                    if let Some(orig) = ORIGINAL_SWAP.get() {
                        let _ = minhook::MinHook::disable_hook((*orig) as _);
                        let _ = minhook::MinHook::remove_hook((*orig) as _);
                    }
                }
                mpv::shutdown();
            }
            _ => {}
        }
        TRUE
    }
}

#[cfg(windows)]
pub use win::*;

#[cfg(not(windows))]
#[no_mangle]
pub extern "C" fn tuffbox_overlay_hook_unsupported() {}
