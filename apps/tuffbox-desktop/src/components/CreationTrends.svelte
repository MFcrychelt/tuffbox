<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { open as openExternal } from "@tauri-apps/plugin-shell";
  import { Sparkles, RefreshCw, Download, AlertTriangle, ExternalLink } from "lucide-svelte";
  import { projectPath } from "../lib/store";
  import { toasts } from "../lib/toast";

  export let swarmEnabled = false;

  type Pair = { modA: string; modB: string; count: number };
  type Group = { mods: string[]; score: number };
  type Preview = {
    projectId: string;
    slug: string;
    name: string;
    version: string;
    fileName?: string | null;
    side: string;
    dependencies: unknown[];
  };
  type MrHit = {
    id: string;
    slug: string;
    name: string;
    description?: string;
    iconUrl?: string | null;
    downloads?: number | null;
    follows?: number | null;
    projectType?: string;
  };

  let pairs: Pair[] = [];
  let groups: Group[] = [];
  let suggestions: string[] = [];
  let popularPacks: MrHit[] = [];
  let popularMods: MrHit[] = [];
  let loading = false;
  let error = "";
  let previewBusy: string | null = null;
  let installBusy: string | null = null;
  let previews: Record<string, Preview | null> = {};
  let lastKey = "";

  function formatCount(n?: number | null): string {
    if (n == null) return "—";
    if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
    if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
    return String(n);
  }

  async function loadModrinth() {
    const path = $projectPath ?? "";
    const [packsPage, modsPage] = await Promise.all([
      invoke<{ results: MrHit[]; total: number }>("search_modrinth_mods", {
        path,
        query: "",
        sort: "downloads",
        contentType: "modpack",
        page: 1,
        pageSize: 8,
      }).catch(() => ({ results: [], total: 0 })),
      invoke<{ results: MrHit[]; total: number }>("search_modrinth_mods", {
        path,
        query: "",
        sort: "follows",
        contentType: "mod",
        page: 1,
        pageSize: 10,
      }).catch(() => ({ results: [], total: 0 })),
    ]);
    popularPacks = packsPage.results ?? [];
    popularMods = modsPage.results ?? [];
  }

  async function loadSwarm() {
    if (!$projectPath || !swarmEnabled) {
      pairs = [];
      groups = [];
      suggestions = [];
      return;
    }
    await invoke("report_mod_cooccurrence", {
      path: $projectPath,
      source: "library_trends_refresh",
    }).catch(() => {});
    const trends: any = await invoke("get_creation_trends", {
      path: $projectPath,
      limit: 20,
    });
    pairs = trends?.mergedPairs ?? trends?.localPairs ?? [];
    groups = trends?.groups ?? [];
    suggestions =
      trends?.suggestions ??
      (await invoke("suggest_mods_from_trends", {
        path: $projectPath,
        limit: 8,
      }).catch(() => []));
  }

  async function refresh() {
    loading = true;
    error = "";
    try {
      await Promise.all([loadModrinth(), loadSwarm()]);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  $: {
    const key = `${swarmEnabled}:${$projectPath ?? ""}`;
    if (key !== lastKey) {
      lastKey = key;
      void refresh();
    }
  }

  onMount(() => {
    void refresh();
  });

  async function openMr(hit: MrHit) {
    const url = `https://modrinth.com/${hit.projectType === "modpack" ? "modpack" : "mod"}/${hit.slug || hit.id}`;
    try {
      await openExternal(url);
    } catch (e) {
      toasts.error(String(e));
    }
  }

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
    if (!previews[slug]) await previewSlug(slug);
    const p = previews[slug];
    if (!p) return;
    if (!confirm(`Install ${p.name} (${p.version}) from Modrinth?`)) return;
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
      <p>Popular Modrinth packs &amp; mods{#if swarmEnabled}, plus TuffSwarm co-occurrence{/if}.</p>
    </div>
    <button class="ghost" disabled={loading} on:click={refresh}>
      <span class:spin={loading} style="display:inline-flex"><RefreshCw size={14} /></span> Refresh
    </button>
  </div>

  {#if error}<div class="err">{error}</div>{/if}

  <section>
    <h3>Popular modpacks · Modrinth</h3>
    {#if loading && popularPacks.length === 0}
      <p class="muted">Loading…</p>
    {:else if popularPacks.length === 0}
      <p class="muted">Couldn’t load Modrinth modpacks right now.</p>
    {:else}
      <div class="hit-grid">
        {#each popularPacks as hit (hit.id)}
          <button type="button" class="hit-card" on:click={() => openMr(hit)}>
            {#if hit.iconUrl}
              <img src={hit.iconUrl} alt="" />
            {:else}
              <span class="hit-fallback">{(hit.name?.[0] || "?").toUpperCase()}</span>
            {/if}
            <div>
              <strong>{hit.name}</strong>
              <small><Download size={11} /> {formatCount(hit.downloads)}</small>
            </div>
            <ExternalLink size={14} />
          </button>
        {/each}
      </div>
    {/if}
  </section>

  <section>
    <h3>Trending mods · Modrinth</h3>
    {#if loading && popularMods.length === 0}
      <p class="muted">Loading…</p>
    {:else if popularMods.length === 0}
      <p class="muted">Couldn’t load Modrinth mods right now.</p>
    {:else}
      <div class="hit-grid mods">
        {#each popularMods as hit (hit.id)}
          <button type="button" class="hit-card compact" on:click={() => openMr(hit)}>
            {#if hit.iconUrl}
              <img src={hit.iconUrl} alt="" />
            {:else}
              <span class="hit-fallback">{(hit.name?.[0] || "?").toUpperCase()}</span>
            {/if}
            <div>
              <strong>{hit.name}</strong>
              <small>{formatCount(hit.follows)} follows · {formatCount(hit.downloads)} dl</small>
            </div>
          </button>
        {/each}
      </div>
    {/if}
  </section>

  {#if !swarmEnabled}
    <div class="gate">
      <AlertTriangle size={16} />
      Enable <strong>Use TuffSwarm network</strong> in Settings for pack co-occurrence groups.
    </div>
  {:else if !$projectPath}
    <div class="gate">Open a project to build TuffSwarm co-occurrence from your installed mods.</div>
  {:else}
    <section>
      <h3>Frequent groups (TuffSwarm)</h3>
      {#if groups.length === 0}
        <p class="muted">No groups yet — need overlapping pairs from real packs.</p>
      {:else}
        <ul>
          {#each groups.slice(0, 8) as g (g.mods.join("|"))}
            <li>
              {#each g.mods as m, i (m)}
                {#if i > 0}<span class="plus">+</span>{/if}
                <code>{m}</code>
              {/each}
              <span>×{g.score}</span>
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <section>
      <h3>Top pairs</h3>
      {#if pairs.length === 0}
        <p class="muted">No pairs yet — install mods or export a pack with TuffSwarm on.</p>
      {:else}
        <ul>
          {#each pairs.slice(0, 10) as p (p.modA + p.modB)}
            <li><code>{p.modA}</code> + <code>{p.modB}</code> <span>×{p.count}</span></li>
          {/each}
        </ul>
      {/if}
    </section>

    <section>
      <h3>Suggested for your pack</h3>
      {#if suggestions.length === 0}
        <p class="muted">No partners yet — install a few mods first.</p>
      {:else}
        <div class="suggest-grid">
          {#each suggestions as slug (slug)}
            <div class="suggest-card">
              <strong>{slug}</strong>
              {#if previews[slug]}
                <small>{previews[slug]?.name} · {previews[slug]?.version}</small>
              {/if}
              <div class="row">
                <button class="ghost mini" disabled={previewBusy === slug} on:click={() => previewSlug(slug)}>
                  {previewBusy === slug ? "…" : "Preview"}
                </button>
                <button class="mini" disabled={installBusy === slug} on:click={() => installSlug(slug)}>
                  <Download size={12} />
                  {installBusy === slug ? "…" : "Install"}
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
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
    padding: 18px;
  }
  .creation-head {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    margin-bottom: 16px;
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
    margin-top: 14px;
    background: var(--bg-elevated);
    border-radius: 8px;
  }
  .err {
    color: #fecaca;
    margin-bottom: 10px;
    font-size: 13px;
  }
  section {
    margin-top: 16px;
  }
  section:first-of-type {
    margin-top: 0;
  }
  h3 {
    font-size: 12px;
    margin: 0 0 10px;
    color: var(--text-muted);
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  .hit-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 8px;
  }
  .hit-grid.mods {
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
  }
  .hit-card {
    display: grid;
    grid-template-columns: 40px 1fr auto;
    gap: 10px;
    align-items: center;
    text-align: left;
    padding: 10px;
    border-radius: 12px;
    border: 1px solid var(--border-color);
    background: var(--bg-elevated);
    color: var(--text-secondary);
    cursor: pointer;
  }
  .hit-card.compact {
    grid-template-columns: 36px 1fr;
  }
  .hit-card:hover {
    border-color: rgba(27, 217, 106, 0.4);
    background: rgba(27, 217, 106, 0.06);
  }
  .hit-card img,
  .hit-fallback {
    width: 40px;
    height: 40px;
    border-radius: 10px;
    object-fit: cover;
    background: var(--bg-tertiary);
  }
  .hit-fallback {
    display: flex;
    align-items: center;
    justify-content: center;
    font-weight: 800;
    color: var(--accent-primary);
  }
  .hit-card.compact img,
  .hit-card.compact .hit-fallback {
    width: 36px;
    height: 36px;
  }
  .hit-card strong {
    display: block;
    color: var(--text-primary);
    font-size: 13px;
    line-height: 1.25;
  }
  .hit-card small {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    margin-top: 2px;
    font-size: 11px;
    color: var(--text-muted);
  }
  .hit-card :global(svg:last-child) {
    color: var(--text-muted);
    opacity: 0.7;
    flex-shrink: 0;
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
    flex-wrap: wrap;
  }
  li span {
    color: var(--text-muted);
  }
  .plus {
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
