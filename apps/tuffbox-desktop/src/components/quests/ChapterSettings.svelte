<script lang="ts">
  import type { QuestChapter, QuestChapterGroup } from "../../lib/api";

  export let chapter: QuestChapter;
  export let chapterGroups: QuestChapterGroup[] = [];
  export let onDirty: () => void;

  const SHAPES = ["", "circle", "square", "rsquare", "diamond", "hexagon", "pentagon", "gear", "none"];

  function boolTri(
    val: boolean | null | undefined,
    on: (v: boolean | null) => void,
  ): { value: string; set: (s: string) => void } {
    return {
      value: val === true ? "true" : val === false ? "false" : "",
      set: (s: string) => {
        if (s === "true") on(true);
        else if (s === "false") on(false);
        else on(null);
      },
    };
  }

  $: hideDef = boolTri(chapter.defaultHideDependencyLines, (v) => {
    chapter.defaultHideDependencyLines = v;
    onDirty();
  });

  let extraKey = "";
  let extraVal = "";

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
  <label>Title<input bind:value={chapter.title} on:input={onDirty} /></label>
  <label>Icon<input bind:value={chapter.icon} on:input={onDirty} placeholder="mod:item" /></label>
  <label
    >Group
    <select
      value={chapter.group ?? ""}
      on:change={(e) => {
        chapter.group = selectVal(e) || null;
        onDirty();
      }}
    >
      <option value="">(none)</option>
      {#each chapterGroups as g}
        <option value={g.id}>{g.title}</option>
      {/each}
    </select>
  </label>
  <label
    >Order index<input
      type="number"
      value={chapter.orderIndex ?? ""}
      on:input={(e) => {
        const v = inputVal(e);
        chapter.orderIndex = v === "" ? null : Number(v);
        onDirty();
      }}
    /></label
  >
  <label>Filename<input bind:value={chapter.filename} on:input={onDirty} placeholder="my_chapter" /></label>
  <label
    >Default quest shape
    <select
      value={chapter.defaultQuestShape ?? ""}
      on:change={(e) => {
        chapter.defaultQuestShape = selectVal(e) || null;
        onDirty();
      }}
    >
      {#each SHAPES as s}
        <option value={s}>{s || "(default)"}</option>
      {/each}
    </select>
  </label>
  <label
    >Default hide dependency lines
    <select
      value={hideDef.value}
      on:change={(e) => hideDef.set(selectVal(e))}
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
      <button type="button" on:click={() => removeExtra(k)}>×</button>
    </div>
  {/each}
  <div class="extra-add">
    <input placeholder="key" bind:value={extraKey} />
    <input placeholder='value or JSON' bind:value={extraVal} />
    <button type="button" on:click={addExtra}>Add</button>
  </div>
</div>

<style>
  .ch-set {
    display: grid;
    gap: 8px;
    padding: 0;
    font-size: 12px;
    background: var(--ftbq-bg-panel, #212126);
    border-left: 1px solid var(--ftbq-border, #3a3a42);
    max-height: 100%;
    overflow: auto;
    min-height: 0;
  }
  .ch-set h4 {
    margin: 0;
    padding: 10px 12px 6px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--ftbq-title-gold, #f2c94c);
    font-weight: 700;
    background: rgba(0, 0, 0, 0.2);
    border-bottom: 1px solid var(--ftbq-border, #3a3a42);
  }
  .ch-set h4:not(:first-child) {
    border-top: 1px solid var(--ftbq-border, #3a3a42);
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
  .ch-set input,
  .ch-set select {
    font-size: 12px;
    background: var(--ftbq-bg, #1a1a1e);
    border: 1px solid var(--ftbq-border, #3a3a42);
    color: var(--ftbq-text, #e8e8e8);
    border-radius: 2px;
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
    background: var(--ftbq-bg, #1a1a1e);
    border: 1px solid var(--ftbq-border, #3a3a42);
    color: var(--ftbq-text, #e8e8e8);
    border-radius: 2px;
  }
</style>
