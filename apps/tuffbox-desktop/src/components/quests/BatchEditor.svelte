<script lang="ts">
  import type { QuestChapter, QuestData } from "../../lib/api";
  import { mcFormat, stripCodes } from "../../lib/mcformat";
  import { SHAPE_OPTIONS } from "../../lib/questTypeLabels";

  interface Props {
    chapters: QuestChapter[];
    selectedIds?: Set<string>;
    onQuestUpdate: (chapterId: string, quest: QuestData) => void;
    onBatchApply?: (questIds: Set<string>, mutator: (q: QuestData) => QuestData) => void;
    onSaveChapter: (chapterId: string) => void;
  }

  let {
    chapters,
    selectedIds = new Set(),
    onQuestUpdate,
    onBatchApply,
    onSaveChapter,
  }: Props = $props();

  // Search state
  let query = $state("");
  let caseSensitive = $state(false);
  let noTitle = $state(false);
  let noSubtitle = $state(false);
  let noDescription = $state(false);
  let scopeChapter = $state("");
  let scopeSelection = $state(false);
  let prevSelectedCount = $state(0);
  let perPage = $state(10);
  let page = $state(0);

  // Mass-apply draft values
  let massOptional = $state<"keep" | "true" | "false">("keep");
  let massSize = $state("");
  let massShape = $state("__keep__");
  let massHideDep = $state<"keep" | "true" | "false" | "unset">("keep");
  let massHideDependent = $state<"keep" | "true" | "false" | "unset">("keep");
  let massStatus = $state<string | null>(null);

  // Turn on when selection appears; clear when selection empties
  $effect(() => {
    const n = selectedIds.size;
    if (n > 0 && prevSelectedCount === 0) scopeSelection = true;
    if (n === 0) scopeSelection = false;
    prevSelectedCount = n;
  });

  // Editing state
  let editingId = $state<string | null>(null);
  let editTitle = $state("");
  let editSubtitle = $state("");
  let editDescription = $state("");
  let saveStatus = $state<Record<string, "saving" | "saved" | "error">>({});

  interface BatchResult {
    quest: QuestData;
    chapterId: string;
    chapterTitle: string;
    matchField: string;
  }

  let results = $derived.by<BatchResult[]>(() => {
    const terms = query.trim()
      ? query.trim().split(/\s+/).filter(Boolean)
      : [];
    const selectionOnly = scopeSelection && selectedIds.size > 0;
    if (!terms.length && !noTitle && !noSubtitle && !noDescription && !selectionOnly) {
      return [];
    }

    const filtered: BatchResult[] = [];

    for (const ch of chapters) {
      if (scopeChapter && ch.title !== scopeChapter && ch.id !== scopeChapter) continue;

      for (const q of ch.quests) {
        if (selectionOnly && !selectedIds.has(q.id)) continue;

        if (noTitle && q.title?.trim()) continue;
        if (noSubtitle && q.subtitle?.trim()) continue;
        if (noDescription && q.description?.length) continue;

        if (terms.length > 0) {
          const titleText = stripCodes(q.title ?? "");
          const subtitleText = stripCodes(q.subtitle ?? "");
          const descText = stripCodes(q.description?.join(" ") ?? "");

          const match = terms.every((term) => {
            const t = caseSensitive ? term : term.toLowerCase();
            return (
              (caseSensitive ? titleText : titleText.toLowerCase()).includes(t) ||
              (caseSensitive ? subtitleText : subtitleText.toLowerCase()).includes(t) ||
              (caseSensitive ? descText : descText.toLowerCase()).includes(t)
            );
          });

          if (!match) continue;
        }

        let matchField = "title";
        if (noTitle) matchField = "title (missing)";
        else if (noSubtitle) matchField = "subtitle (missing)";
        else if (noDescription) matchField = "description (missing)";
        else if (selectionOnly && !terms.length) matchField = "selected";

        filtered.push({
          quest: q,
          chapterId: ch.id,
          chapterTitle: ch.title,
          matchField,
        });
      }
    }

    return filtered;
  });

  let totalPages = $derived(Math.ceil(results.length / perPage));
  let pagedResults = $derived(results.slice(page * perPage, (page + 1) * perPage));
  let canMassApply = $derived(results.length > 0 && !!onBatchApply);

  // Reset page on search change
  $effect(() => {
    void query;
    void scopeChapter;
    void scopeSelection;
    void noTitle;
    void noSubtitle;
    void noDescription;
    page = 0;
  });

  function startEdit(q: QuestData) {
    editingId = q.id;
    editTitle = q.title ?? "";
    editSubtitle = q.subtitle ?? "";
    editDescription = q.description?.join("\n") ?? "";
  }

  function cancelEdit() {
    editingId = null;
  }

  async function saveEdit(chapterId: string, q: QuestData) {
    saveStatus[q.id] = "saving";

    const updated: QuestData = {
      ...q,
      title: editTitle,
      subtitle: editSubtitle || null,
      description: editDescription
        .split("\n")
        .map((l) => l.trimEnd())
        .filter((l) => l.length > 0),
    };

    try {
      onQuestUpdate(chapterId, updated);
      saveStatus[q.id] = "saved";
      setTimeout(() => {
        if (saveStatus[q.id] === "saved") {
          delete saveStatus[q.id];
          saveStatus = saveStatus; // trigger reactivity
        }
      }, 1500);
    } catch {
      saveStatus[q.id] = "error";
    }
  }

  function parseTri(
    mode: "keep" | "true" | "false" | "unset",
  ): boolean | null | undefined {
    if (mode === "keep") return undefined;
    if (mode === "unset") return null;
    return mode === "true";
  }

  function applyMass() {
    if (!onBatchApply || results.length === 0) return;
    const sizeNum = massSize.trim() === "" ? undefined : Number(massSize);
    if (sizeNum !== undefined && (!Number.isFinite(sizeNum) || sizeNum <= 0)) {
      massStatus = "Size must be a positive number";
      return;
    }
    const optional =
      massOptional === "keep" ? undefined : massOptional === "true";
    const shape = massShape === "__keep__" ? undefined : massShape || null;
    const hideDependencyLines = parseTri(massHideDep);
    const hideDependentLines = parseTri(massHideDependent);

    const hasAny =
      optional !== undefined ||
      sizeNum !== undefined ||
      shape !== undefined ||
      hideDependencyLines !== undefined ||
      hideDependentLines !== undefined;
    if (!hasAny) {
      massStatus = "Pick at least one field to apply";
      return;
    }

    const ids = new Set(results.map((r) => r.quest.id));
    onBatchApply(ids, (q) => {
      const next: QuestData = { ...q };
      if (optional !== undefined) next.optional = optional;
      if (sizeNum !== undefined) next.size = sizeNum;
      if (shape !== undefined) next.shape = shape;
      if (hideDependencyLines !== undefined) next.hideDependencyLines = hideDependencyLines;
      if (hideDependentLines !== undefined) next.hideDependentLines = hideDependentLines;
      return next;
    });
    massStatus = `Applied to ${ids.size} quest(s) · one Undo reverts all`;
    setTimeout(() => {
      if (massStatus?.startsWith("Applied")) massStatus = null;
    }, 2500);
  }

  function matchColor(field: string): string {
    if (field.includes("missing")) return "#ef4444";
    return "var(--ftbq-text-muted, #9a9aa0)";
  }
