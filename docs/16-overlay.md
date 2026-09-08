# TuffBox Overlay (universal GL hook)

Steam-style overlay for **any Minecraft version / any loader**: a native DLL is injected into
the game process and detours `opengl32!wglSwapBuffers`, drawing UI into the game framebuffer
(works in **exclusive fullscreen**).

## Architecture

Two processes, no Chromium inside the game:

| Process | Role |
|---------|------|
| **TuffBox launcher (Tauri)** | Backend / IPC proxy — Supabase friends/chat, `youtube_feed`, session |
| **Game JVM + `tuffbox_overlay_hook.dll`** | Frontend — OpenGL present-hook, immediate UI, LRU GPU thumbnails, optional libmpv |

```
Play → write .tuffbox/overlay-session.json
     → start localhost IPC (TUFFBOX_OVERLAY_IPC)
     → spawn JVM with env
     → inject tuffbox_overlay_hook.dll
     → detour wglSwapBuffers → F8 UI
```

### IPC endpoints (`http://127.0.0.1:{port}`)

- `GET /health`
- `GET /session`
- `GET /youtube-feed`
- `GET /friends`
- `GET /chat`
- `GET /youtube-resolve?id=`

### Hook UI (F8)

- Rail: YouTube / Friends / Chat
- YouTube: feed JSON + LRU texture cache (~30) for thumbnails
- Click: libmpv `loadfile` (needs `mpv-2.dll` on PATH / next to the DLL); audio works with `vo=null`; full in-framebuffer video frames are a follow-up
- Esc / F8 closes; keyboard LL-hook swallows input while open

## Legacy JVM mod

[`bridges/overlay`](../bridges/overlay) Fabric/NeoForge **1.21.1** jar is **not** injected by default.
Set `TUFFBOX_OVERLAY_JVM=1` to also copy the old jar + WATERMeDIA (exact 1.21.1 only).

## Build hook DLL

```powershell
cargo build -p tuffbox-overlay-hook --release
# → target/release/tuffbox_overlay_hook.dll
# Copy next to the desktop binary or set TUFFBOX_OVERLAY_HOOK_DLL
```

Optional: copy `mpv-2.dll` (from an mpv Windows build) beside the hook for playback.

## Settings

Settings → General → **In-game overlay** (`ingameOverlay`) gates session + IPC + inject.

## Deploy (social backend)

```bash
supabase db push   # 016_overlay_social.sql
supabase functions deploy overlay-friends --no-verify-jwt
supabase functions deploy overlay-presence --no-verify-jwt
supabase functions deploy overlay-chat-send --no-verify-jwt
supabase functions deploy overlay-chat-poll --no-verify-jwt
```

## Privacy

Unchanged: presence opt-in, chat requires friendship, edge functions own writes via `writeSecret`.
