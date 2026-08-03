/**
 * Undo/Redo history stack for quest editor state.
 * Uses snapshot-based approach for simplicity and reliability.
 */

export interface HistorySnapshot {
  chapters: string; // JSON serialized
  chapterGroups: string;
  selectedChapter: string;
}

export interface HistoryState {
  undoStack: HistorySnapshot[];
  redoStack: HistorySnapshot[];
  maxSize: number;
}

export function createHistoryState(maxSize: number = 100): HistoryState {
  return {
    undoStack: [],
    redoStack: [],
    maxSize,
  };
}

export function pushSnapshot(
  state: HistoryState,
  chapters: unknown[],
  chapterGroups: unknown[],
  selectedChapter: string
): HistoryState {
  const snapshot: HistorySnapshot = {
    chapters: JSON.stringify(chapters),
    chapterGroups: JSON.stringify(chapterGroups),
    selectedChapter,
  };

  // Don't push if identical to top of undo stack
  const top = state.undoStack[state.undoStack.length - 1];
  if (top && top.chapters === snapshot.chapters && top.chapterGroups === snapshot.chapterGroups) {
    return state;
  }

  const newUndo = [...state.undoStack, snapshot];
  if (newUndo.length > state.maxSize) {
    newUndo.shift();
  }

  return {
    ...state,
    undoStack: newUndo,
    redoStack: [], // Clear redo on new action
  };
}

export function undo(
  state: HistoryState,
  currentChapters: unknown[],
  currentChapterGroups: unknown[],
  currentSelectedChapter: string
): { state: HistoryState; snapshot: HistorySnapshot | null } {
  if (state.undoStack.length === 0) {
    return { state, snapshot: null };
  }

  // Save current state to redo stack
  const currentSnapshot: HistorySnapshot = {
    chapters: JSON.stringify(currentChapters),
    chapterGroups: JSON.stringify(currentChapterGroups),
    selectedChapter: currentSelectedChapter,
  };

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
  currentSelectedChapter: string
): { state: HistoryState; snapshot: HistorySnapshot | null } {
  if (state.redoStack.length === 0) {
    return { state, snapshot: null };
  }

  // Save current state to undo stack
  const currentSnapshot: HistorySnapshot = {
    chapters: JSON.stringify(currentChapters),
    chapterGroups: JSON.stringify(currentChapterGroups),
    selectedChapter: currentSelectedChapter,
  };

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
