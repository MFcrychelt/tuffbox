# TuffBox Overlay (in-game)

Discord/Steam-style overlay inside Minecraft: **F8** opens a full-screen overlay over the running game (never pauses singleplayer). Apps on the left rail:

1. **YouTube** — search/browse the shared Minecraft feed (`youtube_feed`, same data as the desktop home feed) or paste any URL; playback via WATERMeDIA (LibVLC). Closing the overlay does **not** stop audio; a PiP widget in the HUD corner keeps the video visible. Media keybinds: F9 play/pause, F10 stop, PgUp/PgDn volume, F7 PiP toggle (all rebindable under Controls → TuffBox Overlay).
2. **Friends** — add by username, incoming requests, presence (online / pack / server), presence opt-out toggle.
3. **Chat** — DM conversations with unread badges (rail + conversation list), ~4s polling.

## Layers

1. **In-game overlay mod** — [`bridges/overlay`](../bridges/overlay) (`core` Java 8 protocol + `common` loader-neutral UI + `fabric`/`neoforge` 1.21.1 anchors). See [`bridges/overlay/MATRIX.md`](../bridges/overlay/MATRIX.md).
2. **Launch inject** — [`overlay_runtime.rs`](../crates/tuffbox-core/src/overlay_runtime.rs) copies the nearest anchor jar into `mods/`, provisions WATERMeDIA 2.1.1 (pinned URL + sha256, download cache, cleanup on exit), writes `.tuffbox/overlay-session.json` (identity + Supabase creds + the same `writeSecret` the cosmetics profile uses).
3. **Supabase** — `player_presence` (heartbeat 30s, stale >2min = offline), `player_friendships` (pending/accepted), `chat_messages` (30d retention); edge functions `overlay-friends` / `overlay-presence` / `overlay-chat-send` / `overlay-chat-poll` (service role; writeSecret ownership against `cosmetics_profiles`, first social write binds a non-public stub profile).
4. **Desktop toggle** — Settings → General → "In-game overlay" (`ingameOverlay`, default on) gates the inject.

## Version matrix

| MC | Fabric | NeoForge |
|----|--------|----------|
| 1.21.1 | full | full |

Other versions/loaders: inject silently skips (game launches without the overlay).

## Privacy

- Presence is opt-in (`presenceOptIn`, toggle on the Friends page); opting out deletes the presence row.
- Chat requires an accepted friendship; messages are only readable via edge functions by the two participants (direct table access denied by RLS).
- WATERMeDIA is a runtime dependency (Polyform Strict license) — fetched from the official Modrinth CDN, never vendored.

## Deploy

```bash
supabase db push   # 016_overlay_social.sql
supabase functions deploy overlay-friends --no-verify-jwt
supabase functions deploy overlay-presence --no-verify-jwt
supabase functions deploy overlay-chat-send --no-verify-jwt
supabase functions deploy overlay-chat-poll --no-verify-jwt
```

## Build

```powershell
cd bridges/overlay
$env:JAVA_HOME = "<JDK 21>"; .\gradlew.bat build syncDesktopResources
```

## Known limits / next steps

- PiP is display-only (no in-HUD click handling — transport via keybinds or the overlay).
- Overlay↔desktop IPC (e.g. "now playing" in the launcher, remote control) is planned through the localhost pattern of `bridges/jei-runtime`.
- YouTube signature breakage is handled by repinning WATERMeDIA (`TUFFBOX_OVERLAY_WATERMEDIA_URL` / `_SHA256` env overrides allow an emergency repin without a launcher release).
- VLC natives: auto-extract on Windows x64; macOS/Linux need system VLC (panel shows a hint, social features unaffected).
