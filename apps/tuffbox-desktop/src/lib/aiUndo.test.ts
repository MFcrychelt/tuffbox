import { get } from "svelte/store";
import { beforeEach, describe, expect, it } from "vitest";
import {
  AI_UNDO_LIMIT,
  aiUndoFor,
  aiUndoStack,
  clearAiUndo,
  dropAiUndo,
  peekAiUndo,
  pushAiUndo,
  type AiUndoEntry,
} from "./aiUndo";
import { writable } from "svelte/store";

function entry(snapshotId: string, projectPath = "U:/pack/a"): Omit<AiUndoEntry, "at"> {
  return {
    snapshotId,
    title: `${snapshotId} patches`,
    detail: "mods/x.jar",
    actor: "Tune AI",
    projectPath,
  };
}

beforeEach(() => clearAiUndo());

describe("aiUndoStack", () => {
  it("returns the newest entry first", () => {
    pushAiUndo(entry("snap-1"));
    pushAiUndo(entry("snap-2"));
    expect(peekAiUndo("U:/pack/a", get(aiUndoStack))?.snapshotId).toBe("snap-2");
  });

  it("keeps the journal bounded", () => {
    for (let i = 0; i < AI_UNDO_LIMIT + 5; i++) pushAiUndo(entry(`snap-${i}`));
    const stack = get(aiUndoStack);
    expect(stack.length).toBe(AI_UNDO_LIMIT);
    expect(stack[0].snapshotId).toBe("snap-5");
    expect(stack[stack.length - 1].snapshotId).toBe(`snap-${AI_UNDO_LIMIT + 4}`);
  });

  it("never offers an undo from another project", () => {
    pushAiUndo(entry("snap-a", "U:/pack/a"));
    pushAiUndo(entry("snap-b", "U:/pack/b"));
    expect(peekAiUndo("U:/pack/a", get(aiUndoStack))?.snapshotId).toBe("snap-a");
    expect(peekAiUndo("U:/pack/c", get(aiUndoStack))).toBeNull();
    expect(peekAiUndo(null, get(aiUndoStack))).toBeNull();
  });

  it("falls back to an older entry of the same project", () => {
    pushAiUndo(entry("snap-1"));
    pushAiUndo(entry("snap-2", "U:/pack/b"));
    expect(peekAiUndo("U:/pack/a", get(aiUndoStack))?.snapshotId).toBe("snap-1");
  });

  it("drops exactly the rolled-back entry", () => {
    pushAiUndo(entry("snap-1"));
    pushAiUndo(entry("snap-2"));
    dropAiUndo("snap-2");
    expect(get(aiUndoStack).map((e) => e.snapshotId)).toEqual(["snap-1"]);

    dropAiUndo("missing");
    expect(get(aiUndoStack).map((e) => e.snapshotId)).toEqual(["snap-1"]);
  });

  it("clears one project or everything", () => {
    pushAiUndo(entry("snap-a", "U:/pack/a"));
    pushAiUndo(entry("snap-b", "U:/pack/b"));
    clearAiUndo("U:/pack/a");
    expect(get(aiUndoStack).map((e) => e.snapshotId)).toEqual(["snap-b"]);

    clearAiUndo();
    expect(get(aiUndoStack)).toEqual([]);
  });

  it("exposes a reactive view for the project being edited", () => {
    const projectPath = writable<string | null>("U:/pack/a");
    const view = aiUndoFor(projectPath);
    expect(get(view)).toBeNull();

    pushAiUndo(entry("snap-a", "U:/pack/a"));
    expect(get(view)?.snapshotId).toBe("snap-a");

    // Switching projects must not show the other pack's undo.
    projectPath.set("U:/pack/b");
    expect(get(view)).toBeNull();
  });
});
