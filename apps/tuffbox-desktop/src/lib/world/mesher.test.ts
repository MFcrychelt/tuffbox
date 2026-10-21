import { describe, expect, it } from "vitest";
import type { WorldColumn, WorldSectionColumn } from "../api";
import {
  SECTION_BLOCKS,
  columnBlockAt,
  decodeSectionStates,
  isAir,
  isOpaque,
  meshColumn,
  type MeshData,
  type NeighborLookup,
} from "./mesher";

const AIR: NeighborLookup = { blockAt: () => "minecraft:air" };
const UNKNOWN: NeighborLookup = { blockAt: () => null };
const GREY = () => 0x808080;

function encodeStates(states: Uint16Array): string {
  let binary = "";
  for (const state of states) {
    binary += String.fromCharCode(state & 0xff, (state >> 8) & 0xff);
  }
  return btoa(binary);
}

/** A section whose palette is `palette`, with `blocks` placed by local index. */
function section(y: number, palette: string[], blocks: Array<[number, number, number, number]>): WorldSectionColumn {
  const states = new Uint16Array(SECTION_BLOCKS);
  for (const [x, by, z, state] of blocks) {
    states[by * 256 + z * 16 + x] = state;
  }
  return {
    y,
    blockPalette: palette.map((name) => ({ name, properties: {} })),
    blockStates: encodeStates(states),
    biomePalette: [],
    biomeStates: "",
  };
}

function column(x: number, z: number, sections: WorldSectionColumn[]): WorldColumn {
  return { x, z, dataVersion: 3465, status: "minecraft:full", sections };
}

/** Face `i` of a mesh: its four vertices and the normal they all carry. */
function faceAt(mesh: MeshData, i: number) {
  const at = i * 12;
  return {
    verts: [0, 1, 2, 3].map((v) => [mesh.positions[at + v * 3], mesh.positions[at + v * 3 + 1], mesh.positions[at + v * 3 + 2]]),
    normal: [mesh.normals[at], mesh.normals[at + 1], mesh.normals[at + 2]],
  };
}

describe("block predicates", () => {
  it("treats every air variant as air and everything else as opaque", () => {
    for (const name of ["minecraft:air", "minecraft:cave_air", "minecraft:void_air"]) {
      expect(isAir(name)).toBe(true);
      expect(isOpaque(name)).toBe(false);
    }
    expect(isOpaque("minecraft:stone")).toBe(true);
    expect(isOpaque(null)).toBe(true);
  });
});

