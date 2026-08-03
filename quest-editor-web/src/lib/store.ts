/**
 * Quest editor types and in-memory store.
 * No backend — everything lives in the browser.
 */

// ─── Types (matching FTB Quests SNBT schema) ────────────────────

export interface QuestTask {
  id: string;
  type: string;
  title?: string | null;
  value?: unknown;
  properties?: Record<string, unknown>;
}

export interface QuestReward {
  id: string;
  type: string;
  title?: string | null;
  properties?: Record<string, unknown>;
}

export interface QuestData {
  id: string;
  title: string;
  subtitle?: string | null;
  description?: string[];
  x: number;
  y: number;
  icon?: string | null;
  dependencies: string[];
  tasks: QuestTask[];
  rewards: QuestReward[];
  optional: boolean;
  shape?: string | null;
  size?: number;
  hideDependencyLines?: boolean | null;
  hideDependentLines?: boolean | null;
  canRepeat?: boolean | null;
  invisible?: boolean | null;
  disableToast?: boolean | null;
  minRequiredDependencies?: number | null;
  dependencyRequirement?: string | null;
  extras?: Record<string, unknown>;
}

export interface QuestChapter {
  id: string;
  title: string;
  icon?: string | null;
  group?: string | null;
  orderIndex?: number | null;
  filename?: string | null;
  defaultQuestShape?: string | null;
  defaultHideDependencyLines?: boolean | null;
  quests: QuestData[];
  extras?: Record<string, unknown>;
  sourceFile?: string | null;
}

export interface QuestChapterGroup {
  id: string;
  title: string;
}

export interface QuestBook {
  chapters: QuestChapter[];
  chapterGroups: QuestChapterGroup[];
  title?: string | null;
  subtitle?: string | null;
  bookSettings?: Record<string, unknown>;
}

export interface QuestValidationIssue {
  questId: string;
  message: string;
}

// ─── SNBT import/export helpers ─────────────────────────────────

import { parseSnbt, serializeSnbt, type SnbtValue } from "./snbt";

function gs(map: Record<string, SnbtValue>, key: string): string | undefined {
  const v = map[key];
  if (typeof v === "string") return v;
  if (v !== undefined && v !== null) return String(v);
  return undefined;
}

function iconIdFromMap(map: Record<string, SnbtValue>): string | undefined {
  const icon = map["icon"];
  if (typeof icon === "string") return icon;
  if (icon && typeof icon === "object" && !Array.isArray(icon)) {
    const id = (icon as Record<string, SnbtValue>)["id"];
    if (typeof id === "string") return id;
  }
  return undefined;
}

function parseDependencies(v: SnbtValue): string[] {
  if (Array.isArray(v)) return v.map((x) => String(x));
  if (typeof v === "string") return [v];
  return [];
}

function parseTasks(arr: SnbtValue[]): QuestTask[] {
  return arr
    .filter((x): x is Record<string, SnbtValue> => typeof x === "object" && x !== null && !Array.isArray(x))
    .map((m) => ({
      id: gs(m, "id") ?? "",
      type: gs(m, "type") ?? "checkmark",
      title: gs(m, "title") ?? null,
      value: m["value"] ?? null,
      properties: extractExtras(m, ["id", "type", "title", "value"]),
    }));
}

function parseRewards(arr: SnbtValue[]): QuestReward[] {
  return arr
    .filter((x): x is Record<string, SnbtValue> => typeof x === "object" && x !== null && !Array.isArray(x))
    .map((m) => ({
      id: gs(m, "id") ?? "",
      type: gs(m, "type") ?? "command",
      title: gs(m, "title") ?? null,
      properties: extractExtras(m, ["id", "type", "title"]),
    }));
}

function extractExtras(
  map: Record<string, SnbtValue>,
  known: string[]
): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const [k, v] of Object.entries(map)) {
    if (!known.includes(k)) out[k] = v;
  }
  return out;
}

// ─── Parse SNBT chapter ─────────────────────────────────────────

