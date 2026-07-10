<script lang="ts">
  import { api, type QuestChapter, type QuestData, type QuestValidationIssue } from "../lib/api";
  import {
    ScrollText,
    RefreshCw,
    Plus,
    Trash2,
    Save,
    AlertTriangle,
    CheckCircle2,
    Map,
    Link2,
  } from "lucide-svelte";
  import { projectPath } from "../lib/store";

  let chapters: QuestChapter[] = [];
  let loading = false;
  let saving = false;
  let error: string | null = null;
  let message: string | null = null;
  let selectedChapter = "";
  let selectedQuest: QuestData | null = null;
  let validationIssues: QuestValidationIssue[] = [];
  let dirtyChapters = new Set<string>();
  let depInputValue = "";
  let lastLoadedPath: string | null = null;

  async function load() {
    if (!$projectPath) return;
    loading = true;
    error = null;
    message = null;
    try {
      const book = await api.quests.load($projectPath);
      chapters = book.chapters ?? [];
      dirtyChapters = new Set();
      if (chapters.length > 0 && !chapters.some((c) => c.id === selectedChapter)) {
        selectedChapter = chapters[0].id;
      }
      validationIssues = await api.quests.validate($projectPath);
      lastLoadedPath = $projectPath;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

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
  }

  function markDirty(chapterId: string) {
    dirtyChapters.add(chapterId);
    dirtyChapters = dirtyChapters;
    chapters = [...chapters];
  }

  function createChapter() {
    const n: QuestChapter = {
      id: `chapter_${Date.now()}`,
      title: `Chapter ${chapters.length + 1}`,
      quests: [],
    };
    chapters = [...chapters, n];
    selectedChapter = n.id;
    selectedQuest = null;
    markDirty(n.id);
  }

  function addQuest() {
    if (!selectedChapter) return;
    const ch = chapters.find((c) => c.id === selectedChapter);
    if (!ch) return;
    const newQ: QuestData = {
      id: crypto.randomUUID().replace(/-/g, "").slice(0, 16),
      title: "New Quest",
      description: [],
      x: 80 + ch.quests.length * 40,
      y: 80 + (ch.quests.length % 3) * 60,
      dependencies: [],
      tasks: [{ id: crypto.randomUUID().replace(/-/g, "").slice(0, 12), type: "checkmark" }],
      rewards: [],
      optional: false,
    };
    ch.quests = [...ch.quests, newQ];
    markDirty(selectedChapter);
    selectedQuest = newQ;
  }

  function removeQuest(q: QuestData) {
    const ch = chapters.find((c) => c.id === selectedChapter);
    if (!ch) return;
    ch.quests = ch.quests.filter((x) => x.id !== q.id);
    markDirty(selectedChapter);
    if (selectedQuest?.id === q.id) selectedQuest = null;
  }

  function addDep(q: QuestData, depId: string) {
    if (!depId.trim() || q.dependencies.includes(depId.trim())) return;
    q.dependencies = [...q.dependencies, depId.trim()];
    markDirty(selectedChapter);
  }

  function handleDepKeydown(e: KeyboardEvent) {
    if (e.key === "Enter" && selectedQuest) {
      addDep(selectedQuest, depInputValue);
      depInputValue = "";
    }
  }

  function questIssues(questId: string) {
    return validationIssues.filter((i) => i.questId === questId);
  }

  $: chapterQuests = chapters.find((c) => c.id === selectedChapter)?.quests ?? [];
  $: currentChapter = chapters.find((c) => c.id === selectedChapter) ?? null;
  $: totalQuests = chapters.reduce((n, c) => n + c.quests.length, 0);
  $: hasDirty = dirtyChapters.size > 0;
  $: if ($projectPath && $projectPath !== lastLoadedPath) load();
</script>

<div class="qe">
  <div class="qe-tb">
    <div class="qe-title"><ScrollText size={18} /> Quest editor</div>
    <div class="qe-actions">
      {#if hasDirty}
        <span class="dirty-badge">{dirtyChapters.size} unsaved</span>
        <button on:click={saveAll} disabled={!$projectPath || saving}>
          <Save size={16} /> {saving ? "Saving…" : "Save all"}
        </button>
      {/if}
      <button class="ghost" on:click={load} disabled={!$projectPath || loading}>
        <RefreshCw size={16} class={loading ? "spin" : ""} />
      </button>
    </div>
  </div>

  {#if $projectPath}
    <div class="qe-stats">
      <span>{chapters.length} chapters</span>
      <span>{totalQuests} quests</span>
      <span class:warn={validationIssues.length > 0}>
        {validationIssues.length === 0 ? "✓ valid" : `${validationIssues.length} issues`}
      </span>
    </div>
  {/if}

  {#if error}<div class="notice error"><AlertTriangle size={14} /> {error}</div>{/if}
  {#if message}<div class="notice success"><CheckCircle2 size={14} /> {message}</div>{/if}

  {#if !$projectPath}
    <div class="empty">Open a project to edit FTB Quests chapters.</div>
  {:else if loading && chapters.length === 0}
    <div class="empty"><RefreshCw size={32} class="spin" /><p>Loading quest book…</p></div>
  {:else if chapters.length === 0}
    <div class="empty">
      <ScrollText size={40} />
      <h3>No FTB Quests chapters found</h3>
      <p>Place <code>.snbt</code> files in <code>config/ftbquests/quests/chapters/</code></p>
      <p class="hint">TuffBox parses SNBT on disk — no Minecraft needed.</p>
      <button on:click={createChapter}><Plus size={16} /> Create first chapter</button>
    </div>
  {:else}
    <div class="qe-lay">
      <aside class="qe-side">
        <h3>Chapters</h3>
        {#each chapters as ch}
          <button
            class="qe-ch-row"
            class:sel={selectedChapter === ch.id}
            class:dirty={dirtyChapters.has(ch.id)}
            on:click={() => ((selectedChapter = ch.id), (selectedQuest = null))}
          >
            <strong>{ch.title}</strong>
            <span>{ch.quests.length} quests</span>
            {#if dirtyChapters.has(ch.id)}<span class="dot" title="Unsaved">●</span>{/if}
          </button>
        {/each}
        <button class="secondary qe-add-ch" on:click={createChapter}><Plus size={14} /> Add chapter</button>
        {#if selectedChapter && dirtyChapters.has(selectedChapter)}
          <button class="qe-save-ch" on:click={() => saveChapter(selectedChapter)} disabled={saving}>
            <Save size={14} /> Save chapter
          </button>
        {/if}
      </aside>

      <section class="qe-main">
        {#if currentChapter}
          <div class="qe-main-h">
            <div>
              <h3>{currentChapter.title}</h3>
              {#if currentChapter.sourceFile}<code class="src">{currentChapter.sourceFile}</code>{/if}
            </div>
            <button on:click={addQuest}><Plus size={14} /> Add quest</button>
          </div>

          {#if chapterQuests.length > 0}
            <div class="qe-map-wrap">
              <div class="qe-map-h"><Map size={14} /> Quest map</div>
              <svg class="qe-map" viewBox="0 0 600 400" preserveAspectRatio="xMidYMid meet">
                {#each chapterQuests as q}
                  {#each q.dependencies as dep}
                    {@const target = chapterQuests.find((x) => x.id === dep)}
                    {#if target}
                      <line
                        x1={q.x + 50}
                        y1={q.y + 20}
                        x2={target.x + 50}
                        y2={target.y + 20}
                        class="dep-line"
                      />
                    {/if}
                  {/each}
                {/each}
                {#each chapterQuests as q}
                  <g
                    class="map-node"
                    class:sel={selectedQuest?.id === q.id}
                    class:has-issue={questIssues(q.id).length > 0}
                    transform="translate({q.x}, {q.y})"
                    on:click={() => (selectedQuest = q)}
                    on:keydown={(e) => e.key === "Enter" && (selectedQuest = q)}
                    role="button"
                    tabindex="0"
                  >
                    <rect width="100" height="40" rx="8" />
                    <text x="50" y="24" text-anchor="middle">{q.title.slice(0, 14)}{q.title.length > 14 ? "…" : ""}</text>
                  </g>
                {/each}
              </svg>
            </div>
          {/if}

          {#if chapterQuests.length === 0}
            <div class="empty compact">No quests yet. Add one to start building the chapter.</div>
          {:else}
            <div class="qe-grid">
              {#each chapterQuests as q}
                <button
                  class="qe-card"
                  class:sel={selectedQuest?.id === q.id}
                  class:warn={questIssues(q.id).length > 0}
                  on:click={() => (selectedQuest = q)}
                >
                  <strong>{q.title}</strong>
                  <div class="qe-card-meta">
                    <span>{q.tasks.length} tasks</span>
                    <span>{q.dependencies.length} deps</span>
                    {#if q.optional}<span class="opt">optional</span>{/if}
                  </div>
                </button>
              {/each}
            </div>
          {/if}
        {:else}
          <div class="empty compact">Select a chapter.</div>
        {/if}
      </section>

      {#if selectedQuest}
        <aside class="qe-detail">
          <div class="qe-det-h">
            <h3>{selectedQuest.title}</h3>
            <button class="ico danger" on:click={() => { if (selectedQuest) removeQuest(selectedQuest); }}><Trash2 size={14} /></button>
          </div>

          {#if questIssues(selectedQuest.id).length > 0}
            <div class="val-warn">
              {#each questIssues(selectedQuest.id) as issue}
                <div><AlertTriangle size={12} /> {issue.message}</div>
              {/each}
            </div>
          {/if}

          <div class="qe-det-fields">
            <label
              >Title<input
                bind:value={selectedQuest.title}
                on:input={() => markDirty(selectedChapter)}
              /></label
            >
            <label
              >Subtitle<input
                bind:value={selectedQuest.subtitle}
                on:input={() => markDirty(selectedChapter)}
                placeholder="Optional"
              /></label
            >
            <label class="checkbox">
              <input
                type="checkbox"
                bind:checked={selectedQuest.optional}
                on:change={() => markDirty(selectedChapter)}
              />
              Optional quest
            </label>
            <label
              >Position (X / Y)
              <div class="xy">
                <input type="number" bind:value={selectedQuest.x} on:input={() => markDirty(selectedChapter)} />
                <input type="number" bind:value={selectedQuest.y} on:input={() => markDirty(selectedChapter)} />
              </div>
            </label>
          </div>

          <h4>Tasks ({selectedQuest.tasks.length})</h4>
          <div class="qe-tasks">
            {#each selectedQuest.tasks as task, i}
              <div class="qe-task-row">
                <code>{task.type || "item"}</code>
                <span>{task.title || `Task ${i + 1}`}</span>
              </div>
            {/each}
          </div>

          <h4><Link2 size={12} /> Dependencies</h4>
          <div class="qe-deps">
            {#each selectedQuest.dependencies as dep}
              <span class="dep-tag"
                >{dep}
                <button
                  class="dep-rm"
                  on:click={() => {
                    if (selectedQuest) {
                      selectedQuest.dependencies = selectedQuest.dependencies.filter((d) => d !== dep);
                      markDirty(selectedChapter);
                    }
                  }}>×</button
                ></span
              >
            {/each}
            <div class="dep-add">
              <input placeholder="Quest ID…" bind:value={depInputValue} on:keydown={handleDepKeydown} />
            </div>
          </div>
        </aside>
      {/if}
    </div>
    <div class="qe-footer">
      <p class="hint">
        Edits save as SNBT to <code>config/ftbquests/quests/chapters/</code>. Auto-snapshot on save.
      </p>
    </div>
  {/if}
</div>

<style>
  .qe { max-width: none; width: 100%; }
  .qe-tb, .qe-title, .qe-actions { display: flex; align-items: center; gap: 10px; }
  .qe-tb { justify-content: space-between; margin-bottom: 12px; }
  .qe-title { color: var(--text-secondary); font-weight: 700; }
  .qe-stats { display: flex; gap: 16px; margin-bottom: 14px; font-size: 12px; color: var(--text-muted); }
  .qe-stats .warn { color: #fbbf24; }
  .dirty-badge { font-size: 11px; color: #fbbf24; padding: 4px 8px; border-radius: 6px; background: rgba(251, 191, 36, 0.1); }
  .notice { display: flex; align-items: center; gap: 8px; padding: 12px 14px; border-radius: var(--border-radius-lg); margin-bottom: 14px; border: 1px solid var(--border-color); }
  .notice.error { color: #fecaca; background: rgba(239, 68, 68, 0.08); border-color: rgba(239, 68, 68, 0.28); }
  .notice.success { color: var(--accent-primary); background: rgba(27, 217, 106, 0.08); border-color: rgba(27, 217, 106, 0.25); }
  .empty { color: var(--text-muted); padding: 80px; text-align: center; background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); display: flex; flex-direction: column; align-items: center; gap: 12px; }
  .empty.compact { padding: 40px; }
  .hint { font-size: 11px; }
  .qe-lay { display: grid; grid-template-columns: 220px 1fr 300px; gap: 16px; }
  .qe-side { background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); padding: 14px; }
  .qe-side h3 { color: var(--text-muted); font-size: 11px; text-transform: uppercase; letter-spacing: 0.06em; margin-bottom: 10px; }
  .qe-ch-row { width: 100%; display: grid; gap: 2px; text-align: left; padding: 8px 10px; border-radius: 8px; background: transparent; color: var(--text-secondary); border: 1px solid transparent; margin-bottom: 4px; position: relative; }
  .qe-ch-row:hover, .qe-ch-row.sel { background: var(--bg-tertiary); border-color: rgba(27, 217, 106, 0.25); color: var(--text-primary); }
  .qe-ch-row.dirty { border-color: rgba(251, 191, 36, 0.3); }
  .qe-ch-row strong { font-size: 13px; }
  .qe-ch-row span { font-size: 11px; color: var(--text-muted); }
  .dot { position: absolute; right: 8px; top: 10px; color: #fbbf24; font-size: 10px; }
  .qe-add-ch, .qe-save-ch { margin-top: 8px; width: 100%; }
  .qe-save-ch { margin-top: 6px; }
  .qe-main { background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); padding: 14px; min-height: 500px; }
  .qe-main-h { display: flex; justify-content: space-between; align-items: flex-start; margin-bottom: 14px; gap: 12px; }
  .qe-main-h h3 { font-size: 16px; margin: 0; }
  .src { font-size: 10px; color: var(--text-muted); display: block; margin-top: 4px; }
  .qe-map-wrap { margin-bottom: 16px; border: 1px solid var(--border-color); border-radius: 10px; overflow: hidden; background: #0a0a0c; }
  .qe-map-h { display: flex; align-items: center; gap: 6px; padding: 8px 12px; font-size: 11px; color: var(--text-muted); border-bottom: 1px solid var(--border-color); text-transform: uppercase; letter-spacing: 0.04em; }
  .qe-map { width: 100%; height: 220px; display: block; }
  .dep-line { stroke: rgba(27, 217, 106, 0.35); stroke-width: 1.5; }
  .map-node { cursor: pointer; }
  .map-node rect { fill: var(--bg-tertiary); stroke: var(--border-color); }
  .map-node.sel rect { stroke: var(--accent-primary); fill: rgba(27, 217, 106, 0.08); }
  .map-node.has-issue rect { stroke: #fbbf24; }
  .map-node text { fill: var(--text-secondary); font-size: 10px; pointer-events: none; }
  .qe-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: 8px; }
  .qe-card { display: grid; gap: 6px; padding: 12px; border-radius: 10px; border: 1px solid var(--border-color); background: var(--bg-tertiary); text-align: left; }
  .qe-card:hover, .qe-card.sel { border-color: rgba(27, 217, 106, 0.35); background: rgba(27, 217, 106, 0.04); }
  .qe-card.warn { border-color: rgba(251, 191, 36, 0.4); }
  .qe-card strong { color: var(--text-primary); font-size: 13px; }
  .qe-card-meta { display: flex; gap: 8px; flex-wrap: wrap; }
  .qe-card-meta span { font-size: 10px; color: var(--text-muted); }
  .qe-card-meta .opt { color: #fbbf24; font-weight: 700; }
  .qe-detail { background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); padding: 14px; max-height: 680px; overflow: auto; }
  .qe-det-h { display: flex; justify-content: space-between; align-items: center; margin-bottom: 14px; }
  .val-warn { padding: 8px 10px; border-radius: 8px; background: rgba(251, 191, 36, 0.08); border: 1px solid rgba(251, 191, 36, 0.25); font-size: 11px; color: #fde68a; margin-bottom: 12px; display: grid; gap: 4px; }
  .ico { width: 28px; height: 28px; padding: 0; display: flex; align-items: center; justify-content: center; background: transparent; border: 1px solid var(--border-color); border-radius: 6px; color: var(--text-muted); cursor: pointer; }
  .ico.danger:hover { background: rgba(239, 68, 68, 0.1); color: #f87171; }
  .qe-det-fields { display: grid; gap: 10px; margin-bottom: 16px; }
  .qe-det-fields label { display: grid; gap: 4px; color: var(--text-muted); font-size: 11px; text-transform: uppercase; letter-spacing: 0.03em; }
  .qe-det-fields input { font-size: 13px; }
  .checkbox { display: flex; align-items: center; gap: 8px; flex-direction: row !important; font-size: 12px !important; text-transform: none !important; }
  .checkbox input { width: auto; }
  .xy { display: grid; grid-template-columns: 1fr 1fr; gap: 6px; }
  h4 { display: flex; align-items: center; gap: 6px; color: var(--text-secondary); font-size: 12px; margin: 14px 0 8px; text-transform: uppercase; letter-spacing: 0.04em; }
  .qe-tasks { display: grid; gap: 4px; }
  .qe-task-row { display: flex; gap: 8px; align-items: center; padding: 6px 8px; border-radius: 6px; background: var(--bg-tertiary); font-size: 11px; }
  .qe-task-row code { font-size: 10px; color: var(--accent-primary); }
  .qe-deps { display: flex; flex-wrap: wrap; gap: 4px; align-items: center; }
  .dep-tag { font-size: 10px; padding: 3px 6px; border-radius: 4px; background: var(--bg-tertiary); color: var(--text-secondary); border: 1px solid var(--border-color); display: flex; align-items: center; gap: 4px; }
  .dep-rm { background: transparent; border: none; color: var(--text-muted); cursor: pointer; font-size: 10px; padding: 0; }
  .dep-rm:hover { color: #f87171; }
  .dep-add { margin-top: 4px; width: 100%; }
  .dep-add input { font-size: 11px; padding: 4px 8px; width: 100%; }
  .qe-footer { margin-top: 12px; }
  :global(.spin) { animation: spin 900ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (max-width: 1100px) { .qe-lay { grid-template-columns: 1fr; } }
</style>
