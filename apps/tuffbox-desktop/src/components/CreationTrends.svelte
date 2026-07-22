<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { Sparkles, RefreshCw, Download, AlertTriangle } from "lucide-svelte";
  import { projectPath } from "../lib/store";
  import { toasts } from "../lib/toast";

  export let swarmEnabled = false;

  type Pair = { modA: string; modB: string; count: number };
  type Preview = {
    projectId: string;
    slug: string;
    name: string;
    version: string;
    fileName?: string | null;
    side: string;
    dependencies: unknown[];
  };

  let pairs: Pair[] = [];
  let suggestions: string[] = [];
  let loading = false;
  let error = "";
  let previewBusy: string | null = null;
  let installBusy: string | null = null;
  let previews: Record<string, Preview | null> = {};
  let lastKey = "";

  async function refresh() {
    if (!$projectPath || !swarmEnabled) return;
    loading = true;
    error = "";
    try {
      await invoke("record_project_cooccurrence", { path: $projectPath });
      const trends: any = await invoke("get_creation_trends", {
        path: $projectPath,
        limit: 20,
      });
      pairs = trends?.localPairs ?? [];
      suggestions = await invoke("suggest_mods_from_trends", {
        path: $projectPath,
        limit: 8,
      });
    } catch (e) {
      error = String(e);
      pairs = [];
      suggestions = [];
    } finally {
      loading = false;
    }
  }

  $: {
    const key = `${swarmEnabled}:${$projectPath ?? ""}`;
    if (key !== lastKey) {
      lastKey = key;
      if (swarmEnabled && $projectPath) void refresh();
    }
  }

  onMount(() => {
    if (swarmEnabled && $projectPath) void refresh();
  });

  async function previewSlug(slug: string) {
    if (!$projectPath) return;
    previewBusy = slug;
    try {
      const preview = await invoke<Preview>("preview_modrinth_install", {
        path: $projectPath,
        modId: slug,
      });
      previews = { ...previews, [slug]: preview };
    } catch (e) {
      previews = { ...previews, [slug]: null };
      toasts.error(`${slug}: ${String(e)}`);
    } finally {
      previewBusy = null;
    }
  }

  async function installSlug(slug: string) {
    if (!$projectPath) return;
    const preview = previews[slug];
    if (!preview) {
      await previewSlug(slug);
    }
    const p = previews[slug];
    if (!p) return;
    const ok = confirm(
      `Install ${p.name} (${p.version}) and dependencies from Modrinth?\nA snapshot may be created by the install path.`,
    );
    if (!ok) return;
    installBusy = slug;
    try {
      await invoke("add_modrinth_mod_with_dependencies", {
        path: $projectPath,
        modId: p.projectId || slug,
        side: p.side || "both",
      });
      toasts.success(`Installed ${p.name}`);
      await refresh();
    } catch (e) {
      toasts.error(String(e));
    } finally {
      installBusy = null;
    }
  }
</script>

<div class="creation">
  <div class="creation-head">
    <Sparkles size={18} />
    <div>
      <h2>Creation trends</h2>
      <p>Local mod co-occurrence → Modrinth install preview (requires TuffSwarm).</p>
    </div>
    <button class="ghost" disabled={loading || !swarmEnabled || !$projectPath} on:click={refresh}>
      <span class:spin={loading} style="display:inline-flex"><RefreshCw size={14} /></span> Refresh
    </button>
  </div>

  {#if !swarmEnabled}
    <div class="gate">
      <AlertTriangle size={16} />
      Creation Mode is locked. Enable <strong>Use TuffSwarm network</strong> in Settings.
    </div>
  {:else if !$projectPath}
    <div class="gate">Open a project to build co-occurrence stats from installed mods.</div>
  {:else}
    {#if error}<div class="err">{error}</div>{/if}

    <section>
      <h3>Top pairs</h3>
      {#if pairs.length === 0}
        <p class="muted">No pairs yet — install mods or refresh after a crash-fix apply.</p>
      {:else}
        <ul>
          {#each pairs.slice(0, 12) as p (p.modA + p.modB)}
            <li><code>{p.modA}</code> + <code>{p.modB}</code> <span>×{p.count}</span></li>
          {/each}
        </ul>
      {/if}
    </section>

    <section>
      <h3>Suggested Modrinth installs</h3>
      {#if suggestions.length === 0}
        <p class="muted">No missing partners inferred from local pairs.</p>
      {:else}
        <div class="suggest-grid">
          {#each suggestions as slug (slug)}
            <div class="suggest-card">
              <strong>{slug}</strong>
              {#if previews[slug]}
                <small>{previews[slug]?.name} · {previews[slug]?.version}</small>
              {/if}
              <div class="row">
                <button
                  class="ghost mini"
                  disabled={previewBusy === slug}
                  on:click={() => previewSlug(slug)}
                >
                  {previewBusy === slug ? "…" : "Preview"}
                </button>
                <button
                  class="mini"
                  disabled={installBusy === slug}
                  on:click={() => installSlug(slug)}
                >
                  <Download size={12} />
                  {installBusy === slug ? "Installing…" : "Install"}
                </button>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </section>
  {/if}
</div>

<style>
  .creation {
    margin-top: 8px;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
    padding: 16px;
  }
  .creation-head {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    margin-bottom: 14px;
  }
  .creation-head h2 {
    margin: 0;
    font-size: 16px;
  }
  .creation-head p {
    margin: 2px 0 0;
    color: var(--text-muted);
    font-size: 12px;
  }
  .creation-head button {
    margin-left: auto;
  }
  .gate {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text-muted);
    font-size: 13px;
    padding: 12px;
    background: var(--bg-elevated);
    border-radius: 8px;
  }
  .err {
    color: #fecaca;
    margin-bottom: 10px;
    font-size: 13px;
  }
  section {
    margin-top: 14px;
  }
  h3 {
    font-size: 13px;
    margin: 0 0 8px;
    color: var(--text-secondary);
  }
  ul {
    list-style: none;
    padding: 0;
    margin: 0;
    display: grid;
    gap: 6px;
  }
  li {
    font-size: 12px;
    display: flex;
    gap: 8px;
    align-items: center;
  }
  li span {
    color: var(--text-muted);
  }
  .muted {
    color: var(--text-muted);
    font-size: 12px;
  }
  .suggest-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    gap: 10px;
  }
  .suggest-card {
    background: var(--bg-elevated);
    border-radius: 10px;
    padding: 10px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .suggest-card small {
    color: var(--text-muted);
  }
  .row {
    display: flex;
    gap: 6px;
  }
  .mini {
    font-size: 12px;
    padding: 4px 8px;
  }
  :global(.spin) {
    animation: spin 1s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
</style>
