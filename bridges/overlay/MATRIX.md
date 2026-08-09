# TuffBox Overlay — version matrix

In-game overlay (Discord/Steam style): F8 opens the overlay — YouTube player
(WATERMeDIA, keeps playing with the GUI closed + PiP HUD), Friends & Chat on
Supabase. The launcher injects the jar + `.tuffbox/overlay-session.json` at
launch time (see `crates/tuffbox-core/src/overlay_runtime.rs`).

## Anchors

| MC version | Fabric | NeoForge | Overlay GUI | YouTube (WATERMeDIA) | Friends/Chat | Status |
|------------|--------|----------|-------------|----------------------|--------------|--------|
| 1.21.1     | ✅     | ✅       | ✅          | ✅ (2.1.1 pinned)    | ✅           | production anchor |

Newer MC (1.21.4+) falls back to the highest anchor ≤ target — the
`~1.21.1` mod dependency bounds keep this strict: no silent forward-run.

## Layout (mirrors bridges/cosmetics)

- `core/` — Java 8 protocol module: session load, Supabase edge-function HTTP,
  social API (`OverlayCore`, `SocialApi`). No Minecraft classes.
- `common/` — shared 1.21.1 source tree (mounted into loader modules via
  `sourceSets.main.java.srcDir`). Vanilla-only UI (`Screen`/manual widgets),
  loader-neutral `OverlayRuntime`, media abstraction (`MediaBackend`),
  panels (YouTube / Friends / Chat), PiP HUD.
  `TuffBoxOverlayClient` (Fabric entrypoint) is excluded from the NeoForge
  compile (`sourceSets.main.java.exclude`).
- `fabric/` — Fabric 1.21.1 anchor (Fabric Loom, Fabric API).
- `neoforge/` — NeoForge 1.21.1 anchor (`RegisterKeyMappingsEvent`,
  `RegisterGuiLayersEvent`, `ClientTickEvent.Post`).

## Runtime dependencies

| Piece | Version | Source | Delivery |
|-------|---------|--------|----------|
| WATERMeDIA | 2.1.1 | Modrinth CDN (pinned sha256) | launcher injects into `mods/`, cleanup on exit |
| Supabase fns | overlay-friends / overlay-presence / overlay-chat-send / overlay-chat-poll | this repo | deployed separately |

VLC natives: WATERMeDIA auto-extracts bundled binaries on Windows x64.
macOS/Linux need a system VLC install — the YouTube panel shows a hint and
the rest of the overlay keeps working.

## Build

```powershell
$env:JAVA_HOME = "<JDK 21>"; .\gradlew.bat build syncDesktopResources
```

Produces `prebuilt/tuffbox-overlay-1.21.1-{fabric,neoforge}-0.1.0.jar`.
The launcher (`tuffbox-core`) picks jars from `prebuilt/` first, then module
`build/libs`, then `TUFFBOX_OVERLAY_DIR`.