function parseSnbtChapter(content: string): QuestChapter | null {
  try {
    const j = parseSnbt(content);
    if (!j || typeof j !== "object" || Array.isArray(j)) return null;
    const m = j as Record<string, SnbtValue>;

    const questsArr = Array.isArray(m["quests"]) ? (m["quests"] as SnbtValue[]) : [];
    const quests: QuestData[] = questsArr
      .filter((q): q is Record<string, SnbtValue> => typeof q === "object" && q !== null && !Array.isArray(q))
      .map((qm) => {
        const q = qm as Record<string, SnbtValue>;
        const desc = q["description"];
        const description = Array.isArray(desc)
          ? desc.map((x) => String(x))
          : typeof desc === "string"
            ? [desc]
            : [];

        return {
          id: gs(q, "id") ?? "",
          title: gs(q, "title") ?? "Quest",
          subtitle: gs(q, "subtitle") ?? null,
          description,
          x: typeof q["x"] === "number" ? q["x"] : parseFloat(String(q["x"] ?? 0)),
          y: typeof q["y"] === "number" ? q["y"] : parseFloat(String(q["y"] ?? 0)),
          icon: iconIdFromMap(q) ?? null,
          dependencies: parseDependencies(q["dependencies"] ?? []),
          tasks: parseTasks(Array.isArray(q["tasks"]) ? (q["tasks"] as SnbtValue[]) : []),
          rewards: parseTasks(Array.isArray(q["rewards"]) ? (q["rewards"] as SnbtValue[]) : []) as QuestReward[],
          optional: q["optional"] === true || q["optional"] === 1,
          shape: gs(q, "shape") ?? null,
          size: typeof q["size"] === "number" ? q["size"] : undefined,
          hideDependencyLines: q["hide_dependency_lines"] === true ? true : q["hide_dependency_lines"] === false ? false : null,
          hideDependentLines: q["hide_dependent_lines"] === true ? true : q["hide_dependent_lines"] === false ? false : null,
          canRepeat: q["can_repeat"] === true ? true : q["can_repeat"] === false ? false : null,
          invisible: q["invisible"] === true ? true : q["invisible"] === false ? false : null,
          disableToast: q["disable_toast"] === true ? true : q["disable_toast"] === false ? false : null,
          minRequiredDependencies: typeof q["min_required_dependencies"] === "number" ? (q["min_required_dependencies"] as number) : null,
          dependencyRequirement: gs(q, "dependency_requirement") ?? null,
          extras: extractExtras(q, [
            "id", "title", "subtitle", "description", "x", "y", "icon",
            "dependencies", "tasks", "rewards", "optional", "shape", "size",
            "hide_dependency_lines", "hide_dependent_lines", "can_repeat",
            "invisible", "disable_toast", "min_required_dependencies",
            "dependency_requirement",
          ]),
        };
      });

    return {
      id: gs(m, "id") ?? "untitled",
      title: gs(m, "title") ?? "Untitled",
      icon: iconIdFromMap(m) ?? null,
      group: gs(m, "group") ?? null,
      orderIndex: typeof m["order_index"] === "number" ? (m["order_index"] as number) : null,
      filename: gs(m, "filename") ?? null,
      defaultQuestShape: gs(m, "default_quest_shape") ?? null,
      defaultHideDependencyLines: m["default_hide_dependency_lines"] === true ? true : m["default_hide_dependency_lines"] === false ? false : null,
      quests,
      extras: extractExtras(m, [
        "id", "title", "icon", "group", "order_index", "quests", "filename",
        "default_quest_shape", "default_hide_dependency_lines",
      ]),
      sourceFile: null,
    };
  } catch {
    return null;
  }
}

// ─── Public API ─────────────────────────────────────────────────

export function loadQuestBookFromSnbt(files: Map<string, string>): QuestBook {
  const chapters: QuestChapter[] = [];

  for (const [path, content] of files) {
    if (!path.endsWith(".snbt")) continue;
    // Skip non-chapter files
    if (path.includes("data.snbt") || path.includes("chapter_groups.snbt") || path.includes("reward_tables/")) continue;

    const ch = parseSnbtChapter(content);
    if (ch) {
      ch.sourceFile = path;
      chapters.push(ch);
    }
  }

  chapters.sort((a, b) => (a.orderIndex ?? 0) - (b.orderIndex ?? 0));

  return {
    chapters,
    chapterGroups: [],
    title: null,
    subtitle: null,
  };
}

// ─── Serialize back to SNBT ─────────────────────────────────────

function serializeQuestTask(t: QuestTask): Record<string, SnbtValue> {
  const obj: Record<string, SnbtValue> = { id: t.id, type: t.type };
  if (t.title) obj.title = t.title;
  if (t.value !== undefined) obj.value = t.value as SnbtValue;
  if (t.properties && Object.keys(t.properties).length > 0) {
    Object.assign(obj, t.properties);
  }
  return obj;
}

