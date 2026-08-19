import { mount } from "svelte";
import App from "./App.svelte";
import "./styles.css";
import "./styles/themes.css";
import "./styles/textures.css";
import { applyTheme, readStoredTheme } from "./lib/themes";
import { installActionFeedback } from "./lib/actionFeedback";

applyTheme(readStoredTheme(), false);
installActionFeedback();

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
