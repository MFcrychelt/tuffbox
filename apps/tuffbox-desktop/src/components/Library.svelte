<script lang="ts">
  import { onMount } from "svelte";
  import {
    Library as LibraryIcon,
    Search,
    Play,
    Plus,
    Download,
    FolderOpen,
    Star,
    Compass,
    LayoutGrid,
  } from "lucide-svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { recentProjects, projectPath, projectInfo, type RecentProject } from "../lib/store";
  import { toasts } from "../lib/toast";
  import { api } from "../lib/api";
  import type { SearchResult } from "../lib/api";
  import { launchWithFeedback } from "../lib/launch";

  export let currentView: "dashboard" | "ide" | "mods" | "graph" | "diagnostics" | "snapshots" | "configs" | "settings" | "project-settings" | "ore-gen" | "recipes" | "quests" | "library" | "me" | "world";

  type Tab = "yours" | "discover";
  let tab: Tab = "yours";

  // ── Your packs (local instances) ────────────────────────────────
  let instanceSizes: Record<string, string> = {};
  let launching: string | null = null;

  function gradientFrom(name: string) {
    const colors = ["#1bd96a", "#8b5cf6", "#3b82f6", "#f59e0b", "#ec4899", "#06b6d4", "#ef4444"];
    let hash = 0;
    for (let i = 0; i < name.length; i++) hash = name.charCodeAt(i) + ((hash << 5) - hash);
    return colors[Math.abs(hash) % colors.length];
  }

  function loadSize(path: string) {
    if (instanceSizes[path]) return;
    api.instance
      .getSize(path)
      .then((s) => {
        instanceSizes[path] = s;
        instanceSizes = { ...instanceSizes };
      })
      .catch(() => {
        instanceSizes[path] = "?";
        instanceSizes = { ...instanceSizes };
      });
  }

  $: if (tab === "yours") $recentProjects.forEach((p) => loadSize(p.path));

  function openPack(project: RecentProject) {
    projectPath.set(project.path);
    projectInfo.set(project.info);
    currentView = "dashboard";
  }

  async function launchPack(project: RecentProject) {
    launching = project.path;
    try {
      await invoke("set_last_opened_project", { path: project.path });
      await launchWithFeedback({ path: project.path, profile: "client" });
    } finally {
      launching = null;
    }
  }

  // ── Discover (Modrinth / CurseForge modpacks) ───────────────────
  type DiscoverResult = SearchResult & { provider?: "modrinth" | "curseforge" };
  type DiscoverProvider = "modrinth" | "curseforge" | "both";

  let query = "";
  let results: DiscoverResult[] = [];
  let loadingDiscover = false;
  let discoverError = "";
  let adding = new Set<string>();
  let discoverProvider: DiscoverProvider = "modrinth";

  function resultKey(result: DiscoverResult): string {
    return `${result.provider ?? "modrinth"}:${result.id}`;
  }

  function interleaveResults(a: DiscoverResult[], b: DiscoverResult[]): DiscoverResult[] {
    const out: DiscoverResult[] = [];
    const max = Math.max(a.length, b.length);
    for (let i = 0; i < max; i++) {
      if (i < a.length) out.push(a[i]);
      if (i < b.length) out.push(b[i]);
    }
    return out;
  }

  async function searchModrinth(): Promise<DiscoverResult[]> {
    const page = await invoke<{ results: SearchResult[]; total: number }>("search_modrinth_mods", {
      path: "",
      query: query.trim(),
      gameVersion: null,
      loader: null,
      category: null,
      environment: null,
      license: null,
      sort: "downloads",
      contentType: "modpack",
      page: 1,
      pageSize: 30,
    });
    return (page.results ?? []).map((r) => ({ ...r, provider: "modrinth" as const }));
  }

  async function searchCurseForge(): Promise<DiscoverResult[]> {
    const hits = await invoke<Array<{
      id: number | string;
      slug: string;
      name: string;
      summary?: string | null;
      iconUrl?: string | null;
      authors?: string[] | null;
      downloadCount?: number | null;
      categories?: string[] | null;
    }>>("search_curseforge_modpacks", {
      query: query.trim(),
      gameVersion: null,
      offset: 0,
    });
    return (hits ?? []).map((h) => ({
      id: String(h.id),
      slug: h.slug,
      name: h.name,
      description: h.summary ?? "",
      projectType: "modpack",
      iconUrl: h.iconUrl,
      author: h.authors?.[0] ?? null,
      downloads: h.downloadCount,
      follows: null,
      categories: h.categories ?? [],
      provider: "curseforge" as const,
    }));
  }

  async function search(_opts?: { reset?: boolean }) {
    loadingDiscover = true;
    discoverError = "";
    try {
      if (discoverProvider === "modrinth") {
        results = await searchModrinth();
      } else if (discoverProvider === "curseforge") {
        results = await searchCurseForge();
      } else {
        const settled = await Promise.allSettled([searchModrinth(), searchCurseForge()]);
        const mr = settled[0].status === "fulfilled" ? settled[0].value : [];
        const cf = settled[1].status === "fulfilled" ? settled[1].value : [];
        const errors = settled
          .filter((s): s is PromiseRejectedResult => s.status === "rejected")
          .map((s) => String(s.reason));
        if (mr.length === 0 && cf.length === 0 && errors.length > 0) {
          throw new Error(errors.join("; "));
        }
        if (errors.length > 0) {
          discoverError = errors.join("; ");
        }
        results = interleaveResults(mr, cf);
      }
    } catch (e) {
      discoverError = String(e);
      results = [];
    } finally {
      loadingDiscover = false;
    }
  }

  function setDiscoverProvider(provider: DiscoverProvider) {
    if (discoverProvider === provider) return;
    discoverProvider = provider;
    search();
  }

  async function addModpack(result: DiscoverResult) {
    const key = resultKey(result);
    adding = new Set([...adding, key]);
    try {
      const home = ((await invoke("get_home_dir").catch(() => "")) as string).replace(/\/$/, "");
      const slug = result.slug || result.id;
      const targetDir = `${home}/TuffBox/instances/${slug}`;
      let source: string;
      if (result.provider === "curseforge") {
        const files = await invoke<Array<{ id: number }>>("get_curseforge_modpack_files", {
          modId: Number(result.id),
          gameVersion: null,
        });
        const fileId = files?.[0]?.id;
        if (fileId == null) throw new Error("No CurseForge files available for this modpack.");
        source = `cf:${result.id}:${fileId}`;
      } else {
        source = await api.modpacks.getModpackUrl(result.id);
      }
      const res: any = await api.modpacks.install(source, targetDir, result.name);
      const info = await invoke("validate_project", { path: res.path }) as import("../lib/api").ProjectSummary;
      const manifestPath = info.manifestPath || res.path;
      recentProjects.add({ path: manifestPath, info: info as any });
      toasts.success(`Added "${result.name}" to your library.`);
      search();
    } catch (e) {
      toasts.error(`Could not add ${result.name}: ${e}`);
    } finally {
      const next = new Set(adding);
      next.delete(key);
      adding = next;
    }
  }

  onMount(() => {
    if (tab === "discover") search();
  });

  function switchTab(t: Tab) {
    tab = t;
    if (t === "discover" && results.length === 0) search();
  }

  function formatCount(n?: number | null): string {
    if (!n) return "0";
    if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
    if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
    return String(n);
  }

  $: discoverPlaceholder =
    discoverProvider === "curseforge"
      ? "Search CurseForge modpacks…"
      : discoverProvider === "both"
        ? "Search modpacks…"
        : "Search Modrinth modpacks…";
