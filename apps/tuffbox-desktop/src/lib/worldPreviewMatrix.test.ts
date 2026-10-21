import { describe, expect, it } from "vitest";
import {
  MESHER_MAX_DATA_VERSION,
  MIN_READ_DATA_VERSION,
  READ_MAX_DATA_VERSION,
  describeWorldVersion,
  previewBadge,
  resolvePreviewStrategy,
} from "./worldPreviewMatrix";

describe("resolvePreviewStrategy", () => {
  it("treats the reader's own range as the outer gate", () => {
    expect(resolvePreviewStrategy(MIN_READ_DATA_VERSION - 1)).toBe("unsupported");
    expect(resolvePreviewStrategy(MIN_READ_DATA_VERSION)).toBe("live");
    expect(resolvePreviewStrategy(READ_MAX_DATA_VERSION + 1)).toBe("unsupported");
  });

  it("separates 'the mesher is too old' from 'the reader cannot read it'", () => {
    expect(resolvePreviewStrategy(MESHER_MAX_DATA_VERSION)).toBe("live");
    expect(resolvePreviewStrategy(MESHER_MAX_DATA_VERSION + 1)).toBe("export");
    expect(resolvePreviewStrategy(READ_MAX_DATA_VERSION)).toBe("export");
  });

  it("refuses to guess when the version is missing", () => {
    for (const missing of [null, undefined, Number.NaN]) {
      expect(resolvePreviewStrategy(missing)).toBe("unsupported");
    }
  });
});

describe("previewBadge", () => {
  it("gives every strategy a distinct label and a hint", () => {
    const badges = (["live", "export", "unsupported"] as const).map(previewBadge);
    expect(new Set(badges.map((b) => b.label)).size).toBe(3);
    for (const badge of badges) {
      expect(badge.hint.length).toBeGreaterThan(0);
    }
    expect(previewBadge("export").tone).toBe("warn");
    expect(previewBadge("live").tone).toBe("ok");
  });
});

describe("describeWorldVersion", () => {
  it("prefers the name the world recorded over a version table", () => {
    expect(describeWorldVersion("1.12.2", 1343)).toBe("1.12.2");
  });

  it("falls back to the raw DataVersion instead of inventing a release name", () => {
    expect(describeWorldVersion("   ", 4435)).toBe("DataVersion 4435");
    expect(describeWorldVersion(null, null)).toBe("unknown version");
  });
});
