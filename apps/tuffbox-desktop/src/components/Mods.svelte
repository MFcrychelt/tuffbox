<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { Search, Filter, Plus, Trash2, RefreshCw } from "lucide-svelte";
  import { projectPath } from "../lib/store";

  let mods: any[] = [];
  let loading = false;
  let filter = "";

  async function load() {
    if (!$projectPath) return;
    loading = true;
    try {
      mods = await invoke("list_mods", { path: $projectPath });
    } finally {
      loading = false;
    }
  }

  $: filtered = mods.filter((m) =>
    m.name.toLowerCase().includes(filter.toLowerCase())
  );

  $: if ($projectPath) load();
</script>

<div class="mods">
  <div class="toolbar">
    <div class="search">
      <Search size={16} />
      <input bind:value={filter} placeholder="Search mods..." />
    </div>
    <div class="actions">
      <button class="secondary">
        <Filter size={16} />
        Filter
      </button>
      <button>
        <Plus size={16} />
        Add Mod
      </button>
      <button class="ghost" on:click={load} title="Refresh">
        <RefreshCw size={16} />
      </button>
    </div>
  </div>

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
                  <div class="mod-avatar">{mod.name[0]}</div>
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
                <button class="icon-btn danger">
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

<style>
  .mods {
    max-width: 1200px;
  }

  .toolbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 16px;
    margin-bottom: 20px;
  }

  .search {
    flex: 1;
    max-width: 360px;
    position: relative;
    display: flex;
    align-items: center;
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

  .actions {
    display: flex;
    gap: 10px;
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
    width: 80px;
    text-align: right;
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
  }

  .mod-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .mod-name-text {
    color: var(--text-primary);
    font-weight: 600;
  }

  .mod-id {
    font-size: 12px;
    color: var(--text-muted);
  }

  .version {
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

  .icon-btn:hover {
    background: var(--bg-hover);
    color: var(--text-secondary);
  }

  .icon-btn.danger:hover {
    background: rgba(239, 68, 68, 0.1);
    color: var(--accent-danger);
  }

  .empty,
  .loading {
    color: var(--text-muted);
    padding: 80px;
    text-align: center;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
  }
</style>