</script>

<div class="library">
  <div class="library-header">
    <div class="title-row">
      <LibraryIcon size={22} />
      <h1>Library</h1>
    </div>
    <div class="tabs">
      <button class:active={tab === "yours"} on:click={() => switchTab("yours")}>
        <LayoutGrid size={15} /> Your packs
      </button>
      <button class:active={tab === "discover"} on:click={() => switchTab("discover")}>
        <Compass size={15} /> Discover
      </button>
    </div>
  </div>

  {#if tab === "yours"}
    {#if $recentProjects.length === 0}
      <div class="empty-state">
        <div class="empty-icon"><LibraryIcon size={40} /></div>
        <h3>No packs yet</h3>
        <p>Create or import a modpack to build your library.</p>
      </div>
    {:else}
      <div class="pack-grid">
        {#each $recentProjects as project (project.path)}
          <div class="pack-card" role="button" tabindex="0"
            on:click={() => openPack(project)}
            on:keydown={(e) => e.key === "Enter" && openPack(project)}>
            <div class="pack-cover" style={`background: linear-gradient(135deg, ${gradientFrom(project.info.name)}, ${gradientFrom(project.info.id)})`}>
              <span class="pack-cover-letter">{project.info.name[0]}</span>
              <button class="pack-play" class:busy={launching === project.path}
                on:click|stopPropagation={() => launchPack(project)}
                title="Play" aria-label="Play {project.info.name}">
                {#if launching === project.path}<span class="mini-spinner"></span>{:else}<Play size={20} fill="currentColor" />{/if}
              </button>
            </div>
            <div class="pack-body">
              <span class="pack-name">{project.info.name}</span>
              <span class="pack-meta">{project.info.minecraftVersion} · {project.info.loaderKind}</span>
              <div class="pack-footer">
                <span class="pack-size">{instanceSizes[project.path] || "…"}</span>
                <button class="pack-open" on:click|stopPropagation={() => openPack(project)}>
                  <FolderOpen size={14} /> Open
                </button>
              </div>
            </div>
          </div>
        {/each}

        <button class="pack-card add-card" on:click={() => (currentView = "dashboard")}>
          <div class="pack-cover add-cover"><Plus size={28} /></div>
          <div class="pack-body"><span class="pack-name">New pack</span></div>
        </button>
      </div>
    {/if}
  {:else}
    <div class="discover-bar">
      <div class="provider-toggle" role="group" aria-label="Catalog provider">
        <button
          type="button"
          class:active={discoverProvider === "modrinth"}
          on:click={() => setDiscoverProvider("modrinth")}
        >Modrinth</button>
        <button
          type="button"
          class:active={discoverProvider === "curseforge"}
          on:click={() => setDiscoverProvider("curseforge")}
        >CurseForge</button>
        <button
          type="button"
          class:active={discoverProvider === "both"}
          on:click={() => setDiscoverProvider("both")}
          title="Search both catalogs at once"
        >Both</button>
      </div>
      <div class="search">
        <Search size={16} />
        <input
          aria-label="Search modpacks"
          bind:value={query}
          placeholder={discoverPlaceholder}
          on:keydown={(e) => e.key === "Enter" && search()}
        />
      </div>
      <button class="search-btn" on:click={() => search()} disabled={loadingDiscover}>
        {loadingDiscover ? "Searching…" : "Search"}
      </button>
    </div>

    {#if discoverError}
      <div class="error">{discoverError}</div>
    {/if}

    {#if loadingDiscover && results.length === 0}
      <div class="loading-state">Loading modpacks…</div>
    {:else if results.length === 0}
      <div class="empty-state">
        <div class="empty-icon"><Compass size={40} /></div>
        <h3>No modpacks found</h3>
        <p>Try a different search.</p>
      </div>
    {:else}
      <div class="pack-grid">
        {#each results as result (resultKey(result))}
          <div class="pack-card discover-card">
            <div class="pack-cover" style={result.iconUrl ? `background: #18181b` : `background: linear-gradient(135deg, ${gradientFrom(result.name)}, ${gradientFrom(result.slug)})`}>
              {#if result.iconUrl}
                <img class="pack-cover-img" src={result.iconUrl} alt="" />
              {:else}
                <span class="pack-cover-letter">{result.name[0]}</span>
              {/if}
            </div>
            <div class="pack-body">
              <div class="pack-title-row">
                <span class="pack-name" title={result.name}>{result.name}</span>
                {#if discoverProvider === "both"}
                  <span
                    class="provider-badge"
                    class:modrinth={(result.provider ?? "modrinth") !== "curseforge"}
                    class:curseforge={result.provider === "curseforge"}
                    title={result.provider === "curseforge" ? "CurseForge" : "Modrinth"}
                  >{result.provider === "curseforge" ? "CF" : "MR"}</span>
                {/if}
              </div>
              <span class="pack-meta">{result.author ?? "Unknown author"}</span>
              <p class="pack-desc">{result.description}</p>
              <div class="pack-stats">
                <span><Download size={12} /> {formatCount(result.downloads)}</span>
                <span><Star size={12} /> {formatCount(result.follows)}</span>
              </div>
              <button class="pack-add" disabled={adding.has(resultKey(result))} on:click={() => addModpack(result)}>
                {#if adding.has(resultKey(result))}<span class="mini-spinner"></span> Adding…{:else}<Plus size={14} /> Add to TuffBox{/if}
              </button>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  {/if}
</div>

<style>
  .library { max-width: 1200px; margin: 0 auto; }

  .library-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 22px;
    gap: 16px;
    flex-wrap: wrap;
  }
  .title-row { display: flex; align-items: center; gap: 10px; color: var(--accent-primary); }
  .title-row h1 { margin: 0; font-size: 22px; color: var(--text-primary); }

  .tabs { display: flex; gap: 6px; }
  .tabs button {
    display: flex; align-items: center; gap: 6px;
    padding: 8px 14px; border-radius: 999px;
    background: var(--bg-secondary); border: 1px solid var(--border-color);
    color: var(--text-secondary); font-size: 13px; font-weight: 600; cursor: pointer;
    transition: all 0.15s ease;
  }
  .tabs button:hover { background: var(--bg-hover); color: var(--text-primary); }
  .tabs button.active {
    border-color: rgba(27, 217, 106, 0.35); background: rgba(27, 217, 106, 0.1); color: var(--accent-primary);
  }

  .pack-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 16px;
  }

  .pack-card {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
    overflow: hidden;
    text-align: left;
    transition: transform 0.15s ease, border-color 0.15s ease, background 0.15s ease;
    cursor: pointer;
    display: flex; flex-direction: column;
  }
  .pack-card:hover { transform: translateY(-2px); border-color: var(--bg-hover); background: var(--bg-tertiary); }

  .pack-cover {
    position: relative;
    height: 120px;
    display: flex; align-items: center; justify-content: center;
  }
  .pack-cover-letter {
    font-size: 44px; font-weight: 900; color: #fff;
    text-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
  }
  .pack-cover-img { width: 100%; height: 100%; object-fit: cover; }

  .pack-play {
    position: absolute; right: 10px; bottom: 10px;
    width: 40px; height: 40px; border-radius: 50%;
    display: flex; align-items: center; justify-content: center;
    background: var(--accent-primary); color: #000; border: none; cursor: pointer;
    box-shadow: 0 6px 16px rgba(27, 217, 106, 0.4);
    transition: transform 0.12s ease;
  }
  .pack-play:hover { transform: scale(1.08); }
  .pack-play.busy { opacity: 0.8; cursor: default; }

  .pack-body { padding: 12px 14px 14px; display: flex; flex-direction: column; gap: 4px; flex: 1; }
  .pack-name {
    font-weight: 700; font-size: 14px; color: var(--text-primary);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .pack-meta { font-size: 12px; color: var(--text-muted); text-transform: capitalize; }
  .pack-desc {
    margin: 4px 0 0; font-size: 12px; color: var(--text-muted); line-height: 1.4;
    display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;
    min-height: 34px;
  }
  .pack-footer { display: flex; align-items: center; justify-content: space-between; margin-top: auto; padding-top: 8px; }
  .pack-size { font-size: 12px; color: var(--text-muted); }
  .pack-open {
    display: inline-flex; align-items: center; gap: 5px;
    padding: 6px 10px; border-radius: 8px; font-size: 12px; font-weight: 600;
    background: var(--bg-tertiary); border: 1px solid var(--border-color); color: var(--text-secondary); cursor: pointer;
    transition: all 0.15s ease;
  }
  .pack-open:hover { border-color: var(--accent-primary); color: var(--accent-primary); }

  .pack-stats { display: flex; gap: 12px; font-size: 12px; color: var(--text-muted); margin-top: 6px; }
  .pack-stats span { display: inline-flex; align-items: center; gap: 4px; }

  .pack-add {
    margin-top: 10px;
    display: inline-flex; align-items: center; justify-content: center; gap: 6px;
    padding: 8px 12px; border-radius: 8px; font-size: 12px; font-weight: 700;
    background: var(--accent-primary); color: #000; border: none; cursor: pointer;
    transition: background 0.15s ease;
  }
  .pack-add:hover { background: var(--accent-hover); }
  .pack-add:disabled { opacity: 0.7; cursor: default; }

  .add-card .add-cover {
    background: var(--bg-elevated); color: var(--text-muted);
    border-bottom: 1px solid var(--border-color);
  }
  .add-card:hover .add-cover { color: var(--accent-primary); }

  .discover-bar {
    display: flex; gap: 10px; margin-bottom: 20px; align-items: center; flex-wrap: wrap;
  }
  .provider-toggle {
    display: inline-flex;
    gap: 4px;
    padding: 3px;
    border-radius: 10px;
    border: 1px solid var(--border-color);
    background: var(--bg-tertiary);
    flex-shrink: 0;
  }
  .provider-toggle button {
    padding: 6px 12px;
    border-radius: 8px;
    border: none;
    background: transparent;
    color: var(--text-secondary);
    font-size: 12px;
    font-weight: 700;
    cursor: pointer;
  }
  .provider-toggle button.active {
    background: rgba(27, 217, 106, 0.14);
    color: var(--text-primary);
  }
  .search {
    flex: 1; min-width: 180px; display: flex; align-items: center; gap: 8px;
    padding: 0 14px; border-radius: 10px; border: 1px solid var(--border-color); background: var(--bg-tertiary);
  }
  .search input { border: 0; background: transparent; color: var(--text-primary); width: 100%; padding: 12px 0; font-size: 14px; }
  .search-btn {
    padding: 0 18px; height: 44px; border-radius: 10px; font-weight: 700; font-size: 13px;
    background: var(--accent-primary); color: #000; border: none; cursor: pointer;
  }
  .search-btn:disabled { opacity: 0.6; }

  .discover-card { cursor: default; }
  .pack-title-row {
    display: flex; align-items: center; gap: 8px; min-width: 0;
  }
  .pack-title-row .pack-name { flex: 1; min-width: 0; }
  .provider-badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 22px;
    height: 18px;
    padding: 0 5px;
    border-radius: 4px;
    font-size: 10px;
    font-weight: 800;
    letter-spacing: 0.02em;
    flex-shrink: 0;
  }
  .provider-badge.modrinth {
    background: rgba(27, 217, 106, 0.18);
    color: #1bd96a;
  }
  .provider-badge.curseforge {
    background: rgba(241, 100, 54, 0.18);
    color: #f16436;
  }

  .mini-spinner {
    width: 14px; height: 14px; border: 2px solid rgba(0, 0, 0, 0.25); border-top-color: #000;
    border-radius: 50%; animation: spin 0.8s linear infinite; display: inline-block;
  }
  @keyframes spin { to { transform: rotate(360deg); } }

  .empty-state, .loading-state {
    display: flex; flex-direction: column; align-items: center; gap: 12px;
    padding: 64px 32px; text-align: center;
    background: var(--bg-secondary); border: 2px dashed var(--border-color);
    border-radius: var(--border-radius-xl); color: var(--text-muted);
  }
  .empty-icon {
    width: 72px; height: 72px; border-radius: 50%; display: flex; align-items: center; justify-content: center;
    background: var(--bg-elevated); color: var(--text-muted);
  }
  .empty-state h3 { margin: 0; font-size: 16px; color: var(--text-primary); }
  .empty-state p { margin: 0; font-size: 13px; max-width: 320px; }

  .error {
    padding: 10px 12px; border-radius: 10px; margin-bottom: 16px;
    background: rgba(239, 68, 68, 0.12); border: 1px solid rgba(239, 68, 68, 0.35); color: #fca5a5;
  }
</style>
