<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { open as openExternal } from "@tauri-apps/plugin-shell";
  import { Sparkles, RefreshCw, Download, AlertTriangle } from "@lucide/svelte";
  import { projectPath } from "../lib/store";
  import { toasts } from "../lib/toast";
  import CatalogProjectView from "./CatalogProjectView.svelte";
  import { trapFocus } from "../lib/focusTrap";

  let { swarmEnabled = false }: { swarmEnabled?: boolean } = $props();

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
  type MpiHit = {
    id: string;
    slug: string;
    name: string;
    description?: string;
    iconUrl?: string | null;
    downloads?: number | null;
    pageUrl?: string;
    url?: string;
    links?: Record<string, string>;
    provider: "modpackindex";
    projectType: "modpack";
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
  type MpiCategory = {
    id: number;
    slug: string;
    name: string;
    kind: "modpack" | "mod" | string;
  };

  let pairs = $state<Pair[]>([]);
  let groups = $state<Group[]>([]);
  let suggestions = $state<string[]>([]);
  let popularPacks = $state<MpiHit[]>([]);
  let popularMods = $state<MrHit[]>([]);
  let packCategories = $state<MpiCategory[]>([]);
  let selectedPackCategoryId = $state<number | null>(null);
  let packQuery = $state("");
  let packSearchTimer: ReturnType<typeof setTimeout> | null = null;
  let loading = $state(false);
  let error = $state("");
  let previewBusy = $state<string | null>(null);
  let installBusy = $state<string | null>(null);
  let previews = $state<Record<string, Preview | null>>({});
  let lastKey = $state("");
  let catalogViewResult = $state<{
    id: string;
    slug: string;
    name: string;
    description: string;
    projectType: string;
    iconUrl?: string | null;
    author?: string | null;
    downloads?: number | null;
    follows?: number | null;
    categories?: string[];
    provider?: string;
  } | null>(null);
  let catalogInstalling = $state(false);

  const packThemeCategories = $derived(packCategories.filter((c) => c.kind === "modpack"));

  function formatCount(n?: number | null): string {
    if (n == null) return "—";
    if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
    if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
    return String(n);
  }

  async function loadCategories() {
    if (packCategories.length > 0) return;
    packCategories = await invoke<MpiCategory[]>("list_modpack_index_categories").catch(
      () => [],
    );
  }

  async function loadPacks() {
    const page = await invoke<{ results: MpiHit[]; total: number }>("search_modpack_index", {
      query: packQuery.trim(),
      page: 1,
      limit: 8,
      categoryId: selectedPackCategoryId,
    }).catch(() => ({ results: [], total: 0 }));
    popularPacks = page.results ?? [];
  }

  function togglePackCategory(id: number) {
    selectedPackCategoryId = selectedPackCategoryId === id ? null : id;
    if (packSearchTimer) {
      clearTimeout(packSearchTimer);
      packSearchTimer = null;
    }
    void loadPacks();
  }

  async function loadMods() {
    const path = $projectPath ?? "";
    const modsPage = await invoke<{ results: MrHit[]; total: number }>("search_modrinth_mods", {
      path,
      query: "",
      sort: "follows",
      contentType: "mod",
      page: 1,
      pageSize: 10,
    }).catch(() => ({ results: [], total: 0 }));
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
      await Promise.all([loadPacks(), loadMods(), loadSwarm()]);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  function schedulePackSearch() {
    if (packSearchTimer) clearTimeout(packSearchTimer);
    packSearchTimer = setTimeout(() => {
      packSearchTimer = null;
      void loadPacks();
    }, 300);
  }

  function onPackQueryKeydown(e: KeyboardEvent) {
    if (e.key !== "Enter") return;
    if (packSearchTimer) {
      clearTimeout(packSearchTimer);
      packSearchTimer = null;
    }
    void loadPacks();
  }

  $effect(() => {
    const key = `${swarmEnabled}:${$projectPath ?? ""}`;
    if (key === lastKey) return;
    lastKey = key;
    void refresh();
  });

  onMount(() => {
    void loadCategories();
    void refresh();
  });

  onDestroy(() => {
    if (packSearchTimer) clearTimeout(packSearchTimer);
  });

  function slugFromUrl(url: string): string {
    const clean = url.replace(/\/$/, "");
    const parts = clean.split("/");
    return parts[parts.length - 1] || clean;
  }

  function openPack(hit: MpiHit) {
    const links = hit.links ?? {};
    const mr =
      links.modrinth ||
      links.Modrinth ||
      (hit.pageUrl?.includes("modrinth.com") ? hit.pageUrl : null) ||
      (hit.url?.includes("modrinth.com") ? hit.url : null);
    const cf =
      links.curseforge ||
      links.CurseForge ||
      (hit.pageUrl?.includes("curseforge.com") ? hit.pageUrl : null) ||
      (hit.url?.includes("curseforge.com") ? hit.url : null);

    let provider: "modrinth" | "curseforge" = "modrinth";
    let id = hit.slug || hit.id;
    if (cf && !mr) {
      provider = "curseforge";
      id = slugFromUrl(cf);
    } else if (mr) {
      provider = "modrinth";
      id = slugFromUrl(mr);
    }

    catalogViewResult = {
      id,
      slug: hit.slug || id,
      name: hit.name,
      description: hit.description || "",
      projectType: "modpack",
      iconUrl: hit.iconUrl ?? null,
      author: null,
      downloads: hit.downloads ?? null,
      follows: null,
      categories: [],
      provider,
    };
  }

  function openMr(hit: MrHit) {
    catalogViewResult = {
      id: hit.id,
      slug: hit.slug || hit.id,
      name: hit.name,
      description: hit.description || "",
      projectType: hit.projectType || "mod",
      iconUrl: hit.iconUrl ?? null,
      author: null,
      downloads: hit.downloads ?? null,
      follows: hit.follows ?? null,
      categories: [],
      provider: "modrinth",
    };
  }

  async function openCatalogExternal() {
    if (!catalogViewResult) return;
    const slugOrId = (catalogViewResult.slug || catalogViewResult.id || "").trim();
    if (!slugOrId) return;
    const isPack = (catalogViewResult.projectType || "").includes("pack");
    const url =
      catalogViewResult.provider === "curseforge"
        ? /^\d+$/.test(slugOrId)
          ? `https://www.curseforge.com/projects/${slugOrId}`
          : `https://www.curseforge.com/minecraft/${isPack ? "modpacks" : "mc-mods"}/${slugOrId}`
        : `https://modrinth.com/${isPack ? "modpack" : "mod"}/${slugOrId}`;
    try {
      await openExternal(url);
    } catch (e) {
      toasts.error(String(e));
    }
  }

  async function installFromCatalog() {
    if (!catalogViewResult) return;
    const id = catalogViewResult.id || catalogViewResult.slug;
    if (!id) return;
    const isPack = (catalogViewResult.projectType || "").toLowerCase().includes("pack");
    catalogInstalling = true;
    try {
      if (isPack) {
        let targetDir = "";
        try {
          const info = await invoke<{ current: string; default: string }>("get_instances_path_info");
          targetDir = (info.current || info.default || "").replace(/[\\/]+$/, "");
        } catch {
          const home = ((await invoke("get_home_dir").catch(() => "")) as string) || "";
          if (home) targetDir = `${home.replace(/[\\/]+$/, "")}/TuffBox/instances`;
        }
        if (!targetDir) {
          toasts.error("Could not resolve instances folder.");
          return;
        }
        let source: string;
        if (catalogViewResult.provider === "curseforge") {
          const files = await invoke<Array<{ id: number; fileName?: string }>>(
            "get_curseforge_modpack_files",
            { modId: Number(catalogViewResult.id) || catalogViewResult.id, gameVersion: null },
          );
          const fileId = files?.[0]?.id;
          if (fileId == null) throw new Error("No CurseForge files for this modpack.");
          source = `cf:${catalogViewResult.id}:${fileId}`;
        } else {
          source = await invoke<string>("get_modrinth_pack_download", { projectId: id });
        }
        await invoke("install_modpack", {
          source,
          targetDir,
          instanceName: catalogViewResult.name,
        });
        toasts.success(`Installed pack ${catalogViewResult.name}`);
        catalogViewResult = null;
        return;
      }
      if (!$projectPath) {
        toasts.error("Open a project first to install mods.");
        return;
      }
      if (catalogViewResult.provider === "curseforge") {
        toasts.info("Use Library → Discover or Content to install CurseForge mods.");
        return;
      }
      await invoke("add_modrinth_mod_with_dependencies", {
        path: $projectPath,
        modId: id,
        side: "both",
      });
      toasts.success(`Installed ${catalogViewResult.name}`);
      catalogViewResult = null;
      await refresh();
    } catch (e) {
      toasts.error(String(e));
    } finally {
      catalogInstalling = false;
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
      <p>
        Suggest more mods from hub co-occurrence. Packs/categories prefer hub
        (<code>/v1/mods/modpacks</code>, <code>/modpack-categories</code>, 15m cache).
        {#if swarmEnabled} TuffSwarm stats enabled.{/if}
      </p>
    </div>
    <button class="ghost" disabled={loading} onclick={refresh}>
      <span class:spin={loading} style="display:inline-flex"><RefreshCw size={14} /></span> Refresh
    </button>
  </div>

  {#if error}<div class="err">{error}</div>{/if}

  <section>
    <h3>{packQuery.trim() ? "Search results" : "Popular modpacks · Modpack Index"}</h3>
    {#if packThemeCategories.length > 0}
      <div class="tag-row" role="group" aria-label="Pack themes">
        {#each packThemeCategories as cat (cat.id)}
          <button
            type="button"
            class="tag-chip"
            class:active={selectedPackCategoryId === cat.id}
            onclick={() => togglePackCategory(cat.id)}
          >
            {cat.name}
          </button>
        {/each}
      </div>
    {/if}
    <div class="pack-search">
      <input
        type="search"
        bind:value={packQuery}
        placeholder="Search modpacks…"
        aria-label="Search modpacks"
        oninput={schedulePackSearch}
        onkeydown={onPackQueryKeydown}
      />
    </div>
    {#if loading && popularPacks.length === 0}
      <p class="muted">Loading…</p>
    {:else if popularPacks.length === 0}
      <p class="muted">
        {packQuery.trim()
          ? "No packs found."
          : "Couldn’t load Modpack Index modpacks right now."}
      </p>
    {:else}
      <div class="hit-grid">
        {#each popularPacks as hit (hit.id)}
          <button type="button" class="hit-card" onclick={() => openPack(hit)}>
            {#if hit.iconUrl}
              <img src={hit.iconUrl} alt="" />
            {:else}
              <span class="hit-fallback">{(hit.name?.[0] || "?").toUpperCase()}</span>
            {/if}
            <div>
              <strong>{hit.name}</strong>
              <small><Download size={11} /> {formatCount(hit.downloads)}</small>
            </div>
          </button>
        {/each}
      </div>
    {/if}
    <p class="attr">
      Pack data from
      <a href="https://www.modpackindex.com" target="_blank" rel="noopener noreferrer"
        >Modpack Index</a
      >
    </p>
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
          <button type="button" class="hit-card compact" onclick={() => openMr(hit)}>
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
                <button class="ghost mini" disabled={previewBusy === slug} onclick={() => previewSlug(slug)}>
                  {previewBusy === slug ? "…" : "Preview"}
                </button>
                <button class="mini" disabled={installBusy === slug} onclick={() => installSlug(slug)}>
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

{#if catalogViewResult}
  <div
    class="catalog-backdrop"
    role="button"
    tabindex="-1"
    onclick={(e) => {
      if (e.target === e.currentTarget) catalogViewResult = null;
    }}
    onkeydown={() => {}}
  >
    <div
      class="catalog-modal"
      role="dialog"
      aria-modal="true"
      use:trapFocus={{ onEscape: () => (catalogViewResult = null) }}
    >
      <CatalogProjectView
        result={catalogViewResult}
        installing={catalogInstalling}
        onback={() => (catalogViewResult = null)}
        oninstall={() => void installFromCatalog()}
        onopenexternal={() => void openCatalogExternal()}
      />
    </div>
  </div>
{/if}

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
  .tag-row {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 0 0 10px;
  }
  .tag-chip {
    font-size: 12px;
    padding: 4px 10px;
    border-radius: 6px;
    border: 1px solid var(--border-color);
    background: var(--bg-elevated);
    color: var(--text-muted);
    cursor: pointer;
  }
  .tag-chip:hover {
    color: var(--text-secondary);
    border-color: rgba(27, 217, 106, 0.35);
  }
  .tag-chip.active {
    color: var(--text-primary);
    border-color: rgba(27, 217, 106, 0.5);
    background: rgba(27, 217, 106, 0.08);
  }
  .pack-search {
    margin: 0 0 10px;
  }
  .pack-search input {
    width: 100%;
    max-width: 320px;
    padding: 8px 12px;
    border-radius: var(--border-radius-sm);
    border: 1px solid var(--border-color);
    background: var(--bg-elevated);
    color: var(--text-primary);
    font-size: 13px;
  }
  .pack-search input::placeholder {
    color: var(--text-muted);
  }
  .attr {
    margin: 10px 0 0;
    font-size: 11px;
    color: var(--text-muted);
  }
  .attr a {
    color: var(--text-muted);
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .attr a:hover {
    color: var(--text-secondary);
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
    border-radius: var(--border-radius-sm);
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
    border-radius: var(--border-radius-md);
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
  .catalog-backdrop {
    position: fixed;
    inset: 0;
    z-index: 90;
    background: rgba(0, 0, 0, 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
  }
  .catalog-modal {
    width: min(920px, 96vw);
    max-height: min(90vh, 900px);
    overflow: auto;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: 18px;
    padding: 0;
  }
</style>
