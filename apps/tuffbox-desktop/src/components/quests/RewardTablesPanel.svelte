<script lang="ts">
  import { Plus, Save, Trash2 } from "lucide-svelte";
  import type { QuestRewardTable } from "../../lib/api";

  export let tables: QuestRewardTable[];
  export let dirty = false;
  export let saving = false;
  export let onChange: () => void;
  export let onSave: (table: QuestRewardTable) => void;
  export let onCreate: () => void;

  let selectedId = "";

  $: if (tables.length === 0) {
    selectedId = "";
  } else if (!tables.some((t) => t.id === selectedId)) {
    selectedId = tables[0].id;
  }
  $: selected = tables.find((t) => t.id === selectedId) ?? null;

  function addEntry() {
    if (!selected) return;
    selected.entries = [...selected.entries, { rewardId: "reward_id", weight: 1 }];
    onChange();
  }

  function removeEntry(i: number) {
    if (!selected) return;
    selected.entries = selected.entries.filter((_, idx) => idx !== i);
    onChange();
  }
</script>

<details class="rt">
  <summary>Reward tables ({tables.length})</summary>
  <div class="rt-body">
    <div class="rt-side">
      {#each tables as t (t.id)}
        <button
          type="button"
          class:sel={selected?.id === t.id}
          on:click={() => (selectedId = t.id)}>{t.title || t.id}</button
        >
      {/each}
      <button type="button" class="add" on:click={onCreate}><Plus size={12} /> New table</button>
    </div>
    {#if selected}
      <div class="rt-edit">
        <label>Id<input bind:value={selected.id} on:input={onChange} /></label>
        <label>Title<input bind:value={selected.title} on:input={onChange} placeholder="Optional" /></label>
        <label
          >Empty weight<input
            type="number"
            step="0.1"
            min="0"
            bind:value={selected.emptyWeight}
            on:input={onChange}
          /></label
        >
        <div class="entries-h">
          <strong>Weighted entries</strong>
          <button type="button" class="mini" on:click={addEntry}><Plus size={12} /></button>
        </div>
        {#each selected.entries as entry, i (i)}
          <div class="entry">
            <input
              bind:value={entry.rewardId}
              on:input={onChange}
              placeholder="reward id"
            />
            <input
              type="number"
              step="0.1"
              min="0"
              bind:value={entry.weight}
              on:input={onChange}
              title="Weight (0 = always)"
            />
            <button type="button" class="ico" on:click={() => removeEntry(i)}><Trash2 size={12} /></button>
          </div>
        {/each}
        <button
          type="button"
          class="save"
          disabled={saving || !dirty}
          on:click={() => onSave(selected)}
        >
          <Save size={12} /> Save table
        </button>
      </div>
    {:else}
      <p class="muted">No reward tables. Create one or add SNBT under reward_tables/.</p>
    {/if}
  </div>
</details>

<style>
  .rt {
    margin-top: 10px;
    border: 1px solid var(--border-color);
    border-radius: 10px;
    background: rgba(12, 14, 18, 0.7);
    padding: 0 10px 10px;
  }
  summary {
    cursor: pointer;
    padding: 10px 4px;
    font-size: 12px;
    font-weight: 700;
    color: var(--text-secondary);
  }
  .rt-body {
    display: grid;
    grid-template-columns: 160px 1fr;
    gap: 10px;
  }
  .rt-side {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .rt-side button {
    text-align: left;
    padding: 6px 8px;
    border-radius: 6px;
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-secondary);
    font-size: 11px;
    cursor: pointer;
  }
  .rt-side button.sel,
  .rt-side button:hover {
    background: rgba(27, 217, 106, 0.08);
    border-color: rgba(27, 217, 106, 0.25);
    color: var(--text-primary);
  }
  .rt-side .add {
    color: var(--accent-primary);
  }
  .rt-edit {
    display: grid;
    gap: 8px;
  }
  .rt-edit label {
    display: grid;
    gap: 3px;
    font-size: 10px;
    text-transform: uppercase;
    color: var(--text-muted);
  }
  .rt-edit input {
    font-size: 12px;
  }
  .entries-h {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 11px;
  }
  .entry {
    display: grid;
    grid-template-columns: 1fr 72px 28px;
    gap: 4px;
  }
  .mini,
  .ico,
  .save {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 4px;
    border-radius: 6px;
    border: 1px solid var(--border-color);
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
  }
  .save {
    padding: 8px;
    color: var(--accent-primary);
    border-color: rgba(27, 217, 106, 0.35);
    background: rgba(27, 217, 106, 0.08);
    font-weight: 700;
    font-size: 12px;
  }
  .save:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .muted {
    font-size: 12px;
    color: var(--text-muted);
  }
  @media (max-width: 900px) {
    .rt-body {
      grid-template-columns: 1fr;
    }
  }
</style>
