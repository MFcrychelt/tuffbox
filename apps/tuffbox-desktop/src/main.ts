import App from "./App.svelte";
import "./styles.css";
import "./styles/themes.css";
import { applyTheme, readStoredTheme } from "./lib/themes";

applyTheme(readStoredTheme(), false);

window.addEventListener("error", (event) => {
  console.error("[tuffbox] uncaught error:", event.error ?? event.message);
});

window.addEventListener("unhandledrejection", (event) => {
  console.error("[tuffbox] unhandled rejection:", event.reason);
});

const app = new App({
  target: document.getElementById("app")!,
});

export default app;
