<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import {
    Search,
    Filter,
    Plus,
    Trash2,
    RefreshCw,
    RotateCw,
    Download,
    X,
  } from "lucide-svelte";
  import { projectPath } from "../lib/store";

  type ModRow = {
    id: string;
    name: string;
    version: string;
    side: "client" | "server" | "both" | "optional" | "unknown" | string;
    source: string;
  };

  type SearchResult = {
    id: string;
    slug: string;
    name: string;
    description: string;
    projectType: string;
    iconUrl?: string | null;
  };

  let mods: ModRow[] = [];
  let loading = false;
  let mutating = false;
  let filter = "";
  let sideFilter = "all";
  let error: string | null = null;
  let lastLoadedPath: string | null = null;

  let addOpen = false;
  let searchQuery = "";
  let searchResults: SearchResult[] = [];
  let searchLoading = false;
  let selectedSide = "both";

  async function load(force = false) {
    if (!$projectPath) return;
    if (!force && lastLoadedPath === $projectPath && mods.length > 0) return;
    loading = true;
    error = null;
    try {
      mods = await invoke("list_mods", { path: $projectPath });
      lastLoadedPath = $projectPath;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function openAddModal() {
    addOpen = true;
    error = null;
    if (searchQuery.trim()) await searchMods();
  }

  async function searchMods() {
    if (!$projectPath || !searchQuery.trim()) return;
    searchLoading = true;
    error = null;
    try {
      searchResults = await invoke("search_modrinth_mods", {
        path: $projectPath,
        query: searchQuery.trim(),
      });
    } catch (e) {
      error = String(e);
    } finally {
      searchLoading = false;
    }
  }

  async function addMod(result: SearchResult) {
    if (!$projectPath) return;
    mutating = true;
    error = null;
    try {
      await invoke("add_modrinth_mod", {
        path: $projectPath,
        modId: result.id,
        side: selectedSide,
      });
      addOpen = false;
      searchResults = [];
      searchQuery = "";
      await load(true);
    } catch (e) {
      error = String(e);
    } finally {
      mutating = false;
    }
  }

  async function removeMod(mod: ModRow) {
    if (!$projectPath) return;
    const ok = window.confirm(
      `Remove ${mod.name}? TuffBox will create an auto snapshot before changing the manifest.`
    );
    if (!ok) return;
    mutating = true;
    error = null;
    try {
      await invoke("remove_project_mod", { path: $projectPath, modId: mod.id });
      await load(true);
    } catch (e) {
      error = String(e);
    } finally {
      mutating = false;
    }
  }

  async function updateMod(mod: ModRow) {
    if (!$projectPath) return;
    mutating = true;
    error = null;
    try {
      await invoke("update_project_mod", { path: $projectPath, modId: mod.id });
      await load(true);
    } catch (e) {
      error = String(e);
    } finally {
      mutating = false;
    }
  }

  function isInstalled(result: SearchResult) {
    return mods.some((m) => m.id === result.slug || m.id === result.id);
  }

  $: filtered = mods.filter((m) => {
    const q = filter.toLowerCase();
    const matchesText =
      m.name.toLowerCase().includes(q) ||
      m.id.toLowerCase().includes(q) ||
      m.version.toLowerCase().includes(q);
    const matchesSide = sideFilter === "all" || m.side === sideFilter;
    return matchesText && matchesSide;
  });

  $: counts = {
    all: mods.length,
    client: mods.filter((m) => m.side === "client").length,
    server: mods.filter((m) => m.side === "server").length,
    both: mods.filter((m) => m.side === "both").length,
  };

  $: if ($projectPath && lastLoadedPath !== $projectPath) load(true);
</script>

<div class="mods">
  <div class="toolbar">
    <div class="search">
      <Search size={16} />
      <input bind:value={filter} placeholder="Search installed mods..." />
    </div>
    <div class="actions">
      <button class="secondary filter-button" disabled>
        <Filter size={16} />
        {sideFilter === "all" ? "All sides" : sideFilter}
      </button>
      <button on:click={openAddModal} disabled={!$projectPath || mutating}>
        <Plus size={16} />
        Add Mod
      </button>
      <button class="ghost" on:click={() => load(true)} title="Refresh" disabled={!$projectPath || loading}>
        <RefreshCw size={16} class={loading ? "spin" : ""} />
      </button>
    </div>
  </div>

  <div class="quick-filters" aria-label="Side filters">
    <button class:active={sideFilter === "all"} on:click={() => (sideFilter = "all")}>All <span>{counts.all}</span></button>
    <button class:active={sideFilter === "both"} on:click={() => (sideFilter = "both")}>Both <span>{counts.both}</span></button>
    <button class:active={sideFilter === "client"} on:click={() => (sideFilter = "client")}>Client <span>{counts.client}</span></button>
    <button class:active={sideFilter === "server"} on:click={() => (sideFilter = "server")}>Server <span>{counts.server}</span></button>
  </div>

  {#if error}
    <div class="error">{error}</div>
  {/if}

  {#if loading}
    <div class="loading">Loading mods...</div>
  {:else if !$projectPath}
    <div class="empty">Open a project to manage mods.</div>
  {:else if filtered.length === 0}
    <div class="empty">No mods found.</div>
  {:else}
    <div class="table-wrap">
      <table>
        <thead>
          <tr>
            <th class="col-name">Project</th>
            <th>Version</th>
            <th>Side</th>
            <th>Source</th>
            <th class="col-actions">Actions</th>
          </tr>
        </thead>
        <tbody>
          {#each filtered as mod}
            <tr>
              <td class="col-name">
                <div class="mod-cell">
                  <div class="mod-avatar">{mod.name?.[0] ?? "?"}</div>
                  <div class="mod-info">
                    <span class="mod-name-text">{mod.name}</span>
                    <span class="mod-id">{mod.id}</span>
                  </div>
                </div>
              </td>
              <td><span class="version">{mod.version}</span></td>
              <td><span class="tag side-{mod.side}">{mod.side}</span></td>
              <td><span class="tag source">{mod.source}</span></td>
              <td class="col-actions">
                <button class="icon-btn" on:click={() => updateMod(mod)} disabled={mutating || mod.source !== "modrinth"} title="Update from Modrinth">
                  <RotateCw size={16} />
                </button>
                <button class="icon-btn danger" on:click={() => removeMod(mod)} disabled={mutating} title="Remove with snapshot">
                  <Trash2 size={16} />
                </button>
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    </div>
  {/if}
</div>

{#if addOpen}
  <div
    class="modal-backdrop"
    role="button"
    tabindex="-1"
    aria-label="Close add mod dialog"
    on:click|self={() => (addOpen = false)}
    on:keydown={(event) => event.key === "Escape" && (addOpen = false)}
  >
    <div class="modal" role="dialog" aria-modal="true">
      <div class="modal-header">
        <div>
          <h2>Add Modrinth mod</h2>
          <p>Search is filtered by the current Minecraft version and loader.</p>
        </div>
        <button class="icon-btn" on:click={() => (addOpen = false)}><X size={18} /></button>
      </div>

      <div class="modal-search">
        <div class="search wide">
          <Search size={16} />
          <input
            bind:value={searchQuery}
            placeholder="Sodium, Iris, JEI..."
            on:keydown={(event) => event.key === "Enter" && searchMods()}
          />
        </div>
        <select bind:value={selectedSide} title="Install side">
          <option value="both">Both</option>
          <option value="client">Client</option>
          <option value="server">Server</option>
        </select>
        <button on:click={searchMods} disabled={searchLoading || !searchQuery.trim()}>
          <Search size={16} />
          Search
        </button>
      </div>

      {#if searchLoading}
        <div class="loading compact">Searching Modrinth...</div>
      {:else if searchResults.length === 0}
        <div class="empty compact">Type a query and press Search.</div>
      {:else}
        <div class="results">
          {#each searchResults as result}
            <div class="result-card">
              <div class="mod-avatar result-avatar">{result.name?.[0] ?? "?"}</div>
              <div class="result-main">
                <div class="result-title">
                  <span>{result.name}</span>
                  <code>{result.slug}</code>
                </div>
                <p>{result.description}</p>
              </div>
              <button on:click={() => addMod(result)} disabled={mutating || isInstalled(result)}>
                <Download size={16} />
                {isInstalled(result) ? "Installed" : "Install"}
              </button>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .mods {
    max-width: 1200px;
  }

  .toolbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 16px;
    margin-bottom: 14px;
  }

  .search {
    flex: 1;
    max-width: 360px;
    position: relative;
    display: flex;
    align-items: center;
  }

  .search.wide {
    max-width: none;
  }

  .search :global(svg) {
    position: absolute;
    left: 14px;
    color: var(--text-muted);
  }

  .search input {
    width: 100%;
    padding-left: 40px;
  }

  .actions,
  .modal-search {
    display: flex;
    gap: 10px;
    align-items: center;
  }

  .quick-filters {
    display: flex;
    gap: 8px;
    margin-bottom: 20px;
  }

  .quick-filters button {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    color: var(--text-secondary);
    padding: 8px 12px;
  }

  .quick-filters button.active {
    border-color: rgba(27, 217, 106, 0.45);
    background: rgba(27, 217, 106, 0.1);
    color: var(--accent-primary);
  }

  .quick-filters span {
    margin-left: 6px;
    color: var(--text-muted);
  }

  .table-wrap {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
    overflow: hidden;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 14px;
  }

  th {
    text-align: left;
    padding: 14px 18px;
    color: var(--text-muted);
    font-weight: 600;
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    background: var(--bg-tertiary);
    border-bottom: 1px solid var(--border-color);
  }

  td {
    padding: 14px 18px;
    border-bottom: 1px solid var(--border-color);
    color: var(--text-secondary);
  }

  tbody tr:last-child td {
    border-bottom: none;
  }

  tbody tr:hover {
    background: rgba(255, 255, 255, 0.02);
  }

  .col-name {
    width: 40%;
  }

  .col-actions {
    width: 120px;
    text-align: right;
    white-space: nowrap;
  }

  .mod-cell {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .mod-avatar {
    width: 40px;
    height: 40px;
    border-radius: var(--border-radius-md);
    background: linear-gradient(135deg, var(--accent-secondary), var(--accent-primary));
    display: flex;
    align-items: center;
    justify-content: center;
    font-weight: 800;
    font-size: 16px;
    color: #fff;
    flex-shrink: 0;
  }

  .mod-info,
  .result-main {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .mod-name-text {
    color: var(--text-primary);
    font-weight: 600;
  }

  .mod-id {
    font-size: 12px;
    color: var(--text-muted);
  }

  .version,
  code {
    font-family: ui-monospace, monospace;
    font-size: 13px;
  }

  .tag {
    display: inline-block;
    padding: 4px 10px;
    border-radius: 999px;
    font-size: 12px;
    font-weight: 600;
    text-transform: uppercase;
    background: var(--bg-elevated);
    color: var(--text-secondary);
  }

  .tag.side-both {
    background: rgba(27, 217, 106, 0.12);
    color: var(--accent-primary);
  }

  .tag.side-client {
    background: rgba(139, 92, 246, 0.12);
    color: var(--accent-secondary);
  }

  .tag.side-server {
    background: rgba(59, 130, 246, 0.12);
    color: #60a5fa;
  }

  .icon-btn {
    width: 32px;
    height: 32px;
    padding: 0;
    background: transparent;
    color: var(--text-muted);
  }

  .icon-btn:hover:not(:disabled) {
    background: var(--bg-hover);
    color: var(--text-secondary);
  }

  .icon-btn.danger:hover:not(:disabled) {
    background: rgba(239, 68, 68, 0.1);
    color: var(--accent-danger);
  }

  .empty,
  .loading,
  .error {
    color: var(--text-muted);
    padding: 80px;
    text-align: center;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
  }

  .error {
    color: #fecaca;
    background: rgba(239, 68, 68, 0.08);
    border-color: rgba(239, 68, 68, 0.25);
    padding: 14px 16px;
    text-align: left;
    margin-bottom: 16px;
  }

  .compact {
    padding: 28px;
  }

  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 50;
    backdrop-filter: blur(10px);
  }

  .modal {
    width: min(820px, calc(100vw - 48px));
    max-height: min(760px, calc(100vh - 48px));
    overflow: auto;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: 22px;
    box-shadow: 0 30px 100px rgba(0, 0, 0, 0.45);
    padding: 22px;
  }

  .modal-header {
    display: flex;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 18px;
  }

  .modal-header h2 {
    margin: 0 0 4px;
  }

  .modal-header p,
  .result-card p {
    margin: 0;
    color: var(--text-muted);
    font-size: 13px;
    line-height: 1.45;
  }

  .modal-search {
    margin-bottom: 16px;
  }

  .modal-search select {
    min-width: 120px;
  }

  .results {
    display: grid;
    gap: 10px;
  }

  .result-card {
    display: grid;
    grid-template-columns: 44px 1fr auto;
    gap: 12px;
    align-items: center;
    padding: 14px;
    border-radius: var(--border-radius-lg);
    background: var(--bg-tertiary);
    border: 1px solid var(--border-color);
  }

  .result-avatar {
    width: 44px;
    height: 44px;
  }

  .result-title {
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--text-primary);
    font-weight: 700;
  }

  code {
    color: var(--text-muted);
    background: var(--bg-elevated);
    border-radius: 999px;
    padding: 3px 8px;
  }

  :global(.spin) {
    animation: spin 900ms linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
