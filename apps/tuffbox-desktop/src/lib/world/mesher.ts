/**
 * Greedy-free section mesher for the 3D world preview.
 *
 * Deliberately free of any `three` import: it emits plain typed arrays, so the
 * geometry can be unit-tested headlessly and the renderer stays a thin consumer.
 *
 * One quad per *visible* block face — a face is visible when the block next to
 * it on that side is neither a real block nor a loaded neighbour. Unknown
 * neighbours (`null` from the lookup) count as opaque, which keeps the outer
 * wall of the loaded region out of view; the caller is responsible for meshing a
 * column only once its border neighbours are known, so nothing has to be
 * re-meshed later.
 */

import type { WorldColumn, WorldSectionColumn } from "../api";

/** Blocks per section edge; must match `SECTION_SIZE` in `world_preview.rs`. */
export const SECTION_SIZE = 16;
/** Block entries in one section (`world_preview.rs::SECTION_BLOCKS`). */
export const SECTION_BLOCKS = SECTION_SIZE * SECTION_SIZE * SECTION_SIZE;

/** A block is "air-like" when its id ends in `air` (`minecraft:air`,
 * `cave_air`, `void_air`) — the only names the mesher needs to know. */
export function isAir(blockName: string): boolean {
  const path = blockName.includes(":") ? blockName.slice(blockName.indexOf(":") + 1) : blockName;
  return path === "air" || path.endsWith("_air");
}

/**
 * Anything that hides the face behind it. `null` means "no data about that
 * neighbour" and counts as opaque: an unloaded world must not paint walls at
 * the edge of the loaded region.
 */
export function isOpaque(blockName: string | null): boolean {
  return blockName === null || !isAir(blockName);
}

/** Block name at absolute world coordinates, or `null` when unknown. */
export interface NeighborLookup {
  blockAt(worldX: number, worldY: number, worldZ: number): string | null;
}

export interface MeshData {
  /** Chunk-local positions, 4 vertices per visible face. */
  positions: Float32Array;
  normals: Float32Array;
  /** Flat per-vertex colour, 0..1 per channel. */
  colors: Float32Array;
  indices: Uint32Array;
  faceCount: number;
}

const EMPTY_MESH: MeshData = {
  positions: new Float32Array(0),
  normals: new Float32Array(0),
  colors: new Float32Array(0),
  indices: new Uint32Array(0),
  faceCount: 0,
};

/** `[normal, four corner offsets]` for the six cube faces. */
const FACES: ReadonlyArray<readonly [readonly [number, number, number], readonly (readonly [number, number, number])[]]> = [
  [[1, 0, 0], [[1, 0, 1], [1, 0, 0], [1, 1, 0], [1, 1, 1]]],
  [[-1, 0, 0], [[0, 0, 0], [0, 0, 1], [0, 1, 1], [0, 1, 0]]],
  [[0, 1, 0], [[0, 1, 1], [1, 1, 1], [1, 1, 0], [0, 1, 0]]],
  [[0, -1, 0], [[0, 0, 0], [1, 0, 0], [1, 0, 1], [0, 0, 1]]],
  [[0, 0, 1], [[0, 0, 1], [1, 0, 1], [1, 1, 1], [0, 1, 1]]],
  [[0, 0, -1], [[1, 0, 0], [0, 0, 0], [0, 1, 0], [1, 1, 0]]],
];

/** Palette indices of one section, decoded from the base64 `u16` array. */
export function decodeSectionStates(section: WorldSectionColumn): Uint16Array {
  if (section.blockStates.length === 0) return new Uint16Array(SECTION_BLOCKS);
  const binary = atob(section.blockStates);
  const states = new Uint16Array(SECTION_BLOCKS);
  for (let i = 0; i < SECTION_BLOCKS; i++) {
    const lo = binary.charCodeAt(i * 2) & 0xff;
    const hi = binary.charCodeAt(i * 2 + 1) & 0xff;
    states[i] = lo | (hi << 8);
  }
  return states;
}

/**
 * Block name at chunk-local coordinates inside one column.
 *
 * Returns `null` only for coordinates outside the 16×16 column footprint — the
 * one place where "no data" means "unknown". A section the world does not have
 * is air: chunks store only generated sections, and the caller has already
 * filtered out prototype chunks.
 */
export function columnBlockAt(column: WorldColumn, x: number, y: number, z: number): string | null {
  if (x < 0 || x >= 16 || z < 0 || z >= 16) return null;
  const sectionY = Math.floor(y / 16);
  const section = column.sections.find((candidate) => candidate.y === sectionY);
  if (!section) return "minecraft:air";
  const localY = y - sectionY * 16;
  const states = decodeSectionStates(section);
  // Vanilla order: `(y << 8) | (z << 4) | x`.
  const index = localY * 256 + z * 16 + x;
  return section.blockPalette[states[index]]?.name ?? null;
}

export type BlockColor = (blockName: string) => number;

/**
 * Builds the visible-face geometry of one column.
 *
 * Positions are chunk-local (`0..16` on x/z, `section.y * 16 .. +16` on y), so
 * the renderer can place the mesh at the chunk origin and never has to rebuild
 * it when the camera moves.
 */
export function meshColumn(column: WorldColumn, lookup: NeighborLookup, colorOf: BlockColor): MeshData {
  if (column.sections.length === 0) return EMPTY_MESH;

  const baseX = column.x * 16;
  const baseZ = column.z * 16;
  const positions: number[] = [];
  const normals: number[] = [];
  const colors: number[] = [];
  const indices: number[] = [];
  let faceCount = 0;

  const blockAt = (worldX: number, worldY: number, worldZ: number): string | null => {
    const localX = worldX - baseX;
    const localZ = worldZ - baseZ;
    if (localX >= 0 && localX < 16 && localZ >= 0 && localZ < 16) {
      return columnBlockAt(column, localX, worldY, localZ);
    }
    return lookup.blockAt(worldX, worldY, worldZ);
  };

  for (const section of column.sections) {
    const states = decodeSectionStates(section);
    const paletteColors = section.blockPalette.map((entry) => colorOf(entry.name));
    const sectionBaseY = section.y * 16;

    for (let localY = 0; localY < 16; localY++) {
      for (let localZ = 0; localZ < 16; localZ++) {
        for (let localX = 0; localX < 16; localX++) {
          const state = states[localY * 256 + localZ * 16 + localX];
          const entry = section.blockPalette[state];
          if (!entry || isAir(entry.name)) continue;

          const worldX = baseX + localX;
          const worldY = sectionBaseY + localY;
          const worldZ = baseZ + localZ;
          const rgb = paletteColors[state];
          const r = ((rgb >> 16) & 0xff) / 255;
          const g = ((rgb >> 8) & 0xff) / 255;
          const b = (rgb & 0xff) / 255;

          for (const [normal, corners] of FACES) {
            if (isOpaque(blockAt(worldX + normal[0], worldY + normal[1], worldZ + normal[2]))) {
              continue;
            }
            const first = positions.length / 3;
            for (const corner of corners) {
              positions.push(
                localX + corner[0],
                sectionBaseY + localY + corner[1],
                localZ + corner[2],
              );
              normals.push(normal[0], normal[1], normal[2]);
              colors.push(r, g, b);
            }
            indices.push(first, first + 1, first + 2, first, first + 2, first + 3);
            faceCount++;
          }
        }
      }
    }
  }

  if (faceCount === 0) return EMPTY_MESH;
  return {
    positions: new Float32Array(positions),
    normals: new Float32Array(normals),
    colors: new Float32Array(colors),
    indices: new Uint32Array(indices),
    faceCount,
  };
}
