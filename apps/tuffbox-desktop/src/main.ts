import { mount } from "svelte";
import App from "./App.svelte";
import "./styles/fonts.css";
import "./styles.css";
import "./styles/themes.css";
import "./styles/textures.css";
import { applyTheme, readStoredTheme } from "./lib/themes";
import { installActionFeedback } from "./lib/actionFeedback";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { installBrowserMockIfNeeded } from "./lib/browserMock";
import { installConsoleBridge } from "./lib/consoleBridge";

// Browser preview (e2b.app, vite dev without Tauri) — install full IPC mock
// synchronously before any component touches `invoke`/`listen`.
// installBrowserMockIfNeeded is async but its synchronous prelude (mockIPC)
// runs immediately; we don't need to await the promise for dev preview.
void installBrowserMockIfNeeded();

applyTheme(readStoredTheme(), false);
installActionFeedback();

// Unified logging (tauri-plugin-tracing): bridge JS console output into the
// Rust tracing subscriber, which writes the rotating logs/tuffbox.*.log file
// alongside Rust spans. Browser preview (no Tauri) skips the interception.
// Never use the plugin's interceptConsole() — see lib/consoleBridge.ts.
if (isTauri()) {
  installConsoleBridge(invoke);
}

// Self-hosted UI fonts come from @fontsource-variable/* via styles/fonts.css.
// A missing package or an unresolvable url() silently drops the whole app to
// the system stack (different metrics everywhere) — say so instead of shipping
// blind. `load()` forces the request, so an empty result means a real failure.
document.fonts
  .load('16px "Inter Variable"')
  .then((faces) => {
    if (faces.length === 0) {
      console.warn(
        '[tuffbox] UI font "Inter Variable" did not load — falling back to the system stack. ' +
          "Check that @fontsource-variable/inter is installed and the urls in styles/fonts.css resolve.",
      );
    }
  })
  .catch((e) => console.warn("[tuffbox] font probe failed:", e));

window.addEventListener("error", (event) => {
  console.error("[tuffbox] uncaught error:", event.error ?? event.message);
});

window.addEventListener("unhandledrejection", (event) => {
  console.error("[tuffbox] unhandled rejection:", event.reason);
});

const app = mount(App, {
  target: document.getElementById("app")!,
});

export default app;
