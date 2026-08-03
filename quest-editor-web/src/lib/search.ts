/**
 * Search functionality for quest editor.
 * Supports searching by title, id, and description.
 */

import type { QuestData, QuestChapter } from "./store";

export interface SearchResult {
  quest: QuestData;
  chapterId: string;
  chapterTitle: string;
  matchField: "title" | "id" | "description";
  matchText: string;
}

export interface SearchState {
  query: string;
  results: SearchResult[];
  selectedIndex: number;
  isOpen: boolean;
}

export function createSearchState(): SearchState {
  return {
    query: "",
    results: [],
    selectedIndex: 0,
    isOpen: false,
  };
}

export function searchQuests(
  query: string,
  chapters: QuestChapter[]
): SearchResult[] {
  if (!query.trim()) return [];

  const lower = query.toLowerCase();
  const results: SearchResult[] = [];

  for (const ch of chapters) {
    for (const q of ch.quests) {
      // Search by title
      if (q.title.toLowerCase().includes(lower)) {
        results.push({
          quest: q,
          chapterId: ch.id,
          chapterTitle: ch.title,
          matchField: "title",
          matchText: q.title,
        });
        continue;
      }

      // Search by id
      if (q.id.toLowerCase().includes(lower)) {
        results.push({
          quest: q,
          chapterId: ch.id,
          chapterTitle: ch.title,
          matchField: "id",
          matchText: q.id,
        });
        continue;
      }

      // Search by description
      if (q.description && q.description.length > 0) {
        const descText = q.description.join(" ");
        if (descText.toLowerCase().includes(lower)) {
          results.push({
            quest: q,
            chapterId: ch.id,
            chapterTitle: ch.title,
            matchField: "description",
            matchText: descText.substring(0, 100),
          });
        }
      }
    }
  }

  return results;
}

export function nextResult(state: SearchState): SearchState {
  if (state.results.length === 0) return state;
  return {
    ...state,
    selectedIndex: (state.selectedIndex + 1) % state.results.length,
  };
}

export function prevResult(state: SearchState): SearchState {
  if (state.results.length === 0) return state;
  return {
    ...state,
    selectedIndex: state.selectedIndex > 0
      ? state.selectedIndex - 1
      : state.results.length - 1,
  };
}
