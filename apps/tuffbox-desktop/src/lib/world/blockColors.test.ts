import { describe, expect, it, vi } from "vitest";
import {
  BLOCK_COLOR_OVERRIDES,
  BlockColorResolver,
  fallbackBlockColor,
  hslToRgb,
  type TextureColorSource,
} from "./blockColors";

describe("fallbackBlockColor", () => {
  it("is stable for a name and different between names", () => {
    expect(fallbackBlockColor("example:modded_block")).toBe(fallbackBlockColor("example:modded_block"));
    expect(fallbackBlockColor("example:modded_block")).not.toBe(fallbackBlockColor("example:other_block"));
  });

  it("prefers the pinned colour", () => {
    expect(fallbackBlockColor("minecraft:water")).toBe(BLOCK_COLOR_OVERRIDES["minecraft:water"]);
  });

  it("allows the override table to be replaced", () => {
    expect(fallbackBlockColor("minecraft:water", {})).not.toBe(BLOCK_COLOR_OVERRIDES["minecraft:water"]);
  });

  it("always produces a valid 24-bit colour", () => {
    for (const name of ["a", "minecraft:stone", "", "very:long/block/name_with_underscores"]) {
      const color = fallbackBlockColor(name);
      expect(Number.isInteger(color)).toBe(true);
      expect(color).toBeGreaterThanOrEqual(0);
      expect(color).toBeLessThanOrEqual(0xffffff);
    }
  });
});

describe("hslToRgb", () => {
  it("maps the primaries and the greys", () => {
    expect(hslToRgb(0, 1, 0.5)).toBe(0xff0000);
    expect(hslToRgb(1 / 3, 1, 0.5)).toBe(0x00ff00);
    expect(hslToRgb(2 / 3, 1, 0.5)).toBe(0x0000ff);
    expect(hslToRgb(0, 0, 0)).toBe(0x000000);
    expect(hslToRgb(0, 0, 1)).toBe(0xffffff);
  });
});

/** A source that answers with fixed textures and a fixed average. */
function fakeSource(textures: Record<string, string | null>, average: number | null = 0x123456) {
  return {
    fetch: vi.fn(async (names: string[]) =>
      Object.fromEntries(names.map((name) => [name, textures[name] ?? null])),
    ),
    average: vi.fn(async () => average),
  } satisfies TextureColorSource;
}

describe("BlockColorResolver", () => {
  it("answers synchronously from the fallback before anything is learned", () => {
    const resolver = new BlockColorResolver(fakeSource({}));
    expect(resolver.colorOf("example:block")).toBe(fallbackBlockColor("example:block"));
  });

  it("learns averaged texture colours for unknown blocks", async () => {
    const resolver = new BlockColorResolver(fakeSource({ "example:block": "data:image/png;base64,x" }, 0xabcdef));
    const learned = await resolver.warm(["example:block"]);
    expect(learned).toBe(1);
    expect(resolver.colorOf("example:block")).toBe(0xabcdef);
  });

  it("does not spend a request on pinned or already-learned names", async () => {
    const source = fakeSource({ "example:block": "data:image/png;base64,x" });
    const resolver = new BlockColorResolver(source);
    await resolver.warm(["example:block"]);
    const again = await resolver.warm(["example:block", "minecraft:water", ""]);

    expect(again).toBe(0);
    expect(source.fetch).toHaveBeenCalledTimes(1);
  });

  it("ignores names the pack has no texture for", async () => {
    const resolver = new BlockColorResolver(fakeSource({}));
    expect(await resolver.warm(["example:block"])).toBe(0);
    expect(resolver.colorOf("example:block")).toBe(fallbackBlockColor("example:block"));
  });

  it("keeps the fallback when a texture cannot be averaged", async () => {
    const resolver = new BlockColorResolver(
      fakeSource({ "example:block": "data:image/png;base64,x" }, null),
    );
    expect(await resolver.warm(["example:block"])).toBe(0);
    expect(resolver.colorOf("example:block")).toBe(fallbackBlockColor("example:block"));
  });

  it("asks once per distinct name", async () => {
    const source = fakeSource({});
    const resolver = new BlockColorResolver(source);
    await resolver.warm(["a:one", "a:one", "a:two"]);
    expect(source.fetch).toHaveBeenCalledWith(["a:one", "a:two"]);
  });
});
