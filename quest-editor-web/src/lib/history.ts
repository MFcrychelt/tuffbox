/**
 * Undo/Redo history stack for quest editor state.
 * Per-chapter JSON strings with structural sharing across snapshots.
 */

export interface HistorySnapshot {
  /** chapter id → JSON.stringify(chapter); unchanged chapters reuse prior string refs */
  chapterJsonById: Record<string, string>;
  chapterOrder: string[];
  chapterGroups: string;
  selectedChapter: string;
}

export interface HistoryState {
  undoStack: HistorySnapshot[];
  redoStack: HistorySnapshot[];
  maxSize: number;
}

type ChapterLike = { id?: unknown };

export function createHistoryState(maxSize: number = 100): HistoryState {
  return {
    undoStack: [],
    redoStack: [],
    maxSize,
  };
}

function chapterId(ch: unknown, index: number): string {
  const id = (ch as ChapterLike)?.id;
  return typeof id === "string" && id.length > 0 ? id : `__idx_${index}`;
}

function buildSnapshot(
  chapters: unknown[],
  chapterGroups: unknown[],
  selectedChapter: string,
  prevMap?: Record<string, string>,
): HistorySnapshot {
  const chapterJsonById: Record<string, string> = {};
  const chapterOrder: string[] = [];
  for (let i = 0; i < chapters.length; i++) {
    const ch = chapters[i];
    const id = chapterId(ch, i);
    chapterOrder.push(id);
    const next = JSON.stringify(ch);
    const prev = prevMap?.[id];
    chapterJsonById[id] = prev !== undefined && prev === next ? prev : next;
  }
  return {
    chapterJsonById,
    chapterOrder,
    chapterGroups: JSON.stringify(chapterGroups),
    selectedChapter,
  };
}

function snapshotsEqual(a: HistorySnapshot, b: HistorySnapshot): boolean {
  if (a.selectedChapter !== b.selectedChapter) return false;
  if (a.chapterGroups !== b.chapterGroups) return false;
  if (a.chapterOrder.length !== b.chapterOrder.length) return false;
  for (let i = 0; i < a.chapterOrder.length; i++) {
    if (a.chapterOrder[i] !== b.chapterOrder[i]) return false;
  }
  for (const id of a.chapterOrder) {
    if (a.chapterJsonById[id] !== b.chapterJsonById[id]) return false;
  }
  return true;
}

export function materializeChapters(snapshot: HistorySnapshot): unknown[] {
  return snapshot.chapterOrder.map((id) => {
    const raw = snapshot.chapterJsonById[id];
    if (raw === undefined) {
      throw new Error(`history snapshot missing chapter ${id}`);
    }
    return JSON.parse(raw);
  });
}

export function pushSnapshot(
  state: HistoryState,
  chapters: unknown[],
  chapterGroups: unknown[],
  selectedChapter: string,
): HistoryState {
  const top = state.undoStack[state.undoStack.length - 1];
  const snapshot = buildSnapshot(
    chapters,
    chapterGroups,
    selectedChapter,
    top?.chapterJsonById,
  );

  if (top && snapshotsEqual(top, snapshot)) {
    return state;
  }

  const newUndo = [...state.undoStack, snapshot];
  if (newUndo.length > state.maxSize) {
    newUndo.shift();
  }

  return {
    ...state,
    undoStack: newUndo,
    redoStack: [],
  };
}

export function undo(
  state: HistoryState,
  currentChapters: unknown[],
  currentChapterGroups: unknown[],
  currentSelectedChapter: string,
): { state: HistoryState; snapshot: HistorySnapshot | null } {
  if (state.undoStack.length === 0) {
    return { state, snapshot: null };
  }

  const top = state.undoStack[state.undoStack.length - 1];
  const currentSnapshot = buildSnapshot(
    currentChapters,
    currentChapterGroups,
    currentSelectedChapter,
    top.chapterJsonById,
  );

  const newUndo = [...state.undoStack];
  const snapshot = newUndo.pop()!;

  return {
    state: {
      ...state,
      undoStack: newUndo,
      redoStack: [...state.redoStack, currentSnapshot],
    },
    snapshot,
  };
}

export function redo(
  state: HistoryState,
  currentChapters: unknown[],
  currentChapterGroups: unknown[],
  currentSelectedChapter: string,
): { state: HistoryState; snapshot: HistorySnapshot | null } {
  if (state.redoStack.length === 0) {
    return { state, snapshot: null };
  }

  const topRedo = state.redoStack[state.redoStack.length - 1];
  const currentSnapshot = buildSnapshot(
    currentChapters,
    currentChapterGroups,
    currentSelectedChapter,
    // Prefer sharing vs undo top, then vs redo entry being applied.
    state.undoStack[state.undoStack.length - 1]?.chapterJsonById ??
      topRedo.chapterJsonById,
  );

  const newRedo = [...state.redoStack];
  const snapshot = newRedo.pop()!;

  return {
    state: {
      ...state,
      undoStack: [...state.undoStack, currentSnapshot],
      redoStack: newRedo,
    },
    snapshot,
  };
}

export function canUndo(state: HistoryState): boolean {
  return state.undoStack.length > 0;
}

export function canRedo(state: HistoryState): boolean {
  return state.redoStack.length > 0;
}

export function clearHistory(): HistoryState {
  return {
    undoStack: [],
    redoStack: [],
    maxSize: 100,
  };
}
