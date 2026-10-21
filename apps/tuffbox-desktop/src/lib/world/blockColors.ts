/**
 * Block colours for the 3D world preview.
 *
 * A recognisable world needs colour, and the honest source is the texture the
 * player's own client jar ships — `get_item_icons_batch` already extracts those
 * PNGs, so `warm()` averages them and the fallback below only covers blocks
 * whose texture is missing (resource packs, modded blocks, no client jar yet).
 *
 * The fallback is deliberately *derived* from the block name rather than a big
 * authored table: a stable hash gives every unknown block a distinct, repeatable
 * colour, and a short override list pins the handful of blocks whose hue carries
 * the image (grass, water, sand, leaves…).
 */

/** Blocks whose hue is worth pinning; everything else falls back to the hash. */
export const BLOCK_COLOR_OVERRIDES: Record<string, number> = {
  "minecraft:grass_block": 0x6a9b3f,
  "minecraft:short_grass": 0x6a9b3f,
  "minecraft:tall_grass": 0x6a9b3f,
  "minecraft:oak_leaves": 0x4a7a2c,
  "minecraft:birch_leaves": 0x6d9440,
  "minecraft:spruce_leaves": 0x35502a,
  "minecraft:jungle_leaves": 0x3f7a20,
  "minecraft:acacia_leaves": 0x5e8f33,
  "minecraft:dark_oak_leaves": 0x35501f,
  "minecraft:water": 0x3f5fbf,
  "minecraft:lava": 0xd45a12,
  "minecraft:stone": 0x7d7d7d,
  "minecraft:cobblestone": 0x6b6b6b,
  "minecraft:deepslate": 0x4f4f52,
  "minecraft:dirt": 0x79553a,
  "minecraft:coarse_dirt": 0x79553a,
  "minecraft:sand": 0xdcd0a0,
  "minecraft:sandstone": 0xd8cd9a,
  "minecraft:gravel": 0x8a8580,
  "minecraft:snow": 0xf2fafa,
  "minecraft:ice": 0x9ec7f0,
  "minecraft:bedrock": 0x333333,
  "minecraft:netherrack": 0x6f3634,
  "minecraft:end_stone": 0xdbdba0,
  "minecraft:clay": 0xa4a8b8,
  "minecraft:oak_planks": 0xb08a4f,
  "minecraft:oak_log": 0x6b5333,
  "minecraft:glass": 0xc8e8f0,
};

/** Stable 32-bit hash — same name, same colour, across sessions. */
function hashName(name: string): number {
  let hash = 0x811c9dc5;
  for (let i = 0; i < name.length; i++) {
    hash ^= name.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  return hash >>> 0;
}

/**
 * Deterministic colour for a block with no known texture.
 *
 * The hue comes from the hash; saturation and lightness are pinned so every
 * fallback block stays readable next to the pinned ones instead of turning into
 * a neon smear.
 */
export function fallbackBlockColor(blockName: string, overrides = BLOCK_COLOR_OVERRIDES): number {
  const pinned = overrides[blockName];
  if (pinned !== undefined) return pinned;
  const hue = hashName(blockName) % 360;
  return hslToRgb(hue / 360, 0.35, 0.52);
}

/** `h`, `s`, `l` in 0..1 — the one place this module does colour maths. */
export function hslToRgb(h: number, s: number, l: number): number {
  const chroma = (1 - Math.abs(2 * l - 1)) * s;
  const sector = h * 6;
  const second = chroma * (1 - Math.abs((sector % 2) - 1));
  const [r, g, b] =
    sector < 1
      ? [chroma, second, 0]
      : sector < 2
        ? [second, chroma, 0]
        : sector < 3
          ? [0, chroma, second]
          : sector < 4
            ? [0, second, chroma]
            : sector < 5
              ? [second, 0, chroma]
              : [chroma, 0, second];
  const match = l - chroma / 2;
  const channel = (value: number) => Math.max(0, Math.min(255, Math.round((value + match) * 255)));
  return (channel(r) << 16) | (channel(g) << 8) | channel(b);
}

/** Where real colours come from: the client jar's textures, via the backend. */
export interface TextureColorSource {
  /** Block name → PNG data URL (or `null` when the pack has no such texture). */
  fetch(names: string[]): Promise<Record<string, string | null>>;
  /** Mean colour of a decoded PNG, or `null` when it cannot be read. */
  average(dataUrl: string): Promise<number | null>;
}

export class BlockColorResolver {
  private readonly learned = new Map<string, number>();

  constructor(
    private readonly source: TextureColorSource,
    private readonly overrides = BLOCK_COLOR_OVERRIDES,
  ) {}

  /** Synchronous lookup — meshing must never wait on the network. */
  colorOf(blockName: string): number {
    return this.learned.get(blockName) ?? fallbackBlockColor(blockName, this.overrides);
  }

  /**
   * Replaces fallback colours with averaged texture colours. Returns how many
   * names it actually learned, so the caller can tell a real texture pass from
   * a no-op and avoid repainting the world for nothing.
   */
  async warm(blockNames: Iterable<string>): Promise<number> {
    const wanted = [...new Set(blockNames)].filter(
      (name) => name.length > 0 && !this.overrides[name] && !this.learned.has(name),
    );
    if (wanted.length === 0) return 0;

    const textures = await this.source.fetch(wanted);
    let learned = 0;
    for (const name of wanted) {
      const dataUrl = textures[name];
      if (!dataUrl) continue;
      const color = await this.source.average(dataUrl);
      if (color === null) continue;
      this.learned.set(name, color);
      learned++;
    }
    return learned;
  }
}
