/** Theme catalog (recreated for TuffBox CSS vars). */

export type ThemeId =
  | "tuffbox"
  | "tuffbox-light"
  | "carbon"
  | "inferno"
  | "aether"
  | "frost"
  | "pixelato"
  | "win95"
  | "solar"
  | "fern"
  | "blaze"
  | "dusk"
  | "glacier";

export interface ThemeMeta {
  id: ThemeId;
  label: string;
  /** Swatch colors for the card preview (bg deep → mid → accent). */
  shades: [string, string, string];
  /** Picker badge only — does not gate rounded-corners CSS. */
  badge?: "Light" | "Sharp" | "Minimal";
}

export const THEMES: ThemeMeta[] = [
  { id: "tuffbox", label: "TuffBox", shades: ["#0b0b0d", "#18181b", "#1bd96a"] },
  { id: "tuffbox-light", label: "TuffBox Light", shades: ["#f8f9fa", "#e9ecef", "#0ca84c"], badge: "Light" },
  { id: "carbon", label: "Carbon", shades: ["#15181e", "#272a35", "#3e85d0"] },
  { id: "inferno", label: "Inferno", shades: ["#120a0a", "#261616", "#ff5722"] },
  { id: "aether", label: "Aether", shades: ["#1e0d3c", "#31155f", "#c084fc"], badge: "Sharp" },
  { id: "frost", label: "Frost", shades: ["#0c4c7a", "#0388d2", "#06b6d4"], badge: "Sharp" },
  { id: "pixelato", label: "Pixelato", shades: ["#101010", "#303030", "#4caf50"], badge: "Sharp" },
  { id: "win95", label: "Win95", shades: ["#a5a5a5", "#c0c0c0", "#000080"], badge: "Sharp" },
  { id: "solar", label: "Solar", shades: ["#1A1200", "#2a1e08", "#FFC500"], badge: "Minimal" },
  { id: "fern", label: "Fern", shades: ["#001A08", "#062814", "#00C85A"], badge: "Minimal" },
  { id: "blaze", label: "Blaze", shades: ["#1A0000", "#2a0808", "#FF2D2D"], badge: "Minimal" },
  { id: "dusk", label: "Dusk", shades: ["#200310", "#3a0a1c", "#FF6B9C"], badge: "Minimal" },
  { id: "glacier", label: "Glacier", shades: ["#001020", "#0a2038", "#A8E6FF"], badge: "Minimal" },
];

const STORAGE_KEY = "tuffbox-theme";

export function readStoredTheme(): ThemeId {
  const raw = localStorage.getItem(STORAGE_KEY) || "tuffbox";
  if (THEMES.some((t) => t.id === raw)) return raw as ThemeId;
  // Migrate legacy dark/light toggle
  if (raw === "dark") return "tuffbox";
  if (raw === "light") return "tuffbox-light";
  return "tuffbox";
}

export function applyTheme(id: ThemeId, persist = false) {
  document.documentElement.setAttribute("data-theme", id);
  if (persist) {
    localStorage.setItem(STORAGE_KEY, id);
  }
}

export function previewTheme(id: ThemeId) {
  applyTheme(id, false);
}

export function commitTheme(id: ThemeId) {
  applyTheme(id, true);
}

export function restoreCommittedTheme() {
  applyTheme(readStoredTheme(), false);
}
