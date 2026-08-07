<script lang="ts">
  import { Plus, Save, ChevronDown, ChevronRight, MoreVertical } from "@lucide/svelte";
  import { iconDisplayId, type QuestChapter, type QuestChapterGroup } from "../../lib/api";
  import { projectPath } from "../../lib/store";
  import QuestItemIcon from "./QuestItemIcon.svelte";
  import { preloadItemIcons } from "./iconCache";

  let {
    chapters,
    chapterGroups = [],
    selectedChapter,
    dirtyIds,
    saving = false,
    onSelect,
    onCreate,
    onSave,
    onDirty,
    onDelete = null,
    onMove = null,
  }: {
    chapters: QuestChapter[];
    chapterGroups?: QuestChapterGroup[];
    selectedChapter: string;
    dirtyIds: Set<string>;
    saving?: boolean;
    onSelect: (id: string) => void;
    onCreate: () => void;
    onSave: (id: string) => void;
    onDirty: (id: string) => void;
    onDelete?: ((id: string) => void) | null;
    onMove?: ((id: string, dir: -1 | 1) => void) | null;
  } = $props();

  let collapsed = $state<Set<string>>(new Set());
  let editingId = $state<string | null>(null);
  let menuId = $state<string | null>(null);
  let iconRevision = $state(0);

  let groupTitle = $derived(new Map(chapterGroups.map((g) => [g.id, g.title])));
  let groups = $derived(buildGroups(chapters, groupTitle));

  $effect(() => {
    if (chapters && $projectPath) {
      void preloadRailIcons(chapters);
    }
  });

  async function preloadRailIcons(list: QuestChapter[]) {
    const ids = list
      .map((c) => iconDisplayId(c.icon))
      .filter((id): id is string => !!id);
    if (!ids.length || !$projectPath) return;
    await preloadItemIcons(ids, $projectPath);
    iconRevision += 1;
  }

  function buildGroups(list: QuestChapter[], titles: Map<string, string>) {
    const order: string[] = [];
    const map = new Map<string, QuestChapter[]>();
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
    return order
      .filter((key) => (map.get(key)?.length ?? 0) > 0)
      .map((key) => ({
        key,
        label: titles.get(key) || key || "Chapters",
        chapters: map.get(key)!,
      }));
  }

  function toggleGroup(key: string) {
    const next = new Set(collapsed);
    if (next.has(key)) next.delete(key);
    else next.add(key);
    collapsed = next;
  }

  function glyph(ch: QuestChapter): string {
    const icon = iconDisplayId(ch.icon);
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
    }
    editingId = null;
  }

  function stripMc(s: string): string {
    return s.replace(/§[0-9a-fk-or]/gi, "").replace(/&[0-9a-fk-or]/gi, "").trim();
  }

  function inputVal(e: Event): string {
    return (e.currentTarget as HTMLInputElement).value;
  }
</script>

