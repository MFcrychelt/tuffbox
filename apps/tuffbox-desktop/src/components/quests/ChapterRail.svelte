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
  let titleEditEl = $state<HTMLInputElement | null>(null);
  let menuId = $state<string | null>(null);
  let menuPos = $state<{ top: number; left: number } | null>(null);
  let iconRevision = $state(0);

  let groupTitle = $derived(new Map(chapterGroups.map((g) => [g.id, g.title])));
  let groups = $derived(buildGroups(chapters, groupTitle));

  $effect(() => {
    if (editingId && titleEditEl) {
      titleEditEl.focus();
      titleEditEl.select();
    }
  });

  $effect(() => {
    if (chapters && $projectPath) {
      void preloadRailIcons(chapters);
    }
  });

  $effect(() => {
    if (!menuId) {
      menuPos = null;
      return;
    }
    const onPtr = (e: PointerEvent) => {
      const el = e.target as HTMLElement | null;
      if (!el?.closest?.(".ch-menu-wrap") && !el?.closest?.(".ch-menu")) menuId = null;
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        menuId = null;
      }
    };
    window.addEventListener("pointerdown", onPtr, true);
    window.addEventListener("keydown", onKey, true);
    return () => {
      window.removeEventListener("pointerdown", onPtr, true);
      window.removeEventListener("keydown", onKey, true);
    };
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
      <Plus size={14} class="flex-shrink-0" />
    </button>
  </div>

  <div class="rail-list">
    {#each groups as g (g.key)}
      {#if groups.length > 1 || g.key}
        <button type="button" class="group-h" onclick={() => toggleGroup(g.key)}>
          {#if collapsed.has(g.key)}<ChevronRight size={12} class="flex-shrink-0" />{:else}<ChevronDown size={12} class="flex-shrink-0" />{/if}
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
                    bind:this={titleEditEl}
                    value={ch.title}
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
                  aria-haspopup="menu"
                  aria-expanded={menuId === ch.id}
                  aria-label={`Chapter actions for ${stripMc(ch.title)}`}
                  onclick={(e) => {
                    e.stopPropagation();
                    if (menuId === ch.id) {
                      menuId = null;
                      return;
                    }
                    const r = (e.currentTarget as HTMLElement).getBoundingClientRect();
                    const width = 128;
                    menuPos = {
                      top: r.bottom + 4,
                      left: Math.max(8, Math.min(r.right - width, window.innerWidth - width - 8)),
                    };
                    menuId = ch.id;
                  }}
                >
                  <MoreVertical size={12} class="flex-shrink-0" />
                </button>
                {#if menuId === ch.id && menuPos}
                  <div
                    class="ch-menu"
                    role="menu"
                    style="top: {menuPos.top}px; left: {menuPos.left}px;"
                  >
                    {#if onMove}
                      <button type="button" role="menuitem" onclick={() => { onMove?.(ch.id, -1); menuId = null; }}>Move up</button>
                      <button type="button" role="menuitem" onclick={() => { onMove?.(ch.id, 1); menuId = null; }}>Move down</button>
                    {/if}
                    {#if onDelete}
                      <button type="button" role="menuitem" class="danger" onclick={() => { onDelete?.(ch.id); menuId = null; }}
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
      <Save size={14} class="flex-shrink-0" /> Save chapter
    </button>
  {/if}
</aside>

<style>
  .rail {
    display: flex;
    flex-direction: column;
    gap: 0;
    background: var(--ftbq-bg-panel);
    border-right: 1px solid var(--ftbq-frame);
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
    border-bottom: 1px solid var(--ftbq-frame);
    background: var(--ftbq-bg-panel);
  }
  .rail-h h3 {
    margin: 0;
    color: var(--text-muted, var(--ftbq-text-muted, #868e96));
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.1em;
    font-weight: 600;
    text-shadow: none;
  }
  .ico {
    width: 24px;
    height: 24px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 6px;
    border: 1px solid var(--ftbq-frame);
    background: var(--bg-secondary, var(--ftbq-bg-panel));
    box-shadow: none;
    color: var(--ftbq-text-muted, #9a9aa0);
    cursor: pointer;
  }
  .ico:hover {
    color: var(--ftbq-text, #e8e8e8);
    border-color: var(--ftbq-frame);
    background: var(--bg-hover, var(--ftbq-btn-hover-top));
    filter: none;
  }
  .ico:active {
    background: var(--bg-active, var(--ftbq-btn-hover-bottom));
    box-shadow: none;
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
    background: color-mix(in srgb, var(--ftbq-accent-green) 10%, transparent);
    border-left-color: var(--ftbq-accent-green);
    box-shadow: none;
  }
  .ch-row-wrap.dirty:not(.sel) {
    border-left-color: color-mix(in srgb, var(--ftbq-quest-started) 40%, transparent);
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
    background: var(--bg-hover, color-mix(in srgb, var(--ftbq-text) 6%, transparent));
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
    position: fixed;
    z-index: 80;
    min-width: 120px;
    display: flex;
    flex-direction: column;
    background: var(--ftbq-bg-panel);
    border: 1px solid var(--ftbq-frame);
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
    background: color-mix(in srgb, var(--ftbq-accent-teal) 12%, transparent);
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
    border-radius: 6px;
    background: var(--ftbq-input-bg);
    border: 1px solid var(--ftbq-frame);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 11px;
    font-weight: 800;
    color: var(--ftbq-text, #e8e8e8);
    box-shadow: none;
  }
  .ch-row-wrap.sel .glyph {
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--ftbq-accent-green) 35%, transparent);
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
    background: var(--ftbq-bg);
    border: 1px solid var(--ftbq-border);
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
    border-radius: 6px;
    border: 1px solid color-mix(in srgb, var(--accent-primary) 45%, var(--ftbq-frame));
    background: var(--accent-primary);
    box-shadow: none;
    color: #fff;
    text-shadow: none;
    font-size: 11px;
    font-weight: 700;
    cursor: pointer;
  }
  .save-ch:hover:not(:disabled) {
    filter: none;
    background: var(--accent-hover, var(--accent-primary));
  }
  .save-ch:disabled {
    opacity: 0.5;
    cursor: default;
  }

  :global(.ftbq-rail svg) {
    flex-shrink: 0;
  }
</style>
