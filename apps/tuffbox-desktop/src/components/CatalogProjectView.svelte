<script lang="ts">
  import { createEventDispatcher, onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import {
    ArrowLeft,
    Download,
    ExternalLink,
    Heart,
    Clock,
    Loader2,
    Package,
  } from "lucide-svelte";
  import { fly } from "svelte/transition";
  import { quintOut } from "svelte/easing";

  export let result: {
    id: string;
    slug: string;
    name: string;
    description: string;
    projectType: string;
    iconUrl?: string | null;
    author?: string | null;
    downloads?: number | null;
    follows?: number | null;
    dateModified?: string | null;
    categories?: string[];
    provider?: string;
  };
  export let minecraftVersion: string | null = null;
  export let loaderKind: string | null = null;
  export let installed = false;
  export let installing = false;

  const dispatch = createEventDispatcher<{
    back: void;
    install: void;
    openExternal: void;
  }>();

  type CatalogDetail = typeof result & {
    descriptionHtml?: string | null;
    authors?: string[];
    license?: string | null;
    clientSide?: string | null;
    serverSide?: string | null;
  };

  type CatalogVersion = {
    id: string;
    versionNumber: string;
    name?: string | null;
    gameVersions: string[];
    loaders: string[];
    datePublished?: string | null;
    versionType?: string;
    compatible?: boolean;
  };

  let detail: CatalogDetail = { ...result };
  let loading = true;
  let versionsLoading = false;
  let versions: CatalogVersion[] = [];
  let tab: "overview" | "versions" = "overview";
  let showIncompatible = false;
  let error: string | null = null;

  $: provider = (result.provider ?? "modrinth").toLowerCase() === "curseforge" ? "curseforge" : "modrinth";
  $: compatibleVersions = versions.filter((v) => v.compatible !== false);
  $: shownVersions = showIncompatible ? versions : compatibleVersions;

  function formatCount(n: number | null | undefined): string {
    if (n == null) return "0";
    if (n >= 1_000_000) return (n / 1_000_000).toFixed(2) + "M";
    if (n >= 1_000) return (n / 1_000).toFixed(1) + "K";
    return String(n);
  }

  function formatRelative(iso: string | null | undefined): string {
    if (!iso) return "—";
    const then = new Date(iso).getTime();
    if (Number.isNaN(then)) return iso.slice(0, 10);
    const days = Math.floor((Date.now() - then) / 86_400_000);
    if (days < 1) return "today";
    if (days === 1) return "1 day ago";
    if (days < 30) return `${days} days ago`;
    const months = Math.floor(days / 30);
    if (months < 12) return `${months} mo ago`;
    return `${Math.floor(months / 12)}y ago`;
  }

  function formatDate(iso: string | null | undefined): string {
    if (!iso) return "—";
    try {
      return new Date(iso).toLocaleDateString(undefined, {
        year: "numeric",
        month: "short",
        day: "numeric",
      });
    } catch {
      return iso.slice(0, 10);
    }
  }

  async function loadDetail() {
    loading = true;
    error = null;
    try {
      detail = await invoke<CatalogDetail>("get_catalog_project", {
        provider,
        projectId: result.id,
      });
    } catch (e) {
      error = String(e);
      detail = { ...result };
    } finally {
      loading = false;
    }
  }

  async function loadVersions() {
    versionsLoading = true;
    try {
      versions = await invoke<CatalogVersion[]>("get_catalog_versions", {
        provider,
        projectId: result.id,
        minecraftVersion: minecraftVersion || null,
        loader: loaderKind || null,
      });
    } catch {
      versions = [];
    } finally {
      versionsLoading = false;
    }
  }

  onMount(() => {
    void loadDetail();
    void loadVersions();
  });
</script>

<div class="catalog-page" transition:fly={{ x: 28, duration: 320, opacity: 0, easing: quintOut }}>
  <header class="page-head">
    <button type="button" class="back" on:click={() => dispatch("back")}>
      <ArrowLeft size={16} /> Back to search
    </button>
    <div class="head-actions">
      <button type="button" class="ghost" on:click={() => dispatch("openExternal")}>
        <ExternalLink size={15} />
        Open on {provider === "curseforge" ? "CurseForge" : "Modrinth"}
      </button>
      <button
        type="button"
        class="primary"
        disabled={installing || installed}
        on:click={() => dispatch("install")}
      >
        <Download size={15} />
        {installed ? "Installed" : installing ? "Installing…" : "Install"}
      </button>
    </div>
  </header>

  <div class="hero">
    <div class="hero-icon">
      {#if detail.iconUrl}
        <img src={detail.iconUrl} alt="" />
      {:else}
        <span>{(detail.name?.[0] ?? "?").toUpperCase()}</span>
      {/if}
    </div>
    <div class="hero-body">
      <div class="eyebrow">
        <span class="provider" class:cf={provider === "curseforge"}>{provider === "curseforge" ? "CurseForge" : "Modrinth"}</span>
        <span class="type">{detail.projectType || "mod"}</span>
      </div>
      <h1>{detail.name}</h1>
      {#if detail.author || (detail.authors && detail.authors.length)}
        <p class="author">by {(detail.authors && detail.authors[0]) || detail.author}</p>
      {/if}
      <div class="stats">
        <span><Download size={13} /> {formatCount(detail.downloads)}</span>
        <span><Heart size={13} /> {formatCount(detail.follows)}</span>
        <span><Clock size={13} /> {formatRelative(detail.dateModified)}</span>
      </div>
      {#if detail.categories?.length}
        <div class="cats">
          {#each detail.categories.slice(0, 8) as cat (cat)}
            <span>{cat}</span>
          {/each}
        </div>
      {/if}
    </div>
  </div>

  <div class="tabs" role="tablist">
    <button type="button" class:active={tab === "overview"} on:click={() => (tab = "overview")}>Overview</button>
    <button type="button" class:active={tab === "versions"} on:click={() => (tab = "versions")}>
      Versions{#if versions.length} ({compatibleVersions.length}){/if}
    </button>
  </div>

  {#if loading}
    <div class="loading"><Loader2 size={18} class="spin" /> Loading project…</div>
  {:else if error}
    <div class="notice">{error}</div>
  {:else if tab === "overview"}
    <section class="overview">
      {#if detail.descriptionHtml}
        <div class="html-body">{@html detail.descriptionHtml}</div>
      {:else}
        <p class="plain">{detail.description || "No description."}</p>
      {/if}
      {#if detail.license || detail.clientSide || detail.serverSide}
        <div class="meta-grid">
          {#if detail.license}<div><span>License</span><code>{detail.license}</code></div>{/if}
          {#if detail.clientSide}<div><span>Client</span><code>{detail.clientSide}</code></div>{/if}
          {#if detail.serverSide}<div><span>Server</span><code>{detail.serverSide}</code></div>{/if}
        </div>
      {/if}
    </section>
  {:else}
    <section class="versions">
      <div class="versions-toolbar">
        <label>
          <input type="checkbox" bind:checked={showIncompatible} />
          Show incompatible
        </label>
        {#if versionsLoading}<span class="muted"><Loader2 size={13} class="spin" /> Loading…</span>{/if}
      </div>
      {#if shownVersions.length === 0}
        <div class="empty"><Package size={18} /> No versions matched this instance.</div>
      {:else}
        <ul>
          {#each shownVersions.slice(0, 40) as v (v.id)}
            <li class:incompat={v.compatible === false}>
              <div>
                <strong>{v.versionNumber || v.name || v.id}</strong>
                <small>
                  {(v.versionType ?? "release")}
                  {#if v.loaders?.length} · {v.loaders.slice(0, 3).join(", ")}{/if}
                  {#if v.gameVersions?.length} · MC {v.gameVersions.slice(0, 4).join(", ")}{/if}
                  {#if v.datePublished} · {formatDate(v.datePublished)}{/if}
                </small>
              </div>
              {#if v.compatible === false}<span class="badge">incompatible</span>{/if}
            </li>
          {/each}
        </ul>
      {/if}
    </section>
  {/if}
</div>

<style>
  .catalog-page {
    display: flex;
    flex-direction: column;
    gap: 14px;
    min-height: 100%;
  }
  .page-head {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
    align-items: center;
  }
  .back, .ghost, .primary {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 34px;
    padding: 0 12px;
    border-radius: 8px;
    font-size: 13px;
    font-weight: 650;
    cursor: pointer;
  }
  .back, .ghost {
    border: 1px solid var(--border-color);
    background: transparent;
    color: var(--text-secondary);
  }
  .primary {
    border: none;
    background: var(--accent-primary);
    color: #04140a;
  }
  .primary:disabled { opacity: 0.55; cursor: not-allowed; }
  .head-actions { display: flex; gap: 8px; flex-wrap: wrap; }

  .hero {
    display: grid;
    grid-template-columns: 88px minmax(0, 1fr);
    gap: 16px;
    padding: 16px;
    border-radius: 14px;
    border: 1px solid var(--border-color);
    background: var(--bg-secondary);
  }
  .hero-icon {
    width: 88px;
    height: 88px;
    border-radius: 14px;
    overflow: hidden;
    background: var(--bg-tertiary);
    display: grid;
    place-items: center;
    font-size: 28px;
    font-weight: 900;
    color: #fff;
  }
  .hero-icon img { width: 100%; height: 100%; object-fit: cover; }
  .eyebrow { display: flex; gap: 8px; margin-bottom: 4px; }
  .provider, .type {
    font-size: 10px;
    font-weight: 800;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 2px 7px;
    border-radius: 999px;
    background: rgba(27, 217, 106, 0.12);
    color: var(--accent-primary);
  }
  .provider.cf { background: rgba(245, 158, 11, 0.14); color: #fbbf24; }
  h1 { margin: 0; font-size: 24px; color: var(--text-primary); }
  .author { margin: 4px 0 0; color: var(--text-muted); font-size: 13px; }
  .stats { display: flex; flex-wrap: wrap; gap: 12px; margin-top: 10px; color: var(--text-secondary); font-size: 12px; }
  .stats span { display: inline-flex; align-items: center; gap: 5px; }
  .cats { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 10px; }
  .cats span {
    padding: 2px 8px;
    border-radius: 999px;
    background: var(--bg-tertiary);
    color: var(--text-muted);
    font-size: 11px;
  }

  .tabs { display: flex; gap: 0; border-bottom: 1px solid var(--border-color); }
  .tabs button {
    padding: 10px 14px;
    border: none;
    border-bottom: 2px solid transparent;
    background: transparent;
    color: var(--text-muted);
    font-weight: 700;
    font-size: 13px;
    cursor: pointer;
  }
  .tabs button.active {
    color: var(--text-primary);
    border-bottom-color: var(--accent-primary);
  }

  .loading, .empty, .notice {
    padding: 20px;
    border-radius: 12px;
    border: 1px dashed var(--border-color);
    color: var(--text-muted);
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .notice { border-style: solid; color: #fecaca; border-color: rgba(239,68,68,.35); }

  .plain { color: var(--text-secondary); line-height: 1.55; white-space: pre-wrap; }
  .html-body {
    color: var(--text-secondary);
    line-height: 1.55;
    overflow-wrap: anywhere;
  }
  .html-body :global(img) { max-width: 100%; height: auto; border-radius: 8px; }
  .html-body :global(a) { color: var(--accent-primary); }
  .meta-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
    gap: 10px;
    margin-top: 16px;
  }
  .meta-grid span { display: block; font-size: 11px; color: var(--text-muted); margin-bottom: 4px; text-transform: uppercase; }
  .meta-grid code { color: var(--text-secondary); font-size: 12px; }

  .versions-toolbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 10px;
    color: var(--text-muted);
    font-size: 12px;
  }
  .versions ul { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 8px; }
  .versions li {
    display: flex;
    justify-content: space-between;
    gap: 10px;
    padding: 10px 12px;
    border-radius: 10px;
    border: 1px solid var(--border-color);
    background: var(--bg-tertiary);
  }
  .versions li.incompat { opacity: 0.55; }
  .versions strong { display: block; color: var(--text-primary); font-size: 13px; }
  .versions small { color: var(--text-muted); font-size: 11px; }
  .badge {
    align-self: center;
    font-size: 10px;
    font-weight: 800;
    text-transform: uppercase;
    color: #fbbf24;
  }
  :global(.spin) { animation: spin 0.8s linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (max-width: 720px) {
    .hero { grid-template-columns: 1fr; }
  }
</style>
