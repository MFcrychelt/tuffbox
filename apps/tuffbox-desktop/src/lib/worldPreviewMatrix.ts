/**
 * Version gating for the 3D world preview.
 *
 * A world can only be previewed live when the reader understands its chunk
 * format *and* the mesher understands its block registry. Those two limits move
 * independently, so they are separate constants and the strategy is derived from
 * both: a world newer than the mesher still opens, but only through the
 * export path (a Minecraft-version-matched renderer outside the app).
 *
 * The reader's limits mirror `crates/tuffbox-core/src/anvil.rs`
 * (`MIN_DATA_VERSION` / `READ_MAX_DATA_VERSION`) — keep them in step; Rust is
 * the source of truth and rejects anything outside its own range.
 */

/** 1.13 — first version with palette + packed indices. */
export const MIN_READ_DATA_VERSION = 1519;
/** 26.2 — newest layout the Anvil reader accepts. */
export const READ_MAX_DATA_VERSION = 4903;
/** 1.21.6 — newest block registry the in-app mesher knows. */
export const MESHER_MAX_DATA_VERSION = 4435;

export type PreviewStrategy = "live" | "export" | "unsupported";

/**
 * How a world with this DataVersion should be previewed.
 *
 * - `live` — read, mesh and render inside the app.
 * - `export` — readable, but the mesher is too old for the registry; the world
 *   can be exported for a version-matched renderer.
 * - `unsupported` — the reader does not understand the format at all.
 */
export function resolvePreviewStrategy(dataVersion: number | null | undefined): PreviewStrategy {
  if (dataVersion === null || dataVersion === undefined || !Number.isFinite(dataVersion)) {
    return "unsupported";
  }
  if (dataVersion < MIN_READ_DATA_VERSION || dataVersion > READ_MAX_DATA_VERSION) {
    return "unsupported";
  }
  return dataVersion > MESHER_MAX_DATA_VERSION ? "export" : "live";
}

export type BadgeTone = "ok" | "warn" | "muted";

export interface PreviewBadge {
  label: string;
  tone: BadgeTone;
  /** One line the UI can show in a tooltip. */
  hint: string;
}

export function previewBadge(strategy: PreviewStrategy): PreviewBadge {
  switch (strategy) {
    case "live":
      return {
        label: "3D preview",
        tone: "ok",
        hint: "This world can be opened in the built-in 3D preview.",
      };
    case "export":
      return {
        label: "Preview via export",
        tone: "warn",
        hint:
          "Newer than 1.21.6 — the built-in mesher does not know this block registry yet. " +
          "Export the world to preview it with a matching renderer.",
      };
    case "unsupported":
      return {
        label: "No 3D preview",
        tone: "muted",
        hint: "Written by a Minecraft version older than 1.13 or newer than the reader supports.",
      };
  }
}

/**
 * Human label for a world's version.
 *
 * Prefers the name the world itself recorded (`level.dat` → `Version.Name`) —
 * that is authoritative, while any table mapping DataVersion to release names we
 * could ship here would be a guess that silently rots. Falls back to the raw
 * DataVersion so the value is still actionable.
 */
export function describeWorldVersion(
  versionName: string | null | undefined,
  dataVersion: number | null | undefined,
): string {
  const name = versionName?.trim();
  if (name) return name;
  if (dataVersion === null || dataVersion === undefined || !Number.isFinite(dataVersion)) {
    return "unknown version";
  }
  return `DataVersion ${dataVersion}`;
}
