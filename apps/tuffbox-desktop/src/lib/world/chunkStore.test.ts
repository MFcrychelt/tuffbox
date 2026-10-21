import { describe, expect, it, vi } from "vitest";
import type { WorldColumn } from "../api";
import { ChunkStore } from "./chunkStore";

function column(x: number, z: number): WorldColumn {
  return { x, z, dataVersion: 3465, status: "minecraft:full", sections: [] };
}

/** A fetcher that records every requested coordinate and can be told what to return. */
function recordingFetcher(existing: (x: number, z: number) => boolean) {
  const calls: string[] = [];
  const fetcher = vi.fn(async (x: number, z: number) => {
    calls.push(`${x}:${z}`);
    return existing(x, z) ? column(x, z) : null;
  });
  return { fetcher, calls };
}

describe("ChunkStore", () => {
  it("issues one request per coordinate even for concurrent callers", async () => {
    const { fetcher, calls } = recordingFetcher(() => true);
    const store = new ChunkStore(fetcher);

    await Promise.all([store.ensure(1, 2), store.ensure(1, 2), store.ensure(1, 2)]);

    expect(calls).toEqual(["1:2"]);
    expect(fetcher).toHaveBeenCalledTimes(1);
    expect(store.get(1, 2)?.x).toBe(1);
  });

  it("remembers an empty coordinate instead of asking again", async () => {
    const { fetcher } = recordingFetcher(() => false);
    const store = new ChunkStore(fetcher);

    await store.ensure(0, 0);
    await store.ensure(0, 0);

    expect(fetcher).toHaveBeenCalledTimes(1);
    expect(store.isKnown(0, 0)).toBe(true);
    expect(store.get(0, 0)).toBeNull();
  });

  it("retries after a failure rather than caching it as absent", async () => {
    let attempt = 0;
    const fetcher = vi.fn(async () => {
      attempt++;
      if (attempt === 1) throw new Error("network");
      return column(0, 0);
    });
    const store = new ChunkStore(fetcher);

    await store.ensure(0, 0);
    expect(store.isKnown(0, 0)).toBe(false);
    await store.ensure(0, 0);
    expect(store.get(0, 0)?.x).toBe(0);
  });

  it("only calls a column meshable once its four side neighbours are known", async () => {
    const { fetcher } = recordingFetcher(() => true);
    const store = new ChunkStore(fetcher);
    await store.ensure(0, 0);
    expect(store.isMeshable(0, 0)).toBe(false);

    for (const [x, z] of [
      [-1, 0],
      [1, 0],
      [0, -1],
    ]) {
      await store.ensure(x, z);
    }
    expect(store.isMeshable(0, 0)).toBe(false);

    await store.ensure(0, 1);
    expect(store.isMeshable(0, 0)).toBe(true);
  });

  it("never reports a column that does not exist as meshable", async () => {
    const { fetcher } = recordingFetcher((x, z) => !(x === 0 && z === 0));
    const store = new ChunkStore(fetcher);
    for (const [x, z] of [
      [0, 0],
      [-1, 0],
      [1, 0],
      [0, -1],
      [0, 1],
    ]) {
      await store.ensure(x, z);
    }
    expect(store.isMeshable(0, 0)).toBe(false);
    expect(store.readyColumns().map((entry) => entry.x)).not.toContain(0);
  });

  it("lists only ready columns and reports each once per call", async () => {
    const { fetcher } = recordingFetcher(() => true);
    const store = new ChunkStore(fetcher);
    for (let x = -1; x <= 2; x++) {
      for (let z = -1; z <= 2; z++) {
        await store.ensure(x, z);
      }
    }
    const ready = store.readyColumns();
    expect(ready.length).toBe(4);
    expect(ready.map((entry) => `${entry.x}:${entry.z}`).sort()).toEqual([
      "0:0",
      "0:1",
      "1:0",
      "1:1",
    ]);
  });

  it("drops the oldest column once it exceeds its capacity", async () => {
    const { fetcher } = recordingFetcher(() => true);
    const store = new ChunkStore(fetcher, 2);
    await store.ensure(0, 0);
    await store.ensure(1, 0);
    await store.ensure(2, 0);
    expect(store.isKnown(0, 0)).toBe(false);
    expect(store.isKnown(2, 0)).toBe(true);
  });

  it("notifies listeners when a fetch settles, and stops on unsubscribe", async () => {
    const { fetcher } = recordingFetcher(() => true);
    const store = new ChunkStore(fetcher);
    const listener = vi.fn();
    const unsubscribe = store.onChange(listener);

    await store.ensure(3, 3);
    expect(listener).toHaveBeenCalledTimes(1);

    unsubscribe();
    await store.ensure(4, 4);
    expect(listener).toHaveBeenCalledTimes(1);
  });
});
