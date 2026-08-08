import { describe, expect, it } from "vitest";
import { layoutTree, _topologicalLayersForTest, applyLayout } from "./layout";
import type { QuestData } from "./types";

function q(id: string, deps: string[] = []): QuestData {
  return {
    id,
    title: id,
    description: [],
    x: 0,
    y: 0,
    dependencies: deps,
    tasks: [],
    rewards: [],
    optional: false,
  };
}

describe("topologicalLayers / layoutTree", () => {
  it("layers a simple chain", () => {
    const quests = [q("a"), q("b", ["a"]), q("c", ["b"])];
    const layers = _topologicalLayersForTest(quests);
    expect(layers.map((l) => l.map((x) => x.id))).toEqual([["a"], ["b"], ["c"]]);
  });

  it("terminates on cyclic dependencies and still places all quests", () => {
    const quests = [q("a", ["b"]), q("b", ["a"]), q("c")];
    const layers = _topologicalLayersForTest(quests);
    const ids = layers.flat().map((x) => x.id).sort();
    expect(ids).toEqual(["a", "b", "c"]);
    // c is a root; a/b are cycle leftovers in a later layer
    expect(layers[0].map((x) => x.id)).toEqual(["c"]);
  });

  it("layoutTree returns finite positions for a pure cycle", () => {
    const quests = [q("a", ["b"]), q("b", ["a"])];
    const positions = layoutTree(quests);
    expect(positions.size).toBe(2);
    for (const pos of positions.values()) {
      expect(Number.isFinite(pos.x)).toBe(true);
      expect(Number.isFinite(pos.y)).toBe(true);
    }
    const applied = applyLayout(quests, positions);
    expect(applied.every((x) => Number.isFinite(x.x) && Number.isFinite(x.y))).toBe(true);
  });

  it("ignores self-dependencies", () => {
    const quests = [q("a", ["a"]), q("b", ["a"])];
    const layers = _topologicalLayersForTest(quests);
    expect(layers[0].map((x) => x.id)).toContain("a");
    expect(layers.flat().map((x) => x.id).sort()).toEqual(["a", "b"]);
  });
});
