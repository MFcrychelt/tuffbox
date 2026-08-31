<script lang="ts">
  import { api, type QuestChapter, type QuestChapterGroup, type QuestData, type QuestValidationIssue, type QuestProgressTeamRef, type QuestProgressSnapshot, type QuestProgressStatus, type QuestPlanMergeResult, stripLocaleOverlay, chapterToSnbtJson } from "../lib/api";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { ScrollText, RefreshCw, Save, AlertTriangle, CheckCircle2, Map as MapIcon, Sparkles, X, Undo2, Redo2, Keyboard, MoreHorizontal, Globe } from "@lucide/svelte";
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
  import BookSettingsPanel from "./quests/BookSettingsPanel.svelte";
  import ChapterGroupsPanel from "./quests/ChapterGroupsPanel.svelte";
  import ProgressPanel from "./quests/ProgressPanel.svelte";
  import QuestKubeJsPanel from "./quests/QuestKubeJsPanel.svelte";
  import ShortcutsModal from "./ui/ShortcutsModal.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import VanillaClientJarPrompt from "./VanillaClientJarPrompt.svelte";
  import { wouldCreateQuestCycle } from "./quests/deps";
  import type { QuestRewardTable } from "../lib/api";
  import { snbtTextsEqual, type SnbtDiffFile } from "../lib/snbtDiff";
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
    dirtyIdsAgainstBaseline,
    patchSavedBaseline,
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
  let noticeClearTimer: ReturnType<typeof setTimeout> | null = null;

  function clearNotices() {
    if (noticeClearTimer) {
      clearTimeout(noticeClearTimer);
      noticeClearTimer = null;
    }
    error = null;
    message = null;
  }

  function flashNotice(kind: "success" | "error", text: string) {
    if (noticeClearTimer) {
      clearTimeout(noticeClearTimer);
      noticeClearTimer = null;
    }
    if (kind === "error") {
      message = null;
      error = text;
    } else {
      error = null;
      message = text;
      noticeClearTimer = setTimeout(() => {
        message = null;
        noticeClearTimer = null;
      }, 5000);
    }
  }
  let selectedChapter = $state("");
  let selectedQuest = $state<QuestData | null>(null);
  let selectedCanvasEdge = $state<{ questId: string; depId: string } | null>(null);
  let applyNeedsSave = $state(false);
  let inspectorFocusField = $state<string | null>(null);
  let inspectorFocusToken = $state(0);
  let validationIssues = $state<QuestValidationIssue[]>([]);
  let dirtyChapters = $state(new Set<string>());
  /** Chapter id → JSON at last successful load/save (disk baseline for dirty + undo). */
  let savedChapterJson = $state<Record<string, string>>({});
  let lastLoadedPath = $state<string | null>(null);
  let fitToken = $state(0);
  let addQuestToken = $state(0);
  let questSearch = $state("");
  let questSearchDebounced = $state("");
  $effect(() => {
    const q = questSearch;
    const t = setTimeout(() => {
      questSearchDebounced = q;
    }, 150);
    return () => clearTimeout(t);
  });
  let showBookPanel = $state(false);
  let showGroupsPanel = $state(false);
  let showTablesPanel = $state(false);
  let showLocalePanel = $state(false);
  let showKubeJsPanel = $state(false);
  let kubeJsFocusId = $state<string | null>(null);
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
  let searchInputEl = $state<HTMLInputElement | null>(null);
  let searchWasOpen = $state(false);
  let batchFocusToken = $state(0);
  let clipboard = $state<QuestData[]>([]);
  let showShortcuts = $state(false);
  let panelTab = $state<"quest" | "info" | "batch" | "colors" | "raw">("info");
  let railWidth = $state(200);
  let inspWidth = $state(300);
  let validateTimer: ReturnType<typeof setTimeout> | null = null;
  let itemCatalogCache = $state<Set<string> | null>(null);
  let snbtDiffOpen = $state(false);
  let snbtDiffTitle = $state("Review SNBT changes");
  let snbtDiffLeftLabel = $state("Disk");
  let snbtDiffRightLabel = $state("Editor");
  let snbtDiffLeft = $state("");
  let snbtDiffRight = $state("");
  let snbtDiffFiles = $state<SnbtDiffFile[] | null>(null);
  let snbtDiffConfirmLabel = $state("Save");
  let snbtDiffResolver: ((ok: boolean) => void) | null = null;

  let confirmOpen = $state(false);
  let confirmTitle = $state("Confirm");
  let confirmMessage = $state("");
  let confirmDanger = $state(false);
  let confirmLabel = $state("Confirm");
  let confirmResolver: ((ok: boolean) => void) | null = null;
  let reloadGen = 0;
  let pathSwitchGen = 0;

  const dismissedVanillaPrompt = new Set<string>();
  let vanillaPromptOpen = $state(false);
  let vanillaPromptVersion = $state("");
  let vanillaPromptSize = $state<number | null>(null);
  let vanillaDownloading = $state(false);
  let vanillaDownloadError = $state<string | null>(null);

  const PANEL_TABS = ["quest", "info", "batch", "colors", "raw"] as const;
  type PanelTab = (typeof PANEL_TABS)[number];

  function onPanelTabKeydown(e: KeyboardEvent) {
    if (e.key !== "ArrowLeft" && e.key !== "ArrowRight" && e.key !== "Home" && e.key !== "End") {
      return;
    }
    const i = PANEL_TABS.indexOf(panelTab as PanelTab);
    if (i < 0) return;
    e.preventDefault();
    let next = i;
    if (e.key === "ArrowRight") next = (i + 1) % PANEL_TABS.length;
    else if (e.key === "ArrowLeft") next = (i - 1 + PANEL_TABS.length) % PANEL_TABS.length;
    else if (e.key === "Home") next = 0;
    else if (e.key === "End") next = PANEL_TABS.length - 1;
    const tab = PANEL_TABS[next]!;
    setPanelTab(tab);
    queueMicrotask(() => {
      document.getElementById(`quest-panel-tab-${tab}`)?.focus();
    });
  }

  function setPanelTab(tab: PanelTab) {
    panelTab = tab;
    if (tab === "batch") batchFocusToken += 1;
  }

  function askConfirm(opts: {
    title: string;
    message: string;
    danger?: boolean;
    confirmLabel?: string;
  }): Promise<boolean> {
    confirmTitle = opts.title;
    confirmMessage = opts.message;
    confirmDanger = opts.danger ?? false;
    confirmLabel = opts.confirmLabel ?? "Confirm";
    confirmOpen = true;
    return new Promise<boolean>((resolve) => {
      confirmResolver = resolve;
    });
  }

  function closeConfirm(ok: boolean) {
    confirmOpen = false;
    const r = confirmResolver;
    confirmResolver = null;
    r?.(ok);
  }

  async function maybeOfferVanillaJar() {
    if (!$projectPath) return;
    if (dismissedVanillaPrompt.has($projectPath)) return;
    try {
      const status = await api.minecraft.clientJarStatus($projectPath);
      if (status.found) return;
      vanillaPromptVersion = status.resolvedVersion || status.version;
      vanillaPromptSize = status.downloadSize ?? null;
      vanillaDownloadError = null;
      vanillaPromptOpen = true;
    } catch {
      // Catalog still works with mod-only items; skip the modal.
    }
  }

  async function downloadVanillaJar() {
    if (!$projectPath) return;
    vanillaDownloading = true;
    vanillaDownloadError = null;
    try {
      await api.minecraft.downloadClientJar($projectPath);
      vanillaPromptOpen = false;
      dismissedVanillaPrompt.delete($projectPath);
      flashNotice("success", `Downloaded Minecraft ${vanillaPromptVersion} client jar.`);
      try {
        const catalog = await api.quests.itemCatalog($projectPath);
        itemCatalogCache = new Set(catalog ?? []);
      } catch {
        itemCatalogCache = null;
      }
      scheduleLiveValidate();
    } catch (e) {
      vanillaDownloadError = String(e);
    } finally {
      vanillaDownloading = false;
    }
  }

  function dismissVanillaPrompt() {
    if ($projectPath) dismissedVanillaPrompt.add($projectPath);
    vanillaPromptOpen = false;
  }

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

  async function refreshItemCatalog() {
    if (!$projectPath) return;
    try {
      const catalog = await api.quests.itemCatalog($projectPath);
      itemCatalogCache = new Set(catalog ?? []);
      scheduleLiveValidate();
    } catch {
      /* keep last catalog */
    }
  }

  $effect(() => {
    void $projectPath;
    let cancelled = false;
    let unlisten: UnlistenFn | undefined;
    void listen("catalog-ready", () => {
      if (!cancelled) void refreshItemCatalog();
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  });

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
    const gen = ++reloadGen;
    loading = true;
    error = null;
    message = null;
    try {
      const book = await api.quests.load($projectPath);
      if (gen !== reloadGen) return;
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
      savedChapterJson = chapterJsonMap(chapters);
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
        if (gen !== reloadGen) return;
        itemCatalogCache = new Set(catalog ?? []);
      } catch {
        if (gen !== reloadGen) return;
        itemCatalogCache = null;
      }
      if (gen === reloadGen) {
        await maybeOfferVanillaJar();
        if (gen !== reloadGen) return;
      }
      validationIssues = await api.quests.validate($projectPath);
      if (gen !== reloadGen) return;
      if (book.loadWarnings?.length) {
        const shown = book.loadWarnings.slice(0, 3).join(" · ");
        const more =
          book.loadWarnings.length > 3
            ? ` (+${book.loadWarnings.length - 3} more)`
            : "";
        flashNotice("error", `Load warnings: ${shown}${more}`);
      }
      lastLoadedPath = $projectPath;
      fitToken += 1;
      await refreshProgressTeams();
    } catch (e) {
      if (gen !== reloadGen) return;
      flashNotice("error", String(e));
    } finally {
      if (gen === reloadGen) loading = false;
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

  const availableLocales = $derived(localeCodes(locales));

  async function requestReload() {
    if (hasDirty) {
      const ok = await askConfirm({
        title: "Reload quests?",
        message: "Reload and discard unsaved quest edits?",
        danger: true,
        confirmLabel: "Discard & reload",
      });
      if (!ok) return;
    }
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
      flashNotice("error", String(e));
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
      flashNotice("error", String(e));
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
      flashNotice("error", "Select a save team first, then Seed.");
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
      flashNotice("success", `Simulate seeded with ${simCompleted.length} completed quest(s)`);
    } catch (e) {
      flashNotice("error", String(e));
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

  function markDirty(_chapterId: string) {
    chapters = [...chapters];
    syncDirtyFromBaseline();
    if (activeLocale) {
      dirtyLocales = new Set([...dirtyLocales, activeLocale]);
    }
    scheduleLiveValidate();
  }

  function markLocaleDirty(code: string) {
    dirtyLocales = new Set([...dirtyLocales, code]);
  }

  function closeBookChrome() {
    bookMenuOpen = false;
    showBookPanel = false;
    showGroupsPanel = false;
    showTablesPanel = false;
    showLocalePanel = false;
    showKubeJsPanel = false;
  }

  function openBookDrawer(panel: "book" | "groups" | "tables" | "locales" | "kubejs") {
    showBookPanel = panel === "book";
    showGroupsPanel = panel === "groups";
    showTablesPanel = panel === "tables";
    showLocalePanel = panel === "locales";
    showKubeJsPanel = panel === "kubejs";
    bookMenuOpen = false;
  }

  function openKubeJsForId(id: string) {
    kubeJsFocusId = id;
    openBookDrawer("kubejs");
  }

  function createCustomForKubeJs(
    kind: "task" | "reward",
    opts?: { title?: string; maxProgress?: number },
  ): string | null {
    if (!selectedQuest || !selectedChapter) return null;
    if (!dirtyChapters.has(selectedChapter)) pushHistory();
    const id = crypto.randomUUID().replace(/-/g, "").slice(0, 16).toUpperCase();
    if (kind === "task") {
      selectedQuest.tasks = [
        ...selectedQuest.tasks,
        {
          id,
          type: "custom",
          title: opts?.title ?? "Custom task",
          properties: { max_progress: opts?.maxProgress ?? 1 },
        },
      ];
    } else {
      selectedQuest.rewards = [
        ...selectedQuest.rewards,
        {
          id,
          type: "custom",
          title: opts?.title ?? "Custom reward",
          properties: {},
        },
      ];
    }
    selectedQuest = { ...selectedQuest };
    markDirty(selectedChapter);
    kubeJsFocusId = id;
    return id;
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
    flashNotice("success", `Created lang/${normalized}.snbt`);
  }

  function jumpToLocaleGap(entry: LocaleGapEntry, targetCode: string) {
    if (targetCode && targetCode !== activeLocale) {
      compareLocale = targetCode;
    } else {
      compareLocale = null;
    }
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
      showLocalePanel = false;
      bookMenuOpen = false;
      return;
    }
    if (entry.chapterId && chapters.some((c) => c.id === entry.chapterId)) {
      selectedChapter = entry.chapterId;
      selectedQuest = null;
      showLocalePanel = false;
      bookMenuOpen = false;
      return;
    }
    if (entry.groupId) {
      openBookDrawer("groups");
      return;
    }
    openBookDrawer("locales");
  }

  async function promptSnbtDiff(opts: {
    title: string;
    leftLabel?: string;
    rightLabel?: string;
    leftText?: string;
    rightText?: string;
    files?: SnbtDiffFile[] | null;
    confirmLabel?: string;
  }): Promise<boolean> {
    snbtDiffTitle = opts.title;
    snbtDiffLeftLabel = opts.leftLabel ?? "Disk";
    snbtDiffRightLabel = opts.rightLabel ?? "Editor";
    snbtDiffLeft = opts.leftText ?? "";
    snbtDiffRight = opts.rightText ?? "";
    snbtDiffFiles = opts.files ?? null;
    snbtDiffConfirmLabel = opts.confirmLabel ?? "Save";
    snbtDiffOpen = true;
    return await new Promise<boolean>((resolve) => {
      snbtDiffResolver = resolve;
    });
  }

  function closeSnbtDiff(ok: boolean) {
    snbtDiffOpen = false;
    snbtDiffFiles = null;
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

  /** Disk vs editor SNBT for a chapter; null if no disk file or identical. */
  async function buildChapterSnbtDiff(
    ch: QuestChapter,
    projectDir: string,
  ): Promise<SnbtDiffFile | null> {
    const { relativePath, filePath } = chapterFilePath(ch, projectDir);
    let diskText: string;
    try {
      diskText = await api.quests.readChapterText(filePath);
    } catch {
      return null;
    }
    const payload = stripLocaleOverlay(chapterToSnbtJson(ch));
    const editorText = await api.quests.previewChapterSnbt(JSON.stringify(payload));
    if (snbtTextsEqual(diskText, editorText)) return null;
    return {
      id: ch.id,
      label: ch.title || relativePath,
      leftText: diskText,
      rightText: editorText,
      leftLabel: relativePath,
      rightLabel: "Editor (about to write)",
    };
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
      const payload = stripLocaleOverlay(chapterToSnbtJson(ch));
      const jsonPayload = JSON.stringify(payload);

      if (!opts?.skipDiff) {
        const diff = await buildChapterSnbtDiff(ch, projectDir);
        if (diff) {
          saving = false;
          const ok = await promptSnbtDiff({
            title: `Save chapter “${ch.title || ch.id}”?`,
            files: [diff],
            confirmLabel: "Write SNBT",
          });
          if (!ok) {
            flashNotice("success", "Save cancelled");
            return "cancelled";
          }
          saving = true;
        }
      }

      await api.quests.saveChapterRaw(filePath, jsonPayload);
      ch.sourceFile = relativePath;
      chapters = [...chapters];
      rememberSavedChapter(chapterId);
      syncDirtyFromBaseline();
      flashNotice("success", `Saved ${ch.quests.length} quests → ${ch.sourceFile}`);
      validationIssues = await api.quests.validate($projectPath);
      return "saved";
    } catch (e) {
      flashNotice("error", String(e));
      return "error";
    } finally {
      saving = false;
    }
  }

  /** Save a chapter before AI branch generation so the anchor quest exists on disk
   *  (backend loads the book from disk). Returns the save outcome. */
  async function saveChapterForAi(
    chapterId: string,
  ): Promise<"saved" | "notdirty" | "cancelled" | "error"> {
    if (!$projectPath) return "error";
    if (!dirtyChapters.has(chapterId)) return "notdirty";
    const r = await saveChapter(chapterId);
    return r;
  }

  async function saveAll() {
    const dirtyIds = [...dirtyChapters];
    const localeCount = dirtyLocales.size;
    const parts: string[] = [];
    if (dirtyIds.length) parts.push(`${dirtyIds.length} chapter(s)`);
    if (rewardTablesDirty) parts.push("reward tables");
    if (bookDirty) parts.push("book data");
    if (groupsDirty) parts.push("chapter groups");
    if (localeCount) parts.push(`${localeCount} locale(s)`);
    if (parts.length === 0) {
      flashNotice("success", "Nothing to save");
      return;
    }
    const chapterNames = dirtyIds
      .map((id) => chapters.find((c) => c.id === id)?.title || id)
      .slice(0, 8);
    const summary =
      `Save All: ${parts.join(", ")}` +
      (chapterNames.length
        ? `\n\nChapters:\n- ${chapterNames.join("\n- ")}${dirtyIds.length > chapterNames.length ? "\n- …" : ""}`
        : "");

    let diffs: SnbtDiffFile[] = [];
    if (dirtyIds.length > 0 && $projectPath) {
      const projectDir = await api.project.getDir($projectPath);
      for (const id of dirtyIds) {
        const ch = chapters.find((c) => c.id === id);
        if (!ch) continue;
        try {
          const d = await buildChapterSnbtDiff(ch, projectDir);
          if (d) diffs.push(d);
        } catch (e) {
          flashNotice("error", String(e));
          return;
        }
      }
    }

    // Single confirm: SNBT review when chapter diffs exist, otherwise a plain confirm.
    if (diffs.length > 0) {
      const ok = await promptSnbtDiff({
        title: `Save all? ${parts.join(", ")}`,
        files: diffs,
        confirmLabel: "Save all",
      });
      if (!ok) {
        flashNotice("success", "Save cancelled");
        return;
      }
    } else {
      const confirmed = await askConfirm({
        title: "Save all changes?",
        message: summary,
        confirmLabel: "Save all",
      });
      if (!confirmed) return;
    }

    try {
      await saveLocaleIfNeeded();
    } catch (e) {
      flashNotice("error", String(e));
      return;
    }
    if (dirtyIds.length > 0) {
      for (const id of dirtyIds) {
        const result = await saveChapter(id, { skipDiff: true });
        if (result === "error") break;
      }
    }
    if (rewardTablesDirty) {
      for (const t of rewardTables) {
        await saveRewardTable(t);
      }
    }
    if (bookDirty) await saveBookData();
    if (groupsDirty) await saveGroups();
    applyNeedsSave = false;
  }

  async function revalidateFromDisk() {
    if (!$projectPath) return;
    try {
      validationIssues = await api.quests.validate($projectPath);
      flashNotice("success", `Disk validate: ${validationIssues.length} issue(s)`);
    } catch (e) {
      flashNotice("error", String(e));
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
        flashNotice("success", "No chapter file on disk yet");
        return;
      }
      const payload = stripLocaleOverlay(chapterToSnbtJson(ch));
      const editorText = await api.quests.previewChapterSnbt(JSON.stringify(payload));
      if (snbtTextsEqual(diskText, editorText)) {
        flashNotice("success", "Editor matches disk SNBT");
        return;
      }
      await promptSnbtDiff({
        title: `Diff vs disk — ${ch.title || ch.id}`,
        files: [
          {
            id: ch.id,
            label: ch.title || relativePath,
            leftText: diskText,
            rightText: editorText,
            leftLabel: relativePath,
            rightLabel: "Editor",
          },
        ],
        confirmLabel: "Close",
      });
    } catch (e) {
      flashNotice("error", String(e));
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

  function syncDirtyFromBaseline() {
    dirtyChapters = new Set(
      dirtyIdsAgainstBaseline(chapterJsonMap(chapters), savedChapterJson),
    );
  }

  function rememberSavedChapter(chapterId: string) {
    const ch = chapters.find((c) => c.id === chapterId);
    if (!ch) return;
    savedChapterJson = patchSavedBaseline(
      savedChapterJson,
      chapterId,
      JSON.stringify(ch),
    );
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

  function clampPanel(n: number, min: number, max: number): number {
    return Math.max(min, Math.min(max, Math.round(n)));
  }

  function startColResize(which: "rail" | "insp", e: PointerEvent) {
    e.preventDefault();
    e.stopPropagation();
    const startX = e.clientX;
    const startW = which === "rail" ? railWidth : inspWidth;
    const target = e.currentTarget as HTMLElement;
    try {
      target.setPointerCapture(e.pointerId);
    } catch {
      /* ignore */
    }
    const onMove = (ev: PointerEvent) => {
      const dx = ev.clientX - startX;
      if (which === "rail") {
        railWidth = clampPanel(startW + dx, 140, 360);
      } else {
        // Dragging the left edge of the inspector: move left → wider.
        inspWidth = clampPanel(startW - dx, 240, 520);
      }
    };
    const onUp = (ev: PointerEvent) => {
      try {
        target.releasePointerCapture(ev.pointerId);
      } catch {
        /* ignore */
      }
      target.removeEventListener("pointermove", onMove);
      target.removeEventListener("pointerup", onUp);
      target.removeEventListener("pointercancel", onUp);
    };
    target.addEventListener("pointermove", onMove);
    target.addEventListener("pointerup", onUp);
    target.addEventListener("pointercancel", onUp);
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
      tasks: [
        {
          id: crypto.randomUUID().replace(/-/g, "").slice(0, 12).toUpperCase(),
          type: "item",
          properties: { count: 1 },
        },
      ],
      rewards: [],
      optional: false,
      size: 1,
      extras: {},
    };
    ch.quests = [...ch.quests, newQ];
    markDirty(selectedChapter);
    selectedQuest = newQ;
    selection = selectSingle(selection, newQ.id);
    panelTab = "quest";
    if (questSearch.trim()) {
      questSearch = "";
      flashNotice("success", "Filter cleared so the new quest is visible");
    }
  }

  async function removeQuest(q: QuestData) {
    const ok = await askConfirm({
      title: "Delete quest?",
      message: `Delete quest "${q.title}" from this chapter?`,
      danger: true,
      confirmLabel: "Delete",
    });
    if (!ok) return;
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

  async function removeSelectedQuests() {
    if (selection.selectedIds.size === 0) return;
    const n = selection.selectedIds.size;
    const ok = await askConfirm({
      title: "Delete selected quests?",
      message: `Delete ${n} selected quest(s)?`,
      danger: true,
      confirmLabel: "Delete",
    });
    if (!ok) return;
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
      flashNotice("error", "That dependency would create a cycle.");
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

  /** Canvas handle connect: dependent (from) lists prereq (toDepId) in dependencies. */
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
      flashNotice("success", `Saved reward table → ${res.relativePath}`);
    } catch (e) {
      flashNotice("error", String(e));
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
    chapters = chapters.map((ch) => {
      let touched = false;
      const quests = ch.quests.map((q) => {
        if (!questIds.has(q.id)) return q;
        touched = true;
        return mutator(q);
      });
      return touched ? { ...ch, quests } : ch;
    });
    syncDirtyFromBaseline();
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
    applyHistorySnapshot(result.snapshot);
    syncDirtyFromBaseline();
    scheduleLiveValidate();
    if (progressMode === "simulate") void refreshSimulate();
  }

  function handleRedo() {
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
    applyHistorySnapshot(result.snapshot);
    syncDirtyFromBaseline();
    scheduleLiveValidate();
    if (progressMode === "simulate") void refreshSimulate();
  }

  function handleCopy() {
    if (selection.selectedIds.size === 0) return;
    const ch = chapters.find((c) => c.id === selectedChapter);
    if (!ch) return;
    clipboard = ch.quests.filter((q) => selection.selectedIds.has(q.id)).map((q) => structuredClone(q));
    flashNotice("success", `Copied ${clipboard.length} quest(s)`);
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
    if (selectedQuest) panelTab = "quest";
    flashNotice("success", `Pasted ${newQuests.length} quest(s)`);
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
    selectedQuest = result.quest as QuestData;
    selection = selectSingle(selection, result.quest.id);
    panelTab = "quest";
    fitToken += 1;
  }

  function handleSearchKeyDown(e: KeyboardEvent) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      search = nextResult(search);
      navigateSearchResult();
      return;
    }
    if (e.key === "ArrowUp") {
      e.preventDefault();
      search = prevResult(search);
      navigateSearchResult();
      return;
    }
    if (e.key === "Enter") {
      e.preventDefault();
      if (e.shiftKey) search = prevResult(search);
      else search = nextResult(search);
      navigateSearchResult();
      return;
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
        panelTab = "quest";
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
      if (
        bookMenuOpen ||
        showBookPanel ||
        showGroupsPanel ||
        showTablesPanel ||
        showLocalePanel ||
        showKubeJsPanel
      ) {
        closeBookChrome();
        return;
      }
      if (issuesOpen) {
        issuesOpen = false;
        return;
      }
      if (aiSidebarOpen) {
        setAiSidebar(false);
        return;
      }
      if (search.isOpen) {
        search = { ...search, isOpen: false };
      } else if (showShortcuts) {
        showShortcuts = false;
      } else {
        selection = clearSelection();
        selectedQuest = null;
        selectedCanvasEdge = null;
      }
      return;
    }

    if ((e.key === "Delete" || e.key === "Backspace") && !isInput) {
      e.preventDefault();
      if (selectedCanvasEdge) {
        const ch = chapters.find((c) => c.id === selectedChapter);
        const q = ch?.quests.find((x) => x.id === selectedCanvasEdge!.questId);
        if (q) removeDep(q, selectedCanvasEdge.depId);
        selectedCanvasEdge = null;
        return;
      }
      removeSelectedQuests();
      return;
    }

    if (!isInput && (e.key === "n" || e.key === "N") && !e.ctrlKey && !e.metaKey && !e.altKey) {
      e.preventDefault();
      if (selectedChapter) addQuestToken += 1;
      return;
    }

    if (["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].includes(e.key)) {
      const t = e.target as HTMLElement | null;
      if (t?.closest?.('[role="application"].viewport, .viewport[role="application"]')) {
        return;
      }
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
      flashNotice(
        "error",
        (result.validation?.errors ?? []).slice(0, 3).join("; ") || "Plan invalid",
      );
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
    syncDirtyFromBaseline();
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
    flashNotice(
      "success",
      `AI plan applied in editor (${result.touchedChapterIds.length} chapter(s)). Undo (Ctrl+Z) to revert.`,
    );
    applyNeedsSave = true;
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
      flashNotice("success", "Book data.snbt saved.");
    } catch (e) {
      flashNotice("error", String(e));
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
      flashNotice("success", "Chapter groups saved.");
    } catch (e) {
      flashNotice("error", String(e));
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

  async function deleteChapter(id: string) {
    const ch = chapters.find((c) => c.id === id);
    const fileHint = ch?.sourceFile
      ? `\n\nEditor-only: the SNBT file “${ch.sourceFile}” stays on disk — delete it manually if you no longer need it.`
      : "\n\nEditor-only: any chapter SNBT file on disk is not deleted.";
    const ok = await askConfirm({
      title: "Remove chapter from editor?",
      message: `Remove “${ch?.title || id}” from the quest book in this session?${fileHint}`,
      danger: true,
      confirmLabel: "Remove from editor",
    });
    if (!ok) return;
    pushHistory();
    chapters = chapters.filter((c) => c.id !== id);
    syncDirtyFromBaseline();
    if (selectedChapter === id) {
      selectedChapter = chapters[0]?.id ?? "";
      selectedQuest = null;
      selection = clearSelection();
      if (panelTab === "quest") panelTab = "info";
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
    // Keep the issues list open so authors can walk multiple findings.
    const qid = issue.questId;
    const msg = (issue.message ?? "").toLowerCase();
    let field: string | null = null;
    if (msg.includes("empty title")) field = "title";
    else if (msg.includes("no tasks") || msg.includes("item task")) field = "tasks";
    else if (msg.includes("missing quest icon")) field = "icon";
    for (const ch of chapters) {
      const q = ch.quests.find((x) => x.id === qid);
      if (q) {
        selectedChapter = ch.id;
        selectedQuest = q;
        selection = selectSingle(selection, q.id);
        panelTab = "quest";
        fitToken += 1;
        if (field) {
          inspectorFocusField = field;
          inspectorFocusToken += 1;
        }
        return;
      }
    }
    // Chapter-level or unknown — try chapter id match
    if (chapters.some((c) => c.id === qid)) {
      selectedChapter = qid;
      selectedQuest = null;
    }
  }

  const filteredChapterQuests = $derived(questSearchDebounced.trim()
    ? chapterQuests.filter((q) => {
        const s = questSearchDebounced.toLowerCase();
        return (
          q.title.toLowerCase().includes(s) ||
          q.id.toLowerCase().includes(s) ||
          (q.subtitle ?? "").toLowerCase().includes(s)
        );
      }).slice(0, 200)
    : chapterQuests);

  const hasDirty = $derived(
    dirtyChapters.size > 0 ||
      rewardTablesDirty ||
      bookDirty ||
      groupsDirty ||
      dirtyLocales.size > 0,
  );
  $effect(() => {
    const justOpened = search.isOpen && !searchWasOpen;
    searchWasOpen = search.isOpen;
    if (!justOpened) return;
    queueMicrotask(() => searchInputEl?.focus({ preventScroll: true }));
  });

  $effect(() => {
    questDirty.set(hasDirty);
  });

  async function handleProjectPathChange(nextPath: string) {
    const gen = ++pathSwitchGen;
    if (!nextPath || nextPath === lastLoadedPath) return;
    if (hasDirty && lastLoadedPath) {
      const prev = lastLoadedPath;
      const ok = await askConfirm({
        title: "Switch project?",
        message: "Discard unsaved quest edits and switch project?",
        danger: true,
        confirmLabel: "Discard & switch",
      });
      if (gen !== pathSwitchGen) return;
      if (!ok) {
        if ($projectPath === nextPath) projectPath.set(prev);
        return;
      }
    }
    if (gen !== pathSwitchGen) return;
    if ($projectPath !== nextPath) return;
    await load();
  }

  $effect(() => {
    const path = $projectPath;
    if (path && path !== lastLoadedPath) {
      void handleProjectPathChange(path);
    }
  });

  $effect(() => {
    const onBeforeUnload = (e: BeforeUnloadEvent) => {
      if (hasDirty) e.preventDefault();
    };
    window.addEventListener("beforeunload", onBeforeUnload);
    return () => window.removeEventListener("beforeunload", onBeforeUnload);
  });

  $effect(() => {
    if (selectedQuest) {
        const fresh = chapterQuests.find((q) => q.id === selectedQuest!.id);
        if (fresh && fresh !== selectedQuest) selectedQuest = fresh;
      }
  });
  $effect(() => {
    if ($questChatFocusId) {
        setAiSidebar(true);
      }
  });

  onDestroy(() => {
    if (validateTimer) clearTimeout(validateTimer);
    if (noticeClearTimer) clearTimeout(noticeClearTimer);
    questDirty.set(false);
  });
  function onWindowPointerDown(e: PointerEvent) {
    const t = e.target as HTMLElement | null;
    if (!t) return;
    if (issuesOpen && !t.closest(".issues-wrap")) {
      issuesOpen = false;
    }
    if (
      (bookMenuOpen || showBookPanel || showGroupsPanel || showTablesPanel || showLocalePanel || showKubeJsPanel) &&
      !t.closest(".tb-pop") &&
      !t.closest(".qe-sheet") &&
      !t.closest(".qe-sheet-backdrop")
    ) {
      closeBookChrome();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} onpointerdowncapture={onWindowPointerDown} />

<div class="qe ftbq flex w-full min-h-0 flex-col">
<div class="qe-tb flex items-center gap-2 flex-wrap shrink-0 justify-between mb-1 px-3 py-[7px] min-h-11 max-h-[52px]">
    <div class="qe-title flex items-center gap-2 flex-wrap min-w-0">
      <ScrollText size={18} />
      {#if bookTitle}<span class="book-name">{stripMc(bookTitle)}</span>{:else}Quest editor{/if}
      {#if $projectPath}
        <span class="tb-chip">{chapters.length} ch</span>
        <span class="tb-chip">{totalQuests} quests</span>
        <div class="issues-wrap">
          <button
            type="button"
            class="issues-btn"
            class:warn={validationIssues.length > 0}
            aria-haspopup="true"
            aria-expanded={issuesOpen}
            aria-controls="quest-issues-pop"
            onclick={() => (issuesOpen = !issuesOpen)}
          >
            {#if validationIssues.length === 0}
              <CheckCircle2 size={12} /> Live
            {:else}
              <AlertTriangle size={12} /> {validationIssues.length} live
            {/if}
          </button>
          {#if issuesOpen}
            <div class="issues-pop" id="quest-issues-pop" role="listbox" aria-label="Validation issues">
              {#if validationIssues.length > 0}
                {#each validationIssues as iss, i (`${iss.questId}-${i}`)}
                  <button type="button" class="issue-row" onclick={() => jumpToIssue(iss)}>
                    <code>{iss.questId.slice(0, 8)}</code>
                    <span>{iss.message}</span>
                  </button>
                {/each}
              {:else}
                <div class="issues-ok"><CheckCircle2 size={12} /> No issues found</div>
              {/if}
              <div class="issues-pop-sep"></div>
              <button
                type="button"
                class="issue-row action"
                title="Re-run Rust validate_quest_book on disk (saved SNBT)"
                onclick={() => void revalidateFromDisk()}
              >
                <RefreshCw size={12} /> Re-run validation on disk
              </button>
            </div>
          {/if}
        </div>
        {#if progressSnap && progressOverlay}
          <span class="tb-chip prog-stat"
            >{progressSnap.completedCount} done · {progressSnap.startedCount} started</span
          >
        {/if}
      {/if}
    </div>
    <div class="qe-actions flex items-center gap-2 flex-wrap">
      <div class="tb-btn-group">
        <button
          type="button"
          class="ghost"
          class:active={aiSidebarOpen}
          title="Quest AI sidebar"
          aria-label="Quest AI sidebar"
          onclick={() => setAiSidebar(!aiSidebarOpen)}
        >
          <Sparkles size={16} /> AI
        </button>
        <button
          type="button"
          class="ghost"
          disabled={!canUndo(history)}
          title="Undo (Ctrl+Z)"
          aria-label="Undo (Ctrl+Z)"
          onclick={handleUndo}
        >
          <Undo2 size={16} />
        </button>
        <button
          type="button"
          class="ghost"
          disabled={!canRedo(history)}
          title="Redo (Ctrl+Y)"
          aria-label="Redo (Ctrl+Y)"
          onclick={handleRedo}
        >
          <Redo2 size={16} />
        </button>
        <button
          type="button"
          class="ghost"
          title="Shortcuts (Ctrl+/)"
          aria-label="Shortcuts (Ctrl+/)"
          onclick={() => (showShortcuts = !showShortcuts)}
        >
          <Keyboard size={16} />
        </button>
        <button
          type="button"
          class="ghost"
          onclick={requestReload}
          disabled={!$projectPath || loading}
          title="Reload from disk"
          aria-label="Reload from disk"
        >
          <RefreshCw size={16} class={loading ? "spin" : ""} />
        </button>
        <div class="tb-pop">
          <button
            type="button"
            class="ghost"
            class:active={bookMenuOpen || showBookPanel || showGroupsPanel || showTablesPanel || showLocalePanel || showKubeJsPanel}
            class:has-dirty={bookDirty || groupsDirty || rewardTablesDirty || dirtyLocales.size > 0}
            title="Book, groups, reward tables, locales, KubeJS"
            aria-haspopup="menu"
            aria-expanded={bookMenuOpen}
            aria-controls="quest-book-menu"
            onclick={() => {
              const chromeOpen =
                bookMenuOpen ||
                showBookPanel ||
                showGroupsPanel ||
                showTablesPanel ||
                showLocalePanel ||
                showKubeJsPanel;
              if (chromeOpen) {
                closeBookChrome();
              } else {
                bookMenuOpen = true;
              }
            }}
          >
            <MoreHorizontal size={16} />
            {#if bookDirty || groupsDirty || rewardTablesDirty || dirtyLocales.size > 0}<span class="dot-mini">●</span>{/if}
          </button>
          {#if bookMenuOpen && $projectPath}
            <div class="book-menu" id="quest-book-menu" role="menu">
              {#if availableLocales.length > 1}
                <label class="menu-locale">
                  <Globe size={14} />
                  <select
                    class="locale-select"
                    value={activeLocale ?? ""}
                    title="Language overlay (lang/*.snbt)"
                    onchange={(e) => {
                      const v = (e.currentTarget as HTMLSelectElement).value;
                      if (v) switchLocale(v);
                    }}
                  >
                    {#each availableLocales as code (code)}
                      <option value={code}>{code}{#if dirtyLocales.has(code)} ●{/if}</option>
                    {/each}
                  </select>
                </label>
                <div class="menu-sep"></div>
              {/if}
              <button
                type="button"
                role="menuitem"
                class:active={showBookPanel}
                onclick={() => openBookDrawer("book")}
              >
                Book settings{#if bookDirty}<span class="dot-mini">●</span>{/if}
              </button>
              <button
                type="button"
                role="menuitem"
                class:active={showGroupsPanel}
                onclick={() => openBookDrawer("groups")}
              >
                Chapter groups{#if groupsDirty}<span class="dot-mini">●</span>{/if}
              </button>
              <button
                type="button"
                role="menuitem"
                class:active={showTablesPanel}
                onclick={() => openBookDrawer("tables")}
              >
                Reward tables{#if rewardTablesDirty}<span class="dot-mini">●</span>{/if}
              </button>
              <button
                type="button"
                role="menuitem"
                class:active={showLocalePanel}
                onclick={() => openBookDrawer("locales")}
              >
                Locales{#if dirtyLocales.size > 0}<span class="dot-mini">●</span>{/if}
              </button>
              <button
                type="button"
                role="menuitem"
                class:active={showKubeJsPanel}
                onclick={() => openBookDrawer("kubejs")}
              >
                KubeJS
              </button>
            </div>
          {/if}
        </div>
      </div>
      {#if hasDirty}
        <span class="dirty-badge"
          >{dirtyChapters.size +
            (rewardTablesDirty ? 1 : 0) +
            (bookDirty ? 1 : 0) +
            (groupsDirty ? 1 : 0) +
            dirtyLocales.size} unsaved</span
        >
        <button type="button" class="primary" onclick={saveAll} disabled={!$projectPath || saving} title="Ctrl+S">
          <Save size={16} class={saving ? "spin" : ""} /> {saving ? "Saving…" : "Save all"}
        </button>
      {/if}
    </div>
  </div>

  {#if applyNeedsSave && hasDirty}
    <div class="apply-save-banner" role="status">
      <span>AI plan is in the editor only — <strong>Save</strong> writes SNBT to disk.</span>
      <button type="button" class="primary mini" onclick={() => void saveAll()} disabled={saving}>
        Save all
      </button>
      <button type="button" class="ghost mini" onclick={() => (applyNeedsSave = false)} aria-label="Dismiss">
        Dismiss
      </button>
    </div>
  {/if}

  {#if $projectPath}
    <ProgressPanel
      bind:open={progressOpen}
      {progressMode}
      bind:progressOverlay
      {progressSnap}
      {progressTeams}
      bind:progressKey
      {progressLoading}
      {simCompleted}
      {simBusy}
      onentersave={() => void enterSaveMode()}
      onentersimulate={() => void enterSimulateMode()}
      onloadprogress={() => void loadProgress()}
      onseed={() => void seedSimulateFromTeam()}
      onreset={() => void resetSimulate()}
      onrefreshsim={() => void refreshSimulate()}
    />
  {/if}

  {#if error}
    <div class="notice error" role="alert" aria-live="assertive">
      <AlertTriangle size={14} />
      <span class="notice-text">{error}</span>
      <button type="button" class="ghost ico notice-dismiss" title="Dismiss" aria-label="Dismiss error" onclick={() => (error = null)}>
        <X size={14} />
      </button>
    </div>
  {/if}
  {#if message}
    <div class="notice success" role="status" aria-live="polite">
      <CheckCircle2 size={14} />
      <span class="notice-text">{message}</span>
      <button type="button" class="ghost ico notice-dismiss" title="Dismiss" aria-label="Dismiss message" onclick={() => (message = null)}>
        <X size={14} />
      </button>
    </div>
  {/if}

  {#if search.isOpen}
    <div class="search-panel">
      <div class="search-bar">
        <input
          type="search"
          id="quest-global-search"
          placeholder="Search fields… (Ctrl+F). Filter toolbar hides nodes."
          aria-label="Search quest fields"
          value={search.query}
          bind:this={searchInputEl}
          aria-controls="quest-search-results"
          aria-activedescendant={search.results.length
            ? `quest-search-hit-${search.selectedIndex}`
            : undefined}
          oninput={(e) => updateSearch(inputVal(e))}
          onkeydown={handleSearchKeyDown}
        />
        <span class="filt-count"
          >{search.results.length
            ? `${search.selectedIndex + 1}/${search.results.length}`
            : "0"}</span
        >
        <button
          type="button"
          class="ghost ico"
          title="Close"
          aria-label="Close search"
          onclick={() => (search = { ...search, isOpen: false })}
          ><X size={14} /></button
        >
      </div>
      {#if search.query.trim() && search.results.length === 0}
        <p class="search-empty" role="status">
          No matches. Try words, <code>/regex/</code>, or <code>re:pattern</code>.
        </p>
      {:else if search.results.length > 0}
        <ul class="search-results" id="quest-search-results" role="listbox" aria-label="Search results">
          {#each search.results.slice(0, 50) as r, i (`${r.chapterId}-${r.quest.id}-${r.matchField}-${i}`)}
            <li>
              <button
                type="button"
                class="search-hit"
                class:active={i === search.selectedIndex}
                id={`quest-search-hit-${i}`}
                role="option"
                aria-selected={i === search.selectedIndex}
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
      <div class="empty-art" aria-hidden="true">
        <span class="art-chapter">
          <span class="art-chapter-dot"></span>
          <span class="art-line"></span>
        </span>
        <span class="art-quest q1"></span>
        <span class="art-quest q2"></span>
        <span class="art-quest q3"></span>
      </div>
      <h3>Start a quest line</h3>
      <p>
        Chapters hold your quests. Create the first one, drop quests on the
        canvas and connect them — then <strong>Save all</strong> writes the SNBT.
      </p>
      <div class="empty-ctas">
        <button type="button" class="empty-cta" onclick={createChapter}>
          <ScrollText size={15} />
          Create first chapter
        </button>
      </div>
      <p class="empty-hint">No files are written until you save.</p>
    </div>
  {:else}
    <div class="qe-body-row flex flex-1 min-h-0 items-stretch overflow-hidden">
    <div
      class="qe-lay"
      class:with-insp={!!selectedChapterObj}
      style="--qe-rail: {railWidth}px; --qe-insp: {inspWidth}px;"
    >
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
      <div
        class="col-resizer"
        role="separator"
        aria-orientation="vertical"
        aria-label="Resize chapters panel"
        onpointerdown={(e) => startColResize("rail", e)}
      ></div>
      <div class="canvas-wrap flex flex-col min-w-0 min-h-0 overflow-hidden">
        <SvelteFlowProvider>
          <QuestCanvas
            quests={filteredChapterQuests}
            {chapters}
            selectedId={selectedQuest?.id ?? null}
            selectedIds={selection.selectedIds}
            issues={validationIssues}
            {fitToken}
            {addQuestToken}
            {progressOverlay}
            {progressStatuses}
            questFilter={questSearch}
            filterTotal={chapterQuests.length}
            onQuestFilterChange={(v) => (questSearch = v)}
            onApplyLayout={applyChapterLayout}
            emptyHint={questSearch.trim()
              ? `No quests match "${questSearch.trim()}"`
              : "Add a quest to get started"}
            showEmptyAddCta={!questSearch.trim()}
            onSelect={(q, e) => {
              if (
                progressMode === "simulate" &&
                progressOverlay &&
                q &&
                e?.altKey &&
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
            onUnlink={(questId, depId) => {
              const ch = chapters.find((c) => c.id === selectedChapter);
              const q = ch?.quests.find((x) => x.id === questId);
              if (q) removeDep(q, depId);
              selectedCanvasEdge = null;
            }}
            onEdgeSelect={(edge) => {
              selectedCanvasEdge = edge;
            }}
            onOpenChapter={(chapterId, questId) => {
              selectedChapter = chapterId;
              const ch = chapters.find((c) => c.id === chapterId);
              const q = questId ? ch?.quests.find((x) => x.id === questId) ?? null : null;
              selectedQuest = q;
              selection = q ? selectSingle(selection, q.id) : clearSelection();
              panelTab = q ? "quest" : "info";
              fitToken += 1;
              flashNotice("success", `Opened chapter “${ch?.title || chapterId.slice(0, 8)}”`);
            }}
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
        <div
          class="col-resizer"
          role="separator"
          aria-orientation="vertical"
          aria-label="Resize quest inspector"
          onpointerdown={(e) => startColResize("insp", e)}
        ></div>
        <div class="side-panel">
          <div
            class="panel-tabs"
            role="tablist"
            aria-label="Quest side panel"
            tabindex="-1"
            onkeydown={onPanelTabKeydown}
          >
            <button
              type="button"
              role="tab"
              class="tab"
              class:active={panelTab === "quest"}
              aria-selected={panelTab === "quest"}
              id="quest-panel-tab-quest"
              tabindex={panelTab === "quest" ? 0 : -1}
              onclick={() => setPanelTab("quest")}
              >Quest</button
            >
            <button
              type="button"
              role="tab"
              class="tab"
              class:active={panelTab === "info"}
              aria-selected={panelTab === "info"}
              id="quest-panel-tab-info"
              tabindex={panelTab === "info" ? 0 : -1}
              onclick={() => setPanelTab("info")}
              >Info</button
            >
            <button
              type="button"
              role="tab"
              class="tab"
              class:active={panelTab === "batch"}
              aria-selected={panelTab === "batch"}
              id="quest-panel-tab-batch"
              tabindex={panelTab === "batch" ? 0 : -1}
              onclick={() => setPanelTab("batch")}
              >Batch</button
            >
            <button
              type="button"
              role="tab"
              class="tab"
              class:active={panelTab === "colors"}
              aria-selected={panelTab === "colors"}
              id="quest-panel-tab-colors"
              tabindex={panelTab === "colors" ? 0 : -1}
              onclick={() => setPanelTab("colors")}
              >Colors</button
            >
            <button
              type="button"
              role="tab"
              class="tab"
              class:active={panelTab === "raw"}
              aria-selected={panelTab === "raw"}
              id="quest-panel-tab-raw"
              tabindex={panelTab === "raw" ? 0 : -1}
              onclick={() => setPanelTab("raw")}
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
                  onOpenKubeJs={(id) => openKubeJsForId(id)}
                  focusFieldToken={inspectorFocusToken}
                  focusField={inspectorFocusField}
                />
              {:else}
                <div class="sel-empty">
                  <button
                    type="button"
                    class="primary"
                    onclick={() => {
                      if (selectedChapter) addQuestToken += 1;
                    }}
                    disabled={!selectedChapter}
                  >Add quest</button>
                  <p class="sel-hint">Double-click canvas or press N</p>
                </div>
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
                focusToken={batchFocusToken}
                onQuestUpdate={handleQuestUpdate}
                onBatchApply={handleBatchApply}
                onSaveChapter={(id) => void saveChapter(id)}
              />
            {:else if panelTab === "colors"}
              <ColorManager {chapters} onQuestUpdate={handleQuestUpdate} />
            {:else if panelTab === "raw"}
              <RawSnbtView
                chapter={selectedChapterObj}
                selectedQuestId={selectedQuest?.id ?? null}
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
        onsavechapter={saveChapterForAi}
        anchorQuest={selectedQuest}
        anchorChapterTitle={selectedChapterObj?.title ?? null}
        targetChapterId={selectedChapter}
      />
    {/if}
    </div>
  {/if}
</div>

<ShortcutsModal isOpen={showShortcuts} onClose={() => (showShortcuts = false)} />

{#if $projectPath && showBookPanel}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="qe-sheet-backdrop" role="presentation" onclick={closeBookChrome}>
    <div
      class="qe-sheet"
      role="dialog"
      tabindex="-1"
      aria-modal="true"
      aria-label="Book settings"
      onclick={(e) => e.stopPropagation()}
    >
      <BookSettingsPanel
        {bookTitle}
        {bookSubtitle}
        {bookSettings}
        {bookDirty}
        {saving}
        onclose={closeBookChrome}
        onsave={() => void saveBookData()}
        onpushhistory={pushHistory}
        ontitlechange={(v) => {
          bookTitle = v;
          bookDirty = true;
        }}
        onsubtitlechange={(v) => {
          bookSubtitle = v;
          bookDirty = true;
        }}
        onsetsettings={(next) => {
          bookSettings = next;
          bookDirty = true;
        }}
      />
    </div>
  </div>
{/if}
{#if $projectPath && showGroupsPanel}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="qe-sheet-backdrop" role="presentation" onclick={closeBookChrome}>
    <div
      class="qe-sheet"
      role="dialog"
      tabindex="-1"
      aria-modal="true"
      aria-label="Chapter groups"
      onclick={(e) => e.stopPropagation()}
    >
      <ChapterGroupsPanel
        {chapterGroups}
        {groupsDirty}
        {saving}
        onclose={closeBookChrome}
        onsave={() => void saveGroups()}
        onadd={addChapterGroup}
        onremove={removeChapterGroup}
        onmove={moveChapterGroup}
        ontitlechange={(id, title) => {
          const g = chapterGroups.find((x) => x.id === id);
          if (!g) return;
          g.title = title;
          g.titleFromSnbt = true;
          groupsDirty = true;
          chapterGroups = [...chapterGroups];
        }}
      />
    </div>
  </div>
{/if}
{#if $projectPath && showTablesPanel}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="qe-sheet-backdrop" role="presentation" onclick={closeBookChrome}>
    <div
      class="qe-sheet"
      role="dialog"
      tabindex="-1"
      aria-modal="true"
      aria-label="Reward tables"
      onclick={(e) => e.stopPropagation()}
    >
      <div class="drawer-h sheet-h">
        <strong>Reward tables</strong>
        <button type="button" class="ghost ico" onclick={closeBookChrome}><X size={14} /></button>
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
  </div>
{/if}
{#if $projectPath && showLocalePanel}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="qe-sheet-backdrop" role="presentation" onclick={closeBookChrome}>
    <div
      class="qe-sheet"
      role="dialog"
      tabindex="-1"
      aria-modal="true"
      aria-label="Locales"
      onclick={(e) => e.stopPropagation()}
    >
      <div class="drawer-h sheet-h">
        <strong>Locales (lang/*.snbt)</strong>
        <button type="button" class="ghost ico" onclick={closeBookChrome}><X size={14} /></button>
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
        onFillGapsFromBase={(targetCode, baseCode, keys) => {
          const base = locales[baseCode] ?? {};
          const target = { ...(locales[targetCode] ?? {}) };
          let n = 0;
          for (const key of keys) {
            const v = base[key];
            if (v === undefined) continue;
            target[key] = structuredClone(v);
            n += 1;
          }
          locales = { ...locales, [targetCode]: target };
          markLocaleDirty(targetCode);
          flashNotice("success", `Filled ${n} key(s) into ${targetCode} from ${baseCode}`);
        }}
      />
    </div>
  </div>
{/if}
{#if $projectPath && showKubeJsPanel}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="qe-sheet-backdrop" role="presentation" onclick={closeBookChrome}>
    <div
      class="qe-sheet qe-sheet-wide"
      role="dialog"
      tabindex="-1"
      aria-modal="true"
      aria-label="KubeJS"
      onclick={(e) => e.stopPropagation()}
    >
      <QuestKubeJsPanel
        {chapters}
        {selectedQuest}
        bind:focusId={kubeJsFocusId}
        onclose={closeBookChrome}
        onjumpquest={(questId, chapterId) => {
          if (chapterId && chapters.some((c) => c.id === chapterId)) {
            selectedChapter = chapterId;
          } else {
            for (const ch of chapters) {
              if (ch.quests.some((q) => q.id === questId)) {
                selectedChapter = ch.id;
                break;
              }
            }
          }
          const q = chapters
            .flatMap((c) => c.quests.map((quest) => ({ quest, ch: c.id })))
            .find((x) => x.quest.id === questId);
          if (q) {
            selectedChapter = q.ch;
            selectedQuest = q.quest;
            selection = selectSingle(selection, q.quest.id);
            panelTab = "quest";
          }
          closeBookChrome();
        }}
        oncreatecustom={(kind, opts) => createCustomForKubeJs(kind, opts)}
        ondirtyquest={() => {
          if (selectedChapter && !dirtyChapters.has(selectedChapter)) pushHistory();
          if (selectedChapter) markDirty(selectedChapter);
          if (selectedQuest) selectedQuest = { ...selectedQuest };
        }}
      />
    </div>
  </div>
{/if}

<SnbtDiffModal
  open={snbtDiffOpen}
  title={snbtDiffTitle}
  leftLabel={snbtDiffLeftLabel}
  rightLabel={snbtDiffRightLabel}
  leftText={snbtDiffLeft}
  rightText={snbtDiffRight}
  files={snbtDiffFiles}
  confirmLabel={snbtDiffConfirmLabel}
  onConfirm={() => closeSnbtDiff(true)}
  onCancel={() => closeSnbtDiff(false)}
/>

{#if confirmOpen}
  <ConfirmDialog
    title={confirmTitle}
    message={confirmMessage}
    danger={confirmDanger}
    confirmLabel={confirmLabel}
    onconfirm={() => closeConfirm(true)}
    oncancel={() => closeConfirm(false)}
  />
{/if}

<VanillaClientJarPrompt
  open={vanillaPromptOpen}
  version={vanillaPromptVersion || "?"}
  downloadSize={vanillaPromptSize}
  downloading={vanillaDownloading}
  error={vanillaDownloadError}
  ondownload={downloadVanillaJar}
  ondismiss={dismissVanillaPrompt}
/>

<style>
  .qe {
    /* layout moved to Tailwind utilities on the root element */
    background: var(--ftbq-bg);
    color: var(--ftbq-text);
  }
  /* Isolate from global TuffBox green primary buttons — flat chrome */
  .qe.ftbq :global(button) {
    border-radius: var(--ftbq-radius-control);
    font-weight: 600;
    box-shadow: none;
    text-shadow: none;
  }
  .qe.ftbq :global(.fmt-bar button) {
    border-radius: 0;
    border: none;
    border-right: 1px solid var(--ftbq-frame);
    background: transparent;
    box-shadow: none;
    text-shadow: none;
    padding: 4px 8px;
  }
  .qe.ftbq :global(.fmt-bar button:last-child) {
    border-right: none;
  }
  .qe.ftbq :global(.fmt-bar button:hover:not(:disabled)) {
    background: var(--bg-hover, var(--ftbq-btn-hover-top));
    color: var(--ftbq-accent-green);
  }
  .qe.ftbq :global(button.ghost),
  .qe.ftbq :global(button.ico) {
    padding: 4px 10px;
    border: 1px solid var(--ftbq-frame);
    background: var(--bg-secondary, var(--ftbq-bg-panel));
    box-shadow: none;
    color: var(--ftbq-text);
    text-shadow: none;
  }
  .qe.ftbq :global(button.ghost:hover:not(:disabled)),
  .qe.ftbq :global(button.ico:hover:not(:disabled)) {
    border-color: var(--ftbq-frame);
    background: var(--bg-hover, var(--ftbq-btn-hover-top));
    color: var(--ftbq-text);
  }
  .qe.ftbq :global(button.ghost:active:not(:disabled)),
  .qe.ftbq :global(button.ico:active:not(:disabled)) {
    background: var(--bg-active, var(--ftbq-btn-hover-bottom));
    box-shadow: none;
  }
  .qe.ftbq :global(button.primary),
  .qe.ftbq :global(.qe-actions > button:not(.ghost)) {
    padding: 6px 12px;
    border: 1px solid color-mix(in srgb, var(--accent-primary) 45%, var(--ftbq-frame));
    background: var(--accent-primary);
    box-shadow: none;
    color: #fff;
    text-shadow: none;
  }
  .qe.ftbq :global(button.primary:hover:not(:disabled)),
  .qe.ftbq :global(.qe-actions > button:not(.ghost):hover:not(:disabled)) {
    filter: none;
    background: var(--accent-hover, var(--accent-primary));
  }
  .qe.ftbq :global(button:disabled) {
    opacity: 0.5;
  }
  .qe.ftbq :global(input),
  .qe.ftbq :global(select),
  .qe.ftbq :global(textarea) {
    border-radius: var(--ftbq-radius-control);
    border: 1px solid var(--ftbq-frame);
    background: var(--ftbq-input-bg);
    box-shadow: none;
    color: var(--ftbq-text);
    min-width: 0;
    outline: none;
    color-scheme: inherit;
    transition:
      border-color 0.12s ease,
      box-shadow 0.12s ease;
  }
  .qe.ftbq :global(input:focus),
  .qe.ftbq :global(select:focus),
  .qe.ftbq :global(textarea:focus) {
    outline: none;
    border-color: var(--ftbq-focus-border);
    box-shadow: 0 0 0 2px var(--ftbq-focus-ring);
  }
  .qe-tb,
  .qe-title,
  .qe-actions {
    /* flex/align/gap moved to Tailwind utilities on elements */
    position: relative;
  }
  .tb-pop {
    position: relative;
  }
  .tb-btn-group {
    display: inline-flex;
    align-items: center;
    flex-shrink: 0;
    border: 1px solid var(--ftbq-frame);
    border-radius: var(--ftbq-radius-control);
    background: var(--bg-secondary, var(--ftbq-bg-panel));
  }
  .qe.ftbq .tb-btn-group :global(button) {
    border: none;
    border-left: 1px solid var(--ftbq-frame);
    border-radius: 0;
    background: transparent;
    padding: 5px 8px;
    color: var(--ftbq-text-muted);
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
  }
  .qe.ftbq .tb-btn-group :global(button:first-child) {
    border-left: none;
  }
  .qe.ftbq .tb-btn-group :global(button:hover:not(:disabled)) {
    background: var(--bg-hover, var(--ftbq-btn-hover-top));
    color: var(--ftbq-text);
  }
  .qe.ftbq .tb-btn-group :global(button.active) {
    background: var(--bg-hover, var(--ftbq-btn-hover-top));
    color: var(--text-primary, var(--ftbq-text));
  }
  .menu-locale {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    font-size: 12px;
    font-weight: 600;
    color: var(--ftbq-text-muted);
  }
  .menu-locale select {
    flex: 1;
    font-size: 11px;
    padding: 4px 6px;
  }
  .menu-sep {
    height: 1px;
    background: var(--ftbq-frame);
    margin: 4px 0;
  }
  .locale-select {
    max-width: 110px;
    font-size: 11px;
    padding: 4px 6px;
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
    background: var(--bg-secondary, var(--ftbq-bg-panel));
    border: 1px solid var(--ftbq-frame);
    border-radius: var(--ftbq-radius-panel);
    box-shadow: var(--shadow-md, 0 4px 12px rgba(0, 0, 0, 0.12));
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
    border-radius: var(--ftbq-radius-control);
    background: transparent;
    color: var(--ftbq-text);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }
  .book-menu button:hover,
  .book-menu button.active {
    background: var(--bg-hover, color-mix(in srgb, var(--ftbq-accent-teal) 12%, transparent));
    color: var(--text-primary, var(--ftbq-text));
  }
  .dot-mini {
    color: var(--ftbq-quest-started);
    margin-left: 4px;
    font-size: 10px;
  }
  .drawer-h {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .drawer-h strong {
    flex: 1;
  }
  .sheet-h {
    padding: 12px 16px 0;
  }
  .qe-sheet-backdrop {
    position: fixed;
    inset: 0;
    z-index: 90;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(0, 0, 0, 0.55);
    padding: 16px;
  }
  .qe-sheet {
    width: min(720px, 96vw);
    max-height: min(88vh, 900px);
    overflow: auto;
    display: flex;
    flex-direction: column;
    background: var(--ftbq-bg-panel);
    border: 1px solid var(--ftbq-frame);
    border-radius: var(--ftbq-radius-sheet);
    box-shadow: 0 18px 48px rgba(0, 0, 0, 0.45);
  }
  .qe-sheet-wide {
    width: min(1100px, 96vw);
  }
  .issues-wrap {
    position: relative;
  }
  .issues-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    border: 1px solid var(--ftbq-frame);
    background: var(--bg-secondary, var(--ftbq-bg-panel));
    box-shadow: none;
    color: var(--ftbq-quest-completed);
    border-radius: 999px;
    padding: 3px 10px;
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    text-shadow: none;
  }
  .issues-btn.warn {
    color: #fbbf24;
  }
  .issues-pop {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    z-index: 30;
    min-width: 320px;
    max-height: min(80vh, 480px);
    overflow: auto;
    background: var(--ftbq-bg-panel);
    border: 1px solid var(--ftbq-frame);
    border-radius: var(--ftbq-radius-panel);
    box-shadow:
      inset 0 0 0 1px rgba(255, 255, 255, 0.06),
      0 10px 24px rgba(0, 0, 0, 0.5);
  }
  .issue-row {
    display: flex;
    gap: 8px;
    width: 100%;
    text-align: left;
    padding: 9px 12px;
    border: none;
    border-bottom: 1px solid var(--ftbq-border);
    background: transparent;
    color: var(--ftbq-text);
    font-size: 12px;
    line-height: 1.45;
    cursor: pointer;
  }
  .issue-row:hover {
    background: rgba(61, 184, 168, 0.1);
  }
  .issue-row.action {
    color: var(--ftbq-text);
    display: inline-flex;
    align-items: center;
    gap: 6px;
    border-bottom: none;
  }
  .issue-row.action:hover {
    color: var(--ftbq-accent-teal);
  }
  .issues-ok {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 10px;
    font-size: 11px;
    color: var(--ftbq-quest-completed);
    border-bottom: 1px solid var(--ftbq-border);
  }
  .issues-pop-sep {
    height: 1px;
    background: var(--ftbq-frame);
    margin: 0;
  }
  .qe-tb {
    /* layout (justify/shrink/padding/sizes) moved to Tailwind utilities */
    background: var(--bg-secondary, var(--ftbq-bg-panel));
    border: 1px solid var(--ftbq-frame);
    border-radius: var(--ftbq-radius-panel);
    box-shadow: none;
  }
  .apply-save-banner {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    padding: 8px 12px;
    margin-bottom: 6px;
    font-size: 12px;
    color: var(--text-primary, var(--ftbq-text));
    background: color-mix(in srgb, var(--ftbq-accent-teal) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--ftbq-accent-teal) 40%, var(--ftbq-frame));
    border-radius: var(--ftbq-radius-panel);
  }
  .apply-save-banner .mini {
    padding: 4px 10px;
    font-size: 11px;
  }
  .qe-title {
    /* layout moved to Tailwind utilities */
    color: var(--text-muted, var(--ftbq-text-muted));
    font-weight: 500;
    font-size: 13px;
    letter-spacing: 0;
  }
  .qe-title .book-name {
    color: var(--text-primary, var(--ftbq-text));
    font-weight: 650;
    text-shadow: none;
  }
  .tb-chip {
    font-size: 11px;
    font-weight: 500;
    padding: 3px 9px;
    border-radius: 999px;
    border: 1px solid var(--ftbq-frame);
    color: var(--ftbq-text-muted);
    background: color-mix(in srgb, var(--ftbq-bg) 70%, transparent);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .prog-stat {
    color: var(--ftbq-quest-completed);
  }
  .dirty-badge {
    font-size: 10px;
    color: var(--accent-warning);
    padding: 3px 8px;
    border-radius: var(--ftbq-radius-control);
    background: color-mix(in srgb, var(--accent-warning) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent-warning) 30%, transparent);
    text-shadow: none;
    animation: none;
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
    border-radius: var(--ftbq-radius-panel);
    margin-bottom: 8px;
    border: 1px solid var(--ftbq-border);
    flex-shrink: 0;
    font-size: 12px;
    text-shadow: none;
  }
  .notice-text {
    flex: 1;
    min-width: 0;
  }
  .notice-dismiss {
    flex-shrink: 0;
    opacity: 0.8;
  }
  .notice-dismiss:hover {
    opacity: 1;
  }
  .notice.error {
    color: #fecaca;
    background: rgba(239, 68, 68, 0.1);
    border-color: rgba(239, 68, 68, 0.35);
  }
  .notice.success {
    color: var(--ftbq-quest-completed);
    background: rgba(85, 201, 90, 0.1);
    border-color: rgba(85, 201, 90, 0.3);
  }
  .empty {
    color: var(--ftbq-text-muted);
    padding: 56px 32px 44px;
    text-align: center;
    background:
      radial-gradient(
        ellipse 70% 46% at 50% 0%,
        color-mix(in srgb, var(--accent-secondary) 7%, transparent),
        transparent
      ),
      var(--ftbq-bg-panel);
    border: 1px solid var(--ftbq-frame);
    border-radius: var(--ftbq-radius-panel);
    box-shadow:
      inset 0 0 0 1px rgba(255, 255, 255, 0.05),
      inset 0 0 48px rgba(0, 0, 0, 0.25);
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    max-width: 560px;
    margin: 0 auto;
  }
  /* Mini-diagram: a chapter node with a line and three quest tiles branching
     off it — communicates "chapters hold quests" at a glance. */
  .empty-art {
    position: relative;
    width: 168px;
    height: 64px;
    margin-bottom: 8px;
  }
  .art-chapter {
    position: absolute;
    left: 0;
    top: 50%;
    transform: translateY(-50%);
    display: flex;
    align-items: center;
    gap: 0;
  }
  .art-chapter-dot {
    width: 34px;
    height: 34px;
    border-radius: var(--ftbq-radius-control);
    background: color-mix(in srgb, var(--accent-secondary) 26%, var(--ftbq-bg-panel));
    border: 1.5px solid color-mix(in srgb, var(--accent-secondary) 60%, transparent);
    box-shadow: 0 0 14px color-mix(in srgb, var(--accent-secondary) 25%, transparent);
  }
  .art-line {
    width: 44px;
    height: 2px;
    background: linear-gradient(
      90deg,
      color-mix(in srgb, var(--accent-secondary) 60%, transparent),
      color-mix(in srgb, var(--ftbq-frame) 80%, transparent)
    );
  }
  .art-quest {
    position: absolute;
    width: 26px;
    height: 26px;
    border-radius: var(--ftbq-radius-control);
    background: color-mix(in srgb, var(--ftbq-frame) 55%, transparent);
    border: 1px solid var(--ftbq-frame);
  }
  .art-quest.q1 { left: 102px; top: 4px; }
  .art-quest.q2 { left: 122px; top: 26px; opacity: 0.75; }
  .art-quest.q3 { left: 102px; top: 46px; opacity: 0.55; }
  .empty h3 {
    margin: 0;
    color: var(--text-primary, var(--ftbq-text));
    font-size: 17px;
    font-weight: 700;
    letter-spacing: 0.01em;
    text-shadow: none;
  }
  .empty p {
    margin: 0;
    max-width: 400px;
    font-size: 13px;
    line-height: 1.55;
  }
  .empty p strong {
    color: var(--text-primary, var(--ftbq-text));
    font-weight: 600;
  }
  .empty-ctas {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 10px;
  }
  .empty-cta {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 11px 20px;
    border: 1px solid color-mix(in srgb, var(--accent-secondary) 55%, transparent);
    border-radius: var(--ftbq-radius-control);
    background:
      linear-gradient(
        180deg,
        color-mix(in srgb, var(--accent-secondary) 88%, #fff 6%),
        var(--accent-secondary)
      );
    box-shadow:
      0 4px 14px color-mix(in srgb, var(--accent-secondary) 28%, transparent),
      inset 0 1px 0 color-mix(in srgb, #fff 22%, transparent);
    color: #fff;
    text-shadow: none;
    font-size: 13px;
    font-weight: 700;
    letter-spacing: 0.01em;
    cursor: pointer;
    transition:
      filter 0.15s ease,
      transform 0.15s ease,
      box-shadow 0.15s ease;
  }
  .empty-cta:hover {
    filter: brightness(1.1);
    transform: translateY(-1px);
    box-shadow:
      0 6px 18px color-mix(in srgb, var(--accent-secondary) 36%, transparent),
      inset 0 1px 0 color-mix(in srgb, #fff 22%, transparent);
  }
  .empty-cta:active {
    transform: translateY(0);
    filter: brightness(0.98);
  }
  .empty-cta:focus-visible {
    outline: 2px solid color-mix(in srgb, var(--accent-secondary) 70%, transparent);
    outline-offset: 2px;
  }
  .empty-hint {
    font-size: 11.5px !important;
    color: color-mix(in srgb, var(--ftbq-text-muted) 75%, transparent) !important;
  }
  .qe-lay {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: var(--qe-rail, 200px) 4px 1fr;
    gap: 0;
    border: 1px solid var(--ftbq-frame);
    border-radius: var(--ftbq-radius-panel);
    overflow: hidden;
    background: var(--ftbq-bg-canvas);
    box-shadow:
      inset 0 0 0 1px rgba(255, 255, 255, 0.05),
      0 2px 8px rgba(0, 0, 0, 0.4);
    min-width: 0;
  }
  .qe-lay.with-insp {
    grid-template-columns: var(--qe-rail, 200px) 4px 1fr 4px var(--qe-insp, 300px);
  }
  .col-resizer {
    width: 4px;
    margin: 0;
    padding: 0;
    border: none;
    cursor: col-resize;
    background: var(--ftbq-frame);
    position: relative;
    z-index: 2;
    touch-action: none;
  }
  .col-resizer::after {
    content: "";
    position: absolute;
    inset: 0 -3px;
  }
  .col-resizer:hover,
  .col-resizer:active {
    background: var(--ftbq-accent-teal);
  }
  .qe-body-row .qe-lay {
    flex: 1;
    min-height: 0;
  }
  .filt-count {
    flex: 1;
    padding: 5px 8px;
    font-size: 12px;
    color: var(--ftbq-text-muted);
    transition: border-color 0.12s ease, box-shadow 0.12s ease;
  }
  .search-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 12px;
    border-bottom: 1px solid var(--ftbq-frame);
    background: var(--ftbq-bg-panel);
    flex-shrink: 0;
  }
  .search-bar input {
    flex: 1;
    padding: 5px 8px;
    font-size: 12px;
  }
  .search-bar input:focus {
    border-color: var(--ftbq-focus-border);
    box-shadow: 0 0 0 2px var(--ftbq-focus-ring);
    outline: none;
  }
  .search-panel {
    flex-shrink: 0;
    color: inherit;
    font-size: 11px;
    cursor: pointer;
    transition: background 0.12s ease, border-color 0.12s ease;
  }
  .search-panel .search-bar {
    border-bottom: none;
  }
  .search-empty {
    margin: 0;
    padding: 8px 12px 10px;
    font-size: 11px;
    color: var(--ftbq-text-muted);
  }
  .search-empty code {
    font-size: 10px;
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
    border-radius: var(--ftbq-radius-control);
    background: rgba(0, 0, 0, 0.2);
    color: inherit;
    font-size: 11px;
    cursor: pointer;
  }
  .search-hit:hover,
  .search-hit.active {
    border-color: var(--ftbq-accent-teal);
    background: rgba(61, 184, 168, 0.12);
  }
  .search-hit:focus-visible {
    outline: 2px solid var(--ftbq-accent-teal);
    outline-offset: -2px;
  }
  .hit-ch {
    color: var(--ftbq-title-gold);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    letter-spacing: 0.01em;
    cursor: pointer;
    transition: background 0.12s ease, color 0.12s ease;
  }
  .hit-field {
    color: var(--ftbq-text-muted);
    font-size: 10px;
    letter-spacing: 0.02em;
  }
  .hit-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .search-more {
    font-size: 11px;
    color: var(--ftbq-text-muted);
    padding: 4px 8px;
  }
  .side-panel {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    background: var(--ftbq-bg-panel);
    border-left: 1px solid var(--ftbq-frame);
  }
  .panel-tabs {
    display: flex;
    flex-shrink: 0;
    gap: 1px;
    padding: 4px 4px 0;
    border-bottom: 1px solid var(--ftbq-frame);
    background: var(--bg-tertiary, var(--ftbq-bg));
  }
  .panel-tabs .tab {
    flex: 1;
    min-width: 0;
    padding: 8px 2px;
    border: 1px solid transparent;
    border-bottom: none;
    border-radius: var(--ftbq-radius-control) var(--ftbq-radius-control) 0 0;
    background: transparent;
    color: var(--text-muted, var(--ftbq-text-muted));
    font-size: 12px;
    font-weight: 500;
    letter-spacing: 0.01em;
    cursor: pointer;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    transition: color 0.12s ease, background 0.12s ease;
  }
  .panel-tabs .tab:last-child {
    border-right: none;
    margin-bottom: -1px;
    box-shadow: none;
  }
  .panel-tabs .tab:focus-visible {
    outline: 2px solid var(--ftbq-accent-teal);
    outline-offset: -2px;
  }
  .panel-tabs .tab:hover {
    color: var(--text-secondary, var(--ftbq-text));
    background: color-mix(in srgb, var(--bg-hover, var(--ftbq-btn-hover-top)) 55%, transparent);
  }
  .panel-tabs .tab.active {
    color: var(--text-primary, var(--ftbq-text));
    background: var(--bg-secondary, var(--ftbq-bg-panel));
    border-color: var(--ftbq-frame);
    border-bottom-color: var(--bg-secondary, var(--ftbq-bg-panel));
    margin-bottom: -1px;
    box-shadow: none;
  }
  .panel-content {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--ftbq-bg-panel);
  }
  .sel-hint {
    margin: 0;
    padding: 8px 12px 12px;
    font-size: 12px;
    line-height: 1.5;
    color: var(--ftbq-text-muted);
    border-top: 1px solid var(--ftbq-frame);
  }
  .sel-empty {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
    padding: 16px 12px;
  }
  .sel-empty .sel-hint {
    border-top: none;
    padding: 0;
    margin: 0;
  }
  .qe-actions .active {
    color: var(--text-primary, var(--ftbq-text));
    border-color: var(--ftbq-focus-border);
    background: var(--bg-hover, var(--ftbq-btn-hover-top));
  }
  .qe-actions button:focus-visible {
    outline: 2px solid var(--ftbq-accent-teal);
    outline-offset: 1px;
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
    .qe-lay {
      grid-template-columns: minmax(140px, 160px) 4px 1fr;
    }
    .qe-lay.with-insp {
      grid-template-columns: minmax(140px, 160px) 4px 1fr 4px minmax(220px, 260px);
    }
  }
</style>
