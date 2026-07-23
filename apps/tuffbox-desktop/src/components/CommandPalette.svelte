<script lang="ts">
  import { createEventDispatcher, onMount, tick } from "svelte";
  import { Search, ArrowRight, CornerDownLeft } from "lucide-svelte";
  import { trapFocus } from "../lib/focusTrap";
  import { recentProjects } from "../lib/store";

  const dispatch = createEventDispatcher<{ close: void; navigate: string }>();

  type Item = { id: string; label: string; category: string; shortcut?: string };
  
  let query = "";
  let inputEl: HTMLInputElement;
  let selectedIndex = 0;

  const allItems: Item[] = [
    { id: "dashboard", label: "Launcher", category: "Views", shortcut: "Ctrl+1" },
    { id: "ide", label: "Open IDE", category: "Views", shortcut: "Ctrl+2" },
    { id: "mods", label: "Mods", category: "Views", shortcut: "Ctrl+3" },
    { id: "graph", label: "Dependency Graph", category: "Views", shortcut: "Ctrl+4" },
    { id: "configs", label: "Config Editor", category: "Views", shortcut: "Ctrl+5" },
    { id: "diagnostics", label: "Health Check", category: "Views", shortcut: "Ctrl+6" },
    { id: "crash-votes", label: "Crash Votes", category: "Views" },
    { id: "snapshots", label: "Snapshots", category: "Views", shortcut: "Ctrl+7" },
    { id: "world", label: "World", category: "Views", shortcut: "Ctrl+8" },
    { id: "library", label: "Library", category: "Views" },
    { id: "me", label: "Me", category: "Views" },
    { id: "ore-gen", label: "Ore Distribution", category: "Views" },
    { id: "recipes", label: "Recipe Browser", category: "Views" },
    { id: "quests", label: "Quest Editor", category: "Views" },
    { id: "settings", label: "Settings", category: "Actions" },
    { id: "project-settings", label: "Project Settings", category: "Actions" },
    { id: "new-instance", label: "Create New Instance", category: "Actions" },
    { id: "shortcuts", label: "Keyboard Shortcuts", category: "Actions" },
  ];

  $: filtered = query.trim()
    ? allItems.filter(
        (item) =>
          item.label.toLowerCase().includes(query.toLowerCase()) ||
          item.category.toLowerCase().includes(query.toLowerCase())
      )
    : allItems;

  $: grouped = groupItems(filtered);
  $: flatList = filtered;
  $: if (selectedIndex >= flatList.length) selectedIndex = Math.max(0, flatList.length - 1);

  function groupItems(items: Item[]) {
    const groups: Record<string, Item[]> = {};
    for (const item of items) {
      (groups[item.category] ??= []).push(item);
    }
    return groups;
  }

  function handleKeydown(e: KeyboardEvent) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      selectedIndex = Math.min(selectedIndex + 1, flatList.length - 1);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      selectedIndex = Math.max(selectedIndex - 1, 0);
    } else if (e.key === "Enter" && flatList[selectedIndex]) {
      e.preventDefault();
      selectItem(flatList[selectedIndex]);
    }
  }

  function selectItem(item: Item) {
    dispatch("navigate", item.id);
    dispatch("close");
  }

  function scrollToSelected() {
    const el = document.querySelector(`.cmd-item[data-index="${selectedIndex}"]`);
    el?.scrollIntoView({ block: "nearest" });
  }

  $: if (selectedIndex >= 0) tick().then(scrollToSelected);

  onMount(() => {
    inputEl?.focus();
  });
</script>

