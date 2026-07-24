<script lang="ts">
  import { Plus, Save, ChevronDown, ChevronRight } from "lucide-svelte";
  import type { QuestChapter, QuestChapterGroup } from "../../lib/api";

  export let chapters: QuestChapter[];
  export let chapterGroups: QuestChapterGroup[] = [];
  export let selectedChapter: string;
  export let dirtyIds: Set<string>;
  export let saving = false;
  export let onSelect: (id: string) => void;
  export let onCreate: () => void;
  export let onSave: (id: string) => void;
  export let onDirty: (id: string) => void;

  let collapsed = new Set<string>();
  let editingId: string | null = null;

  $: groupTitle = new Map(chapterGroups.map((g) => [g.id, g.title]));
  $: groups = buildGroups(chapters, groupTitle);

  function buildGroups(list: QuestChapter[], titles: Map<string, string>) {
    const order: string[] = [];
    const map = new Map<string, QuestChapter[]>();
    // Prefer declared group order from chapter_groups.snbt
    for (const g of chapterGroups) {
      if (!map.has(g.id)) {
        map.set(g.id, []);
        order.push(g.id);
      }
    }
    for (const ch of list) {
      const g = (ch.group && ch.group.trim()) || "";
      if (!map.has(g)) {
        map.set(g, []);
        order.push(g);
      }
      map.get(g)!.push(ch);
    }
    // Drop empty declared groups with no chapters
    return order
      .filter((key) => (map.get(key)?.length ?? 0) > 0)
      .map((key) => ({
        key,
        label: titles.get(key) || key || "Chapters",
        chapters: map.get(key)!,
      }));
  }

  function toggleGroup(key: string) {
    if (collapsed.has(key)) collapsed.delete(key);
    else collapsed.add(key);
    collapsed = collapsed;
  }

  function glyph(ch: QuestChapter): string {
    const icon = ch.icon?.trim();
    if (icon) {
      const leaf = icon.includes(":") ? icon.split(":").pop()! : icon;
      return (leaf[0] || "?").toUpperCase();
    }
    return (stripMc(ch.title)[0] || "?").toUpperCase();
  }

  function commitTitle(ch: QuestChapter, value: string) {
    const next = value.trim();
    if (next && next !== ch.title) {
      ch.title = next;
      onDirty(ch.id);
      chapters = chapters;
    }
    editingId = null;
  }

  function stripMc(s: string): string {
    return s.replace(/§[0-9a-fk-or]/gi, "").replace(/&[0-9a-fk-or]/gi, "").trim();
  }
</script>