function serializeQuest(q: QuestData): Record<string, SnbtValue> {
  const obj: Record<string, SnbtValue> = {
    id: q.id,
    title: q.title,
    x: q.x,
    y: q.y,
  };
  if (q.subtitle) obj.subtitle = q.subtitle;
  if (q.description && q.description.length > 0) obj.description = q.description;
  if (q.icon) obj.icon = q.icon;
  if (q.dependencies.length > 0) obj.dependencies = q.dependencies;
  if (q.tasks.length > 0) obj.tasks = q.tasks.map(serializeQuestTask);
  if (q.rewards.length > 0) obj.rewards = q.rewards.map(serializeQuestTask);
  if (q.optional) obj.optional = 1;
  if (q.shape) obj.shape = q.shape;
  if (q.size !== undefined && q.size !== 1) obj.size = q.size;
  if (q.hideDependencyLines !== null) obj.hide_dependency_lines = q.hideDependencyLines ? 1 : 0;
  if (q.hideDependentLines !== null) obj.hide_dependent_lines = q.hideDependentLines ? 1 : 0;
  if (q.canRepeat !== null) obj.can_repeat = q.canRepeat ? 1 : 0;
  if (q.invisible !== null) obj.invisible = q.invisible ? 1 : 0;
  if (q.disableToast !== null) obj.disable_toast = q.disableToast ? 1 : 0;
  if (q.minRequiredDependencies !== null) obj.min_required_dependencies = q.minRequiredDependencies!;
  if (q.dependencyRequirement) obj.dependency_requirement = q.dependencyRequirement;
  if (q.extras) Object.assign(obj, q.extras);
  return obj;
}

function serializeChapter(ch: QuestChapter): string {
  const obj: Record<string, SnbtValue> = {
    id: ch.id,
    title: ch.title,
  };
  if (ch.icon) obj.icon = ch.icon;
  if (ch.group) obj.group = ch.group;
  if (ch.orderIndex !== null && ch.orderIndex !== undefined) obj.order_index = ch.orderIndex;
  if (ch.filename) obj.filename = ch.filename;
  if (ch.defaultQuestShape) obj.default_quest_shape = ch.defaultQuestShape;
  if (ch.defaultHideDependencyLines !== null) obj.default_hide_dependency_lines = ch.defaultHideDependencyLines ? 1 : 0;
  if (ch.quests.length > 0) obj.quests = ch.quests.map(serializeQuest);
  if (ch.extras) Object.assign(obj, ch.extras);

  return serializeSnbt(obj);
}

export function exportChapterSnbt(ch: QuestChapter): string {
  return serializeChapter(ch);
}

// ─── Validation ─────────────────────────────────────────────────

export function validateQuestBook(book: QuestBook): QuestValidationIssue[] {
  const issues: QuestValidationIssue[] = [];
  const allIds = new Set<string>();

  for (const ch of book.chapters) {
    for (const q of ch.quests) {
      if (allIds.has(q.id)) {
        issues.push({ questId: q.id, message: `Duplicate quest id "${q.id}"` });
      }
      allIds.add(q.id);

      if (!q.title.trim()) {
        issues.push({ questId: q.id, message: "Empty title" });
      }
      if (q.tasks.length === 0) {
        issues.push({ questId: q.id, message: "No tasks defined" });
      }
    }

    // Check dependency references
    for (const q of ch.quests) {
      for (const dep of q.dependencies) {
        if (!allIds.has(dep)) {
          // Also check task ids
          const taskExists = ch.quests.some((oq) => oq.tasks.some((t) => t.id === dep));
          if (!taskExists) {
            issues.push({ questId: q.id, message: `Missing dependency "${dep}"` });
          }
        }
      }
    }

    // Cycle detection
    const visited = new Set<string>();
    const inStack = new Set<string>();
    function hasCycle(qid: string): boolean {
      if (inStack.has(qid)) return true;
      if (visited.has(qid)) return false;
      visited.add(qid);
      inStack.add(qid);
      const q = ch.quests.find((x) => x.id === qid);
      if (q) {
        for (const dep of q.dependencies) {
          if (hasCycle(dep)) return true;
        }
      }
      inStack.delete(qid);
      return false;
    }
    for (const q of ch.quests) {
      visited.clear();
      inStack.clear();
      if (hasCycle(q.id)) {
        issues.push({ questId: q.id, message: "Dependency cycle detected" });
        break; // one per chapter is enough
      }
    }
  }

  return issues;
}

// ─── LocalStorage persistence ───────────────────────────────────

const STORAGE_KEY = "tuffbox.quest_editor.book";

export function saveToStorage(book: QuestBook): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(book));
  } catch { /* ignore */ }
}

export function loadFromStorage(): QuestBook | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    return JSON.parse(raw) as QuestBook;
  } catch {
    return null;
  }
}

export function clearStorage(): void {
  localStorage.removeItem(STORAGE_KEY);
}
