/**
 * Chunk column cache for the 3D world preview.
 *
 * Framework-free on purpose: the renderer subscribes and decides what to draw,
 * while this class owns the three facts that are easy to get wrong — one fetch
 * per coordinate, "absent" cached as a real answer, and a bounded cache so
 * flying across a world does not grow the heap forever.
 *
 * The readiness rule lives here too: a column may only be meshed once its four
 * side neighbours are *known*. Without that, the mesher would treat the border
 * as unknown and hide faces that a later-loaded neighbour would have exposed,
 * and the chunk would have to be meshed twice.
 */

import type { WorldColumn } from "../api";

export type ColumnFetcher = (chunkX: number, chunkZ: number) => Promise<WorldColumn | null>;

export interface ReadyColumn {
  x: number;
  z: number;
  column: WorldColumn;
}

const DEFAULT_CAPACITY = 512;

export class ChunkStore {
  private readonly columns = new Map<string, WorldColumn | null>();
  private readonly inFlight = new Map<string, Promise<void>>();
  private readonly listeners = new Set<() => void>();

  constructor(
    private readonly fetchColumn: ColumnFetcher,
    private readonly capacity = DEFAULT_CAPACITY,
  ) {}

  private static key(x: number, z: number) {
    return `${x}:${z}`;
  }

  get(x: number, z: number): WorldColumn | null | undefined {
    return this.columns.get(ChunkStore.key(x, z));
  }

  /** True once the coordinate has been asked for, even if the world has no chunk there. */
  isKnown(x: number, z: number): boolean {
    return this.columns.has(ChunkStore.key(x, z));
  }

  /** A column that exists *and* can be meshed without lying about its borders. */
  isMeshable(x: number, z: number): boolean {
    if (!this.get(x, z)) return false;
    return (
      this.isKnown(x - 1, z) && this.isKnown(x + 1, z) && this.isKnown(x, z - 1) && this.isKnown(x, z + 1)
    );
  }

  /** Fetches at most once per coordinate; concurrent callers share the request. */
  ensure(x: number, z: number): Promise<void> {
    const key = ChunkStore.key(x, z);
    if (this.columns.has(key)) return Promise.resolve();
    const pending = this.inFlight.get(key);
    if (pending) return pending;

    const request = this.fetchColumn(x, z)
      .then((column) => {
        this.columns.set(key, column ?? null);
        this.evict();
      })
      .catch(() => {
        // A failed fetch must not become a permanent "no chunk here": drop it so
        // the next visible-coordinate pass retries.
      })
      .finally(() => {
        this.inFlight.delete(key);
        this.notify();
      });

    this.inFlight.set(key, request);
    return request;
  }

  /** Every known column that is ready to mesh. */
  readyColumns(): ReadyColumn[] {
    const out: ReadyColumn[] = [];
    for (const [key, column] of this.columns) {
      if (!column) continue;
      const [x, z] = key.split(":").map(Number);
      if (this.isMeshable(x, z)) out.push({ x, z, column });
    }
    return out;
  }

  onChange(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private notify() {
    for (const listener of this.listeners) listener();
  }

  private evict() {
    while (this.columns.size > this.capacity) {
      // Insertion order is the access order here: the renderer walks a moving
      // window, so the oldest entry is the one furthest behind the camera.
      const oldest = this.columns.keys().next();
      if (oldest.done) return;
      this.columns.delete(oldest.value);
    }
  }
}
