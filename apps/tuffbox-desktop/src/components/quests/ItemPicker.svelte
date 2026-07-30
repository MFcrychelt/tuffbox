<script lang="ts">
  import { onMount } from "svelte";
  import { Search, X } from "lucide-svelte";
  import { api } from "../../lib/api";
  import { projectPath } from "../../lib/store";

  export let open = false;
  export let onPick: (itemId: string) => void;
  export let onClose: () => void;

  let query = "";
  let loading = false;
  let error = "";
  let catalog: string[] = [];
  let icons: Record<string, string | null> = {};
  let loaded = false;

  $: filtered = filterCatalog(catalog, query).slice(0, 120);

  onMount(() => {
    // noop — load when opened
  });

  $: if (open && !loaded && $projectPath) {
    void loadCatalog();
  }

  async function loadCatalog() {
    loading = true;
    error = "";
    try {
      catalog = await api.quests.itemCatalog($projectPath!);
      loaded = true;
    } catch (e) {
      error = String(e);
      // Fallback: recipe scan client-side
      try {
        const scan = await api.recipes.scan($projectPath!);
        const set = new Set<string>();
        for (const r of scan.recipes ?? []) {
          if (r.outputId && !r.outputId.startsWith("#")) set.add(r.outputId);
          for (const id of r.inputIds ?? []) {
            if (id && !id.startsWith("#")) set.add(id);
          }
        }
        catalog = [...set].sort();
        loaded = true;
        error = "";
      } catch (e2) {
        error = String(e2);
      }
    } finally {
      loading = false;
    }
  }

  $: if (open && filtered.length) {
    void preloadIcons(filtered.slice(0, 48));
  }

  async function preloadIcons(ids: string[]) {
    const need = ids.filter((id) => icons[id] === undefined);
    if (!need.length || !$projectPath) return;
    for (const id of need) icons[id] = null; // mark in-flight
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
  <div class="overlay" role="dialog" aria-modal="true" on:keydown={onKey}>
    <button type="button" class="backdrop" aria-label="Close" on:click={onClose}></button>
    <div class="panel">
      <div class="panel-h">
        <strong>Pick item</strong>
        <button type="button" class="ico" on:click={onClose}><X size={14} /></button>
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
            <button type="button" class="cell" title={id} on:click={() => pick(id)}>
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
    background: #12161e;
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-md);
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.5);
    overflow: hidden;
  }
  .panel-h {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 12px 14px;
    border-bottom: 1px solid var(--border-color);
  }
  .search {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--border-color);
    color: var(--text-muted);
  }
  .search input {
    flex: 1;
    border: none;
    background: transparent;
    color: var(--text-primary);
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
    border-radius: var(--border-radius-sm);
    border: 1px solid var(--border-color);
    background: #0c1016;
    color: var(--text-secondary);
    cursor: pointer;
    min-height: 72px;
  }
  .cell:hover {
    border-color: rgba(27, 217, 106, 0.4);
    color: var(--text-primary);
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
    background: #1a222c;
    border-radius: 4px;
    font-weight: 800;
    color: var(--accent-primary);
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
    color: var(--text-muted);
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
    border-radius: 6px;
    border: 1px solid var(--border-color);
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
  }
</style>
