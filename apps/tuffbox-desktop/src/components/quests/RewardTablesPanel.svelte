<script lang="ts">
  import { Plus, Save, Trash2 } from "@lucide/svelte";
  import type { QuestRewardTable } from "../../lib/api";

  let {
    tables,
    dirty = false,
    saving = false,
    onChange,
    onSave,
    onCreate,
  }: {
    tables: QuestRewardTable[];
    dirty?: boolean;
    saving?: boolean;
    onChange: () => void;
    onSave: (table: QuestRewardTable) => void;
    onCreate: () => void;
  } = $props();

  let selectedId = $state("");

  $effect(() => {
    if (tables.length === 0) {
      selectedId = "";
    } else if (!tables.some((t) => t.id === selectedId)) {
      selectedId = tables[0].id;
    }
  });

  let selected = $derived(tables.find((t) => t.id === selectedId) ?? null);

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
          onclick={() => (selectedId = t.id)}>{t.title || t.id}</button
        >
      {/each}
      <button type="button" class="add" onclick={onCreate}><Plus size={12} /> New table</button>
    </div>
    {#if selected}
      <div class="rt-edit">
        <label>Id<input bind:value={selected.id} oninput={onChange} /></label>
        <label>Title<input bind:value={selected.title} oninput={onChange} placeholder="Optional" /></label>
        <label
          >Empty weight<input
            type="number"
            step="0.1"
            min="0"
            bind:value={selected.emptyWeight}
            oninput={onChange}
          /></label
        >
        <div class="entries-h">
          <strong>Weighted entries</strong>
          <button type="button" class="mini" onclick={addEntry}><Plus size={12} /></button>
        </div>
        {#each selected.entries as entry, i (i)}
          <div class="entry">
            <input
              bind:value={entry.rewardId}
              oninput={onChange}
              placeholder="reward id"
            />
            <input
              type="number"
              step="0.1"
              min="0"
              bind:value={entry.weight}
              oninput={onChange}
              title="Weight (0 = always)"
            />
            <button type="button" class="ico" onclick={() => removeEntry(i)}><Trash2 size={12} /></button>
          </div>
        {/each}
        <button
          type="button"
          class="save"
          disabled={saving || !dirty}
          onclick={() => onSave(selected!)}
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
    border: 1px solid var(--ftbq-border, #3a3a42);
    border-radius: 2px;
    background: var(--ftbq-bg-panel, #212126);
    padding: 0 10px 10px;
    color: var(--ftbq-text, #e8e8e8);
  }
  summary {
    cursor: pointer;
    padding: 10px 4px;
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--ftbq-text-muted, #9a9aa0);
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
    border-radius: 2px;
    border: 1px solid var(--ftbq-border, #3a3a42);
    background: rgba(0, 0, 0, 0.25);
    color: var(--ftbq-text-muted, #9a9aa0);
    font-size: 11px;
    cursor: pointer;
  }
  .rt-side button.sel,
  .rt-side button:hover {
    background: rgba(61, 184, 168, 0.1);
    border-color: var(--ftbq-accent-teal, #3db8a8);
    color: var(--ftbq-text, #e8e8e8);
  }
  .rt-side .add {
    color: var(--ftbq-accent-green, #55c95a);
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
    color: var(--ftbq-text-muted, #9a9aa0);
  }
  .rt-edit input {
    font-size: 12px;
    background: var(--ftbq-bg, #1a1a1e);
    border: 1px solid var(--ftbq-border, #3a3a42);
    color: inherit;
    border-radius: 2px;
    padding: 6px 8px;
  }
  .entries-h {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 11px;
    color: var(--ftbq-text-muted, #9a9aa0);
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
    border-radius: 2px;
    border: 1px solid var(--ftbq-border, #3a3a42);
    background: transparent;
    color: var(--ftbq-text-muted, #9a9aa0);
    cursor: pointer;
  }
  .save {
    padding: 8px;
    color: var(--ftbq-accent-green, #55c95a);
    border-color: rgba(85, 201, 90, 0.35);
    background: rgba(85, 201, 90, 0.08);
    font-weight: 700;
    font-size: 12px;
  }
  .save:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .muted {
    font-size: 12px;
    color: var(--ftbq-text-muted, #9a9aa0);
  }
  @media (max-width: 900px) {
    .rt-body {
      grid-template-columns: 1fr;
    }
  }
</style>
