# TuffBox Overlay — version matrix

Universal path: **OpenGL present-hook DLL** (`tuffbox_overlay_hook`) — any MC version,
any loader (Forge / Fabric / Quilt / NeoForge). Injected by the launcher after process start.

| Path | MC / loader | UI | YouTube | Friends/Chat | Status |
|------|-------------|----|---------|--------------|--------|
| GL hook | any | F8 framebuffer UI | feed + LRU thumbs + libmpv (optional DLL) | via Tauri IPC | **primary** |
| Legacy JVM jar | exact 1.21.1 Fabric/NeoForge | Screen + WATERMeDIA | full PiP | in-mod | opt-in `TUFFBOX_OVERLAY_JVM=1` |

## Layout

- `bridges/overlay-hook/` — Rust cdylib (`tuffbox_overlay_hook.dll`)
- `apps/tuffbox-desktop/src-tauri/src/overlay_hook.rs` — IPC proxy + inject
- `bridges/overlay/` — legacy Fabric/NeoForge 1.21.1 mod (not injected by default)

## Build

```powershell
cargo build -p tuffbox-overlay-hook --release
copy target\release\tuffbox_overlay_hook.dll apps\tuffbox-desktop\src-tauri\binaries\
```