<div class="cmd-backdrop" role="button" tabindex="-1" on:click={(e) => e.target === e.currentTarget && dispatch("close")} on:keydown={() => {}}>
  <div class="cmd-dialog" role="dialog" aria-modal="true" aria-label="Command palette" use:trapFocus={{ onEscape: () => dispatch("close") }}>
    <div class="cmd-input-wrap">
      <Search size={18} class="cmd-search-icon" />
      <input
        bind:this={inputEl}
        bind:value={query}
        class="cmd-input"
        type="text"
        placeholder="Search views, actions..."
        spellcheck="false"
        on:keydown={handleKeydown}
      />
      <kbd class="cmd-esc">ESC</kbd>
    </div>

    <div class="cmd-results">
      {#each Object.entries(grouped) as [category, items]}
        <div class="cmd-group">
          <div class="cmd-group-label">{category}</div>
          {#each items as item, i}
            {@const globalIdx = flatList.indexOf(item)}
            <button
              class="cmd-item"
              class:selected={globalIdx === selectedIndex}
              data-index={globalIdx}
              on:click={() => selectItem(item)}
              on:mouseenter={() => (selectedIndex = globalIdx)}
            >
              <span class="cmd-item-label">{item.label}</span>
              <span class="cmd-item-right">
                {#if item.shortcut}
                  <kbd>{item.shortcut}</kbd>
                {/if}
                <ArrowRight size={14} class="cmd-item-arrow" />
              </span>
            </button>
          {/each}
        </div>
      {:else}
        <div class="cmd-empty">No results for "{query}"</div>
      {/each}
    </div>

    <div class="cmd-footer">
      <span><kbd>↑↓</kbd> navigate</span>
      <span><CornerDownLeft size={12} /> select</span>
      <span><kbd>esc</kbd> close</span>
    </div>
  </div>
</div>

<style>
  .cmd-backdrop {
    position: fixed; inset: 0; background: rgba(0, 0, 0, 0.55);
    display: flex; align-items: flex-start; justify-content: center;
    padding-top: min(20vh, 160px);
    z-index: 300; backdrop-filter: blur(8px);
  }

  .cmd-dialog {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-xl);
    width: 520px;
    max-height: 440px;
    box-shadow: var(--shadow-lg), 0 0 60px rgba(27, 217, 106, 0.08);
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .cmd-input-wrap {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 16px 20px;
    border-bottom: 1px solid var(--border-color);
  }

  .cmd-input-wrap :global(.cmd-search-icon) {
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .cmd-input {
    flex: 1;
    background: transparent;
    border: none;
    color: var(--text-primary);
    font-size: 16px;
    font-family: inherit;
    outline: none;
    padding: 0;
  }

  .cmd-input::placeholder {
    color: var(--text-muted);
  }

  .cmd-esc {
    font-size: 11px;
    color: var(--text-muted);
    background: var(--bg-tertiary);
    border: 1px solid var(--border-color);
    border-radius: 4px;
    padding: 2px 6px;
    font-family: inherit;
    flex-shrink: 0;
  }

  .cmd-results {
    overflow-y: auto;
    flex: 1;
    padding: 8px;
  }

  .cmd-group {
    margin-bottom: 4px;
  }

  .cmd-group-label {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 6px 12px 4px;
  }

  .cmd-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    padding: 10px 12px;
    background: transparent;
    border: none;
    border-radius: var(--border-radius-sm);
    color: var(--text-secondary);
    font-size: 14px;
    cursor: pointer;
    text-align: left;
    transition: background 0.08s ease;
  }

  .cmd-item:hover,
  .cmd-item.selected {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  .cmd-item.selected {
    background: rgba(27, 217, 106, 0.1);
  }

  .cmd-item-label {
    flex: 1;
  }

  .cmd-item-right {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text-muted);
  }

  .cmd-item-right kbd {
    font-size: 11px;
    background: var(--bg-tertiary);
    border: 1px solid var(--border-color);
    border-radius: 4px;
    padding: 1px 5px;
    font-family: inherit;
  }

  .cmd-item-right :global(.cmd-item-arrow) {
    opacity: 0;
    transition: opacity 0.1s;
  }

  .cmd-item.selected .cmd-item-right :global(.cmd-item-arrow) {
    opacity: 1;
    color: var(--accent-primary);
  }

  .cmd-empty {
    padding: 32px;
    text-align: center;
    color: var(--text-muted);
    font-size: 13px;
  }

  .cmd-footer {
    display: flex;
    align-items: center;
    gap: 16px;
    padding: 10px 20px;
    border-top: 1px solid var(--border-color);
    font-size: 12px;
    color: var(--text-muted);
  }

  .cmd-footer span {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .cmd-footer kbd {
    font-size: 10px;
    background: var(--bg-tertiary);
    border: 1px solid var(--border-color);
    border-radius: 3px;
    padding: 1px 4px;
    font-family: inherit;
  }
</style>
