import { derived, writable, type Readable } from "svelte/store";

/**
 * Journal of AI actions that can still be rolled back.
 *
 * Every surface that applies a plan through `apply_action_plan` gets a snapshot
 * id back; this keeps the last few of them so the user can undo an AI edit
 * without hunting through the Snapshots screen. The rollback itself is the
 * existing `rollback_snapshot` — this module only remembers *what* to undo.
 *
 * Entries are project-scoped because snapshots are: two open packs must not be
 * able to undo into each other.
 */

export interface AiUndoEntry {
  /** Snapshot taken *before* the actions were applied. */
  snapshotId: string;
  /** Short human label, e.g. `3 patches`. */
  title: string;
  /** What changed, for the tooltip. */
  detail: string;
  /** Which AI surface produced it (`Tune AI`, `Quest AI`, …). */
  actor: string;
  /** Project the snapshot belongs to; rollback is project-scoped. */
  projectPath: string;
  at: number;
}

/**
 * How many entries the journal keeps. Snapshots cost a few MB each, the chat
 * can be long, and an undo that is older than the last handful of edits is not
 * something anyone reaches for.
 */
export const AI_UNDO_LIMIT = 20;

export const aiUndoStack = writable<AiUndoEntry[]>([]);

export function pushAiUndo(entry: Omit<AiUndoEntry, "at">): void {
  aiUndoStack.update((stack) => [...stack, { ...entry, at: Date.now() }].slice(-AI_UNDO_LIMIT));
}

/** Newest entry for a project, or `null`. Pure so it can be unit-tested. */
export function peekAiUndo(
  projectPath: string | null | undefined,
  stack: readonly AiUndoEntry[],
): AiUndoEntry | null {
  if (!projectPath) return null;
  for (let index = stack.length - 1; index >= 0; index--) {
    if (stack[index].projectPath === projectPath) return stack[index];
  }
  return null;
}

/** Forgets one entry — after its rollback succeeded, or when the user dismisses it. */
export function dropAiUndo(snapshotId: string): void {
  aiUndoStack.update((stack) => stack.filter((entry) => entry.snapshotId !== snapshotId));
}

export function clearAiUndo(projectPath?: string): void {
  aiUndoStack.update((stack) =>
    projectPath === undefined ? [] : stack.filter((entry) => entry.projectPath !== projectPath),
  );
}

/** Reactive view of the newest undoable action for the project being edited. */
export function aiUndoFor(
  projectPath: Readable<string | null | undefined>,
): Readable<AiUndoEntry | null> {
  return derived([aiUndoStack, projectPath], ([$stack, $path]) => peekAiUndo($path, $stack));
}
