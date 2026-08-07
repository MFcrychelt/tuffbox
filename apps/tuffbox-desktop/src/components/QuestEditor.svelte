<script lang="ts">
  import { api, type QuestChapter, type QuestChapterGroup, type QuestData, type QuestValidationIssue, type QuestProgressTeamRef, type QuestProgressSnapshot, type QuestProgressStatus, type QuestPlanMergeResult, stripLocaleOverlay, chapterToSnbtJson } from "../lib/api";
  import { ScrollText, RefreshCw, Save, AlertTriangle, CheckCircle2, Map as MapIcon, Eye, Sparkles, X, Undo2, Redo2, Keyboard } from "@lucide/svelte";
  import { onDestroy } from "svelte";
  import { projectPath, questDirty, questChatFocusId } from "../lib/store";
  import EmptyState from "./EmptyState.svelte";
  import ChapterRail from "./quests/ChapterRail.svelte";
  import { SvelteFlowProvider } from "@xyflow/svelte";
  import QuestCanvas from "./quests/QuestCanvas.svelte";
  import QuestInspector from "./quests/QuestInspector.svelte";
  import ChapterSettings from "./quests/ChapterSettings.svelte";
  import RewardTablesPanel from "./quests/RewardTablesPanel.svelte";
  import QuestAiSidebar from "./quests/QuestAiSidebar.svelte";
  import BatchEditor from "./quests/BatchEditor.svelte";
  import ColorManager from "./quests/ColorManager.svelte";
  import RawSnbtView from "./quests/RawSnbtView.svelte";
  import SnbtDiffModal from "./quests/SnbtDiffModal.svelte";
  import LocalePanel from "./quests/LocalePanel.svelte";
  import ShortcutsModal from "./ui/ShortcutsModal.svelte";
  import { wouldCreateQuestCycle } from "./quests/deps";
  import type { QuestRewardTable } from "../lib/api";
  import { snbtTextsEqual } from "../lib/snbtDiff";
  import {
    applyLocaleOverlay,
    clearLocaleTitles,
    cloneLocaleMap,
    harvestLocaleMap,
    isValidLocaleCode,
    localeCodes,
    type LocaleGapEntry,
    type LocaleMap,
  } from "../lib/questLocale";
  import {
    createHistoryState,
    pushSnapshot,
    undo as historyUndo,
    redo as historyRedo,
    canUndo,
    canRedo,
    clearHistory,
    materializeChapters,
    diffDirtyChapterIds,
    chapterJsonMap,
    serializeBookMeta,
    parseBookMeta,
    type HistoryExtras,
    type HistorySnapshot,
  } from "../lib/questHistory";
  import {
    createSelectionState,
    selectSingle,
    toggleSelect,
    addToSelection,
    selectAll,
    clearSelection,
  } from "../lib/questSelection";
  import {
    createSearchState,
    searchQuests,
    nextResult,
    prevResult,
  } from "../lib/questSearch";
  import { stripCodes } from "../lib/mcformat";
  import { validateQuestBook } from "../lib/questValidate";
  import { applyLayout, positionsForMode, type LayoutMode } from "../lib/questLayout";

  const BOOK_BOOL_KEYS = [
    "pause_game",
    "show_lock_icons",
    "hide_offline",
    "drop_loot_crates",
    "disable_gui",
    "disable_toast",
    "disable_cheating",
    "default_consume_items",
  ] as const;
  const BOOK_STRING_KEYS = ["default_quest_shape", "theme", "progression_mode"] as const;
  const BOOK_NUMBER_KEYS = ["default_quest_size"] as const;
  const BOOK_CURATED = new Set<string>([...BOOK_BOOL_KEYS, ...BOOK_STRING_KEYS, ...BOOK_NUMBER_KEYS]);

  function asLocaleMaps(
    raw: Record<string, Record<string, string | string[] | unknown>> | undefined,
  ): Record<string, LocaleMap> {
    const out: Record<string, LocaleMap> = {};
    for (const [code, map] of Object.entries(raw ?? {})) {
      const cleaned: LocaleMap = {};
      for (const [k, v] of Object.entries(map)) {
        if (typeof v === "string" || Array.isArray(v)) cleaned[k] = v as string | string[];
      }
      out[code] = cleaned;
    }
    return out;
  }

  const AI_SIDEBAR_KEY = "tuffbox.quests.aiSidebar";

  function readAiSidebarPref(): boolean {
    try {
      return localStorage.getItem(AI_SIDEBAR_KEY) === "true";
    } catch {
      return false;
    }
  }

  let chapters = $state<QuestChapter[]>([]);
  let chapterGroups = $state<QuestChapterGroup[]>([]);
  let bookTitle = $state<string | null>(null);
  let bookSubtitle = $state<string | null>(null);
  let bookSettings = $state<Record<string, unknown>>({});
  let locales = $state<Record<string, LocaleMap>>({});
  let activeLocale = $state<string | null>(null);
  let dirtyLocales = $state(new Set<string>());
  let compareLocale = $state<string | null>(null);
  let rewardTables = $state<QuestRewardTable[]>([]);
  let rewardTablesDirty = $state(false);
  let bookDirty = $state(false);
  let groupsDirty = $state(false);
  let loading = $state(false);
  let saving = $state(false);
  let error = $state<string | null>(null);
  let message = $state<string | null>(null);
  let selectedChapter = $state("");
  let selectedQuest = $state<QuestData | null>(null);
  let validationIssues = $state<QuestValidationIssue[]>([]);
  let dirtyChapters = $state(new Set<string>());
  let lastLoadedPath = $state<string | null>(null);
  let fitToken = $state(0);
  let questSearch = $state("");
  let showBookPanel = $state(false);
  let showGroupsPanel = $state(false);
  let showTablesPanel = $state(false);
  let showLocalePanel = $state(false);
  let bookMenuOpen = $state(false);
  let issuesOpen = $state(false);
  let progressOpen = $state(false);

  // Phase C — player progress overlay (read-only) + G-Playtest simulate
  let progressTeams = $state<QuestProgressTeamRef[]>([]);
  let progressKey = $state(""); // relativePath
  let progressOverlay = $state(false);
  let progressSnap = $state<QuestProgressSnapshot | null>(null);
  let progressLoading = $state(false);
  let progressMode = $state<"save" | "simulate">("save");
  let simCompleted = $state<string[]>([]);
  let simBusy = $state(false);

  let aiSidebarOpen = $state(readAiSidebarPref());
  let history = $state(createHistoryState());
  let selection = $state(createSelectionState());
  let search = $state(createSearchState());
  let clipboard = $state<QuestData[]>([]);
  let showShortcuts = $state(false);
  let panelTab = $state<"quest" | "info" | "batch" | "colors" | "raw">("info");
  let validateTimer: ReturnType<typeof setTimeout> | null = null;
  let itemCatalogCache = $state<Set<string> | null>(null);
  let snbtDiffOpen = $state(false);
  let snbtDiffTitle = $state("Review SNBT changes");
  let snbtDiffLeftLabel = $state("Disk");
  let snbtDiffRightLabel = $state("Editor");
  let snbtDiffLeft = $state("");
  let snbtDiffRight = $state("");
  let snbtDiffConfirmLabel = $state("Save");
  let snbtDiffResolver: ((ok: boolean) => void) | null = null;

  function scheduleLiveValidate() {
    if (validateTimer) clearTimeout(validateTimer);
    validateTimer = setTimeout(() => {
      validateTimer = null;
      validationIssues = validateQuestBook(
        { chapters, chapterGroups },
        { availableItems: itemCatalogCache },
      );
    }, 200);
  }

  function setAiSidebar(open: boolean) {
    aiSidebarOpen = open;
    try {
      localStorage.setItem(AI_SIDEBAR_KEY, open ? "true" : "false");
    } catch {
      /* ignore */
    }
  }

  async function load() {
    if (!$projectPath) return;
    loading = true;
    error = null;
    message = null;
    try {
      const book = await api.quests.load($projectPath);
      locales = asLocaleMaps(book.locales);
      const overlay = applyLocaleOverlay(
        {
          title: book.title ?? null,
          subtitle: book.subtitle ?? null,
          activeLocale: book.activeLocale ?? null,
          locales,
          chapterGroups: book.chapterGroups ?? [],
          chapters: book.chapters ?? [],
        },
        book.activeLocale,
      );
      chapters = overlay.chapters;
      chapterGroups = overlay.chapterGroups;
      bookTitle = overlay.title ?? null;
      bookSubtitle = overlay.subtitle ?? null;
      activeLocale = overlay.activeLocale ?? null;
      bookSettings = book.bookSettings ?? {};
      bookDirty = false;
      groupsDirty = false;
      rewardTables = (book.rewardTables ?? []).map((t) => ({
        ...t,
        rewards: t.rewards ?? [],
        emptyWeight: t.emptyWeight ?? 0,
        extras: t.extras ?? {},
      }));
      rewardTablesDirty = false;
      dirtyChapters = new Set();
      dirtyLocales = new Set();
      compareLocale = null;
      history = clearHistory();
      selection = clearSelection();
      search = createSearchState();
      clipboard = [];
      if (chapters.length > 0 && !chapters.some((c) => c.id === selectedChapter)) {
        selectedChapter = chapters[0].id;
      }
      selectedQuest = null;
      try {
        const catalog = await api.quests.itemCatalog($projectPath);
        itemCatalogCache = new Set(catalog ?? []);
      } catch {
        itemCatalogCache = null;
      }
      validationIssues = await api.quests.validate($projectPath);
      lastLoadedPath = $projectPath;
      fitToken += 1;
      await refreshProgressTeams();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  function switchLocale(code: string) {
    if (activeLocale && activeLocale !== code) {
      const base = locales[activeLocale] ?? {};
      locales = {
        ...locales,
        [activeLocale]: harvestLocaleMap({ chapters, chapterGroups }, base),
      };
    }
    const book = {
      title: bookTitle,
      subtitle: bookSubtitle,
      activeLocale: code,
      locales,
      chapterGroups,
      chapters,
    };
    clearLocaleTitles(book);
    applyLocaleOverlay(book, code);
    chapters = [...book.chapters];
    chapterGroups = [...book.chapterGroups];
    bookTitle = book.title ?? null;
    bookSubtitle = book.subtitle ?? null;
    activeLocale = book.activeLocale ?? code;
    if (compareLocale === activeLocale) compareLocale = null;
    if (selectedQuest) {
      const fresh = chapters
        .flatMap((c) => c.quests)
        .find((q) => q.id === selectedQuest!.id);
      selectedQuest = fresh ?? null;
    }
  }

  function bookBool(key: string): boolean {
    const v = bookSettings[key];
    return v === true || v === 1 || v === "1" || v === "true";
  }

  function setBookBool(key: string, value: boolean) {
    pushHistory();
    bookSettings = { ...bookSettings, [key]: value };
    bookDirty = true;
  }

  function bookString(key: string): string {
    const v = bookSettings[key];
    return typeof v === "string" ? v : v == null ? "" : String(v);
  }

  function setBookString(key: string, value: string) {
    pushHistory();
    bookSettings = { ...bookSettings, [key]: value };
    bookDirty = true;
  }

  function bookNumber(key: string): string {
    const v = bookSettings[key];
    return typeof v === "number" ? String(v) : v == null || v === "" ? "" : String(v);
  }

  function setBookNumber(key: string, raw: string) {
    pushHistory();
    const next = { ...bookSettings };
    if (raw.trim() === "") delete next[key];
    else {
      const n = Number(raw);
      next[key] = Number.isFinite(n) ? n : raw;
    }
    bookSettings = next;
    bookDirty = true;
  }

  function removeBookSetting(key: string) {
    pushHistory();
    const next = { ...bookSettings };
    delete next[key];
    bookSettings = next;
    bookDirty = true;
  }

  const bookExtraEntries = $derived(
    Object.entries(bookSettings).filter(([k]) => !BOOK_CURATED.has(k)),
  );
  const availableLocales = $derived(localeCodes(locales));

  function requestReload() {
    if (hasDirty && !confirm("Reload and discard unsaved quest edits?")) return;
    void load();
  }

  async function refreshProgressTeams() {
    if (!$projectPath) return;
    try {
      progressTeams = await api.quests.listProgressTeams($projectPath);
      if (progressKey && !progressTeams.some((t) => t.relativePath === progressKey)) {
        progressKey = "";
        progressSnap = null;
        progressOverlay = false;
      }
    } catch {
      progressTeams = [];
    }
  }

  async function loadProgress() {
    if (!$projectPath || !progressKey) {
      if (progressMode === "save") progressSnap = null;
      return;
    }
    progressLoading = true;
    try {
      progressSnap = await api.quests.loadProgress(progressKey, $projectPath);
      progressOverlay = true;
      progressMode = "save";
    } catch (e) {
      error = String(e);
      progressSnap = null;
    } finally {
      progressLoading = false;
    }
  }

  function editorBookPayload() {
    return {
      chapters,
      chapterGroups,
      rewardTables,
      title: bookTitle,
      subtitle: bookSubtitle,
      bookSettings,
      locales,
      activeLocale,
    };
  }

  async function refreshSimulate() {
    simBusy = true;
    try {
      progressSnap = await api.quests.simulateProgress(
        editorBookPayload(),
        simCompleted,
      );
      progressOverlay = true;
    } catch (e) {
      error = String(e);
    } finally {
      simBusy = false;
    }
  }

  async function enterSimulateMode() {
    progressMode = "simulate";
    await refreshSimulate();
  }

  async function enterSaveMode() {
    progressMode = "save";
    if (progressKey) await loadProgress();
    else {
      progressSnap = null;
      progressOverlay = false;
    }
  }

  async function seedSimulateFromTeam() {
    if (!$projectPath || !progressKey) {
      error = "Select a save team first, then Seed.";
      return;
    }
    progressLoading = true;
    try {
      const snap = await api.quests.loadProgress(progressKey, $projectPath);
      simCompleted = Object.entries(snap.statuses ?? {})
        .filter(([, st]) => st === "completed")
        .map(([id]) => id);
      progressMode = "simulate";
      await refreshSimulate();
      message = `Simulate seeded with ${simCompleted.length} completed quest(s)`;
    } catch (e) {
      error = String(e);
    } finally {
      progressLoading = false;
    }
  }

  async function resetSimulate() {
    simCompleted = [];
    await refreshSimulate();
  }

  async function toggleSimQuest(questId: string) {
    if (progressMode !== "simulate" || !progressOverlay) return;
    if (simCompleted.includes(questId)) {
      simCompleted = simCompleted.filter((id) => id !== questId);
    } else {
      simCompleted = [...simCompleted, questId];
    }
    await refreshSimulate();
  }

  function inputVal(e: Event): string {
    return (e.currentTarget as HTMLInputElement).value;
  }

  const progressStatuses = $derived((progressSnap?.statuses ?? {}) as Record<string, QuestProgressStatus>);
  const progressTeamLabel = $derived(progressTeams.find((t) => t.relativePath === progressKey));

  function markDirty(chapterId: string) {
    dirtyChapters = new Set([...dirtyChapters, chapterId]);
    chapters = [...chapters];
    if (activeLocale) {
      dirtyLocales = new Set([...dirtyLocales, activeLocale]);
    }
    scheduleLiveValidate();
  }

  function markLocaleDirty(code: string) {
    dirtyLocales = new Set([...dirtyLocales, code]);
  }

  async function saveLocaleIfNeeded() {
    if (!$projectPath || dirtyLocales.size === 0) return;
    // Harvest active overlay into its map only when that locale is dirty.
    if (activeLocale && dirtyLocales.has(activeLocale)) {
      const base = locales[activeLocale] ?? {};
      const harvested = harvestLocaleMap({ chapters, chapterGroups }, base);
      locales = { ...locales, [activeLocale]: harvested };
    }
    for (const code of [...dirtyLocales]) {
      const map = locales[code];
      if (!map) continue;
      await api.quests.saveLocale(code, map, $projectPath);
    }
    dirtyLocales = new Set();
  }

  async function createLocale(code: string, fromCode: string | null) {
    if (!$projectPath) throw new Error("No project open");
    const normalized = code.trim().toLowerCase();
    if (!isValidLocaleCode(normalized)) {
      throw new Error("Use a code like en_us or ru_ru");
    }
    if (locales[normalized]) {
      throw new Error(`Locale “${normalized}” already exists`);
    }
    // Persist active harvest before cloning so copy-from is fresh when from===active.
    if (activeLocale && dirtyLocales.has(activeLocale)) {
      const base = locales[activeLocale] ?? {};
      locales = {
        ...locales,
        [activeLocale]: harvestLocaleMap({ chapters, chapterGroups }, base),
      };
    }
    let seed: LocaleMap;
    if (fromCode && locales[fromCode]) {
      seed = cloneLocaleMap(locales[fromCode]!);
    } else {
      seed = harvestLocaleMap({ chapters, chapterGroups }, {});
    }
    locales = { ...locales, [normalized]: seed };
    await api.quests.saveLocale(normalized, seed, $projectPath);
    dirtyLocales = new Set([...dirtyLocales].filter((c) => c !== normalized));
    switchLocale(normalized);
    message = `Created lang/${normalized}.snbt`;
  }

  function jumpToLocaleGap(entry: LocaleGapEntry) {
    showLocalePanel = true;
    bookMenuOpen = true;
    let questId = entry.questId;
    if (!questId && entry.key.startsWith("task.")) {
      const taskId = entry.key.split(".")[1];
      for (const ch of chapters) {
        const owner = ch.quests.find((q) => q.tasks?.some((t) => t.id === taskId));
        if (owner) {
          questId = owner.id;
          break;
        }
      }
    }
    if (questId) {
      jumpToIssue({ questId, message: entry.key });
      panelTab = "quest";
      return;
    }
    if (entry.chapterId && chapters.some((c) => c.id === entry.chapterId)) {
      selectedChapter = entry.chapterId;
      selectedQuest = null;
      return;
    }
    if (entry.groupId) {
      showGroupsPanel = true;
      showLocalePanel = false;
    }
  }

  async function promptSnbtDiff(opts: {
    title: string;
    leftLabel?: string;
    rightLabel?: string;
    leftText: string;
    rightText: string;
    confirmLabel?: string;
  }): Promise<boolean> {
    snbtDiffTitle = opts.title;
    snbtDiffLeftLabel = opts.leftLabel ?? "Disk";
    snbtDiffRightLabel = opts.rightLabel ?? "Editor";
    snbtDiffLeft = opts.leftText;
    snbtDiffRight = opts.rightText;
    snbtDiffConfirmLabel = opts.confirmLabel ?? "Save";
    snbtDiffOpen = true;
    return await new Promise<boolean>((resolve) => {
      snbtDiffResolver = resolve;
    });
  }

  function closeSnbtDiff(ok: boolean) {
    snbtDiffOpen = false;
    const r = snbtDiffResolver;
    snbtDiffResolver = null;
    r?.(ok);
  }

  function chapterFilePath(ch: QuestChapter, projectDir: string): { relativePath: string; filePath: string } {
    const relativePath =
      ch.sourceFile ??
      `config/ftbquests/quests/chapters/${ch.filename ?? ch.id}.snbt`;
    const sep = projectDir.includes("\\") ? "\\" : "/";
    const filePath = `${projectDir.replace(/[/\\]+$/, "")}${sep}${relativePath.replace(/^[/\\]+/, "").replace(/[/\\]/g, sep)}`;
    return { relativePath: relativePath.replace(/\\/g, "/"), filePath };
  }

  async function saveChapter(
    chapterId: string,
    opts?: { skipDiff?: boolean },
  ): Promise<"saved" | "cancelled" | "error"> {
    if (!$projectPath) return "error";
    const ch = chapters.find((c) => c.id === chapterId);
    if (!ch) return "error";
    saving = true;
    error = null;
    message = null;
    try {
      await saveLocaleIfNeeded();
      const projectDir = await api.project.getDir($projectPath);
      const { relativePath, filePath } = chapterFilePath(ch, projectDir);
      // Locale text lives in lang/*.snbt — never hardcode it back into the chapter file.
      const payload = stripLocaleOverlay(chapterToSnbtJson(ch));
      const jsonPayload = JSON.stringify(payload);

      if (!opts?.skipDiff) {
        let diskText: string | null = null;
        try {
          diskText = await api.quests.readChapterText(filePath);
        } catch {
          diskText = null;
        }
        if (diskText != null) {
          const editorText = await api.quests.previewChapterSnbt(jsonPayload);
          if (!snbtTextsEqual(diskText, editorText)) {
            saving = false;
            const ok = await promptSnbtDiff({
              title: `Save chapter “${ch.title || ch.id}”?`,
              leftLabel: relativePath,
              rightLabel: "Editor (about to write)",
              leftText: diskText,
              rightText: editorText,
              confirmLabel: "Write SNBT",
            });
            if (!ok) {
              message = "Save cancelled";
              return "cancelled";
            }
            saving = true;
          }
        }
      }

      await api.quests.saveChapterRaw(filePath, jsonPayload);
      ch.sourceFile = relativePath;
      dirtyChapters = new Set([...dirtyChapters].filter((id) => id !== chapterId));
      chapters = [...chapters];
      message = `Saved ${ch.quests.length} quests → ${ch.sourceFile}`;
      validationIssues = await api.quests.validate($projectPath);
      return "saved";
    } catch (e) {
      error = String(e);
      return "error";
    } finally {
      saving = false;
    }
  }

  async function saveAll() {
    try {
      await saveLocaleIfNeeded();
    } catch (e) {
      error = String(e);
      return;
    }
    const parts: string[] = [];
    if (dirtyChapters.size) parts.push(`${dirtyChapters.size} chapter(s)`);
    if (rewardTablesDirty) parts.push("reward tables");
    if (bookDirty) parts.push("book data");
    if (groupsDirty) parts.push("chapter groups");
    if (parts.length === 0) {
      message = "Nothing to save";
      return;
    }
    const chapterNames = [...dirtyChapters]
      .map((id) => chapters.find((c) => c.id === id)?.title || id)
      .slice(0, 8);
    const summary =
      `Save All: ${parts.join(", ")}` +
      (chapterNames.length
        ? `\n\nChapters:\n- ${chapterNames.join("\n- ")}${dirtyChapters.size > chapterNames.length ? "\n- …" : ""}`
        : "");
    if (!window.confirm(summary)) return;

    for (const id of [...dirtyChapters]) {
      const result = await saveChapter(id);
      if (result === "cancelled") break;
    }
    if (rewardTablesDirty) {
      for (const t of rewardTables) {
        await saveRewardTable(t);
      }
    }
    if (bookDirty) await saveBookData();
    if (groupsDirty) await saveGroups();
  }

  async function revalidateFromDisk() {
    if (!$projectPath) return;
    try {
      validationIssues = await api.quests.validate($projectPath);
      message = `Disk validate: ${validationIssues.length} issue(s)`;
    } catch (e) {
      error = String(e);
    }
  }

  async function showChapterDiffVsDisk(chapterId?: string) {
    if (!$projectPath) return;
    const ch = chapters.find((c) => c.id === (chapterId ?? selectedChapter));
    if (!ch) return;
    try {
      const projectDir = await api.project.getDir($projectPath);
      const { relativePath, filePath } = chapterFilePath(ch, projectDir);
      let diskText: string;
      try {
        diskText = await api.quests.readChapterText(filePath);
      } catch {
        message = "No chapter file on disk yet";
        return;
      }
      const payload = stripLocaleOverlay(chapterToSnbtJson(ch));
      const editorText = await api.quests.previewChapterSnbt(JSON.stringify(payload));
      if (snbtTextsEqual(diskText, editorText)) {
        message = "Editor matches disk SNBT";
        return;
      }
      await promptSnbtDiff({
        title: `Diff vs disk — ${ch.title || ch.id}`,
        leftLabel: relativePath,
        rightLabel: "Editor",
        leftText: diskText,
        rightText: editorText,
        confirmLabel: "Close",
      });
    } catch (e) {
      error = String(e);
    }
  }

  function historyExtras(): HistoryExtras {
    return {
      bookMetaJson: serializeBookMeta({
        title: bookTitle,
        subtitle: bookSubtitle,
        bookSettings,
      }),
      rewardTablesJson: JSON.stringify(rewardTables),
    };
  }

  function pushHistory() {
    history = pushSnapshot(
      history,
      chapters,
      chapterGroups,
      selectedChapter,
      historyExtras(),
    );
  }

  function applyHistorySnapshot(snapshot: HistorySnapshot) {
    const beforeMeta = historyExtras().bookMetaJson;
    const beforeTables = historyExtras().rewardTablesJson;
    const beforeGroups = JSON.stringify(chapterGroups);

    chapters = materializeChapters(snapshot) as QuestChapter[];
    chapterGroups = JSON.parse(snapshot.chapterGroups);
    selectedChapter = snapshot.selectedChapter;
    const meta = parseBookMeta(snapshot.bookMetaJson);
    bookTitle = meta.title;
    bookSubtitle = meta.subtitle;
    bookSettings = meta.bookSettings;
    try {
      rewardTables = JSON.parse(snapshot.rewardTablesJson) as QuestRewardTable[];
    } catch {
      rewardTables = [];
    }
    selectedQuest = null;
    selection = clearSelection();
    if (panelTab === "quest") panelTab = "info";
    if (beforeMeta !== snapshot.bookMetaJson) bookDirty = true;
    if (beforeTables !== snapshot.rewardTablesJson) rewardTablesDirty = true;
    if (beforeGroups !== snapshot.chapterGroups) groupsDirty = true;
  }

  function selectChapter(id: string) {
    selectedChapter = id;
    selectedQuest = null;
    selection = clearSelection();
    if (panelTab === "quest") panelTab = "info";
    fitToken += 1;
  }

  function selectQuestOnCanvas(q: QuestData | null, e?: MouseEvent) {
    if (!q) {
      selection = clearSelection();
      selectedQuest = null;
      if (panelTab === "quest") panelTab = "info";
      return;
    }
    if (e?.shiftKey) selection = toggleSelect(selection, q.id);
    else if (e?.ctrlKey || e?.metaKey) selection = addToSelection(selection, q.id);
    else selection = selectSingle(selection, q.id);
    selectedQuest = q;
    panelTab = "quest";
  }

  function applyChapterLayout(mode: LayoutMode) {
    if (!selectedChapter) return;
    const ch = chapters.find((c) => c.id === selectedChapter);
    if (!ch || ch.quests.length === 0) return;
    pushHistory();
    const positions = positionsForMode(ch.quests, mode);
    ch.quests = applyLayout(ch.quests, positions);
    markDirty(selectedChapter);
    fitToken += 1;
  }

  function createChapter() {
    pushHistory();
    const n: QuestChapter = {
      id: `chapter_${Date.now().toString(16)}`,
      title: `Chapter ${chapters.length + 1}`,
      titleFromSnbt: true,
      quests: [],
      extras: {},
      orderIndex: chapters.length,
      filename: `chapter_${chapters.length + 1}`,
    };
    chapters = [...chapters, n];
    selectChapter(n.id);
    markDirty(n.id);
  }

  function addQuestAt(x: number, y: number) {
    if (!selectedChapter) return;
    pushHistory();
    const ch = chapters.find((c) => c.id === selectedChapter);
    if (!ch) return;
    const newQ: QuestData = {
      id: crypto.randomUUID().replace(/-/g, "").slice(0, 16).toUpperCase(),
      title: "New Quest",
      titleFromSnbt: true,
      description: [],
      x,
      y,
      dependencies: [],
      tasks: [{ id: crypto.randomUUID().replace(/-/g, "").slice(0, 12).toUpperCase(), type: "checkmark" }],
      rewards: [],
      optional: false,
      size: 1,
      extras: {},
    };
    ch.quests = [...ch.quests, newQ];
    markDirty(selectedChapter);
    selectedQuest = newQ;
    selection = selectSingle(selection, newQ.id);
  }

  function removeQuest(q: QuestData) {
    if (!confirm(`Delete quest "${q.title}" from this chapter?`)) return;
    pushHistory();
    const ch = chapters.find((c) => c.id === selectedChapter);
    if (!ch) return;
    ch.quests = ch.quests.filter((x) => x.id !== q.id);
    for (const other of ch.quests) {
      if (other.dependencies.includes(q.id)) {
        other.dependencies = other.dependencies.filter((d) => d !== q.id);
      }
    }
    markDirty(selectedChapter);
    if (selectedQuest?.id === q.id) {
      selectedQuest = null;
      if (panelTab === "quest") panelTab = "info";
    }
    selection = clearSelection();
  }

  function removeSelectedQuests() {
    if (selection.selectedIds.size === 0) return;
    const n = selection.selectedIds.size;
    if (!confirm(`Delete ${n} selected quest(s)?`)) return;
    pushHistory();
    const ch = chapters.find((c) => c.id === selectedChapter);
    if (!ch) return;
    ch.quests = ch.quests.filter((x) => !selection.selectedIds.has(x.id));
    for (const other of ch.quests) {
      other.dependencies = other.dependencies.filter((d) => !selection.selectedIds.has(d));
    }
    markDirty(selectedChapter);
    selectedQuest = null;
    selection = clearSelection();
    if (panelTab === "quest") panelTab = "info";
  }

  function moveQuest(q: QuestData, x: number, y: number) {
    q.x = x;
    q.y = y;
    markDirty(selectedChapter);
    if (selectedQuest?.id === q.id) selectedQuest = q;
  }

  function moveSelectedQuests(dx: number, dy: number) {
    if (selection.selectedIds.size === 0) return;
    pushHistory();
    const ch = chapters.find((c) => c.id === selectedChapter);
    if (!ch) return;
    for (const q of ch.quests) {
      if (selection.selectedIds.has(q.id)) {
        q.x += dx;
        q.y += dy;
      }
    }
    markDirty(selectedChapter);
    if (selectedQuest && selection.selectedIds.has(selectedQuest.id)) {
      selectedQuest = { ...selectedQuest };
    }
  }

  /** Would adding depId as a dependency of questId create a cycle? */
  function wouldCycle(questId: string, depId: string, list: QuestData[]): boolean {
    return wouldCreateQuestCycle(questId, depId, list);
  }

  function addDep(q: QuestData, depId: string) {
    if (!depId || q.dependencies.includes(depId) || depId === q.id) return;
    const allQuests = chapters.flatMap((c) => c.quests);
    if (wouldCycle(q.id, depId, allQuests)) {
      error = "That dependency would create a cycle.";
      return;
    }
    error = null;
    pushHistory();
    q.dependencies = [...q.dependencies, depId];
    markDirty(selectedChapter);
    selectedQuest = q;
  }

  function removeDep(q: QuestData, depId: string) {
    pushHistory();
    q.dependencies = q.dependencies.filter((d) => d !== depId);
    markDirty(selectedChapter);
    selectedQuest = q;
  }

  /** Shift+drag: from depends on to. */
  function linkQuests(fromId: string, toDepId: string) {
    const ch = chapters.find((c) => c.id === selectedChapter);
    const q = ch?.quests.find((x) => x.id === fromId);
    if (!q) return;
    addDep(q, toDepId);
  }

  function createRewardTable() {
    pushHistory();
    const t: QuestRewardTable = {
      id: `table_${Date.now().toString(16)}`,
      title: `Table ${rewardTables.length + 1}`,
      rewards: [],
      emptyWeight: 0,
      extras: {},
    };
    rewardTables = [...rewardTables, t];
    rewardTablesDirty = true;
  }

  async function saveRewardTable(table: QuestRewardTable) {
    if (!$projectPath) return;
    saving = true;
    error = null;
    try {
      const res = await api.quests.saveRewardTable(table, table.sourceFile, $projectPath);
      table.sourceFile = res.relativePath;
      rewardTables = [...rewardTables];
      rewardTablesDirty = false;
      message = `Saved reward table → ${res.relativePath}`;
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  const chapterQuests = $derived(chapters.find((c) => c.id === selectedChapter)?.quests ?? []);
  const selectedChapterObj = $derived(chapters.find((c) => c.id === selectedChapter) ?? null);
  const rewardTableIds = $derived(rewardTables.map((t) => t.id));
  const totalQuests = $derived(chapters.reduce((n, c) => n + c.quests.length, 0));

  /** Strip Minecraft formatting codes for toolbar display. */
  function stripMc(s: string): string {
    return stripCodes(s).trim();
  }

  function handleQuestUpdate(chapterId: string, updated: QuestData) {
    pushHistory();
    const ch = chapters.find((c) => c.id === chapterId);
    if (!ch) return;
    ch.quests = ch.quests.map((q) => (q.id === updated.id ? updated : q));
    markDirty(chapterId);
    if (selectedQuest?.id === updated.id) selectedQuest = updated;
  }

  /** One history snapshot, then mutate all matching quests (batch mass-apply). */
  function handleBatchApply(questIds: Set<string>, mutator: (q: QuestData) => QuestData) {
    if (questIds.size === 0) return;
    pushHistory();
    const dirty = new Set(dirtyChapters);
    chapters = chapters.map((ch) => {
      let touched = false;
      const quests = ch.quests.map((q) => {
        if (!questIds.has(q.id)) return q;
        touched = true;
        return mutator(q);
      });
      if (touched) dirty.add(ch.id);
      return touched ? { ...ch, quests } : ch;
    });
    dirtyChapters = dirty;
    if (selectedQuest && questIds.has(selectedQuest.id)) {
      for (const ch of chapters) {
        const q = ch.quests.find((x) => x.id === selectedQuest!.id);
        if (q) {
          selectedQuest = q;
          break;
        }
      }
    }
    scheduleLiveValidate();
  }

  function handleUndo() {
    const beforeMap = chapterJsonMap(chapters);
    const extras = historyExtras();
    const result = historyUndo(
      history,
      chapters,
      chapterGroups,
      selectedChapter,
      extras,
    );
    if (!result.snapshot) return;
    history = result.state;
    const changed = diffDirtyChapterIds(
      { chapterJsonById: beforeMap },
      result.snapshot,
    );
    applyHistorySnapshot(result.snapshot);
    dirtyChapters = new Set([...dirtyChapters, ...changed]);
    scheduleLiveValidate();
    if (progressMode === "simulate") void refreshSimulate();
  }

  function handleRedo() {
    const beforeMap = chapterJsonMap(chapters);
    const extras = historyExtras();
    const result = historyRedo(
      history,
      chapters,
      chapterGroups,
      selectedChapter,
      extras,
    );
    if (!result.snapshot) return;
    history = result.state;
    const changed = diffDirtyChapterIds(
      { chapterJsonById: beforeMap },
      result.snapshot,
    );
    applyHistorySnapshot(result.snapshot);
    dirtyChapters = new Set([...dirtyChapters, ...changed]);
    scheduleLiveValidate();
    if (progressMode === "simulate") void refreshSimulate();
  }

  function handleCopy() {
    if (selection.selectedIds.size === 0) return;
    const ch = chapters.find((c) => c.id === selectedChapter);
    if (!ch) return;
    clipboard = ch.quests.filter((q) => selection.selectedIds.has(q.id)).map((q) => structuredClone(q));
    message = `Copied ${clipboard.length} quest(s)`;
  }

  function handlePaste() {
    if (clipboard.length === 0 || !selectedChapter) return;
    pushHistory();
    const ch = chapters.find((c) => c.id === selectedChapter);
    if (!ch) return;

    const newIds = new Map<string, string>();
    const newQuests: QuestData[] = [];
    for (const q of clipboard) {
      const newId = crypto.randomUUID().replace(/-/g, "").slice(0, 16).toUpperCase();
      newIds.set(q.id, newId);
    }
    for (const q of clipboard) {
      const newId = newIds.get(q.id)!;
      newQuests.push({
        ...structuredClone(q),
        id: newId,
        titleFromSnbt: true,
        x: q.x + 1,
        y: q.y + 1,
        dependencies: q.dependencies.map((d) => newIds.get(d) ?? d),
        tasks: (q.tasks ?? []).map((t) => ({
          ...t,
          id: crypto.randomUUID().replace(/-/g, "").slice(0, 12).toUpperCase(),
        })),
        rewards: (q.rewards ?? []).map((r) => ({
          ...r,
          id: crypto.randomUUID().replace(/-/g, "").slice(0, 12).toUpperCase(),
        })),
      });
    }
    ch.quests = [...ch.quests, ...newQuests];
    markDirty(selectedChapter);
    selection = selectAll(newQuests.map((q) => q.id));
    selectedQuest = newQuests[0] ?? null;
    message = `Pasted ${newQuests.length} quest(s)`;
  }

  function handleSelectAll() {
    selection = selectAll(chapterQuests.map((q) => q.id));
  }

  function updateSearch(query: string) {
    search = { ...search, query, results: searchQuests(query, chapters), selectedIndex: 0 };
  }

  function navigateSearchResult() {
    const result = search.results[search.selectedIndex];
    if (!result) return;
    if (result.chapterId !== selectedChapter) {
      selectChapter(result.chapterId);
    }
    selectedQuest = result.quest;
    selection = selectSingle(selection, result.quest.id);
    fitToken += 1;
  }

  function handleSearchKeyDown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      if (e.shiftKey) search = prevResult(search);
      else search = nextResult(search);
      navigateSearchResult();
    }
    if (e.key === "Escape") {
      search = { ...search, isOpen: false };
    }
  }

  function onSearchKey(e: KeyboardEvent) {
    if (e.key === "Escape") {
      questSearch = "";
      return;
    }
    if (e.key === "Enter") {
      e.preventDefault();
      const first = filteredChapterQuests[0];
      if (first) {
        selectedQuest = first;
        selection = selectSingle(selection, first.id);
        fitToken += 1;
      }
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    const target = e.target as HTMLElement;
    const isInput = !!target?.closest?.("input,textarea,select,[contenteditable=true]");

    if (e.ctrlKey || e.metaKey) {
      switch (e.key.toLowerCase()) {
        case "z":
          e.preventDefault();
          if (e.shiftKey) handleRedo();
          else handleUndo();
          return;
        case "y":
          e.preventDefault();
          handleRedo();
          return;
        case "c":
          if (!isInput) {
            e.preventDefault();
            handleCopy();
          }
          return;
        case "v":
          if (!isInput) {
            e.preventDefault();
            handlePaste();
          }
          return;
        case "a":
          if (!isInput) {
            e.preventDefault();
            handleSelectAll();
          }
          return;
        case "s":
          e.preventDefault();
          if (hasDirty && !saving) void saveAll();
          return;
        case "f":
          e.preventDefault();
          search = { ...search, isOpen: !search.isOpen };
          return;
        case "/":
          e.preventDefault();
          showShortcuts = !showShortcuts;
          return;
        case "0":
          e.preventDefault();
          fitToken += 1;
          return;
      }
    }

    if (e.key === "Escape") {
      if (search.isOpen) {
        search = { ...search, isOpen: false };
      } else if (showShortcuts) {
        showShortcuts = false;
      } else {
        selection = clearSelection();
        selectedQuest = null;
      }
      return;
    }

    if ((e.key === "Delete" || e.key === "Backspace") && !isInput) {
      e.preventDefault();
      removeSelectedQuests();
      return;
    }

    if (["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].includes(e.key)) {
      if (!isInput && selection.selectedIds.size > 0) {
        e.preventDefault();
        const step = e.shiftKey ? 5 : 1;
        switch (e.key) {
          case "ArrowUp":
            moveSelectedQuests(0, -step);
            break;
          case "ArrowDown":
            moveSelectedQuests(0, step);
            break;
          case "ArrowLeft":
            moveSelectedQuests(-step, 0);
            break;
          case "ArrowRight":
            moveSelectedQuests(step, 0);
            break;
        }
      }
    }
  }

  function applyMergeResult(result: QuestPlanMergeResult) {
    if (!result.validation?.valid) {
      error = (result.validation?.errors ?? []).slice(0, 3).join("; ") || "Plan invalid";
      return;
    }
    pushHistory();
    const b = result.book;
    chapters = b.chapters ?? [];
    chapterGroups = b.chapterGroups ?? chapterGroups;
    bookTitle = b.title ?? bookTitle;
    bookSubtitle = b.subtitle ?? bookSubtitle;
    if (b.rewardTables?.length) {
      rewardTables = b.rewardTables.map((t) => ({
        ...t,
        rewards: t.rewards ?? [],
        emptyWeight: t.emptyWeight ?? 0,
        extras: t.extras ?? {},
      }));
      rewardTablesDirty = true;
    }
    if ((b.chapterGroups?.length ?? 0) > 0) {
      groupsDirty = true;
    }
    dirtyChapters = new Set([
      ...dirtyChapters,
      ...(result.touchedChapterIds ?? []),
    ]);
    if (result.touchedChapterIds?.length) {
      selectedChapter = result.touchedChapterIds[0];
    } else if (chapters.length && !chapters.some((c) => c.id === selectedChapter)) {
      selectedChapter = chapters[0].id;
    }
    selectedQuest = null;
    if (panelTab === "quest") panelTab = "info";
    validationIssues = (result.validation.bookErrors ?? []).map((e) => ({
      questId: e.questId,
      message: e.message,
    }));
    if (validationIssues.length === 0) {
      validationIssues = validateQuestBook(
        { chapters, chapterGroups },
        { availableItems: itemCatalogCache },
      );
    }
    message = `AI plan applied in editor (${result.touchedChapterIds.length} chapter(s)). Save to write SNBT.`;
    fitToken += 1;
  }

  async function saveBookData() {
    if (!$projectPath) return;
    saving = true;
    error = null;
    try {
      await api.quests.saveBookData(
        { title: bookTitle, subtitle: bookSubtitle, bookSettings },
        $projectPath,
      );
      bookDirty = false;
      message = "Book data.snbt saved.";
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  async function saveGroups() {
    if (!$projectPath) return;
    saving = true;
    error = null;
    try {
      await saveLocaleIfNeeded();
      await api.quests.saveChapterGroups(chapterGroups, $projectPath);
      groupsDirty = false;
      message = "Chapter groups saved.";
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  function addChapterGroup() {
    pushHistory();
    const id = Math.random().toString(16).slice(2, 10).toUpperCase();
    chapterGroups = [
      ...chapterGroups,
      { id, title: `Group ${chapterGroups.length + 1}`, titleFromSnbt: true },
    ];
    groupsDirty = true;
  }

  function removeChapterGroup(id: string) {
    pushHistory();
    chapterGroups = chapterGroups.filter((g) => g.id !== id);
    for (const ch of chapters) {
      if (ch.group === id) {
        ch.group = null;
        markDirty(ch.id);
      }
    }
    groupsDirty = true;
  }

  function moveChapterGroup(id: string, dir: -1 | 1) {
    const idx = chapterGroups.findIndex((g) => g.id === id);
    if (idx < 0) return;
    const j = idx + dir;
    if (j < 0 || j >= chapterGroups.length) return;
    pushHistory();
    const next = [...chapterGroups];
    const tmp = next[idx];
    next[idx] = next[j];
    next[j] = tmp;
    chapterGroups = next;
    groupsDirty = true;
  }

  function deleteChapter(id: string) {
    if (
      !confirm(
        "Remove this chapter from the editor? The SNBT file may remain on disk — delete or empty it manually if needed.",
      )
    ) {
      return;
    }
    pushHistory();
    chapters = chapters.filter((c) => c.id !== id);
    dirtyChapters = new Set([...dirtyChapters].filter((x) => x !== id));
    if (selectedChapter === id) {
      selectedChapter = chapters[0]?.id ?? "";
      selectedQuest = null;
      selection = clearSelection();
    }
  }

  function moveChapter(id: string, dir: -1 | 1) {
    const idx = chapters.findIndex((c) => c.id === id);
    if (idx < 0) return;
    const j = idx + dir;
    if (j < 0 || j >= chapters.length) return;
    pushHistory();
    const next = [...chapters];
    const tmp = next[idx];
    next[idx] = next[j];
    next[j] = tmp;
    next.forEach((c, i) => {
      c.orderIndex = i;
      markDirty(c.id);
    });
    chapters = next;
  }

  function jumpToIssue(issue: QuestValidationIssue) {
    issuesOpen = false;
    const qid = issue.questId;
    for (const ch of chapters) {
      const q = ch.quests.find((x) => x.id === qid);
      if (q) {
        selectedChapter = ch.id;
        selectedQuest = q;
        selection = selectSingle(selection, q.id);
        fitToken += 1;
        return;
      }
    }
    // Chapter-level or unknown — try chapter id match
    if (chapters.some((c) => c.id === qid)) {
      selectedChapter = qid;
      selectedQuest = null;
    }
  }

  const filteredChapterQuests = $derived(questSearch.trim()
    ? chapterQuests.filter((q) => {
        const s = questSearch.toLowerCase();
        return (
          q.title.toLowerCase().includes(s) ||
          q.id.toLowerCase().includes(s) ||
          (q.subtitle ?? "").toLowerCase().includes(s)
        );
      })
    : chapterQuests);

  const hasDirty = $derived(
    dirtyChapters.size > 0 ||
      rewardTablesDirty ||
      bookDirty ||
      groupsDirty ||
      dirtyLocales.size > 0,
  );
  $effect(() => {
    questDirty.set(hasDirty);
  });
  $effect(() => {
    if ($projectPath && $projectPath !== lastLoadedPath) load();
  });
  $effect(() => {
    if (selectedQuest) {
        const fresh = chapterQuests.find((q) => q.id === selectedQuest!.id);
        if (fresh && fresh !== selectedQuest) selectedQuest = fresh;
      }
  });
  $effect(() => {
    if (progressKey || progressOverlay) progressOpen = true;
  });

  $effect(() => {
    if ($questChatFocusId) {
        setAiSidebar(true);
      }
  });

  onDestroy(() => {
    if (validateTimer) clearTimeout(validateTimer);
    questDirty.set(false);
  });
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="qe ftbq">
  <div class="qe-tb">
    <div class="qe-title">
      <ScrollText size={18} />
      {#if bookTitle}<span class="book-name">{stripMc(bookTitle)}</span>{:else}Quest editor{/if}
    </div>
    <div class="qe-actions">
      <div class="locale-controls">
        <select
          class="locale-select"
          value={activeLocale ?? ""}
          title="Language overlay (lang/*.snbt)"
          disabled={availableLocales.length === 0}
          onchange={(e) => {
            const v = (e.currentTarget as HTMLSelectElement).value;
            if (v) switchLocale(v);
          }}
        >
          {#if availableLocales.length === 0}
            <option value="">No lang files</option>
          {:else}
            {#each availableLocales as code (code)}
              <option value={code}>{code}{#if dirtyLocales.has(code)} ●{/if}</option>
            {/each}
          {/if}
        </select>
        <button
          type="button"
          class="ghost ico"
          title="Locales — create, gaps, compare"
          onclick={() => {
            showLocalePanel = true;
            showBookPanel = false;
            showGroupsPanel = false;
            showTablesPanel = false;
            bookMenuOpen = true;
          }}
        >+ New</button>
      </div>
      <div class="tb-pop">
        <button
          type="button"
          class="ghost"
          class:active={bookMenuOpen || showBookPanel || showGroupsPanel || showTablesPanel || showLocalePanel}
          class:has-dirty={bookDirty || groupsDirty || rewardTablesDirty || dirtyLocales.size > 0}
          title="Book, groups, reward tables, locales"
          onclick={() => {
            bookMenuOpen = !bookMenuOpen;
            if (!bookMenuOpen) {
              showBookPanel = false;
              showGroupsPanel = false;
              showTablesPanel = false;
              showLocalePanel = false;
            }
          }}
        >
          Book{#if bookDirty || groupsDirty || rewardTablesDirty || dirtyLocales.size > 0}<span class="dot-mini">●</span>{/if}
        </button>
        {#if bookMenuOpen && $projectPath}
          <div class="book-menu" role="menu">
            <button
              type="button"
              role="menuitem"
              class:active={showBookPanel}
              onclick={() => {
                showBookPanel = true;
                showGroupsPanel = false;
                showTablesPanel = false;
                showLocalePanel = false;
              }}
            >
              Book settings{#if bookDirty}<span class="dot-mini">●</span>{/if}
            </button>
            <button
              type="button"
              role="menuitem"
              class:active={showGroupsPanel}
              onclick={() => {
                showGroupsPanel = true;
                showBookPanel = false;
                showTablesPanel = false;
                showLocalePanel = false;
              }}
            >
              Chapter groups{#if groupsDirty}<span class="dot-mini">●</span>{/if}
            </button>
            <button
              type="button"
              role="menuitem"
              class:active={showTablesPanel}
              onclick={() => {
                showTablesPanel = true;
                showBookPanel = false;
                showGroupsPanel = false;
                showLocalePanel = false;
              }}
            >
              Reward tables{#if rewardTablesDirty}<span class="dot-mini">●</span>{/if}
            </button>
            <button
              type="button"
              role="menuitem"
              class:active={showLocalePanel}
              onclick={() => {
                showLocalePanel = true;
                showBookPanel = false;
                showGroupsPanel = false;
                showTablesPanel = false;
              }}
            >
              Locales{#if dirtyLocales.size > 0}<span class="dot-mini">●</span>{/if}
            </button>
          </div>
        {/if}
        {#if showBookPanel && $projectPath}
          <div class="drawer drawer-wide">
            <div class="drawer-h">
              <strong>Book (data.snbt)</strong>
              <button type="button" class="ghost ico" onclick={() => { showBookPanel = false; bookMenuOpen = false; }}
                ><X size={14} /></button
              >
            </div>
            <label
              >Title<input
                value={bookTitle ?? ""}
                oninput={(e) => {
                  if (!bookDirty) pushHistory();
                  bookTitle = inputVal(e);
                  bookDirty = true;
                }}
              /></label
            >
            <label
              >Subtitle<input
                value={bookSubtitle ?? ""}
                oninput={(e) => {
                  if (!bookDirty) pushHistory();
                  bookSubtitle = inputVal(e);
                  bookDirty = true;
                }}
              /></label
            >
            <div class="book-flags">
              {#each BOOK_BOOL_KEYS as key (key)}
                <label class="book-check">
                  <input
                    type="checkbox"
                    checked={bookBool(key)}
                    onchange={(e) => setBookBool(key, (e.currentTarget as HTMLInputElement).checked)}
                  />
                  {key}
                </label>
              {/each}
            </div>
            {#each BOOK_STRING_KEYS as key (key)}
              <label
                >{key}<input
                  value={bookString(key)}
                  oninput={(e) => setBookString(key, inputVal(e))}
                /></label
              >
            {/each}
            <label
              >default_quest_size<input
                type="number"
                step="0.25"
                min="0"
                value={bookNumber("default_quest_size")}
                oninput={(e) => setBookNumber("default_quest_size", inputVal(e))}
                placeholder="optional"
              /></label
            >
            {#if bookExtraEntries.length > 0}
              <p class="drawer-hint">Other data.snbt keys</p>
              {#each bookExtraEntries as [k, v] (k)}
                <div class="group-row book-extra">
                  <code>{k}</code>
                  <span class="extra-val">{typeof v === "string" ? v : JSON.stringify(v)}</span>
                  <button type="button" class="ghost" onclick={() => removeBookSetting(k)}>Remove</button>
                </div>
              {/each}
            {/if}
            <p class="drawer-hint">Included in Save all · or save here</p>
            <button type="button" onclick={saveBookData} disabled={saving || !bookDirty}
              >Save book</button
            >
          </div>
        {/if}
        {#if showGroupsPanel && $projectPath}
          <div class="drawer drawer-wide">
            <div class="drawer-h">
              <strong>Chapter groups</strong>
              <button type="button" class="ghost" onclick={addChapterGroup}>+ Group</button>
              <button type="button" class="ghost ico" onclick={() => { showGroupsPanel = false; bookMenuOpen = false; }}
                ><X size={14} /></button
              >
            </div>
            {#each chapterGroups as g, gi (g.id)}
              <div class="group-row">
                <code>{g.id}</code>
                <input
                  bind:value={g.title}
                  oninput={() => {
                    g.titleFromSnbt = true;
                    groupsDirty = true;
                  }}
                />
                <button
                  type="button"
                  class="ghost"
                  disabled={gi === 0}
                  title="Move up"
                  onclick={() => moveChapterGroup(g.id, -1)}>↑</button
                >
                <button
                  type="button"
                  class="ghost"
                  disabled={gi === chapterGroups.length - 1}
                  title="Move down"
                  onclick={() => moveChapterGroup(g.id, 1)}>↓</button
                >
                <button type="button" class="ghost" onclick={() => removeChapterGroup(g.id)}
                  >Remove</button
                >
              </div>
            {/each}
            <p class="drawer-hint">Included in Save all</p>
            <button type="button" onclick={saveGroups} disabled={saving || !groupsDirty}
              >Save groups</button
            >
          </div>
        {/if}
        {#if showTablesPanel && $projectPath}
          <div class="drawer drawer-tables">
            <div class="drawer-h">
              <strong>Reward tables</strong>
              <button type="button" class="ghost ico" onclick={() => { showTablesPanel = false; bookMenuOpen = false; }}
                ><X size={14} /></button
              >
            </div>
            <RewardTablesPanel
              tables={rewardTables}
              dirty={rewardTablesDirty}
              {saving}
              tableIds={rewardTableIds}
              onChange={() => {
                if (!rewardTablesDirty) pushHistory();
                rewardTablesDirty = true;
                rewardTables = [...rewardTables];
              }}
              onSave={saveRewardTable}
              onCreate={createRewardTable}
            />
          </div>
        {/if}
        {#if showLocalePanel && $projectPath}
          <div class="drawer drawer-wide">
            <div class="drawer-h">
              <strong>Locales (lang/*.snbt)</strong>
              <button
                type="button"
                class="ghost ico"
                onclick={() => {
                  showLocalePanel = false;
                  bookMenuOpen = false;
                }}><X size={14} /></button
              >
            </div>
            <LocalePanel
              {locales}
              {activeLocale}
              {compareLocale}
              {chapterGroups}
              {chapters}
              onCreateLocale={createLocale}
              onJumpGap={jumpToLocaleGap}
              onCompareLocaleChange={(code) => {
                compareLocale = code && code !== activeLocale ? code : null;
              }}
            />
          </div>
        {/if}
      </div>
      <button
        type="button"
        class="ghost"
        class:active={aiSidebarOpen}
        title="Quest AI sidebar"
        onclick={() => setAiSidebar(!aiSidebarOpen)}
      >
        <Sparkles size={16} /> AI
      </button>
      <button
        type="button"
        class="ghost"
        disabled={!canUndo(history)}
        title="Undo (Ctrl+Z)"
        onclick={handleUndo}
      >
        <Undo2 size={16} />
      </button>
      <button
        type="button"
        class="ghost"
        disabled={!canRedo(history)}
        title="Redo (Ctrl+Y)"
        onclick={handleRedo}
      >
        <Redo2 size={16} />
      </button>
      <button
        type="button"
        class="ghost"
        title="Shortcuts (Ctrl+/)"
        onclick={() => (showShortcuts = !showShortcuts)}
      >
        <Keyboard size={16} />
      </button>
      {#if hasDirty}
        <span class="dirty-badge"
          >{dirtyChapters.size +
            (rewardTablesDirty ? 1 : 0) +
            (bookDirty ? 1 : 0) +
            (groupsDirty ? 1 : 0) +
            dirtyLocales.size} unsaved</span
        >
        <button type="button" class="primary" onclick={saveAll} disabled={!$projectPath || saving} title="Ctrl+S">
          <Save size={16} /> {saving ? "Saving…" : "Save all"}
        </button>
      {/if}
      <button
        type="button"
        class="ghost"
        onclick={requestReload}
        disabled={!$projectPath || loading}
        title="Reload from disk"
      >
        <RefreshCw size={16} class={loading ? "spin" : ""} />
      </button>
    </div>
  </div>

  {#if $projectPath}
    <div class="qe-stats">
      <span>{chapters.length} chapters</span>
      <span>{totalQuests} quests</span>
      <div class="issues-wrap">
        <button
          type="button"
          class="issues-btn"
          class:warn={validationIssues.length > 0}
          disabled={validationIssues.length === 0}
          onclick={() => (issuesOpen = !issuesOpen)}
        >
          {validationIssues.length === 0 ? "✓ valid" : `${validationIssues.length} issues`}
        </button>
        <button
          type="button"
          class="issues-btn"
          title="Re-run Rust validate_quest_book on disk"
          onclick={() => void revalidateFromDisk()}
        >
          Revalidate
        </button>
        {#if issuesOpen && validationIssues.length > 0}
          <div class="issues-pop">
            {#each validationIssues.slice(0, 40) as iss, i (`${iss.questId}-${i}`)}
              <button type="button" class="issue-row" onclick={() => jumpToIssue(iss)}>
                <code>{iss.questId.slice(0, 8)}</code>
                <span>{iss.message}</span>
              </button>
            {/each}
          </div>
        {/if}
      </div>
      {#if progressSnap && progressOverlay}
        <span class="prog-stat"
          >{progressSnap.completedCount} done · {progressSnap.startedCount} started · {progressSnap.name}</span
        >
      {/if}
    </div>
    <details class="prog-details" bind:open={progressOpen}>
      <summary><Eye size={14} /> Progress</summary>
      <div class="prog-bar">
        <div class="prog-modes">
          <button
            type="button"
            class="ghost"
            class:sel={progressMode === "save"}
            onclick={() => void enterSaveMode()}
            >Save overlay</button
          >
          <button
            type="button"
            class="ghost"
            class:sel={progressMode === "simulate"}
            onclick={() => void enterSimulateMode()}
            >Simulate</button
          >
        </div>
        <label class="prog-toggle">
          <input
            type="checkbox"
            bind:checked={progressOverlay}
            disabled={!progressSnap}
            title="Show progress on canvas"
          />
          Overlay
        </label>
        {#if progressMode === "save"}
          <select
            bind:value={progressKey}
            onchange={loadProgress}
            disabled={progressLoading || progressTeams.length === 0}
          >
            <option value="">
              {progressTeams.length === 0
                ? "No saves/*/ftbquests progress"
                : "Select team / player…"}
            </option>
            {#each progressTeams as t (t.relativePath)}
              <option value={t.relativePath}>{t.world} — {t.name}</option>
            {/each}
          </select>
          <button
            type="button"
            class="ghost"
            disabled={progressLoading || !progressKey}
            onclick={loadProgress}
            title="Reload progress"
          >
            <RefreshCw size={14} class={progressLoading ? "spin" : ""} />
          </button>
        {:else}
          <span class="prog-sim-hint"
            >Click quests on canvas to toggle complete ({simCompleted.length})</span
          >
          <button
            type="button"
            class="ghost"
            disabled={progressLoading || !progressKey}
            onclick={() => void seedSimulateFromTeam()}
            title="Copy completed quests from selected save team"
            >Seed from team</button
          >
          <button
            type="button"
            class="ghost"
            disabled={simBusy}
            onclick={() => void resetSimulate()}
            >Reset</button
          >
          <button
            type="button"
            class="ghost"
            disabled={simBusy}
            onclick={() => void refreshSimulate()}
            title="Reclassify"
          >
            <RefreshCw size={14} class={simBusy ? "spin" : ""} />
          </button>
        {/if}
        {#if progressMode === "save" && progressTeamLabel}
          <code class="prog-path">{progressTeamLabel.relativePath}</code>
        {/if}
      </div>
    </details>
  {/if}

  {#if error}<div class="notice error"><AlertTriangle size={14} /> {error}</div>{/if}
  {#if message}<div class="notice success"><CheckCircle2 size={14} /> {message}</div>{/if}

  {#if search.isOpen}
    <div class="search-panel">
      <div class="search-bar">
        <input
          type="search"
          placeholder="Search all quests… (Enter next, Shift+Enter prev, Esc close)"
          value={search.query}
          oninput={(e) => updateSearch(inputVal(e))}
          onkeydown={handleSearchKeyDown}
        />
        <span class="filt-count"
          >{search.results.length
            ? `${search.selectedIndex + 1}/${search.results.length}`
            : "0"}</span
        >
        <button type="button" class="ghost ico" title="Close" onclick={() => (search = { ...search, isOpen: false })}
          ><X size={14} /></button
        >
      </div>
      {#if search.results.length > 0}
        <ul class="search-results">
          {#each search.results.slice(0, 50) as r, i (`${r.chapterId}-${r.quest.id}-${r.matchField}-${i}`)}
            <li>
              <button
                type="button"
                class="search-hit"
                class:active={i === search.selectedIndex}
                onclick={() => {
                  search = { ...search, selectedIndex: i };
                  navigateSearchResult();
                }}
              >
                <span class="hit-ch">{r.chapterTitle}</span>
                <span class="hit-field">{r.matchField}</span>
                <span class="hit-text">{r.matchText}</span>
              </button>
            </li>
          {/each}
          {#if search.results.length > 50}
            <li class="search-more">+{search.results.length - 50} more — use Enter to cycle</li>
          {/if}
        </ul>
      {/if}
    </div>
  {/if}

  {#if !$projectPath}
    <EmptyState icon={MapIcon} title="No project selected" description="Open a project to edit FTB Quests chapters." />
  {:else if loading && chapters.length === 0}
    <div class="empty"><RefreshCw size={32} class="spin" /><p>Loading quest book…</p></div>
  {:else if chapters.length === 0}
    <div class="empty">
      <ScrollText size={40} />
      <h3>Start a quest line</h3>
      <p>Create a chapter, add quests on the canvas, then Save all to write SNBT.</p>
      <button type="button" class="empty-cta" onclick={createChapter}><span>+</span> Create first chapter</button>
    </div>
  {:else}
    <div class="qe-body-row">
    <div class="qe-lay" class:with-insp={!!selectedChapterObj}>
      <ChapterRail
        {chapters}
        {chapterGroups}
        {selectedChapter}
        dirtyIds={dirtyChapters}
        {saving}
        onSelect={selectChapter}
        onCreate={createChapter}
        onSave={saveChapter}
        onDirty={markDirty}
        onDelete={deleteChapter}
        onMove={moveChapter}
      />
      <div class="canvas-wrap">
        <div class="canvas-tools">
          <input
            type="search"
            placeholder="Filter quests… (Enter = jump, Esc = clear)"
            bind:value={questSearch}
            onkeydown={onSearchKey}
          />
          {#if questSearch}
            <span class="filt-count">{filteredChapterQuests.length}/{chapterQuests.length}</span>
          {/if}
          <div class="layout-btns" title="Auto-layout current chapter">
            <button type="button" class="layout-btn" onclick={() => applyChapterLayout("tree")}>Tree</button>
            <button type="button" class="layout-btn" onclick={() => applyChapterLayout("grid")}>Grid</button>
            <button type="button" class="layout-btn" onclick={() => applyChapterLayout("circle")}>Circle</button>
          </div>
        </div>
        <SvelteFlowProvider>
          <QuestCanvas
            quests={filteredChapterQuests}
            {chapters}
            selectedId={selectedQuest?.id ?? null}
            selectedIds={selection.selectedIds}
            issues={validationIssues}
            {fitToken}
            {progressOverlay}
            {progressStatuses}
            emptyHint={questSearch.trim()
              ? `No quests match “${questSearch.trim()}”`
              : "Double-click to add a quest"}
            onSelect={(q, e) => {
              if (
                progressMode === "simulate" &&
                progressOverlay &&
                q &&
                !e?.shiftKey &&
                !e?.ctrlKey &&
                !e?.metaKey
              ) {
                void toggleSimQuest(q.id);
              }
              selectQuestOnCanvas(q, e);
            }}
            onMove={moveQuest}
            onAddAt={addQuestAt}
            onLink={linkQuests}
            onSelectMultiple={(ids) => {
              selection = selectAll(ids);
              if (ids.length === 1) {
                selectedQuest = chapterQuests.find((q) => q.id === ids[0]) ?? null;
                if (selectedQuest) panelTab = "quest";
              }
            }}
          />
        </SvelteFlowProvider>
      </div>
      {#if selectedChapterObj}
        <div class="side-panel">
          <div class="panel-tabs">
            <button type="button" class="tab" class:active={panelTab === "quest"} onclick={() => (panelTab = "quest")}
              >Quest</button
            >
            <button type="button" class="tab" class:active={panelTab === "info"} onclick={() => (panelTab = "info")}
              >Info</button
            >
            <button type="button" class="tab" class:active={panelTab === "batch"} onclick={() => (panelTab = "batch")}
              >Batch</button
            >
            <button type="button" class="tab" class:active={panelTab === "colors"} onclick={() => (panelTab = "colors")}
              >Colors</button
            >
            <button type="button" class="tab" class:active={panelTab === "raw"} onclick={() => (panelTab = "raw")}
              >Raw</button
            >
          </div>
          <div class="panel-content">
            {#if panelTab === "quest"}
              {#if selectedQuest}
                <QuestInspector
                  quest={selectedQuest}
                  {chapterQuests}
                  {chapters}
                  issues={validationIssues}
                  {rewardTableIds}
                  {activeLocale}
                  {compareLocale}
                  compareMap={compareLocale ? locales[compareLocale] ?? null : null}
                  availableLocales={availableLocales}
                  onDirty={() => {
                    if (!dirtyChapters.has(selectedChapter)) pushHistory();
                    markDirty(selectedChapter);
                  }}
                  onCompareDirty={(code) => markLocaleDirty(code)}
                  onCompareLocaleChange={(code) => {
                    compareLocale = code && code !== activeLocale ? code : null;
                  }}
                  onCompareMapChange={(code, map) => {
                    locales = { ...locales, [code]: map };
                    markLocaleDirty(code);
                  }}
                  onRemove={() => {
                    if (selectedQuest) removeQuest(selectedQuest);
                  }}
                  onAddDep={(id) => {
                    if (selectedQuest) addDep(selectedQuest, id);
                  }}
                  onRemoveDep={(id) => {
                    if (selectedQuest) removeDep(selectedQuest, id);
                  }}
                />
              {:else}
                <p class="sel-hint">Select a quest on the canvas</p>
              {/if}
            {:else if panelTab === "info"}
              <ChapterSettings
                chapter={selectedChapterObj}
                {chapterGroups}
                onDirty={() => {
                  if (!dirtyChapters.has(selectedChapter)) pushHistory();
                  markDirty(selectedChapter);
                }}
              />
              {#if selection.selectedIds.size > 0}
                <p class="sel-hint">{selection.selectedIds.size} quest(s) selected · Del to remove · Ctrl+C copy</p>
              {/if}
            {:else if panelTab === "batch"}
              <BatchEditor
                {chapters}
                selectedIds={selection.selectedIds}
                onQuestUpdate={handleQuestUpdate}
                onBatchApply={handleBatchApply}
                onSaveChapter={(id) => void saveChapter(id)}
              />
            {:else if panelTab === "colors"}
              <ColorManager {chapters} onQuestUpdate={handleQuestUpdate} />
            {:else if panelTab === "raw"}
              <RawSnbtView
                chapter={selectedChapterObj}
                onDiffVsDisk={() => void showChapterDiffVsDisk()}
              />
            {/if}
          </div>
        </div>
      {/if}
    </div>
    {#if aiSidebarOpen}
      <QuestAiSidebar
        open={aiSidebarOpen}
        onclose={() => setAiSidebar(false)}
        onapply={applyMergeResult}
        anchorQuest={selectedQuest}
        anchorChapterTitle={selectedChapterObj?.title ?? null}
        targetChapterId={selectedChapter}
      />
    {/if}
    </div>
    <div class="qe-footer">
      <p class="hint">
        Ctrl+S save · Ctrl+Z undo · Ctrl+F search · Ctrl+/ shortcuts · Shift/Ctrl multi-select. SNBT →
        <code>config/ftbquests/quests/chapters/</code>.
      </p>
    </div>
  {/if}
</div>

<ShortcutsModal isOpen={showShortcuts} onClose={() => (showShortcuts = false)} />
<SnbtDiffModal
  open={snbtDiffOpen}
  title={snbtDiffTitle}
  leftLabel={snbtDiffLeftLabel}
  rightLabel={snbtDiffRightLabel}
  leftText={snbtDiffLeft}
  rightText={snbtDiffRight}
  confirmLabel={snbtDiffConfirmLabel}
  onConfirm={() => closeSnbtDiff(true)}
  onCancel={() => closeSnbtDiff(false)}
/>

<style>
  .qe.ftbq {
    --ftbq-bg: #1a1a1e;
    --ftbq-bg-panel: #212126;
    --ftbq-bg-canvas: #2b2b30;
    --ftbq-border: #3a3a42;
    --ftbq-frame: #101014;
    --ftbq-text: #e8e8e8;
    --ftbq-text-muted: #9a9aa0;
    --ftbq-quest-default: #ffffff;
    --ftbq-quest-locked: #6b6b6b;
    --ftbq-quest-started: #f2c94c;
    --ftbq-quest-completed: #55c95a;
    --ftbq-line: #5c8a9e;
    --ftbq-line-done: #55c95a;
    --ftbq-accent-teal: #3db8a8;
    --ftbq-accent-green: #55c95a;
    --ftbq-title-gold: #f2c94c;
    --ftbq-node-fill: #18181c;
  }
  .qe {
    max-width: none;
    width: 100%;
    height: 100%;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background:
      radial-gradient(ellipse at 50% 0%, rgba(255, 255, 255, 0.03), transparent 55%),
      var(--ftbq-bg, #1a1a1e);
    color: var(--ftbq-text, #e8e8e8);
  }
  /* Isolate from global TuffBox green primary buttons */
  .qe.ftbq :global(button) {
    border-radius: 3px;
    font-weight: 600;
    box-shadow: none;
  }
  .qe.ftbq :global(button.ghost),
  .qe.ftbq :global(button.ico) {
    padding: 4px 10px;
    border: 1px solid var(--ftbq-frame, #101014);
    background: linear-gradient(180deg, #3a3a42, #2a2a31);
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.12),
      inset 0 -1px 0 rgba(0, 0, 0, 0.45);
    color: var(--ftbq-text, #e8e8e8);
    text-shadow: 1px 1px 0 rgba(0, 0, 0, 0.6);
  }
  .qe.ftbq :global(button.ghost:hover:not(:disabled)),
  .qe.ftbq :global(button.ico:hover:not(:disabled)) {
    border-color: var(--ftbq-frame, #101014);
    background: linear-gradient(180deg, #47503f, #32382d);
    color: #d6f5d0;
  }
  .qe.ftbq :global(button.ghost:active:not(:disabled)),
  .qe.ftbq :global(button.ico:active:not(:disabled)) {
    box-shadow: inset 0 1px 3px rgba(0, 0, 0, 0.5);
  }
  .qe.ftbq :global(button.primary),
  .qe.ftbq :global(.qe-actions > button:not(.ghost)) {
    padding: 6px 12px;
    border: 1px solid #12380f;
    background: linear-gradient(180deg, #4fae53, #35833a);
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.25),
      inset 0 -1px 0 rgba(0, 0, 0, 0.35);
    color: #eaffe9;
    text-shadow: 1px 1px 0 rgba(0, 0, 0, 0.5);
  }
  .qe.ftbq :global(button.primary:hover:not(:disabled)),
  .qe.ftbq :global(.qe-actions > button:not(.ghost):hover:not(:disabled)) {
    filter: brightness(1.12);
  }
  .qe.ftbq :global(button:disabled) {
    opacity: 0.5;
  }
  .qe.ftbq :global(input),
  .qe.ftbq :global(select),
  .qe.ftbq :global(textarea) {
    border-radius: 3px;
    border: 1px solid #0c0c0f;
    background: #141419;
    box-shadow:
      inset 1px 1px 3px rgba(0, 0, 0, 0.55),
      inset -1px -1px 0 rgba(255, 255, 255, 0.05);
    color: var(--ftbq-text, #e8e8e8);
    min-width: 0;
  }
  .qe.ftbq :global(input:focus),
  .qe.ftbq :global(select:focus),
  .qe.ftbq :global(textarea:focus) {
    outline: none;
    border-color: var(--ftbq-title-gold, #f2c94c);
    box-shadow:
      inset 1px 1px 3px rgba(0, 0, 0, 0.55),
      0 0 6px rgba(242, 201, 76, 0.35);
  }
  .qe-tb,
  .qe-title,
  .qe-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    position: relative;
  }
  .tb-pop {
    position: relative;
  }
  .locale-controls {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .locale-select {
    max-width: 110px;
    font-size: 11px;
    padding: 4px 6px;
  }
  .book-flags {
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 160px;
    overflow: auto;
    padding: 4px 0;
  }
  .book-check {
    display: flex !important;
    flex-direction: row !important;
    align-items: center;
    gap: 8px;
    font-size: 11px;
    text-transform: none;
    color: var(--ftbq-text, #e8e8e8);
    letter-spacing: 0;
  }
  .book-extra .extra-val {
    flex: 1;
    font-size: 10px;
    color: var(--ftbq-text-muted, #9a9aa0);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .book-menu {
    position: absolute;
    top: calc(100% + 4px);
    right: 0;
    z-index: 45;
    min-width: 180px;
    padding: 6px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    background: var(--ftbq-bg-panel, #212126);
    border: 1px solid var(--ftbq-frame, #101014);
    border-radius: 3px;
    box-shadow:
      inset 0 0 0 1px rgba(255, 255, 255, 0.06),
      0 12px 28px rgba(0, 0, 0, 0.55);
  }
  .book-menu button {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    width: 100%;
    text-align: left;
    padding: 8px 10px;
    border: none;
    border-radius: 3px;
    background: transparent;
    color: var(--ftbq-text, #e8e8ea);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }
  .book-menu button:hover,
  .book-menu button.active {
    background: rgba(61, 184, 168, 0.12);
    color: var(--ftbq-accent-teal, #3db8a8);
  }
  .dot-mini {
    color: var(--ftbq-quest-started, #f2c94c);
    margin-left: 4px;
    font-size: 10px;
  }
  .drawer {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    z-index: 40;
    width: 280px;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    background: var(--ftbq-bg-panel, #212126);
    border: 1px solid var(--ftbq-frame, #101014);
    border-radius: 3px;
    box-shadow:
      inset 0 0 0 1px rgba(255, 255, 255, 0.06),
      0 12px 28px rgba(0, 0, 0, 0.55);
  }
  .drawer-wide {
    width: 360px;
    max-height: min(70vh, 560px);
    overflow: auto;
  }
  .drawer-tables {
    width: min(520px, 90vw);
    max-height: min(70vh, 560px);
    overflow: auto;
  }
  .drawer-h {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .drawer-h strong {
    flex: 1;
  }
  .drawer label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 12px;
    color: var(--ftbq-text-muted, #9a9aa0);
  }
  .drawer input {
    background: #141419;
    border: 1px solid #0c0c0f;
    color: inherit;
    border-radius: 3px;
    padding: 6px 8px;
    box-shadow: inset 1px 1px 3px rgba(0, 0, 0, 0.55);
  }
  .drawer-hint {
    margin: 0;
    font-size: 11px;
    color: var(--ftbq-text-muted, #9a9aa0);
  }
  .group-row {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .group-row input {
    flex: 1;
  }
  .issues-wrap {
    position: relative;
  }
  .issues-btn {
    border: 1px solid var(--ftbq-frame, #101014);
    background: linear-gradient(180deg, #3a3a42, #2a2a31);
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.1),
      inset 0 -1px 0 rgba(0, 0, 0, 0.4);
    color: var(--ftbq-quest-completed, #55c95a);
    border-radius: 3px;
    padding: 2px 8px;
    font-size: 12px;
    cursor: pointer;
    text-shadow: 1px 1px 0 rgba(0, 0, 0, 0.6);
  }
  .issues-btn.warn {
    color: #fbbf24;
  }
  .issues-btn:disabled {
    cursor: default;
  }
  .issues-pop {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    z-index: 30;
    min-width: 320px;
    max-height: 240px;
    overflow: auto;
    background: var(--ftbq-bg-panel, #212126);
    border: 1px solid var(--ftbq-frame, #101014);
    border-radius: 3px;
    box-shadow:
      inset 0 0 0 1px rgba(255, 255, 255, 0.06),
      0 10px 24px rgba(0, 0, 0, 0.5);
  }
  .issue-row {
    display: flex;
    gap: 8px;
    width: 100%;
    text-align: left;
    padding: 8px 10px;
    border: none;
    border-bottom: 1px solid var(--ftbq-border, #3a3a42);
    background: transparent;
    color: var(--ftbq-text, #e8e8e8);
    font-size: 11px;
    cursor: pointer;
  }
  .issue-row:hover {
    background: rgba(61, 184, 168, 0.1);
  }
  .prog-details {
    flex-shrink: 0;
    margin: 0 12px 8px;
    border: 1px solid var(--ftbq-frame, #101014);
    border-radius: 3px;
    background: var(--ftbq-bg-panel, #212126);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.05);
    padding: 0 8px;
  }
  .prog-details summary {
    display: flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
    padding: 6px 4px;
    font-size: 12px;
    color: var(--ftbq-text-muted, #9a9aa0);
    list-style: none;
  }
  .prog-details summary::-webkit-details-marker {
    display: none;
  }
  .qe-tb {
    justify-content: space-between;
    margin-bottom: 4px;
    flex-shrink: 0;
    padding: 6px 10px;
    min-height: 40px;
    max-height: 48px;
    background: linear-gradient(180deg, rgba(255, 255, 255, 0.04), rgba(0, 0, 0, 0.22)),
      var(--ftbq-bg-panel, #212126);
    border: 1px solid var(--ftbq-frame, #101014);
    border-radius: 3px;
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.07),
      inset 0 -1px 0 rgba(0, 0, 0, 0.4),
      0 2px 6px rgba(0, 0, 0, 0.35);
  }
  .qe-title {
    color: var(--ftbq-text-muted, #9a9aa0);
    font-weight: 700;
    font-size: 13px;
    letter-spacing: 0.02em;
  }
  .qe-title .book-name {
    color: var(--ftbq-title-gold, #f2c94c);
    text-shadow: 2px 2px 0 rgba(0, 0, 0, 0.65);
  }
  .qe-stats {
    display: flex;
    gap: 16px;
    margin-bottom: 6px;
    font-size: 11px;
    color: var(--ftbq-text-muted, #9a9aa0);
    flex-shrink: 0;
    flex-wrap: wrap;
    padding: 2px 6px;
    text-shadow: 1px 1px 0 rgba(0, 0, 0, 0.6);
  }
  .qe-stats .warn {
    color: var(--ftbq-quest-started, #f2c94c);
  }
  .prog-stat {
    color: var(--ftbq-quest-completed, #55c95a);
  }
  .prog-bar {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    margin-bottom: 8px;
    padding: 6px 10px;
    border-radius: 3px;
    border: 1px solid var(--ftbq-frame, #101014);
    background: var(--ftbq-bg-panel, #212126);
    box-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.05);
    color: var(--ftbq-text-muted, #9a9aa0);
    font-size: 11px;
    flex-shrink: 0;
  }
  .prog-modes {
    display: inline-flex;
    gap: 4px;
  }
  .prog-modes .ghost.sel {
    color: var(--ftbq-accent-teal, #3db8a8);
    border-color: rgba(61, 184, 168, 0.45);
    background: rgba(61, 184, 168, 0.1);
  }
  .prog-sim-hint {
    color: var(--ftbq-text-muted, #9a9aa0);
  }
  .prog-toggle {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--ftbq-text-muted, #9a9aa0);
    cursor: pointer;
  }
  .prog-bar select {
    min-width: 200px;
    max-width: 360px;
    font-size: 12px;
  }
  .prog-path {
    font-size: 10px;
    color: var(--ftbq-text-muted, #9a9aa0);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 280px;
  }
  .dirty-badge {
    font-size: 10px;
    color: #ffd971;
    padding: 3px 8px;
    border-radius: 3px;
    background: rgba(242, 201, 76, 0.14);
    border: 1px solid rgba(242, 201, 76, 0.3);
    text-shadow: 1px 1px 0 rgba(0, 0, 0, 0.6);
    animation: badge-glow 2s ease-in-out infinite;
  }
  @keyframes badge-glow {
    0%,
    100% {
      box-shadow: 0 0 3px rgba(242, 201, 76, 0.15);
    }
    50% {
      box-shadow: 0 0 9px rgba(242, 201, 76, 0.4);
    }
  }
  .notice {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    border-radius: 3px;
    margin-bottom: 8px;
    border: 1px solid var(--ftbq-border, #3a3a42);
    flex-shrink: 0;
    font-size: 12px;
    text-shadow: 1px 1px 0 rgba(0, 0, 0, 0.5);
  }
  .notice.error {
    color: #fecaca;
    background: rgba(239, 68, 68, 0.1);
    border-color: rgba(239, 68, 68, 0.35);
  }
  .notice.success {
    color: var(--ftbq-quest-completed, #55c95a);
    background: rgba(85, 201, 90, 0.1);
    border-color: rgba(85, 201, 90, 0.3);
  }
  .empty {
    color: var(--ftbq-text-muted, #9a9aa0);
    padding: 48px 32px;
    text-align: center;
    background: var(--ftbq-bg-panel, #212126);
    border: 1px solid var(--ftbq-frame, #101014);
    border-radius: 3px;
    box-shadow:
      inset 0 0 0 1px rgba(255, 255, 255, 0.05),
      inset 0 0 48px rgba(0, 0, 0, 0.25);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
  }
  .empty h3 {
    margin: 0;
    color: var(--ftbq-title-gold, #f2c94c);
    font-size: 16px;
    text-shadow: 2px 2px 0 rgba(0, 0, 0, 0.65);
  }
  .empty p {
    margin: 0;
    max-width: 360px;
    font-size: 13px;
    line-height: 1.45;
  }
  .empty-cta {
    margin-top: 8px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 10px 16px;
    border: 1px solid #0f3a34;
    border-radius: 3px;
    background: linear-gradient(180deg, #3aa79a, #2a7d73);
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.22),
      inset 0 -1px 0 rgba(0, 0, 0, 0.35);
    color: #e7fffb;
    text-shadow: 1px 1px 0 rgba(0, 0, 0, 0.5);
    font-weight: 700;
    cursor: pointer;
  }
  .empty-cta:hover {
    filter: brightness(1.12);
  }
  .hint {
    font-size: 11px;
  }
  .qe-lay {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: 180px 1fr;
    gap: 0;
    border: 1px solid var(--ftbq-frame, #101014);
    border-radius: 3px;
    overflow: hidden;
    background: var(--ftbq-bg-canvas, #2b2b30);
    box-shadow:
      inset 0 0 0 1px rgba(255, 255, 255, 0.05),
      0 2px 8px rgba(0, 0, 0, 0.4);
    min-width: 0;
  }
  .qe-body-row {
    display: flex;
    flex: 1;
    min-height: 0;
    gap: 0;
    align-items: stretch;
    overflow: hidden;
  }
  .qe-body-row .qe-lay {
    flex: 1;
    min-height: 0;
  }
  .canvas-wrap {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }
  .canvas-tools {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    border-bottom: 1px solid var(--ftbq-frame, #101014);
    background: linear-gradient(180deg, rgba(255, 255, 255, 0.03), rgba(0, 0, 0, 0.2)),
      var(--ftbq-bg-panel, #212126);
    box-shadow: inset 0 -1px 0 rgba(255, 255, 255, 0.05);
  }
  .canvas-tools input {
    flex: 1;
    background: #141419;
    border: 1px solid #0c0c0f;
    color: inherit;
    border-radius: 3px;
    padding: 4px 8px;
    font-size: 12px;
  }
  .filt-count {
    font-size: 11px;
    color: var(--ftbq-text-muted, #9a9aa0);
  }
  .layout-btns {
    display: flex;
    gap: 4px;
    flex-shrink: 0;
  }
  .layout-btn {
    padding: 3px 8px;
    font-size: 11px;
    border: 1px solid var(--ftbq-border, #3a3a42);
    background: rgba(0, 0, 0, 0.25);
    color: var(--ftbq-text-muted, #9a9aa0);
    border-radius: 3px;
    cursor: pointer;
  }
  .layout-btn:hover {
    color: var(--ftbq-text, #e8e8e8);
    border-color: var(--ftbq-accent-teal, #3db8a8);
  }
  .search-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    border-bottom: 1px solid var(--ftbq-frame, #101014);
    background: linear-gradient(90deg, rgba(61, 184, 168, 0.12), rgba(0, 0, 0, 0.2));
    flex-shrink: 0;
  }
  .search-bar input {
    flex: 1;
    padding: 5px 8px;
    font-size: 12px;
  }
  .search-panel {
    flex-shrink: 0;
    border-bottom: 1px solid var(--ftbq-frame, #101014);
    background: var(--ftbq-bg-panel, #212126);
  }
  .search-panel .search-bar {
    border-bottom: none;
  }
  .search-results {
    list-style: none;
    margin: 0;
    padding: 0 8px 8px;
    max-height: 180px;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .search-hit {
    display: grid;
    grid-template-columns: minmax(72px, 1fr) auto minmax(0, 2fr);
    gap: 8px;
    width: 100%;
    text-align: left;
    padding: 5px 8px;
    border: 1px solid transparent;
    border-radius: 3px;
    background: rgba(0, 0, 0, 0.2);
    color: inherit;
    font-size: 11px;
    cursor: pointer;
  }
  .search-hit:hover,
  .search-hit.active {
    border-color: var(--ftbq-accent-teal, #3db8a8);
    background: rgba(61, 184, 168, 0.12);
  }
  .hit-ch {
    color: var(--ftbq-title-gold, #f2c94c);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hit-field {
    color: var(--ftbq-text-muted, #9a9aa0);
    text-transform: uppercase;
    font-size: 9px;
    letter-spacing: 0.04em;
  }
  .hit-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .search-more {
    font-size: 11px;
    color: var(--ftbq-text-muted, #9a9aa0);
    padding: 4px 8px;
  }
  .side-panel {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    background: var(--ftbq-bg-panel, #212126);
    border-left: 1px solid var(--ftbq-frame, #101014);
  }
  .panel-tabs {
    display: flex;
    flex-shrink: 0;
    border-bottom: 1px solid var(--ftbq-frame, #101014);
    background: linear-gradient(180deg, rgba(255, 255, 255, 0.04), rgba(0, 0, 0, 0.25));
  }
  .panel-tabs .tab {
    flex: 1;
    padding: 7px 4px;
    border: none;
    border-right: 1px solid var(--ftbq-frame, #101014);
    background: transparent;
    color: var(--ftbq-text-muted, #9a9aa0);
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    cursor: pointer;
  }
  .panel-tabs .tab:last-child {
    border-right: none;
  }
  .panel-tabs .tab:hover {
    color: var(--ftbq-text, #e8e8e8);
  }
  .panel-tabs .tab.active {
    color: var(--ftbq-title-gold, #f2c94c);
    background: rgba(242, 201, 76, 0.08);
    box-shadow: inset 0 -2px 0 var(--ftbq-title-gold, #f2c94c);
  }
  .panel-content {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
  .sel-hint {
    margin: 0;
    padding: 8px 12px 12px;
    font-size: 11px;
    color: var(--ftbq-text-muted, #9a9aa0);
    border-top: 1px solid var(--ftbq-frame, #101014);
  }
  .qe-lay.with-insp {
    grid-template-columns: 200px 1fr 300px;
  }
  .qe-footer {
    margin-top: 8px;
    flex-shrink: 0;
  }
  .qe-footer .hint {
    margin: 0;
    color: var(--ftbq-text-muted, #9a9aa0);
  }
  .qe-actions .active {
    color: var(--ftbq-title-gold, #f2c94c);
    border-color: var(--ftbq-accent-green, #55c95a);
  }
  :global(.spin) {
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (max-width: 900px) {
    .qe-lay,
    .qe-lay.with-insp {
      grid-template-columns: 160px 1fr;
    }
    .qe-lay.with-insp {
      grid-template-columns: 160px 1fr minmax(220px, 260px);
    }
  }
</style>