describe("meshColumn", () => {
  it("emits one quad per visible face with a winding that matches the normal", () => {
    // Interior of the section, so all six sides are exposed.
    const chunk = column(0, 0, [section(0, ["minecraft:air", "minecraft:stone"], [[1, 8, 1, 1]])]);
    const mesh = meshColumn(chunk, AIR, GREY);

    expect(mesh.faceCount).toBe(6);
    expect(mesh.indices.length).toBe(6 * 6);
    for (let face = 0; face < 6; face++) {
      const { verts, normal } = faceAt(mesh, face);
      const e1 = verts[1].map((v, k) => v - verts[0][k]);
      const e2 = verts[2].map((v, k) => v - verts[0][k]);
      const cross = [
        e1[1] * e2[2] - e1[2] * e2[1],
        e1[2] * e2[0] - e1[0] * e2[2],
        e1[0] * e2[1] - e1[1] * e2[0],
      ];
      const scale = Math.hypot(cross[0], cross[1], cross[2]);
      expect(scale).toBeGreaterThan(0);
      for (let axis = 0; axis < 3; axis++) {
        expect(cross[axis] / scale).toBeCloseTo(normal[axis], 6);
      }
    }
  });

  it("culls the shared face between two touching blocks", () => {
    const blocks: Array<[number, number, number, number]> = [
      [1, 8, 1, 1],
      [2, 8, 1, 1],
    ];
    const mesh = meshColumn(column(0, 0, [section(0, ["minecraft:air", "minecraft:stone"], blocks)]), AIR, GREY);
    expect(mesh.faceCount).toBe(10);
  });

  it("skips air entirely", () => {
    const mesh = meshColumn(column(0, 0, [section(0, ["minecraft:air"], [[1, 8, 1, 0]])]), AIR, GREY);
    expect(mesh.faceCount).toBe(0);
    expect(mesh.positions.length).toBe(0);
  });

  it("treats an unknown neighbour as opaque so the region edge stays closed", () => {
    const chunk = column(0, 0, [section(0, ["minecraft:air", "minecraft:stone"], [[0, 8, 0, 1]])]);
    // At the chunk corner two sides face coordinates outside this column, which
    // an unloaded world reports as unknown.
    expect(meshColumn(chunk, UNKNOWN, GREY).faceCount).toBe(4);
    // With the neighbours known to be air, the same block is fully visible.
    expect(meshColumn(chunk, AIR, GREY).faceCount).toBe(6);
  });

  it("keeps blocks of a second section at their own height", () => {
    const mesh = meshColumn(
      column(0, 0, [section(0, ["minecraft:air", "minecraft:stone"], [[1, 8, 1, 1]]), section(1, ["minecraft:air", "minecraft:stone"], [[1, 0, 1, 1]])]),
      AIR,
      GREY,
    );
    const ys = new Set<number>();
    for (let i = 1; i < mesh.positions.length; i += 3) ys.add(mesh.positions[i]);
    expect(Math.min(...ys)).toBe(8);
    expect(Math.max(...ys)).toBe(17);
  });

  it("culls the face between two stacked sections", () => {
    const stacked = column(0, 0, [
      section(0, ["minecraft:air", "minecraft:stone"], [[1, 15, 1, 1]]),
      section(1, ["minecraft:air", "minecraft:stone"], [[1, 0, 1, 1]]),
    ]);
    const alone = column(0, 0, [section(0, ["minecraft:air", "minecraft:stone"], [[1, 15, 1, 1]])]);
    // Touching sections hide the shared horizontal face: 6 + 6 − 2.
    expect(meshColumn(stacked, AIR, GREY).faceCount).toBe(10);
    // With no section above, the top of the block is exposed.
    expect(meshColumn(alone, AIR, GREY).faceCount).toBe(6);
  });

  it("carries the palette colour onto every vertex", () => {
    const chunk = column(0, 0, [section(0, ["minecraft:air", "minecraft:stone"], [[1, 8, 1, 1]])]);
    const mesh = meshColumn(chunk, AIR, () => 0xff8000);
    expect(mesh.colors[0]).toBeCloseTo(1);
    expect(mesh.colors[1]).toBeCloseTo(0x80 / 255);
    expect(mesh.colors[2]).toBeCloseTo(0);
  });

  it("keeps indices inside the vertex array", () => {
    const chunk = column(0, 0, [section(0, ["minecraft:air", "minecraft:stone"], [[1, 8, 1, 1]])]);
    const mesh = meshColumn(chunk, AIR, GREY);
    const vertices = mesh.positions.length / 3;
    for (const index of mesh.indices) {
      expect(index).toBeLessThan(vertices);
    }
  });
});

describe("columnBlockAt", () => {
  it("maps local coordinates through the section palette in vanilla order", () => {
    const chunk = column(0, 0, [section(0, ["minecraft:air", "minecraft:stone"], [[3, 2, 5, 1]])]);
    expect(columnBlockAt(chunk, 3, 2, 5)).toBe("minecraft:stone");
    expect(columnBlockAt(chunk, 5, 2, 3)).toBe("minecraft:air");
    expect(columnBlockAt(chunk, 3, 2, 5)).not.toBeNull();
  });

  it("finds blocks in negative sections", () => {
    const chunk = column(0, 0, [section(-4, ["minecraft:air", "minecraft:deepslate"], [[0, 15, 0, 1]])]);
    expect(columnBlockAt(chunk, 0, -4 * 16 + 15, 0)).toBe("minecraft:deepslate");
  });

  it("treats a missing section as air but keeps outside-the-column unknown", () => {
    const chunk = column(0, 0, [section(0, ["minecraft:stone"], [])]);
    // Not generated is air, not "unknown": the mesher must not paint walls at
    // the top and bottom of the loaded section range.
    expect(columnBlockAt(chunk, 0, 500, 0)).toBe("minecraft:air");
    expect(columnBlockAt(chunk, 0, -500, 0)).toBe("minecraft:air");
    // Outside the 16×16 footprint there is nothing to consult.
    expect(columnBlockAt(chunk, -1, 0, 0)).toBeNull();
    expect(columnBlockAt(chunk, 16, 0, 0)).toBeNull();
  });
});

describe("decodeSectionStates", () => {
  it("round-trips a single-entry palette section as all zeros", () => {
    const states = decodeSectionStates(section(0, ["minecraft:stone"], []));
    expect(states.length).toBe(SECTION_BLOCKS);
    expect(states.every((state) => state === 0)).toBe(true);
  });
});