<aside class="rail ftbq-rail">
  <div class="rail-h">
    <h3>Chapters</h3>
    <button type="button" class="ico" title="Add chapter" onclick={onCreate}>
      <Plus size={14} />
    </button>
  </div>

  <div class="rail-list">
    {#each groups as g (g.key)}
      {#if groups.length > 1 || g.key}
        <button type="button" class="group-h" onclick={() => toggleGroup(g.key)}>
          {#if collapsed.has(g.key)}<ChevronRight size={12} />{:else}<ChevronDown size={12} />{/if}
          <span>{g.label}</span>
        </button>
      {/if}
      {#if !collapsed.has(g.key)}
        {#each g.chapters as ch (ch.id)}
          <div
            class="ch-row-wrap"
            class:sel={selectedChapter === ch.id}
            class:dirty={dirtyIds.has(ch.id)}
          >
            <button
              type="button"
              class="ch-row"
              onclick={() => {
                onSelect(ch.id);
                menuId = null;
              }}
              ondblclick={() => (editingId = ch.id)}
              onkeydown={(e) => {
                if (e.key === "Delete" && onDelete) {
                  e.preventDefault();
                  onDelete(ch.id);
                }
              }}
            >
              <span class="glyph" title={iconDisplayId(ch.icon) || ""}>
                <QuestItemIcon
                  itemId={iconDisplayId(ch.icon)}
                  fallback={glyph(ch)}
                  size={16}
                  revision={iconRevision}
                />
              </span>
              <span class="ch-text">
                {#if editingId === ch.id}
                  <input
                    class="title-edit"
                    value={ch.title}
                    autofocus
                    onclick={(e) => e.stopPropagation()}
                    onkeydown={(e) => {
                      if (e.key === "Enter") commitTitle(ch, inputVal(e));
                      if (e.key === "Escape") editingId = null;
                    }}
                    onblur={(e) => commitTitle(ch, inputVal(e))}
                  />
                {:else}
                  <strong>{stripMc(ch.title)}</strong>
                {/if}
                <span>{ch.quests.length} quests</span>
              </span>
              {#if dirtyIds.has(ch.id)}<span class="dot" title="Unsaved">●</span>{/if}
            </button>
            {#if onMove || onDelete}
              <div class="ch-menu-wrap">
                <button
                  type="button"
                  class="ico tiny"
                  title="Chapter actions"
                  onclick={(e) => {
                    e.stopPropagation();
                    menuId = menuId === ch.id ? null : ch.id;
                  }}
                >
                  <MoreVertical size={12} />
                </button>
                {#if menuId === ch.id}
                  <div class="ch-menu">
                    {#if onMove}
                      <button type="button" onclick={() => { onMove?.(ch.id, -1); menuId = null; }}>Move up</button>
                      <button type="button" onclick={() => { onMove?.(ch.id, 1); menuId = null; }}>Move down</button>
                    {/if}
                    {#if onDelete}
                      <button type="button" class="danger" onclick={() => { onDelete?.(ch.id); menuId = null; }}
                        >Delete…</button
                      >
                    {/if}
                  </div>
                {/if}
              </div>
            {/if}
          </div>
        {/each}
      {/if}
    {/each}
  </div>

  {#if selectedChapter && dirtyIds.has(selectedChapter)}
    <button type="button" class="save-ch" disabled={saving} onclick={() => onSave(selectedChapter)}>
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
    border-right: 1px solid #101014;
    box-shadow: inset -1px 0 0 rgba(255, 255, 255, 0.05);
    padding: 0;
    min-height: 0;
    height: 100%;
  }
  .rail-h {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 7px 8px;
    border-bottom: 1px solid #101014;
    background: linear-gradient(180deg, rgba(255, 255, 255, 0.04), rgba(0, 0, 0, 0.22));
    box-shadow: inset 0 -1px 0 rgba(255, 255, 255, 0.05);
  }
  .rail-h h3 {
    margin: 0;
    color: var(--ftbq-title-gold, #f2c94c);
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    font-weight: 700;
    text-shadow: 1px 1px 0 rgba(0, 0, 0, 0.7);
  }
  .ico {
    width: 24px;
    height: 24px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 3px;
    border: 1px solid #101014;
    background: linear-gradient(180deg, #3a3a42, #2a2a31);
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.12),
      inset 0 -1px 0 rgba(0, 0, 0, 0.45);
    color: var(--ftbq-text-muted, #9a9aa0);
    cursor: pointer;
  }
  .ico:hover {
    color: var(--ftbq-text, #e8e8e8);
    border-color: #101014;
    background: linear-gradient(180deg, #46464f, #32323a);
    filter: brightness(1.08);
  }
  .ico:active {
    box-shadow: inset 0 1px 3px rgba(0, 0, 0, 0.5);
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
    display: flex;
    align-items: stretch;
    border: none;
    border-left: 3px solid transparent;
  }
  .ch-row-wrap.sel {
    background: linear-gradient(90deg, rgba(85, 201, 90, 0.16), rgba(85, 201, 90, 0.05));
    border-left-color: var(--ftbq-accent-green, #55c95a);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.04), inset 0 -1px 0 rgba(0, 0, 0, 0.3);
  }
  .ch-row-wrap.dirty:not(.sel) {
    border-left-color: rgba(242, 201, 76, 0.4);
  }
  .ch-row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1;
    min-width: 0;
    margin: 0;
    padding: 6px 10px;
    border: none;
    border-radius: 0;
    background: transparent;
    color: var(--ftbq-text, #e8e8e8);
    font-weight: 500;
    text-align: left;
    cursor: pointer;
    box-shadow: none;
  }
  .ch-row:hover {
    background: rgba(255, 255, 255, 0.04);
  }
  .ch-row-wrap.sel .ch-row {
    color: var(--ftbq-text, #e8e8e8);
  }
  .ch-menu-wrap {
    position: relative;
    padding: 4px 4px 4px 0;
    opacity: 0.4;
  }
  .ch-row-wrap:hover .ch-menu-wrap,
  .ch-row-wrap.sel .ch-menu-wrap {
    opacity: 1;
  }
  .ch-menu {
    position: absolute;
    right: 0;
    top: 100%;
    z-index: 20;
    min-width: 120px;
    display: flex;
    flex-direction: column;
    background: var(--ftbq-bg-panel, #212126);
    border: 1px solid #101014;
    border-radius: 3px;
    box-shadow:
      inset 0 0 0 1px rgba(255, 255, 255, 0.06),
      0 8px 20px rgba(0, 0, 0, 0.55);
  }
  .ch-menu button {
    text-align: left;
    border: none;
    background: transparent;
    color: var(--ftbq-text, #e8e8e8);
    padding: 8px 10px;
    font-size: 11px;
    cursor: pointer;
  }
  .ch-menu button:hover {
    background: rgba(61, 184, 168, 0.12);
  }
  .ch-menu button.danger {
    color: #f87171;
  }
  .ico.tiny {
    width: 18px;
    height: 18px;
    font-size: 11px;
    padding: 0;
  }
  .glyph {
    width: 26px;
    height: 26px;
    flex-shrink: 0;
    border-radius: 3px;
    background: #141419;
    border: 1px solid #0c0c0f;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 11px;
    font-weight: 800;
    color: var(--ftbq-text, #e8e8e8);
    box-shadow:
      inset 2px 2px 0 rgba(0, 0, 0, 0.55),
      inset -1px -1px 0 rgba(255, 255, 255, 0.08);
  }
  .ch-row-wrap.sel .glyph {
    box-shadow:
      inset 2px 2px 0 rgba(0, 0, 0, 0.55),
      inset -1px -1px 0 rgba(255, 255, 255, 0.08),
      0 0 6px rgba(85, 201, 90, 0.45);
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
    border-radius: 3px;
    border: 1px solid #12380f;
    background: linear-gradient(180deg, #4fae53, #35833a);
    box-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.25),
      inset 0 -1px 0 rgba(0, 0, 0, 0.35);
    color: #eaffe9;
    text-shadow: 1px 1px 0 rgba(0, 0, 0, 0.5);
    font-size: 11px;
    font-weight: 700;
    cursor: pointer;
  }
  .save-ch:hover:not(:disabled) {
    filter: brightness(1.12);
  }
  .save-ch:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