</script>

<div class="batch-editor">
  <div class="batch-header">
    <h3 class="batch-title">Batch Editor</h3>
    <span class="batch-count">
      {results.length} result{results.length !== 1 ? "s" : ""}
    </span>
  </div>

  <div class="batch-search">
    <input
      type="text"
      class="search-input"
      placeholder="Search quests..."
      bind:value={query}
    />
    <div class="filters">
      <label class="filter">
        <input type="checkbox" bind:checked={caseSensitive} />
        Case sensitive
      </label>
      <label class="filter">
        <input type="checkbox" bind:checked={noTitle} />
        No title
      </label>
      <label class="filter">
        <input type="checkbox" bind:checked={noSubtitle} />
        No subtitle
      </label>
      <label class="filter">
        <input type="checkbox" bind:checked={noDescription} />
        No description
      </label>
    </div>
    <div class="scope-row">
      <select bind:value={scopeChapter}>
        <option value="">All chapters</option>
        {#each chapters as ch (ch.id)}
          <option value={ch.id}>{ch.title} ({ch.quests.length})</option>
        {/each}
      </select>
      <label class="filter" title="Limit batch rows to canvas multi-selection">
        <input type="checkbox" bind:checked={scopeSelection} disabled={selectedIds.size === 0} />
        Selected only ({selectedIds.size})
      </label>
      <select bind:value={perPage}>
        <option value={5}>5 per page</option>
        <option value={10}>10 per page</option>
        <option value={20}>20 per page</option>
        <option value={50}>50 per page</option>
      </select>
    </div>
  </div>

  {#if canMassApply}
    <div class="mass-bar">
      <span class="mass-label">Mass apply ({results.length})</span>
      <label class="mass-field">
        Optional
        <select bind:value={massOptional}>
          <option value="keep">keep</option>
          <option value="true">true</option>
          <option value="false">false</option>
        </select>
      </label>
      <label class="mass-field">
        Size
        <input type="number" step="0.25" min="0.25" placeholder="keep" bind:value={massSize} />
      </label>
      <label class="mass-field">
        Shape
        <select bind:value={massShape}>
          <option value="__keep__">keep</option>
          {#each SHAPE_OPTIONS as s (s.id || "_default")}
            <option value={s.id}>{s.label}</option>
          {/each}
        </select>
      </label>
      <label class="mass-field">
        Hide dep lines
        <select bind:value={massHideDep}>
          <option value="keep">keep</option>
          <option value="true">true</option>
          <option value="false">false</option>
          <option value="unset">unset</option>
        </select>
      </label>
      <label class="mass-field">
        Hide dependent
        <select bind:value={massHideDependent}>
          <option value="keep">keep</option>
          <option value="true">true</option>
          <option value="false">false</option>
          <option value="unset">unset</option>
        </select>
      </label>
      <button type="button" class="btn primary small" onclick={applyMass}>Apply</button>
      {#if massStatus}
        <span class="mass-status">{massStatus}</span>
      {/if}
    </div>
  {/if}

  {#if results.length === 0}
    <div class="batch-empty">
      {#if query.trim() || noTitle || noSubtitle || noDescription || scopeSelection}
        <p>No quests match your search.</p>
      {:else}
        <p>Type a search query or check a filter to find quests.</p>
      {/if}
    </div>
  {:else}
    <div class="batch-results">
      {#each pagedResults as r (r.quest.id + r.chapterId)}
        <div class="quest-card" class:editing={editingId === r.quest.id}>
          <div class="quest-header" role="button" tabindex="0"
            onclick={() => editingId === r.quest.id ? null : startEdit(r.quest)}
            onkeydown={(e) => { if (e.key === "Enter") startEdit(r.quest); }}
          >
            <span class="quest-chapter" style="color: {matchColor(r.matchField)}">
              {r.chapterTitle}
            </span>
            <span class="quest-id">{r.quest.id}</span>
            {#if saveStatus[r.quest.id]}
              <span class="save-badge" class:saving={saveStatus[r.quest.id] === "saving"}
                class:saved={saveStatus[r.quest.id] === "saved"}
                class:error={saveStatus[r.quest.id] === "error"}>
                {saveStatus[r.quest.id] === "saving" ? "Saving..." :
                 saveStatus[r.quest.id] === "saved" ? "Saved" : "Error"}
              </span>
            {/if}
          </div>

          {#if editingId === r.quest.id}
            <div class="edit-form">
              <label class="field">
                <span class="field-label">Title</span>
                <input type="text" bind:value={editTitle} />
                <span class="field-preview">{@html mcFormat(editTitle)}</span>
              </label>
              <label class="field">
                <span class="field-label">Subtitle</span>
                <input type="text" bind:value={editSubtitle} />
                <span class="field-preview">{@html mcFormat(editSubtitle)}</span>
              </label>
              <label class="field">
                <span class="field-label">Description</span>
                <textarea rows="4" bind:value={editDescription}></textarea>
                <span class="field-preview desc">{@html mcFormat(editDescription)}</span>
              </label>
              <div class="edit-actions">
                <button type="button" class="btn primary small"
                  onclick={() => saveEdit(r.chapterId, r.quest)}>
                  Save
                </button>
                <button type="button" class="btn ghost small" onclick={cancelEdit}>
                  Cancel
                </button>
                <button type="button" class="btn ghost small"
                  onclick={() => onSaveChapter(r.chapterId)}>
                  Save chapter
                </button>
              </div>
            </div>
          {:else}
            <div class="quest-preview">
                <div class="preview-row">
                  <span class="preview-label">Title:</span>
                  <span class="preview-value">{r.quest.title ?? ''}</span>
                </div>
              {#if r.quest.subtitle}
                <div class="preview-row">
                  <span class="preview-label">Subtitle:</span>
                  <span class="preview-value">{r.quest.subtitle}</span>
                </div>
              {/if}
              {#if r.quest.description?.length}
                <div class="preview-row">
                  <span class="preview-label">Description:</span>
                  <span class="preview-value desc">{r.quest.description.slice(0, 2).join(" ")}{r.quest.description.length > 2 ? "..." : ""}</span>
                </div>
              {/if}
            </div>
          {/if}
        </div>
      {/each}
    </div>

    {#if totalPages > 1}
      <div class="pagination">
        <button type="button" class="btn ghost small" disabled={page === 0}
          onclick={() => page--}>
          ← Prev
        </button>
        <span class="page-info">
          Page {page + 1} of {totalPages}
        </span>
        <button type="button" class="btn ghost small" disabled={page >= totalPages - 1}
          onclick={() => page++}>
          Next →
        </button>
      </div>
    {/if}
  {/if}
</div>

<style>
  .batch-editor {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
    background: var(--ftbq-bg-panel, #212126);
    color: var(--ftbq-text, #e8e8e8);
  }
  .batch-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px;
    border-bottom: 1px solid var(--ftbq-border, #3a3a42);
    flex-shrink: 0;
  }
  .batch-title {
    font-size: 12px;
    font-weight: 700;
    color: var(--ftbq-title-gold, #f2c94c);
  }
  .batch-count {
    font-size: 10px;
    color: var(--ftbq-text-muted, #9a9aa0);
  }
  .batch-search {
    padding: 8px 12px;
    border-bottom: 1px solid var(--ftbq-border, #3a3a42);
    display: flex;
    flex-direction: column;
    gap: 6px;
    flex-shrink: 0;
  }
  .search-input {
    width: 100%;
    padding: 6px 8px;
    font-size: 12px;
    background: #141419;
    border: 1px solid #0c0c0f;
    box-shadow: inset 1px 1px 3px rgba(0, 0, 0, 0.55);
    color: var(--ftbq-text, #e8e8e8);
    border-radius: 3px;
  }
  .filters {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .filter {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    color: var(--ftbq-text-muted, #9a9aa0);
    cursor: pointer;
  }
  .filter input { width: 12px; height: 12px; }
  .scope-row {
    display: flex;
    gap: 8px;
  }
  .scope-row select {
    flex: 1;
    font-size: 11px;
    padding: 4px 6px;
    background: #141419;
    border: 1px solid #0c0c0f;
    color: var(--ftbq-text, #e8e8e8);
    border-radius: 3px;
  }

  .mass-bar {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 8px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--ftbq-border, #3a3a42);
    background: rgba(0, 0, 0, 0.18);
    flex-shrink: 0;
  }
  .mass-label {
    font-size: 10px;
    font-weight: 700;
    color: var(--ftbq-title-gold, #f2c94c);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    align-self: center;
  }
  .mass-field {
    display: flex;
    flex-direction: column;
    gap: 2px;
    font-size: 10px;
    color: var(--ftbq-text-muted, #9a9aa0);
  }
  .mass-field select,
  .mass-field input {
    min-width: 72px;
    max-width: 110px;
    font-size: 11px;
    padding: 3px 5px;
    background: #141419;
    border: 1px solid #0c0c0f;
    color: var(--ftbq-text, #e8e8e8);
    border-radius: 3px;
  }
  .mass-status {
    font-size: 10px;
    color: var(--ftbq-accent-green, #55c95a);
    align-self: center;
  }

  .batch-empty {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--ftbq-text-muted, #9a9aa0);
    font-size: 12px;
    padding: 24px;
  }

  .batch-results {
    flex: 1;
    overflow-y: auto;
    padding: 8px 12px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .quest-card {
    border: 1px solid var(--ftbq-border, #3a3a42);
    border-radius: 3px;
    overflow: hidden;
    transition: border-color 0.15s;
  }
  .quest-card:hover { border-color: var(--ftbq-accent-teal, #3db8a8); }
  .quest-card.editing { border-color: var(--ftbq-accent-teal, #3db8a8); }

  .quest-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 8px;
    background: var(--ftbq-bg, #1a1a1e);
    cursor: pointer;
    font-size: 11px;
  }
  .quest-chapter {
    font-weight: 600;
    white-space: nowrap;
  }
  .quest-id {
    font-family: monospace;
    color: var(--ftbq-text-muted, #9a9aa0);
    font-size: 10px;
  }
  .save-badge {
    margin-left: auto;
    font-size: 10px;
    padding: 1px 6px;
    border-radius: 2px;
    font-weight: 600;
  }
  .save-badge.saving { color: var(--ftbq-text-muted, #9a9aa0); }
  .save-badge.saved { color: var(--ftbq-accent-green, #55c95a); background: rgba(85,201,90,0.15); }
  .save-badge.error { color: #ef4444; background: rgba(239,68,68,0.15); }

  .quest-preview {
    padding: 6px 8px;
    font-size: 11px;
  }
  .preview-row {
    display: flex;
    gap: 6px;
    margin-bottom: 2px;
  }
  .preview-label {
    color: var(--ftbq-text-muted, #9a9aa0);
    min-width: 70px;
    flex-shrink: 0;
  }
  .preview-value {
    color: var(--ftbq-text, #e8e8e8);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .preview-value.desc {
    white-space: normal;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
  }

  .edit-form {
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
    background: var(--ftbq-bg, #1a1a1e);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .field-label {
    font-size: 10px;
    font-weight: 600;
    color: var(--ftbq-text-muted, #9a9aa0);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .field input, .field textarea {
    width: 100%;
    font-size: 12px;
    padding: 4px 6px;
    font-family: inherit;
    background: #141419;
    border: 1px solid #0c0c0f;
    box-shadow: inset 1px 1px 3px rgba(0, 0, 0, 0.55);
    color: var(--ftbq-text, #e8e8e8);
    border-radius: 3px;
  }
  .field textarea {
    resize: vertical;
    min-height: 60px;
    font-family: monospace;
    font-size: 11px;
  }
  .field-preview {
    font-size: 11px;
    padding: 4px 6px;
    background: rgba(0,0,0,0.15);
    border-radius: 2px;
    min-height: 20px;
    line-height: 1.4;
    word-break: break-word;
  }
  .field-preview.desc {
    max-height: 80px;
    overflow-y: auto;
  }
  .edit-actions {
    display: flex;
    gap: 6px;
    margin-top: 4px;
  }

  .pagination {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 8px 12px;
    border-top: 1px solid var(--ftbq-border, #3a3a42);
    flex-shrink: 0;
  }
  .page-info {
    font-size: 11px;
    color: var(--ftbq-text-muted, #9a9aa0);
  }

  .btn {
    padding: 6px 12px;
    border: 1px solid var(--ftbq-border, #3a3a42);
    background: rgba(0,0,0,0.25);
    color: var(--ftbq-text, #e8e8e8);
    font-size: 12px;
    font-weight: 600;
    border-radius: 2px;
    cursor: pointer;
  }
  .btn:hover:not(:disabled) { border-color: var(--ftbq-accent-teal, #3db8a8); }
  .btn:disabled { opacity: 0.4; cursor: default; }
  .btn.primary {
    border-color: var(--ftbq-accent-green, #55c95a);
    background: rgba(85,201,90,0.18);
    color: var(--ftbq-accent-green, #55c95a);
  }
  .btn.ghost { background: transparent; }
  .btn.small { padding: 4px 8px; font-size: 11px; }
</style>
