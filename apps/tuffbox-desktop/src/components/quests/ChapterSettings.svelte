<script lang="ts">
  import type { QuestChapter, QuestChapterGroup } from "../../lib/api";
  import { SHAPE_OPTIONS } from "../../lib/questTypeLabels";
  import ItemStackEditor from "./ItemStackEditor.svelte";

  let {
    chapter,
    chapterGroups = [],
    onDirty,
  }: {
    chapter: QuestChapter;
    chapterGroups?: QuestChapterGroup[];
    onDirty: () => void;
  } = $props();

  let hideDefValue = $derived(
    chapter.defaultHideDependencyLines === true
      ? "true"
      : chapter.defaultHideDependencyLines === false
        ? "false"
        : "",
  );

  function setHideDef(s: string) {
    if (s === "true") chapter.defaultHideDependencyLines = true;
    else if (s === "false") chapter.defaultHideDependencyLines = false;
    else chapter.defaultHideDependencyLines = null;
    onDirty();
  }

  let extraKey = $state("");
  let extraVal = $state("");

  function ensureExtras() {
    if (!chapter.extras) chapter.extras = {};
    return chapter.extras;
  }

  function addExtra() {
    const k = extraKey.trim();
    if (!k) return;
    let parsed: unknown = extraVal;
    try {
      parsed = JSON.parse(extraVal);
    } catch {
      /* keep string */
    }
    ensureExtras()[k] = parsed as never;
    chapter.extras = { ...chapter.extras };
    extraKey = "";
    extraVal = "";
    onDirty();
  }

  function removeExtra(k: string) {
    if (!chapter.extras) return;
    delete chapter.extras[k];
    chapter.extras = { ...chapter.extras };
    onDirty();
  }

  function selectVal(e: Event): string {
    return (e.currentTarget as HTMLSelectElement).value;
  }
  function inputVal(e: Event): string {
    return (e.currentTarget as HTMLInputElement).value;
  }
</script>

<div class="ch-set ftbq-panel">
  <h4>Chapter settings</h4>
  <label
    >Title<input
      bind:value={chapter.title}
      oninput={() => {
        chapter.titleFromSnbt = true;
        onDirty();
      }}
    /></label
  >
  <div class="icon-edit">
    <ItemStackEditor
      label="Icon"
      value={chapter.icon ?? null}
      allowFilters={false}
      onChange={(v) => {
        chapter.icon = v;
        onDirty();
      }}
    />
  </div>
  <label
    >Group
    <select
      value={chapter.group ?? ""}
      onchange={(e) => {
        chapter.group = selectVal(e) || null;
        onDirty();
      }}
    >
      <option value="">(none)</option>
      {#each chapterGroups as g (g.id)}
        <option value={g.id}>{g.title}</option>
      {/each}
    </select>
  </label>
  <label
    >Order index<input
      type="number"
      value={chapter.orderIndex ?? ""}
      oninput={(e) => {
        const v = inputVal(e);
        chapter.orderIndex = v === "" ? null : Number(v);
        onDirty();
      }}
    /></label
  >
  <label>Filename<input bind:value={chapter.filename} oninput={onDirty} placeholder="my_chapter" /></label>
  <label
    >Default quest shape
    <select
      value={chapter.defaultQuestShape ?? ""}
      onchange={(e) => {
        chapter.defaultQuestShape = selectVal(e) || null;
        onDirty();
      }}
    >
      {#each SHAPE_OPTIONS as s (s.id || "_default")}
        <option value={s.id}>{s.label}</option>
      {/each}
    </select>
  </label>
  <label
    >Default hide dependency lines
    <select
      value={hideDefValue}
      onchange={(e) => setHideDef(selectVal(e))}
    >
      <option value="">unset</option>
      <option value="true">true</option>
      <option value="false">false</option>
    </select>
  </label>

  <h4>Extra SNBT fields</h4>
  {#each Object.entries(chapter.extras ?? {}) as [k, v] (k)}
    <div class="extra-row">
      <code>{k}</code>
      <span>{typeof v === "string" ? v : JSON.stringify(v)}</span>
      <button type="button" onclick={() => removeExtra(k)}>×</button>
    </div>
  {/each}
  <div class="extra-add">
    <input placeholder="key" bind:value={extraKey} />
    <input placeholder='value or JSON' bind:value={extraVal} />
    <button type="button" onclick={addExtra}>Add</button>
  </div>
</div>

<style>
  .ch-set {
    display: grid;
    gap: 8px;
    padding: 0 0 16px;
    font-size: 12px;
    background: var(--ftbq-bg-panel, #212126);
    border-left: 1px solid #101014;
    box-shadow: inset 1px 0 0 rgba(255, 255, 255, 0.05);
    max-height: 100%;
    overflow: auto;
    overflow-x: hidden;
    min-height: 0;
    min-width: 0;
  }
  .ch-set h4 {
    margin: 0;
    padding: 10px 12px 6px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--ftbq-title-gold, #f2c94c);
    font-weight: 700;
    background: linear-gradient(180deg, rgba(255, 255, 255, 0.04), rgba(0, 0, 0, 0.25));
    border-bottom: 1px solid #101014;
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.05);
    text-shadow: 1px 1px 0 rgba(0, 0, 0, 0.6);
  }
  .ch-set h4:not(:first-child) {
    border-top: 1px solid #101014;
    color: var(--ftbq-accent-teal, #3db8a8);
  }
  .ch-set label {
    display: grid;
    gap: 4px;
    color: var(--ftbq-text-muted, #9a9aa0);
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    padding: 0 12px;
  }
  .icon-edit {
    padding: 0 12px;
  }
  .ch-set input,
  .ch-set select {
    font-size: 12px;
    background: #141419;
    border: 1px solid #0c0c0f;
    box-shadow: inset 1px 1px 3px rgba(0, 0, 0, 0.55);
    color: var(--ftbq-text, #e8e8e8);
    border-radius: 3px;
    text-transform: none;
  }
  .extra-row {
    display: grid;
    grid-template-columns: 1fr 1fr auto;
    gap: 6px;
    align-items: center;
    padding: 0 12px;
    font-size: 11px;
  }
  .extra-add {
    display: grid;
    grid-template-columns: 1fr 1fr auto;
    gap: 6px;
    padding: 0 12px 12px;
  }
  .extra-add input,
  .extra-add button {
    font-size: 11px;
    background: #141419;
    border: 1px solid #0c0c0f;
    color: var(--ftbq-text, #e8e8e8);
    border-radius: 3px;
  }
  .extra-add button {
    background: linear-gradient(180deg, #3a3a42, #2a2a31);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.1);
    cursor: pointer;
  }
  .extra-add button:hover {
    background: linear-gradient(180deg, #47503f, #32382d);
    color: #d6f5d0;
  }
</style>