<aside class="rail ftbq-rail">
  <div class="rail-h">
    <h3>Chapters</h3>
    <button type="button" class="ico" title="Add chapter" on:click={onCreate}>
      <Plus size={14} />
    </button>
  </div>

  <div class="rail-list">
    {#each groups as g (g.key)}
      {#if groups.length > 1 || g.key}
        <button type="button" class="group-h" on:click={() => toggleGroup(g.key)}>
          {#if collapsed.has(g.key)}<ChevronRight size={12} />{:else}<ChevronDown size={12} />{/if}
          <span>{g.label}</span>
        </button>
      {/if}
      {#if !collapsed.has(g.key)}
        {#each g.chapters as ch (ch.id)}
          <div class="ch-row-wrap" class:sel={selectedChapter === ch.id} class:dirty={dirtyIds.has(ch.id)}>
            <button
              type="button"
              class="ch-row"
              on:click={() => onSelect(ch.id)}
              on:dblclick={() => (editingId = ch.id)}
            >
              <span class="glyph" title={ch.icon || ""}>{glyph(ch)}</span>
              <span class="ch-text">
                {#if editingId === ch.id}
                  <input
                    class="title-edit"
                    value={ch.title}
                    autofocus
                    on:click|stopPropagation
                    on:keydown={(e) => {
                      if (e.key === "Enter") commitTitle(ch, (e.target as HTMLInputElement).value);
                      if (e.key === "Escape") editingId = null;
                    }}
                    on:blur={(e) => commitTitle(ch, (e.target as HTMLInputElement).value)}
                  />
                {:else}
                  <strong>{stripMc(ch.title)}</strong>
                {/if}
                <span>{ch.quests.length} quests</span>
              </span>
              {#if dirtyIds.has(ch.id)}<span class="dot" title="Unsaved">●</span>{/if}
            </button>
          </div>
        {/each}
      {/if}
    {/each}
  </div>

  {#if selectedChapter && dirtyIds.has(selectedChapter)}
    <button type="button" class="save-ch" disabled={saving} on:click={() => onSave(selectedChapter)}>
      <Save size={14} /> Save chapter
    </button>
  {/if}
</aside>

<style>
  .rail {
    display: flex;
    flex-direction: column;
    gap: 0;
    background: var(--ftbq-bg-panel, #212126);
    border-right: 1px solid var(--ftbq-border, #3a3a42);
    padding: 0;
    min-height: 0;
    height: 100%;
  }
  .rail-h {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 10px 8px;
    border-bottom: 1px solid var(--ftbq-border, #3a3a42);
    background: rgba(0, 0, 0, 0.15);
  }
  .rail-h h3 {
    margin: 0;
    color: var(--ftbq-text-muted, #9a9aa0);
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-weight: 700;
  }
  .ico {
    width: 24px;
    height: 24px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 2px;
    border: 1px solid var(--ftbq-border, #3a3a42);
    background: rgba(0, 0, 0, 0.25);
    color: var(--ftbq-text-muted, #9a9aa0);
    cursor: pointer;
  }
  .ico:hover {
    color: var(--ftbq-text, #e8e8e8);
    border-color: var(--ftbq-accent-teal, #3db8a8);
    background: rgba(61, 184, 168, 0.1);
  }
  .rail-list {
    flex: 1;
    overflow: auto;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 0;
    padding: 4px 0;
  }
  .group-h {
    display: flex;
    align-items: center;
    gap: 4px;
    width: 100%;
    padding: 8px 10px 4px;
    border: none;
    background: transparent;
    color: var(--ftbq-text-muted, #9a9aa0);
    font-size: 9px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    cursor: pointer;
    font-weight: 700;
  }
  .group-h:hover {
    color: var(--ftbq-text, #e8e8e8);
  }
  .ch-row-wrap {
    position: relative;
    border: none;
    border-left: 3px solid transparent;
  }
  .ch-row-wrap.sel {
    background: linear-gradient(
      90deg,
      rgba(61, 184, 168, 0.18) 0%,
      rgba(85, 201, 90, 0.08) 60%,
      transparent 100%
    );
    border-left-color: var(--ftbq-accent-teal, #3db8a8);
    box-shadow: inset 3px 0 0 var(--ftbq-accent-green, #55c95a);
  }
  .ch-row-wrap.sel::before {
    content: "";
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    width: 3px;
    background: linear-gradient(
      180deg,
      var(--ftbq-accent-teal, #3db8a8),
      var(--ftbq-accent-green, #55c95a)
    );
    pointer-events: none;
  }
  .ch-row-wrap.dirty:not(.sel) {
    border-left-color: rgba(242, 201, 76, 0.4);
  }
  .ch-row {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    text-align: left;
    padding: 6px 10px 6px 12px;
    border: none;
    background: transparent;
    color: var(--ftbq-text, #e8e8e8);
    cursor: pointer;
    position: relative;
  }
  .ch-row:hover {
    background: rgba(255, 255, 255, 0.04);
  }
  .ch-row-wrap.sel .ch-row {
    color: var(--ftbq-text, #e8e8e8);
  }
  .glyph {
    width: 24px;
    height: 24px;
    flex-shrink: 0;
    border-radius: 2px;
    background: var(--ftbq-node-fill, #18181c);
    border: 2px solid var(--ftbq-border, #3a3a42);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 11px;
    font-weight: 800;
    color: var(--ftbq-text, #e8e8e8);
    box-shadow: inset 0 1px 3px rgba(0, 0, 0, 0.4);
  }
  .ch-row-wrap.sel .glyph {
    border-color: var(--ftbq-accent-green, #55c95a);
  }
  .ch-text {
    display: grid;
    gap: 1px;
    min-width: 0;
    flex: 1;
  }
  .ch-text strong {
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 600;
  }
  .ch-text span {
    font-size: 9px;
    color: var(--ftbq-text-muted, #9a9aa0);
  }
  .title-edit {
    font-size: 12px;
    font-weight: 600;
    padding: 2px 4px;
    width: 100%;
    background: var(--ftbq-bg, #1a1a1e);
    border: 1px solid var(--ftbq-border, #3a3a42);
    color: var(--ftbq-text, #e8e8e8);
  }
  .dot {
    color: var(--ftbq-quest-started, #f2c94c);
    font-size: 10px;
  }
  .save-ch {
    width: calc(100% - 16px);
    margin: 8px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 7px;
    border-radius: 2px;
    border: 1px solid var(--ftbq-accent-green, #55c95a);
    background: rgba(85, 201, 90, 0.12);
    color: var(--ftbq-quest-completed, #55c95a);
    font-size: 11px;
    font-weight: 700;
    cursor: pointer;
  }
  .save-ch:hover:not(:disabled) {
    background: rgba(85, 201, 90, 0.2);
  }
  .save-ch:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
