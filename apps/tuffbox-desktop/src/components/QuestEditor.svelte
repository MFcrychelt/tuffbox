<script lang="ts">
  import { api, type QuestChapter, type QuestChapterGroup, type QuestData, type QuestValidationIssue, type QuestProgressTeamRef, type QuestProgressSnapshot, type QuestProgressStatus, type QuestPlanMergeResult } from "../lib/api";
  import { ScrollText, RefreshCw, Save, AlertTriangle, CheckCircle2, Map, Eye, Sparkles, X } from "@lucide/svelte";
  import { onDestroy } from "svelte";
  import { projectPath, questDirty, questChatFocusId } from "../lib/store";
  import EmptyState from "./EmptyState.svelte";
  import ChapterRail from "./quests/ChapterRail.svelte";
  import QuestCanvas from "./quests/QuestCanvas.svelte";
  import QuestInspector from "./quests/QuestInspector.svelte";
  import ChapterSettings from "./quests/ChapterSettings.svelte";
  import RewardTablesPanel from "./quests/RewardTablesPanel.svelte";
  import QuestAiSidebar from "./quests/QuestAiSidebar.svelte";
  import { wouldCreateQuestCycle } from "./quests/deps";
  import type { QuestRewardTable } from "../lib/api";

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
  let bookMenuOpen = $state(false);
  let issuesOpen = $state(false);
  let progressOpen = $state(false);

  // Phase C — player progress overlay (read-only)
  let progressTeams = $state<QuestProgressTeamRef[]>([]);
  let progressKey = $state(""); // relativePath
  let progressOverlay = $state(false);
  let progressSnap = $state<QuestProgressSnapshot | null>(null);
  let progressLoading = $state(false);

  let aiSidebarOpen = $state(readAiSidebarPref());
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
      chapters = book.chapters ?? [];
      chapterGroups = book.chapterGroups ?? [];
      bookTitle = book.title ?? null;
      bookSubtitle = book.subtitle ?? null;
      bookSettings = book.bookSettings ?? {};
      bookDirty = false;
      groupsDirty = false;
      rewardTables = (book.rewardTables ?? []).map((t) => ({
        ...t,
        entries: t.entries ?? [],
        emptyWeight: t.emptyWeight ?? 0,
      }));
      rewardTablesDirty = false;
      dirtyChapters = new Set();
      if (chapters.length > 0 && !chapters.some((c) => c.id === selectedChapter)) {
        selectedChapter = chapters[0].id;
      }
      selectedQuest = null;
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
      progressSnap = null;
      return;
    }
    progressLoading = true;
    try {
      progressSnap = await api.quests.loadProgress(progressKey, $projectPath);
      progressOverlay = true;
    } catch (e) {
      error = String(e);
      progressSnap = null;
    } finally {
      progressLoading = false;
    }
  }

  function inputVal(e: Event): string {
    return (e.currentTarget as HTMLInputElement).value;
  }

  const progressStatuses = $derived((progressSnap?.statuses ?? {}) as Record<string, QuestProgressStatus>);
  const progressTeamLabel = $derived(progressTeams.find((t) => t.relativePath === progressKey));

  async function saveChapter(chapterId: string) {
    if (!$projectPath) return;
    const ch = chapters.find((c) => c.id === chapterId);
    if (!ch) return;
    saving = true;
    error = null;
    message = null;
    try {
      const result = await api.quests.saveChapter(ch, ch.sourceFile, $projectPath);
      ch.sourceFile = result.relativePath;
      dirtyChapters = new Set([...dirtyChapters].filter((id) => id !== chapterId));
      chapters = [...chapters];
      message = `Saved ${result.questCount} quests → ${result.relativePath}`;
      validationIssues = await api.quests.validate($projectPath);
    } catch (e) {
      error = String(e);
    } finally {
      saving = false;
    }
  }

  async function saveAll() {
    for (const id of [...dirtyChapters]) {
      await saveChapter(id);
    }
    if (rewardTablesDirty) {
      for (const t of rewardTables) {
        await saveRewardTable(t);
      }
    }
    if (bookDirty) await saveBookData();
    if (groupsDirty) await saveGroups();
  }

  function markDirty(chapterId: string) {
    dirtyChapters = new Set([...dirtyChapters, chapterId]);
    chapters = [...chapters];
  }

  function selectChapter(id: string) {
    selectedChapter = id;
    selectedQuest = null;
    fitToken += 1;
  }

  function createChapter() {
    const n: QuestChapter = {
      id: `chapter_${Date.now().toString(16)}`,
      title: `Chapter ${chapters.length + 1}`,
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
    const ch = chapters.find((c) => c.id === selectedChapter);
    if (!ch) return;
    const newQ: QuestData = {
      id: crypto.randomUUID().replace(/-/g, "").slice(0, 16).toUpperCase(),
      title: "New Quest",
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
  }

  function removeQuest(q: QuestData) {
    if (!confirm(`Delete quest "${q.title}" from this chapter?`)) return;
    const ch = chapters.find((c) => c.id === selectedChapter);
    if (!ch) return;
    ch.quests = ch.quests.filter((x) => x.id !== q.id);
    for (const other of ch.quests) {
      if (other.dependencies.includes(q.id)) {
        other.dependencies = other.dependencies.filter((d) => d !== q.id);
      }
    }
    markDirty(selectedChapter);
    if (selectedQuest?.id === q.id) selectedQuest = null;
  }

  function moveQuest(q: QuestData, x: number, y: number) {
    q.x = x;
    q.y = y;
    markDirty(selectedChapter);
    if (selectedQuest?.id === q.id) selectedQuest = q;
  }

  /** Would adding depId as a dependency of questId create a cycle? */
  function wouldCycle(questId: string, depId: string, list: QuestData[]): boolean {
    return wouldCreateQuestCycle(questId, depId, list);
  }

  function addDep(q: QuestData, depId: string) {
    if (!depId || q.dependencies.includes(depId) || depId === q.id) return;
    const list = chapters.find((c) => c.id === selectedChapter)?.quests ?? [];
    if (wouldCycle(q.id, depId, list)) {
      error = "That dependency would create a cycle.";
      return;
    }
    error = null;
    q.dependencies = [...q.dependencies, depId];
    markDirty(selectedChapter);
    selectedQuest = q;
  }

  function removeDep(q: QuestData, depId: string) {
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
    const t: QuestRewardTable = {
      id: `table_${Date.now().toString(16)}`,
      title: `Table ${rewardTables.length + 1}`,
      entries: [],
      emptyWeight: 0,
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
    return s.replace(/§[0-9a-fk-or]/gi, "").replace(/&[0-9a-fk-or]/gi, "").trim();
  }

  function applyMergeResult(result: QuestPlanMergeResult) {
    if (!result.validation?.valid) {
      error = (result.validation?.errors ?? []).slice(0, 3).join("; ") || "Plan invalid";
      return;
    }
    const b = result.book;
    chapters = b.chapters ?? [];
    chapterGroups = b.chapterGroups ?? chapterGroups;
    bookTitle = b.title ?? bookTitle;
    bookSubtitle = b.subtitle ?? bookSubtitle;
    if (b.rewardTables?.length) {
      rewardTables = b.rewardTables.map((t) => ({
        ...t,
        entries: t.entries ?? [],
        emptyWeight: t.emptyWeight ?? 0,
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
    if (chapters.length && !chapters.some((c) => c.id === selectedChapter)) {
      selectedChapter = chapters[0].id;
    }
    selectedQuest = null;
    validationIssues = (result.validation.bookErrors ?? []).map((e) => ({
      questId: e.questId,
      message: e.message,
    }));
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
    const id = Math.random().toString(16).slice(2, 10).toUpperCase();
    chapterGroups = [...chapterGroups, { id, title: `Group ${chapterGroups.length + 1}` }];
    groupsDirty = true;
  }

  function removeChapterGroup(id: string) {
    chapterGroups = chapterGroups.filter((g) => g.id !== id);
    for (const ch of chapters) {
      if (ch.group === id) {
        ch.group = null;
        markDirty(ch.id);
      }
    }
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
    chapters = chapters.filter((c) => c.id !== id);
    dirtyChapters = new Set([...dirtyChapters].filter((x) => x !== id));
    if (selectedChapter === id) {
      selectedChapter = chapters[0]?.id ?? "";
      selectedQuest = null;
    }
  }

  function moveChapter(id: string, dir: -1 | 1) {
    const idx = chapters.findIndex((c) => c.id === id);
    if (idx < 0) return;
    const j = idx + dir;
    if (j < 0 || j >= chapters.length) return;
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
        fitToken += 1;
      }
    }
  }

  function handleKeydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
      e.preventDefault();
      if (hasDirty && !saving) void saveAll();
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

  const hasDirty = $derived(dirtyChapters.size > 0 || rewardTablesDirty || bookDirty || groupsDirty);
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
      <div class="tb-pop">
        <button
          type="button"
          class="ghost"
          class:active={bookMenuOpen || showBookPanel || showGroupsPanel || showTablesPanel}
          class:has-dirty={bookDirty || groupsDirty || rewardTablesDirty}
          title="Book, groups, reward tables"
          onclick={() => {
            bookMenuOpen = !bookMenuOpen;
            if (!bookMenuOpen) {
              showBookPanel = false;
              showGroupsPanel = false;
              showTablesPanel = false;
            }
          }}
        >
          Book{#if bookDirty || groupsDirty || rewardTablesDirty}<span class="dot-mini">●</span>{/if}
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
              }}
            >
              Reward tables{#if rewardTablesDirty}<span class="dot-mini">●</span>{/if}
            </button>
          </div>
        {/if}
        {#if showBookPanel && $projectPath}
          <div class="drawer">
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
                  bookTitle = inputVal(e);
                  bookDirty = true;
                }}
              /></label
            >
            <label
              >Subtitle<input
                value={bookSubtitle ?? ""}
                oninput={(e) => {
                  bookSubtitle = inputVal(e);
                  bookDirty = true;
                }}
              /></label
            >
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
            {#each chapterGroups as g (g.id)}
              <div class="group-row">
                <code>{g.id}</code>
                <input bind:value={g.title} oninput={() => (groupsDirty = true)} />
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
              onChange={() => {
                rewardTablesDirty = true;
                rewardTables = [...rewardTables];
              }}
              onSave={saveRewardTable}
              onCreate={createRewardTable}
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
      {#if hasDirty}
        <span class="dirty-badge"
          >{dirtyChapters.size +
            (rewardTablesDirty ? 1 : 0) +
            (bookDirty ? 1 : 0) +
            (groupsDirty ? 1 : 0)} unsaved</span
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
        <label class="prog-toggle">
          <input
            type="checkbox"
            bind:checked={progressOverlay}
            disabled={!progressSnap}
            title="Show player progress on canvas"
          />
          Overlay
        </label>
        <select
          bind:value={progressKey}
          onchange={loadProgress}
          disabled={progressLoading || progressTeams.length === 0}
        >
          <option value="">
            {progressTeams.length === 0 ? "No saves/*/ftbquests progress" : "Select team / player…"}
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
        {#if progressTeamLabel}
          <code class="prog-path">{progressTeamLabel.relativePath}</code>
        {/if}
      </div>
    </details>
  {/if}

  {#if error}<div class="notice error"><AlertTriangle size={14} /> {error}</div>{/if}
  {#if message}<div class="notice success"><CheckCircle2 size={14} /> {message}</div>{/if}

  {#if !$projectPath}
    <EmptyState icon={Map} title="No project selected" description="Open a project to edit FTB Quests chapters." />
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
    <div class="qe-lay" class:with-insp={!!selectedQuest || !!selectedChapterObj}>
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
        </div>
        <QuestCanvas
          quests={filteredChapterQuests}
          selectedId={selectedQuest?.id ?? null}
          issues={validationIssues}
          {fitToken}
          {progressOverlay}
          {progressStatuses}
          emptyHint={questSearch.trim()
            ? `No quests match “${questSearch.trim()}”`
            : "Double-click to add a quest"}
          onSelect={(q) => (selectedQuest = q)}
          onMove={moveQuest}
          onAddAt={addQuestAt}
          onLink={linkQuests}
        />
      </div>
      {#if selectedQuest}
        <QuestInspector
          quest={selectedQuest}
          {chapterQuests}
          issues={validationIssues}
          {rewardTableIds}
          onDirty={() => markDirty(selectedChapter)}
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
      {:else if selectedChapterObj}
        <ChapterSettings
          chapter={selectedChapterObj}
          {chapterGroups}
          onDirty={() => markDirty(selectedChapter)}
        />
      {/if}
    </div>
    {#if aiSidebarOpen}
      <QuestAiSidebar
        open={aiSidebarOpen}
        onclose={() => setAiSidebar(false)}
        onapply={applyMergeResult}
      />
    {/if}
    </div>
    <div class="qe-footer">
      <p class="hint">
        Edits save as SNBT to <code>config/ftbquests/quests/chapters/</code>. Auto-snapshot on save. AI merge is memory-only until Save.
      </p>
    </div>
  {/if}
</div>

<style>
  .qe.ftbq {
    --ftbq-bg: #1a1a1e;
    --ftbq-bg-panel: #212126;
    --ftbq-bg-canvas: #2b2b30;
    --ftbq-border: #3a3a42;
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
    background: var(--ftbq-bg, #1a1a1e);
    color: var(--ftbq-text, #e8e8e8);
  }
  /* Isolate from global TuffBox green primary buttons */
  .qe.ftbq :global(button) {
    border-radius: 2px;
    font-weight: 600;
    box-shadow: none;
  }
  .qe.ftbq :global(button.ghost),
  .qe.ftbq :global(button.ico) {
    padding: 4px 10px;
    border: 1px solid var(--ftbq-border, #3a3a42);
    background: rgba(0, 0, 0, 0.25);
    color: var(--ftbq-text, #e8e8e8);
  }
  .qe.ftbq :global(button.ghost:hover:not(:disabled)),
  .qe.ftbq :global(button.ico:hover:not(:disabled)) {
    border-color: var(--ftbq-accent-green, #55c95a);
    background: rgba(85, 201, 90, 0.1);
    color: var(--ftbq-text, #e8e8e8);
  }
  .qe.ftbq :global(button.primary),
  .qe.ftbq :global(.qe-actions > button:not(.ghost)) {
    padding: 6px 12px;
    border: 1px solid var(--ftbq-accent-green, #55c95a);
    background: rgba(85, 201, 90, 0.18);
    color: var(--ftbq-quest-completed, #55c95a);
  }
  .qe.ftbq :global(button.primary:hover:not(:disabled)),
  .qe.ftbq :global(.qe-actions > button:not(.ghost):hover:not(:disabled)) {
    background: rgba(85, 201, 90, 0.28);
  }
  .qe.ftbq :global(button:disabled) {
    opacity: 0.5;
  }
  .qe.ftbq :global(input),
  .qe.ftbq :global(select),
  .qe.ftbq :global(textarea) {
    border-radius: 2px;
    border-color: var(--ftbq-border, #3a3a42);
    background: var(--ftbq-bg, #1a1a1e);
    color: var(--ftbq-text, #e8e8e8);
    min-width: 0;
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
    border: 1px solid var(--ftbq-border, #3a3a42);
    border-radius: 2px;
    box-shadow: 0 12px 28px rgba(0, 0, 0, 0.45);
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
    border-radius: 2px;
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
    border: 1px solid var(--ftbq-border, #3a3a42);
    border-radius: 2px;
    box-shadow: 0 12px 28px rgba(0, 0, 0, 0.45);
  }
  .drawer-wide {
    width: 360px;
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
    background: var(--ftbq-bg, #1a1a1e);
    border: 1px solid var(--ftbq-border, #3a3a42);
    color: inherit;
    border-radius: 2px;
    padding: 6px 8px;
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
    border: 1px solid var(--ftbq-border, #3a3a42);
    background: transparent;
    color: var(--ftbq-quest-completed, #55c95a);
    border-radius: 2px;
    padding: 2px 8px;
    font-size: 12px;
    cursor: pointer;
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
    border: 1px solid var(--ftbq-border, #3a3a42);
    border-radius: 2px;
    box-shadow: 0 10px 24px rgba(0, 0, 0, 0.4);
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
    border: 1px solid var(--ftbq-border, #3a3a42);
    border-radius: 2px;
    background: var(--ftbq-bg-panel, #212126);
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
    background: var(--ftbq-bg-panel, #212126);
    border: 1px solid var(--ftbq-border, #3a3a42);
    border-radius: 2px;
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.04);
  }
  .qe-title {
    color: var(--ftbq-text-muted, #9a9aa0);
    font-weight: 700;
    font-size: 13px;
    letter-spacing: 0.02em;
  }
  .qe-title .book-name {
    color: var(--ftbq-title-gold, #f2c94c);
    text-shadow: 0 1px 2px rgba(0, 0, 0, 0.5);
  }
  .qe-stats {
    display: flex;
    gap: 16px;
    margin-bottom: 6px;
    font-size: 11px;
    color: var(--ftbq-text-muted, #9a9aa0);
    flex-shrink: 0;
    flex-wrap: wrap;
    padding: 0 2px;
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
    border-radius: 2px;
    border: 1px solid var(--ftbq-border, #3a3a42);
    background: var(--ftbq-bg-panel, #212126);
    color: var(--ftbq-text-muted, #9a9aa0);
    font-size: 11px;
    flex-shrink: 0;
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
    color: var(--ftbq-quest-started, #f2c94c);
    padding: 3px 8px;
    border-radius: 2px;
    background: rgba(242, 201, 76, 0.12);
    border: 1px solid rgba(242, 201, 76, 0.25);
  }
  .notice {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    border-radius: 2px;
    margin-bottom: 8px;
    border: 1px solid var(--ftbq-border, #3a3a42);
    flex-shrink: 0;
    font-size: 12px;
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
    border: 1px solid var(--ftbq-border, #3a3a42);
    border-radius: 2px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
  }
  .empty h3 {
    margin: 0;
    color: var(--ftbq-title-gold, #f2c94c);
    font-size: 16px;
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
    border: 1px solid var(--ftbq-accent-teal, #3db8a8);
    border-radius: 2px;
    background: rgba(61, 184, 168, 0.14);
    color: var(--ftbq-accent-teal, #3db8a8);
    font-weight: 700;
    cursor: pointer;
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
    border: 1px solid var(--ftbq-border, #3a3a42);
    border-radius: 2px;
    overflow: hidden;
    background: var(--ftbq-bg-canvas, #2b2b30);
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
    border-bottom: 1px solid var(--ftbq-border, #3a3a42);
    background: var(--ftbq-bg-panel, #212126);
  }
  .canvas-tools input {
    flex: 1;
    background: var(--ftbq-bg, #1a1a1e);
    border: 1px solid var(--ftbq-border, #3a3a42);
    color: inherit;
    border-radius: 4px;
    padding: 4px 8px;
    font-size: 12px;
  }
  .filt-count {
    font-size: 11px;
    color: var(--ftbq-text-muted, #9a9aa0);
  }
  .qe-lay.with-insp {
    grid-template-columns: 200px 1fr 280px;
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
