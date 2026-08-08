<script lang="ts">
  import { Search, X } from "@lucide/svelte";
  import { api } from "../../lib/api";
  import { projectPath } from "../../lib/store";

  let {
    open = false,
    onPick,
    onClose,
  }: {
    open?: boolean;
    onPick: (itemId: string) => void;
    onClose: () => void;
  } = $props();

  let query = $state("");
  let loading = $state(false);
  let error = $state("");
  let catalog = $state<string[]>([]);
  let icons = $state<Record<string, string | null>>({});
  let loadedForPath = $state<string | null>(null);

  let filtered = $derived(filterCatalog(catalog, query).slice(0, 120));

  $effect(() => {
    const path = $projectPath;
    if (!path) return;
    if (loadedForPath !== null && path !== loadedForPath) {
      loadedForPath = null;
      catalog = [];
      error = "";
    }
  });

  $effect(() => {
    if (open && $projectPath && loadedForPath !== $projectPath) {
      void loadCatalog();
    }
  });

  $effect(() => {
    if (open && filtered.length) {
      void preloadIcons(filtered.slice(0, 48));
    }
  });

  async function loadCatalog() {
    const path = $projectPath;
    if (!path) return;
    loading = true;
    error = "";
    try {
      const entries = await api.recipes.listItemCatalog(path);
      catalog = (entries ?? []).map((entry) => entry.id).sort();
      if (catalog.length === 0) {
        const fallback = await api.quests.itemCatalog(path);
        catalog = (fallback ?? []).slice().sort();
      }
      loadedForPath = path;
    } catch (e) {
      error = String(e);
      try {
        const scan = await api.recipes.scan(path);
        const set = new Set<string>();
        for (const r of scan.recipes ?? []) {
          if (r.outputId && !r.outputId.startsWith("#")) set.add(r.outputId);
          for (const id of r.inputIds ?? []) {
            if (id && !id.startsWith("#")) set.add(id);
          }
        }
        catalog = [...set].sort();
        loadedForPath = path;
        error = catalog.length ? "" : String(e);
      } catch (e2) {
        error = String(e2);
      }
    } finally {
      loading = false;
    }
  }

  async function preloadIcons(ids: string[]) {
    const need = ids.filter((id) => icons[id] === undefined);
    if (!need.length || !$projectPath) return;
    for (const id of need) icons[id] = null;
    icons = icons;
    try {
      const batch = await api.recipes.itemIconsBatch(need, $projectPath);
      icons = { ...icons, ...batch };
    } catch {
      /* ignore */
    }
  }

  function filterCatalog(ids: string[], q: string): string[] {
    const t = q.trim().toLowerCase();
    if (!t) return ids;
    const tokens = t.split(/\s+/).filter(Boolean);
    return ids.filter((id) => {
      const low = id.toLowerCase();
      return tokens.every((tok) => {
        if (tok.startsWith("@")) return low.startsWith(tok.slice(1)) || low.includes(`:${tok.slice(1)}`);
        return low.includes(tok);
      });
    });
  }

  function pick(id: string) {
    onPick(id);
    onClose();
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === "Escape") onClose();
  }
</script>

{#if open}
  <div class="overlay" role="dialog" aria-modal="true" onkeydown={onKey}>
    <button type="button" class="backdrop" aria-label="Close" onclick={onClose}></button>
    <div class="panel">
      <div class="panel-h">
        <strong>Pick item</strong>
        <button type="button" class="ico" onclick={onClose}><X size={14} /></button>
      </div>
      <div class="search">
        <Search size={14} />
        <input bind:value={query} placeholder="Search id… (@mod, name)" autofocus />
      </div>
      {#if loading}
        <p class="muted">Loading catalog…</p>
      {:else if error}
        <p class="err">{error}</p>
      {:else}
        <div class="grid">
          {#each filtered as id (id)}
            <button type="button" class="cell" title={id} onclick={() => pick(id)}>
              {#if icons[id]}
                <img src={icons[id]} alt="" />
              {:else}
                <span class="ph">{id.split(":")[1]?.[0] ?? "?"}</span>
              {/if}
              <span class="id">{id}</span>
            </button>
          {/each}
        </div>
        {#if filtered.length === 0}
          <p class="muted">No matches</p>
        {/if}
      {/if}
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    z-index: 80;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .backdrop {
    position: absolute;
    inset: 0;
    border: none;
    background: rgba(0, 0, 0, 0.55);
    cursor: pointer;
  }
  .panel {
    position: relative;
    width: min(560px, 92vw);
    max-height: min(70vh, 640px);
    display: flex;
    flex-direction: column;
    background: var(--ftbq-bg-panel);
    border: 1px solid var(--ftbq-border);
    border-radius: 2px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.5);
    overflow: hidden;
    color: var(--ftbq-text, #e8e8e8);
  }
  .panel-h {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 12px 14px;
    border-bottom: 1px solid var(--ftbq-border);
  }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--ftbq-border);
    color: var(--ftbq-text-muted, #9a9aa0);
  }
  .search input {
    flex: 1;
    border: none;
    background: transparent;
    color: var(--ftbq-text, #e8e8e8);
    font-size: 13px;
    outline: none;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
    gap: 6px;
    padding: 10px;
    overflow: auto;
  }
  .cell {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 8px 6px;
    border-radius: 2px;
    border: 1px solid var(--ftbq-border);
    background: var(--ftbq-bg);
    color: var(--ftbq-text-muted, #9a9aa0);
    cursor: pointer;
    min-height: 72px;
    font-weight: 500;
    box-shadow: none;
  }
  .cell:hover {
    border-color: var(--ftbq-accent-green, #55c95a);
    color: var(--ftbq-text, #e8e8e8);
    background: rgba(85, 201, 90, 0.08);
  }
  .cell img {
    width: 32px;
    height: 32px;
    image-rendering: pixelated;
  }
  .ph {
    width: 32px;
    height: 32px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--ftbq-node-fill);
    border-radius: 2px;
    font-weight: 800;
    color: var(--ftbq-title-gold, #f2c94c);
  }
  .id {
    font-size: 9px;
    word-break: break-all;
    text-align: center;
    line-height: 1.2;
  }
  .muted,
  .err {
    padding: 16px;
    font-size: 12px;
    color: var(--ftbq-text-muted, #9a9aa0);
  }
  .err {
    color: #fca5a5;
  }
  .ico {
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 2px;
    border: 1px solid var(--ftbq-border);
    background: transparent;
    color: var(--ftbq-text-muted, #9a9aa0);
    cursor: pointer;
    box-shadow: none;
  }
</style>
