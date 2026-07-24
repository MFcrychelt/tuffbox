<script lang="ts">
  import { api, type QuestChapter, type QuestChapterGroup, type QuestData, type QuestValidationIssue, type QuestProgressTeamRef, type QuestProgressSnapshot, type QuestProgressStatus, type QuestPlanMergeResult } from "../lib/api";
  import { ScrollText, RefreshCw, Save, AlertTriangle, CheckCircle2, Map, Eye, Sparkles } from "lucide-svelte";
  import { projectPath } from "../lib/store";
  import EmptyState from "./EmptyState.svelte";
  import ChapterRail from "./quests/ChapterRail.svelte";
  import QuestCanvas from "./quests/QuestCanvas.svelte";
  import QuestInspector from "./quests/QuestInspector.svelte";
  import ChapterSettings from "./quests/ChapterSettings.svelte";
  import RewardTablesPanel from "./quests/RewardTablesPanel.svelte";
  import { wouldCreateQuestCycle } from "./quests/deps";
  import type { QuestRewardTable } from "../lib/api";

  let chapters: QuestChapter[] = [];
  let chapterGroups: QuestChapterGroup[] = [];
  let bookTitle: string | null = null;
  let bookSettings: Record<string, unknown> = {};
  let rewardTables: QuestRewardTable[] = [];
  let rewardTablesDirty = false;
  let loading = false;
  let saving = false;
  let error: string | null = null;
  let message: string | null = null;
  let selectedChapter = "";
  let selectedQuest: QuestData | null = null;
  let validationIssues: QuestValidationIssue[] = [];
  let dirtyChapters = new Set<string>();
  let lastLoadedPath: string | null = null;
  let fitToken = 0;

  // Phase C — player progress overlay (read-only)
  let progressTeams: QuestProgressTeamRef[] = [];
  let progressKey = ""; // relativePath
  let progressOverlay = false;
  let progressSnap: QuestProgressSnapshot | null = null;
  let progressLoading = false;

  // AI QuestPlan: natural-language prompt → merge preview
  let aiPanelOpen = false;
  let aiPrompt =
    "создай главу 1: начало развития, в ней квесты - 1. добудь 10 дерева, накопай 20 булыги - награда 10 палок.";
  let aiRaw = "";
  let aiShowJson = false;
  let aiForceAi = false;
  let aiMerging = false;
  let aiPreview: QuestPlanMergeResult | null = null;

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
      bookSettings = book.bookSettings ?? {};
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

  $: progressStatuses = (progressSnap?.statuses ?? {}) as Record<string, QuestProgressStatus>;
  $: progressTeamLabel = progressTeams.find((t) => t.relativePath === progressKey);

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
      dirtyChapters.delete(chapterId);
      dirtyChapters = dirtyChapters;
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
  }

  function markDirty(chapterId: string) {
    dirtyChapters.add(chapterId);
    dirtyChapters = dirtyChapters;
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

  $: chapterQuests = chapters.find((c) => c.id === selectedChapter)?.quests ?? [];
  $: selectedChapterObj = chapters.find((c) => c.id === selectedChapter) ?? null;
  $: rewardTableIds = rewardTables.map((t) => t.id);
  $: totalQuests = chapters.reduce((n, c) => n + c.quests.length, 0);
  $: hasDirty = dirtyChapters.size > 0 || rewardTablesDirty;
  $: if ($projectPath && $projectPath !== lastLoadedPath) load();
  $: if (selectedQuest) {
    const fresh = chapterQuests.find((q) => q.id === selectedQuest!.id);
    if (fresh && fresh !== selectedQuest) selectedQuest = fresh;
  }

  /** Strip Minecraft formatting codes for toolbar display. */
  function stripMc(s: string): string {
    return s.replace(/§[0-9a-fk-or]/gi, "").replace(/&[0-9a-fk-or]/gi, "").trim();
  }

  async function previewAiPlan() {
    if (!$projectPath) return;
    aiMerging = true;
    error = null;
    aiPreview = null;
    try {
      if (aiShowJson && aiRaw.trim()) {
        aiPreview = await api.quests.parseAndMergePlan(aiRaw, $projectPath);
      } else if (aiPrompt.trim()) {
        aiPreview = await api.quests.generateFromPrompt(aiPrompt, aiForceAi, $projectPath);
      } else {
        error = "Напиши запрос или вставь QuestPlan JSON.";
        return;
      }
      if (!aiPreview.validation.valid) {
        error = aiPreview.validation.errors.slice(0, 3).join("; ");
      }
    } catch (e) {
      error = String(e);
    } finally {
      aiMerging = false;
    }
  }

  function applyAiPreview() {
    if (!aiPreview || !aiPreview.validation.valid) return;
    const b = aiPreview.book;
    chapters = b.chapters ?? [];
    chapterGroups = b.chapterGroups ?? chapterGroups;
    bookTitle = b.title ?? bookTitle;
    if (b.rewardTables?.length) {
      rewardTables = b.rewardTables.map((t) => ({
        ...t,
        entries: t.entries ?? [],
        emptyWeight: t.emptyWeight ?? 0,
      }));
    }
    const dirty = new Set(dirtyChapters);
    for (const id of aiPreview.touchedChapterIds ?? []) dirty.add(id);
    dirtyChapters = dirty;
    if (chapters.length && !chapters.some((c) => c.id === selectedChapter)) {
      selectedChapter = chapters[0].id;
    }
    selectedQuest = null;
    validationIssues = (aiPreview.validation.bookErrors ?? []).map((e) => ({
      questId: e.questId,
      message: e.message,
    }));
    message = `AI plan applied in editor (${aiPreview.touchedChapterIds.length} chapter(s)). Save to write SNBT.`;
    aiPanelOpen = false;
    aiPreview = null;
    fitToken += 1;
  }
</script>

<div class="qe ftbq">
  <div class="qe-tb">
    <div class="qe-title">
      <ScrollText size={18} />
      {#if bookTitle}<span class="book-name">{stripMc(bookTitle)}</span>{:else}Quest editor{/if}
    </div>
    <div class="qe-actions">
      <button
        type="button"
        class="ghost"
        class:active={aiPanelOpen}
        title="Paste AI QuestPlan JSON"
        on:click={() => (aiPanelOpen = !aiPanelOpen)}
      >
        <Sparkles size={16} /> Создать
      </button>
      {#if hasDirty}
        <span class="dirty-badge"
          >{dirtyChapters.size + (rewardTablesDirty ? 1 : 0)} unsaved</span
        >
        <button type="button" on:click={saveAll} disabled={!$projectPath || saving}>
          <Save size={16} /> {saving ? "Saving…" : "Save all"}
        </button>
      {/if}
      <button type="button" class="ghost" on:click={load} disabled={!$projectPath || loading}>
        <RefreshCw size={16} class={loading ? "spin" : ""} />
      </button>
    </div>
  </div>

  {#if aiPanelOpen && $projectPath}
    <div class="ai-panel">
      <div class="ai-h">
        <strong>Создать квесты</strong>
        <span>Опиши главу обычным текстом → Сгенерировать → Применить → Save</span>
      </div>
      <textarea
        class="ai-raw"
        rows="3"
        placeholder="создай главу 1: начало развития, в ней квесты - 1. добудь 10 дерева, накопай 20 булыги - награда 10 палок."
        bind:value={aiPrompt}
      ></textarea>
      <label class="ai-opt">
        <input type="checkbox" bind:checked={aiForceAi} />
        Всегда через нейросеть (не эвристику)
      </label>
      <label class="ai-opt">
        <input type="checkbox" bind:checked={aiShowJson} />
        Вставить готовый QuestPlan JSON
      </label>
      {#if aiShowJson}
        <textarea class="ai-raw" rows="6" placeholder={'{ "schemaVersion": 1, … }'} bind:value={aiRaw}
        ></textarea>
      {/if}
      <div class="ai-actions">
        <button
          type="button"
          disabled={aiMerging || (!aiPrompt.trim() && !(aiShowJson && aiRaw.trim()))}
          on:click={previewAiPlan}
        >
          {aiMerging ? "Генерация…" : "Сгенерировать"}
        </button>
        <button type="button" disabled={!aiPreview?.validation?.valid} on:click={applyAiPreview}>
          Применить в редактор
        </button>
      </div>
      {#if aiPreview}
        <div class="ai-preview" class:bad={!aiPreview.validation.valid}>
          <p>{aiPreview.plan.humanExplanation}</p>
          <p class="meta">
            {aiPreview.plan.source ?? "ai"} · confidence {(aiPreview.plan.confidence * 100).toFixed(0)}% ·
            touched {aiPreview.touchedChapterIds.length} ·
            {aiPreview.validation.valid ? "ok" : "errors"}
          </p>
          {#if aiPreview.notes?.length}
            <ul>{#each aiPreview.notes as n}<li>{n}</li>{/each}</ul>
          {/if}
          {#if aiPreview.validation.errors?.length}
            <ul class="errs">{#each aiPreview.validation.errors as e}<li>{e}</li>{/each}</ul>
          {/if}
          {#if aiPreview.validation.warnings?.length}
            <ul class="warns">{#each aiPreview.validation.warnings as w}<li>{w}</li>{/each}</ul>
          {/if}
        </div>
      {/if}
    </div>
  {/if}

  {#if $projectPath}
    <div class="qe-stats">
      <span>{chapters.length} chapters</span>
      <span>{totalQuests} quests</span>
      <span class:warn={validationIssues.length > 0}>
        {validationIssues.length === 0 ? "✓ valid" : `${validationIssues.length} issues`}
      </span>
      {#if progressSnap && progressOverlay}
        <span class="prog-stat"
          >{progressSnap.completedCount} done · {progressSnap.startedCount} started · {progressSnap.name}</span
        >
      {/if}
    </div>
    <div class="prog-bar">
      <Eye size={14} />
      <label class="prog-toggle">
        <input
          type="checkbox"
          bind:checked={progressOverlay}
          disabled={!progressSnap}
          title="Show player progress on canvas"
        />
        Progress overlay
      </label>
      <select
        bind:value={progressKey}
        on:change={loadProgress}
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
        on:click={loadProgress}
        title="Reload progress"
      >
        <RefreshCw size={14} class={progressLoading ? "spin" : ""} />
      </button>
      {#if progressTeamLabel}
        <code class="prog-path">{progressTeamLabel.relativePath}</code>
      {/if}
    </div>
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
      <h3>No FTB Quests chapters found</h3>
      <p>Place <code>.snbt</code> files in <code>config/ftbquests/quests/chapters/</code></p>
      <p class="hint">TuffBox parses SNBT on disk — no Minecraft needed.</p>
      <button type="button" on:click={createChapter}><span>+</span> Create first chapter</button>
    </div>
  {:else}
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
      />
      <QuestCanvas
        quests={chapterQuests}
        selectedId={selectedQuest?.id ?? null}
        issues={validationIssues}
        {fitToken}
        {progressOverlay}
        {progressStatuses}
        onSelect={(q) => (selectedQuest = q)}
        onMove={moveQuest}
        onAddAt={addQuestAt}
        onLink={linkQuests}
      />
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
    <RewardTablesPanel
      tables={rewardTables}
      dirty={rewardTablesDirty}
      {saving}
      onChange={() => {
        rewardTablesDirty = true;
        rewardTables = rewardTables;
      }}
      onSave={saveRewardTable}
      onCreate={createRewardTable}
    />
    <div class="qe-footer">
      <p class="hint">
        Edits save as SNBT to <code>config/ftbquests/quests/chapters/</code>. Auto-snapshot on save.
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
  .qe-tb,
  .qe-title,
  .qe-actions {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .qe-tb {
    justify-content: space-between;
    margin-bottom: 8px;
    flex-shrink: 0;
    padding: 8px 12px;
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
    color: var(--text-secondary);
    cursor: pointer;
  }
  .prog-bar select {
    min-width: 200px;
    max-width: 360px;
    font-size: 12px;
  }
  .prog-path {
    font-size: 10px;
    color: var(--text-muted);
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
    padding: 80px;
    text-align: center;
    background: var(--ftbq-bg-panel, #212126);
    border: 1px solid var(--ftbq-border, #3a3a42);
    border-radius: 2px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
  }
  .hint {
    font-size: 11px;
  }
  .qe-lay {
    flex: 1;
    min-height: 560px;
    display: grid;
    grid-template-columns: 200px 1fr;
    gap: 0;
    border: 1px solid var(--ftbq-border, #3a3a42);
    border-radius: 2px;
    overflow: hidden;
    background: var(--ftbq-bg-canvas, #2b2b30);
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
    color: var(--text-muted);
  }
  .ai-panel {
    margin-bottom: 8px;
    padding: 10px 12px;
    border-radius: 2px;
    border: 1px solid var(--ftbq-border, #3a3a42);
    background: var(--ftbq-bg-panel, #212126);
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .ai-h {
    display: flex;
    gap: 12px;
    align-items: baseline;
    flex-wrap: wrap;
    font-size: 12px;
    color: var(--text-muted);
  }
  .ai-h strong {
    color: var(--text-primary);
  }
  .ai-raw {
    width: 100%;
    font-family: ui-monospace, monospace;
    font-size: 11px;
    resize: vertical;
    min-height: 120px;
  }
  .ai-actions {
    display: flex;
    gap: 8px;
  }
  .ai-opt {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-muted);
    cursor: pointer;
  }
  .ai-preview {
    font-size: 12px;
    color: var(--text-secondary);
    border-top: 1px solid var(--border-color);
    padding-top: 8px;
  }
  .ai-preview.bad {
    color: #fca5a5;
  }
  .ai-preview .meta {
    color: var(--text-muted);
    margin: 4px 0;
  }
  .ai-preview ul {
    margin: 4px 0 0;
    padding-left: 18px;
  }
  .ai-preview .errs {
    color: #fca5a5;
  }
  .ai-preview .warns {
    color: #fbbf24;
  }
  .qe-actions .active {
    color: var(--accent, #93c5fd);
  }
  :global(.spin) {
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (max-width: 1100px) {
    .qe-lay,
    .qe-lay.with-insp {
      grid-template-columns: 1fr;
      grid-auto-rows: minmax(180px, auto);
    }
  }
</style>
