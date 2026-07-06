<script lang="ts">
  import { onMount } from "svelte";
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
    ArrowUpDown,
    Loader2,
    GitGraph,
    Zap,
    Lightbulb,
    ArrowUpCircle,
    Sparkles,
  } from "lucide-svelte";
  import { projectPath } from "../lib/store";

  type ModRow = {
    id: string;
    name: string;
    version: string;
    side: "client" | "server" | "both" | "optional" | "unknown" | string;
    source: string;
    projectId?: string | null;
    fileName?: string | null;
    iconUrl?: string | null;
    clientSide?: string | null;
    serverSide?: string | null;
    contentType?: "mod" | "resourcepack" | "shader" | "datapack" | string;
  };

  type SearchResult = {
    id: string;
    slug: string;
    name: string;
    description: string;
    projectType: string;
    iconUrl?: string | null;
    clientSide?: string | null;
    serverSide?: string | null;
  };

  type InstallPreview = {
    projectId: string;
    slug: string;
    name: string;
    version: string;
    fileName?: string | null;
    side: string;
    dependencies: { type: string; target: string; versionConstraint?: string | null; reason?: string | null }[];
  };

  let mods: ModRow[] = [];
  let loading = false;
  let mutating = false;
  let filter = "";
  let sideFilter = "all";
  let contentFilter = "mod"; // mod, resourcepack, datapack, shader
  let error: string | null = null;
  let lastLoadedPath: string | null = null;

  let addOpen = false;
  let searchQuery = "";
  let searchResults: SearchResult[] = [];
  let searchLoading = false;
  let selectedSide = "auto";
  let iconCache: Record<string, string | null> = {};
  let filterGameVersion = "";
  let filterLoader = "fabric";
  let filterCategory = "";
  let filterEnvironment = "";
  let filterLicense = "";
  let sortBy = "relevance";
  let previewLoadingId = "";
  let previews: Record<string, InstallPreview | null> = {};
  let pendingInstall: SearchResult | null = null;
  let selectedResultIds: Record<string, boolean> = {};

  // --- Version picker (change mod version) ---
  type ModVersion = { id: string; versionNumber: string; gameVersions: string[]; loaders: string[] };
  let versionPickerMod: ModRow | null = null;
  let availableVersions: ModVersion[] = [];
  let versionPickerLoading = false;
  let versionPickerError: string | null = null;
  let versionPickerChanging = false;

  async function openVersionPicker(mod: ModRow) {
    if (!$projectPath || !mod.projectId) return;
    versionPickerMod = mod;
    versionPickerLoading = true;
    versionPickerError = null;
    availableVersions = [];
    try {
      const info: any = await invoke("validate_project", { path: $projectPath });
      availableVersions = await invoke("get_mod_versions", {
        modId: mod.projectId,
        minecraftVersion: info.minecraftVersion,
        loader: info.loaderKind,
      });
    } catch (e) {
      versionPickerError = String(e);
    } finally {
      versionPickerLoading = false;
    }
  }

  async function changeVersion(versionId: string) {
    if (!$projectPath || !versionPickerMod) return;
    versionPickerChanging = true;
    versionPickerError = null;
    try {
      await invoke("change_mod_version", {
        path: $projectPath,
        modId: versionPickerMod.id,
        newVersionId: versionId,
      });
      versionPickerMod = null;
      availableVersions = [];
      await load(true);
    } catch (e) {
      versionPickerError = String(e);
    } finally {
      versionPickerChanging = false;
    }
  }

  // --- Post-bulk-install dependency resolution ---
  let dependencyDialogOpen = false;
  let dependencyMissingCount = 0;
  let dependencyResolving = false;

  let confirmOpen = false;
  let confirmMod: ModRow | null = null;

  function showRemoveConfirm(mod: ModRow) { confirmMod = mod; confirmOpen = true; }

  async function doRemove() {
    if (!$projectPath || !confirmMod) return;
    confirmOpen = false;
    mutating = true; error = null;
    try { await invoke("remove_project_mod", { path: $projectPath, modId: confirmMod.id }); await load(true); }
    catch (e) { error = String(e); }
    finally { mutating = false; confirmMod = null; }
  }

  // Mod recommendations
  let recommendations: any[] = [];
  let recsLoading = false;

  async function loadRecommendations() {
    if (!$projectPath) return;
    recsLoading = true;
    try { recommendations = await invoke("recommend_mods", { path: $projectPath }); }
    catch { recommendations = []; }
    finally { recsLoading = false; }
  }

  // Batch update state
  let updateList: any[] = [];
  let updateCheckLoading = false;
  let updateApplying = false;

  async function checkForUpdates() {
    if (!$projectPath) return;
    updateCheckLoading = true;
    error = null;
    try {
      updateList = await invoke("check_mod_updates", { path: $projectPath });
    } catch (e) {
      error = String(e);
    } finally {
      updateCheckLoading = false;
    }
  }

  async function applyAllUpdates() {
    if (!$projectPath || updateList.length === 0) return;
    updateApplying = true;
    error = null;
    try {
      const updated: string[] = await invoke("update_all_mods", { path: $projectPath });
      message = `Updated ${updated.length} mods: ${updated.join(", ")}`;
      updateList = [];
      await load(true);
    } catch (e) {
      error = String(e);
    } finally {
      updateApplying = false;
    }
  }

  function checkMissingDepsAfterInstall() {
    // After bulk install, check the graph for missing edges
    if (!$projectPath) return;
    invoke("get_graph", { path: $projectPath }).then((graph: any) => {
      const missing = (graph.edges ?? []).filter(
        (e: any) => e.kind === "Requires" && !(graph.nodes ?? []).some((n: any) => n.id === e.to)
      );
      if (missing.length > 0) {
        dependencyMissingCount = missing.length;
        dependencyDialogOpen = true;
      }
    }).catch(() => {});
  }

  async function resolveDepsViaGraph() {
    dependencyDialogOpen = false;
    // Switch to graph view — signal via a custom event
    window.dispatchEvent(new CustomEvent("tuffbox:open-graph"));
  }

  async function autoResolveDeps() {
    if (!$projectPath) return;
    dependencyResolving = true;
    error = null;
    try {
      const installed: string[] = await invoke("resolve_missing_dependencies", { path: $projectPath });
      dependencyDialogOpen = false;
      message = installed.length ? `Auto-installed ${installed.length} dependencies: ${installed.join(", ")}` : "No missing dependencies to install.";
      await load(true);
    } catch (e) {
      error = String(e);
    } finally {
      dependencyResolving = false;
    }
  }

  let message: string | null = null;

  // Change plan preview before install
  let planPreviewOpen = false;
  let planPreviewMod: SearchResult | null = null;
  let planPreviewLoading = false;
  let planPreviewDeps: InstallPreview | null = null;

  async function showPlanPreview(result: SearchResult) {
    planPreviewMod = result;
    planPreviewOpen = true;
    planPreviewLoading = true;
    try {
      planPreviewDeps = await invoke("preview_modrinth_install", { path: $projectPath, modId: result.id });
    } catch {
      planPreviewDeps = null;
    } finally {
      planPreviewLoading = false;
    }
  }

  async function confirmFromPlan(withDeps: boolean) {
    if (!$projectPath || !planPreviewMod) return;
    planPreviewOpen = false;
    mutating = true;
    error = null;
    try {
      if (withDeps) {
        await invoke("add_modrinth_mod_with_dependencies", { path: $projectPath, modId: planPreviewMod.id, side: selectedSide });
      } else {
        await invoke("add_modrinth_mod", { path: $projectPath, modId: planPreviewMod.id, side: selectedSide });
      }
      addOpen = false;
      selectedResultIds = {};
      searchResults = [];
      searchQuery = "";
      await load(true);
      checkMissingDepsAfterInstall();
    } catch (e) {
      error = String(e);
    } finally {
      mutating = false;
    }
  }

  // Populated from the real Mojang version manifest via get_minecraft_versions
  // instead of a hand-maintained list, so it never goes stale as new
  // Minecraft versions ship.
  let gameVersions: string[] = [];
  const loaders = ["Fabric", "Forge", "NeoForge"];
  const categories = [
    "Adventure", "Cursed", "Decoration", "Economy", "Equipment", "Food", "Game Mechanics", "Library",
    "Magic", "Management", "Minigame", "Mobs", "Optimization", "Social", "Storage", "Technology",
    "Transportation", "Utility", "World Generation"
  ];
  const sortOptions = [
    { id: "relevance", label: "Relevance" },
    { id: "downloads", label: "Downloads" },
    { id: "follows", label: "Followers" },
    { id: "newest", label: "Date published" },
    { id: "updated", label: "Date updated" },
  ];

  onMount(async () => {
    try {
      const versions: { id: string; popular: boolean }[] = await invoke("get_minecraft_versions");
      // Show the curated "popular" set plus a handful of the newest
      // releases, so the filter stays short but current.
      const popular = versions.filter((v) => v.popular).map((v) => v.id);
      const latest = versions.filter((v) => !v.popular).slice(0, 8).map((v) => v.id);
      gameVersions = [...new Set([...latest, ...popular])];
    } catch {
      // Network unavailable at startup — filter list stays empty, "Current / all" still works.
    }
  });

  async function load(force = false) {
    if (!$projectPath) return;
    if (!force && lastLoadedPath === $projectPath && mods.length > 0) return;
    loading = true;
    error = null;
    try {
      mods = await invoke("list_mods", { path: $projectPath });
      lastLoadedPath = $projectPath;
      hydrateInstalledIcons(mods);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  function contentTypeForFilter(filter: string): string {
    switch (filter) {
      case "resourcepack": return "resourcepack";
      case "datapack": return "datapack";
      case "shader": return "shader";
      default: return "mod";
    }
  }

  function switchContentFilter(next: string) {
    contentFilter = next;
    if (addOpen) searchMods();
  }

  async function openAddModal() {
    addOpen = true;
    error = null;
    await searchMods();
  }

  async function searchMods() {
    if (!$projectPath) return;
    searchLoading = true;
    error = null;
    try {
      searchResults = await invoke("search_modrinth_mods", {
        path: $projectPath,
        query: searchQuery.trim(),
        gameVersion: filterGameVersion || null,
        // Resourcepacks/datapacks/shaders aren't tied to a mod loader on
        // Modrinth; sending a loader facet for them would return zero
        // results, so only apply it for the "mod" tab.
        loader: contentFilter === "mod" && filterLoader ? filterLoader.toLowerCase() : null,
        category: filterCategory || null,
        environment: filterEnvironment || null,
        license: filterLicense || null,
        sort: sortBy,
        contentType: contentTypeForFilter(contentFilter),
      });
    } catch (e) {
      error = String(e);
    } finally {
      searchLoading = false;
    }
  }

  async function loadInstallPreview(result: SearchResult) {
    if (!$projectPath) return;
    if (previews[result.id] !== undefined) return;
    previewLoadingId = result.id;
    try {
      previews[result.id] = await invoke("preview_modrinth_install", { path: $projectPath, modId: result.id });
      previews = { ...previews };
    } catch {
      previews[result.id] = null;
      previews = { ...previews };
    } finally {
      previewLoadingId = "";
    }
  }

  async function startInstallPlan(result: SearchResult) {
    pendingInstall = result;
    await loadInstallPreview(result);
  }

  function toggleResultSelection(result: SearchResult) {
    selectedResultIds = { ...selectedResultIds, [result.id]: !selectedResultIds[result.id] };
  }

  function selectVisibleResults() {
    const next = { ...selectedResultIds };
    for (const result of searchResults) {
      if (!isInstalled(result)) next[result.id] = true;
    }
    selectedResultIds = next;
  }

  function clearResultSelection() {
    selectedResultIds = {};
  }

  async function bulkInstallSelected() {
    if (!$projectPath || selectedResults.length === 0) return;
    mutating = true;
    error = null;
    try {
      await invoke("add_modrinth_mods_with_dependencies", {
        path: $projectPath,
        modIds: selectedResults.map((result) => result.id),
        side: selectedSide,
      });
      addOpen = false;
      selectedResultIds = {};
      searchResults = [];
      searchQuery = "";
      await load(true);
      checkMissingDepsAfterInstall();
    } catch (e) {
      error = String(e);
    } finally {
      mutating = false;
    }
  }

  async function confirmInstall(withDependencies = false) {
    if (!$projectPath || !pendingInstall) return;
    mutating = true;
    error = null;
    try {
      if (withDependencies) {
        await invoke("add_modrinth_mod_with_dependencies", {
          path: $projectPath,
          modId: pendingInstall.id,
          side: selectedSide,
        });
      } else {
        await invoke("add_modrinth_mod", {
          path: $projectPath,
          modId: pendingInstall.id,
          side: selectedSide,
        });
      }
      addOpen = false;
      pendingInstall = null;
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
    showRemoveConfirm(mod);
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

  async function hydrateInstalledIcons(rows: ModRow[]) {
    const targets = rows
      .filter((mod) => mod.source === "modrinth" && mod.projectId && iconCache[mod.projectId] === undefined)
      .map((mod) => mod.projectId as string);
    const unique = Array.from(new Set(targets));
    if (unique.length === 0) return;
    await Promise.all(
      unique.map(async (projectId) => {
        try {
          iconCache[projectId] = await invoke("get_modrinth_project_icon", { projectId });
        } catch {
          iconCache[projectId] = null;
        }
      })
    );
    iconCache = { ...iconCache };
  }

  function modIconUrl(mod: ModRow) {
    return mod.iconUrl ?? (mod.projectId ? iconCache[mod.projectId] : null);
  }

  function isInstalled(result: SearchResult) {
    return mods.some((m) => m.id === result.slug || m.id === result.id || m.projectId === result.id);
  }

  function iconFallback(name: string) {
    return name?.[0]?.toUpperCase() ?? "?";
  }

  function depKind(dep: { type: string }) {
    return String(dep.type ?? "").toLowerCase();
  }

  function requiredDeps(preview: InstallPreview | null | undefined) {
    return (preview?.dependencies ?? []).filter((dep) => depKind(dep).includes("requires"));
  }

  function conflictDeps(preview: InstallPreview | null | undefined) {
    return (preview?.dependencies ?? []).filter((dep) => {
      const kind = depKind(dep);
      return kind.includes("conflict") || kind.includes("break") || kind.includes("incompatible");
    });
  }

  $: filtered = mods.filter((m) => {
    const q = filter.toLowerCase();
    const matchesText =
      m.name.toLowerCase().includes(q) ||
      m.id.toLowerCase().includes(q) ||
      m.version.toLowerCase().includes(q);
    const matchesSide = sideFilter === "all" || m.side === sideFilter;
    const matchesContentType = (m.contentType ?? "mod") === contentFilter;
    return matchesText && matchesSide && matchesContentType;
  });

  $: selectedResults = searchResults.filter((result) => selectedResultIds[result.id] && !isInstalled(result));

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
    <div class="tabs" style="display: flex; gap: 8px; margin-bottom: 12px; overflow-x: auto;">
      <button class={contentFilter === "mod" ? "primary" : "secondary"} on:click={() => switchContentFilter("mod")}>Mods</button>
      <button class={contentFilter === "resourcepack" ? "primary" : "secondary"} on:click={() => switchContentFilter("resourcepack")}>Resourcepacks</button>
      <button class={contentFilter === "datapack" ? "primary" : "secondary"} on:click={() => switchContentFilter("datapack")}>Datapacks</button>
      <button class={contentFilter === "shader" ? "primary" : "secondary"} on:click={() => switchContentFilter("shader")}>Shaders</button>
    </div>
    <div style="display: flex; justify-content: space-between; gap: 16px; align-items: center;">
      <div class="search" style="flex: 1;">
        <Search size={16} />
        <input bind:value={filter} placeholder="Search {contentFilter}s..." />
      </div>
      <div class="actions">
        <button class="secondary filter-button" disabled>
          <Filter size={16} />
          {sideFilter === "all" ? "All sides" : sideFilter}
        </button>
        <button on:click={openAddModal} disabled={!$projectPath || mutating}>
          <Plus size={16} />
          Add {contentFilter}
        </button>
        <button class="secondary" on:click={async () => {
          loading = true;
          try {
            mods = await invoke("sync_mods_folder", { path: $projectPath });
            hydrateInstalledIcons(mods);
          } catch(e) {
            error = String(e);
          } finally {
            loading = false;
          }
        }} disabled={!$projectPath || loading} title="Scan all content folders (mods/, resourcepacks/, shaderpacks/, datapacks/)">
          <RotateCw size={16} /> Sync folders
        </button>
        <button class="secondary" on:click={checkForUpdates} disabled={!$projectPath || updateCheckLoading} title="Check all mods for updates">
          <Sparkles size={16} />
          {updateCheckLoading ? "Checking..." : updateList.length > 0 ? `${updateList.length} updates` : "Check updates"}
        </button>
        <button class="secondary" on:click={loadRecommendations} disabled={!$projectPath || recsLoading} title="Get mod recommendations">
          <Lightbulb size={16} />
          {recsLoading ? "..." : "Suggestions"}
        </button>
        <button class="ghost" on:click={() => load(true)} title="Refresh" disabled={!$projectPath || loading}>
          <RefreshCw size={16} class={loading ? "spin" : ""} />
        </button>
      </div>
    </div>
  </div>

  <div class="quick-filters" aria-label="Side filters">
    <button class:active={sideFilter === "all"} on:click={() => (sideFilter = "all")}>All <span>{counts.all}</span></button>
    <button class:active={sideFilter === "both"} on:click={() => (sideFilter = "both")}>Both <span>{counts.both}</span></button>
    <button class:active={sideFilter === "client"} on:click={() => (sideFilter = "client")}>Client <span>{counts.client}</span></button>
    <button class:active={sideFilter === "server"} on:click={() => (sideFilter = "server")}>Server <span>{counts.server}</span></button>
  </div>

  {#if recommendations.length > 0}
    <div class="recs-panel">
      <div class="recs-header"><h3><Lightbulb size={16} /> Recommendations ({recommendations.length})</h3></div>
      <div class="recs-list">
        {#each recommendations as rec}
          <div class="recs-row">
            <div class="recs-main">
              <span class="recs-prio {rec.priority}">{rec.priority}</span>
              <strong>{rec.name}</strong>
              <span>{rec.description}</span>
            </div>
            <button class="secondary mini" on:click={async () => {
              if (!$projectPath) return;
              mutating = true;
              try {
                await invoke("add_modrinth_mod_with_dependencies", { path: $projectPath, modId: rec.slug, side: "auto" });
                recommendations = recommendations.filter((r) => r.slug !== rec.slug);
                await load(true);
                checkMissingDepsAfterInstall();
              } catch(e) { error = String(e); }
              finally { mutating = false; }
            }} disabled={mutating}>
              <Plus size={12} /> Install
            </button>
          </div>
        {/each}
      </div>
    </div>
  {/if}

  {#if updateList.length > 0}
    <div class="update-panel">
      <div class="update-panel-header">
        <h3><ArrowUpCircle size={16} /> {updateList.length} mod update{updateList.length > 1 ? "s" : ""} available</h3>
        <button on:click={applyAllUpdates} disabled={updateApplying}>
          <Sparkles size={16} /> {updateApplying ? "Updating..." : "Update all"}
        </button>
      </div>
      <div class="update-list">
        {#each updateList as update}
          <div class="update-row">
            <div class="update-main">
              <strong>{update.name}</strong>
              <span>{update.currentVersion} → <code>{update.latestVersion}</code></span>
            </div>
            <button class="secondary mini" on:click={async () => {
              if (!$projectPath) return;
              mutating = true;
              try {
                await invoke("update_project_mod", { path: $projectPath, modId: update.modId });
                updateList = updateList.filter((u) => u.modId !== update.modId);
                await load(true);
              } catch(e) { error = String(e); }
              finally { mutating = false; }
            }} disabled={mutating}>
              <RotateCw size={12} /> Update
            </button>
          </div>
        {/each}
      </div>
    </div>
  {/if}

  {#if error}
    <div class="error">{error}</div>
  {/if}
  {#if message}
    <div class="notice success">{message}</div>
  {/if}

  {#if loading}
    <div class="loading">Loading mods...</div>
  {:else if !$projectPath}
    <div class="empty">Open a project to manage mods.</div>
  {:else if filtered.length === 0}
    <div class="empty">No mods found.</div>
  {:else}
    <div class="installed-list">
      {#each filtered as mod}
        <article class="installed-card">
          <div class="mod-icon">
            {#if modIconUrl(mod)}
              <img src={modIconUrl(mod)} alt="" loading="lazy" />
            {:else}
              <span>{iconFallback(mod.name)}</span>
            {/if}
          </div>
          <div class="installed-main">
            <div class="installed-title">
              <strong>{mod.name}</strong>
              <code>{mod.id}</code>
            </div>
            <div class="installed-meta">
              <span class="version">{mod.version}</span>
              {#if mod.fileName}<span>{mod.fileName}</span>{/if}
            </div>
          </div>
          <div class="installed-tags">
            <span class="tag side-{mod.side}">{mod.side}</span>
            <span class="tag source">{mod.source}</span>
          </div>
          <div class="card-actions">
            <button class="icon-btn" on:click={() => openVersionPicker(mod)} disabled={mutating || mod.source !== "modrinth"} title="Change version">
              <ArrowUpDown size={16} />
            </button>
            <button class="icon-btn" on:click={() => updateMod(mod)} disabled={mutating || mod.source !== "modrinth"} title="Update to latest from Modrinth">
              <RotateCw size={16} />
            </button>
            <button class="icon-btn danger" on:click={() => showRemoveConfirm(mod)} disabled={mutating} title="Remove with snapshot">
              <Trash2 size={16} />
            </button>
          </div>
        </article>
      {/each}
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
          <option value="auto">Auto side</option>
          <option value="both">Both</option>
          <option value="client">Client</option>
          <option value="server">Server</option>
        </select>
        <label class="sort-select">
          Sort by:
          <select bind:value={sortBy} on:change={() => searchMods()}>
            {#each sortOptions as option}<option value={option.id}>{option.label}</option>{/each}
          </select>
        </label>
        <button on:click={searchMods} disabled={searchLoading}>
          <Search size={16} />
          Search
        </button>
      </div>

      <div class="browser-layout">
        <aside class="filter-panel">
          <h3>Game version</h3>
          <button class:active={!filterGameVersion} on:click={() => { filterGameVersion = ""; searchMods(); }}>Current / all</button>
          {#each gameVersions as version}
            <button class:active={filterGameVersion === version} on:click={() => { filterGameVersion = version; searchMods(); }}>{version}</button>
          {/each}
          <button class="muted-filter" on:click={() => { filterGameVersion = ""; searchMods(); }}>Show all versions</button>

          {#if contentFilter === "mod"}
            <h3>Loader</h3>
            {#each loaders as loaderName}
              <button class:active={filterLoader === loaderName.toLowerCase()} on:click={() => { filterLoader = loaderName.toLowerCase(); searchMods(); }}>{loaderName}</button>
            {/each}
          {/if}

          <h3>Category</h3>
          <button class:active={!filterCategory} on:click={() => { filterCategory = ""; searchMods(); }}>All categories</button>
          {#each categories as category}
            <button class:active={filterCategory === category} on:click={() => { filterCategory = category; searchMods(); }}>{category}</button>
          {/each}

          <h3>Environment</h3>
          <button class:active={filterEnvironment === "client"} on:click={() => { filterEnvironment = filterEnvironment === "client" ? "" : "client"; searchMods(); }}>Client</button>
          <button class:active={filterEnvironment === "server"} on:click={() => { filterEnvironment = filterEnvironment === "server" ? "" : "server"; searchMods(); }}>Server</button>

          <h3>License</h3>
          <label class="check-row"><input type="checkbox" checked={filterLicense === "open-source"} on:change={() => { filterLicense = filterLicense === "open-source" ? "" : "open-source"; searchMods(); }} /> Open source</label>
        </aside>

        <section class="browser-results">
          <div class="bulk-bar">
            <div>
              <strong>{selectedResults.length}</strong>
              <span>selected for bulk install</span>
            </div>
            <div class="bulk-actions">
              <button class="ghost" on:click={selectVisibleResults} disabled={searchResults.length === 0}>Select visible</button>
              <button class="ghost" on:click={clearResultSelection} disabled={selectedResults.length === 0}>Clear</button>
              <button on:click={bulkInstallSelected} disabled={selectedResults.length === 0 || mutating}>Install selected + dependencies</button>
            </div>
          </div>
          {#if searchLoading}
            <div class="loading compact">Loading Modrinth projects...</div>
          {:else if searchResults.length === 0}
            <div class="empty compact">No projects found. Adjust filters or search text.</div>
          {:else}
            <div class="results">
          {#each searchResults as result}
            <article class="result-card" class:installed={isInstalled(result)} class:selected={selectedResultIds[result.id]} on:mouseenter={() => loadInstallPreview(result)} on:focusin={() => loadInstallPreview(result)}>
              <label class="select-result" title="Select for bulk install">
                <input type="checkbox" checked={!!selectedResultIds[result.id]} disabled={isInstalled(result)} on:change={() => toggleResultSelection(result)} />
              </label>
              <div class="result-icon">
                {#if result.iconUrl}
                  <img src={result.iconUrl} alt="" loading="lazy" />
                {:else}
                  <span>{iconFallback(result.name)}</span>
                {/if}
              </div>
              <div class="result-main">
                <div class="result-title">
                  <span>{result.name}</span>
                  <code>{result.slug}</code>
                </div>
                <div class="env-row">
                  <small>Client: {result.clientSide ?? "unknown"}</small>
                  <small>Server: {result.serverSide ?? "unknown"}</small>
                </div>
                <p>{result.description}</p>
                {#if previewLoadingId === result.id}
                  <div class="install-preview muted">Loading install preview...</div>
                {:else if previews[result.id]}
                  <div class="install-preview">
                    <span>Version: {previews[result.id]?.version}</span>
                    <span>Side: {previews[result.id]?.side}</span>
                    <span>Deps: {previews[result.id]?.dependencies.length ?? 0}</span>
                    {#if previews[result.id]?.dependencies.length}
                      <div class="deps">
                        {#each previews[result.id]?.dependencies.slice(0, 4) ?? [] as dep}
                          <code>{dep.type}:{dep.target}</code>
                        {/each}
                      </div>
                    {/if}
                  </div>
                {/if}
              </div>
              <button on:click={() => startInstallPlan(result)} disabled={mutating || isInstalled(result)}>
                <Download size={16} />
                {isInstalled(result) ? "Installed" : "Install plan"}
              </button>
            </article>
          {/each}
            </div>
          {/if}
        </section>
      </div>

      {#if pendingInstall}
        <div class="install-plan-panel">
          <div>
            <span class="plan-eyebrow">Install plan</span>
            <h3>{pendingInstall.name}</h3>
            {#if previews[pendingInstall.id]}
              <p>
                Version {previews[pendingInstall.id]?.version} · side {previews[pendingInstall.id]?.side} ·
                {previews[pendingInstall.id]?.dependencies.length ?? 0} dependencies
              </p>
              {#if conflictDeps(previews[pendingInstall.id]).length}
                <div class="conflict-warning">
                  <strong>Conflict warning</strong>
                  <span>This project declares incompatible dependencies. Review before installing.</span>
                  <div class="deps">
                    {#each conflictDeps(previews[pendingInstall.id]) as dep}
                      <code>{dep.type}:{dep.target}</code>
                    {/each}
                  </div>
                </div>
              {/if}
              {#if previews[pendingInstall.id]?.dependencies.length}
                <div class="dependency-tree">
                  <strong>Dependency tree</strong>
                  {#if requiredDeps(previews[pendingInstall.id]).length}
                    {#each requiredDeps(previews[pendingInstall.id]) as dep}
                      <div class="dep-node"><span>requires</span><code>{dep.target}</code></div>
                    {/each}
                  {:else}
                    <div class="dep-node muted"><span>No required dependencies detected.</span></div>
                  {/if}
                </div>
              {/if}
            {:else}
              <p>Preview unavailable; TuffBox will still create a snapshot before installing.</p>
            {/if}
          </div>
          <div class="plan-actions">
            <button class="ghost" on:click={() => (pendingInstall = null)}>Cancel</button>
            <button class="secondary" on:click={() => confirmInstall(false)} disabled={mutating}>Install only this mod</button>
            <button on:click={() => confirmInstall(true)} disabled={mutating}>Install with dependencies</button>
          </div>
        </div>
      {/if}
    </div>
  </div>
{/if}

<!-- Version picker modal -->
{#if versionPickerMod}
  <div class="modal-backdrop" role="button" tabindex="-1" on:click={() => (versionPickerMod = null)} on:keydown={() => {}}>
    <div class="modal version-modal" role="dialog" aria-modal="true" on:click|stopPropagation>
      <div class="modal-header">
        <div>
          <h2>Change version: {versionPickerMod.name}</h2>
          <p>Current: {versionPickerMod.version}. Choose a different version.</p>
        </div>
        <button class="icon-btn" on:click={() => (versionPickerMod = null)} aria-label="Close"><X size={18} /></button>
      </div>
      {#if versionPickerError}<div class="error compact">{versionPickerError}</div>{/if}
      {#if versionPickerLoading}
        <div class="loading compact"><Loader2 size={20} class="spin" /> Loading versions...</div>
      {:else if availableVersions.length === 0}
        <div class="empty compact">No compatible versions found for this mod.</div>
      {:else}
        <div class="version-list">
          {#each availableVersions as v}
            <button
              class="version-row"
              class:current={v.versionNumber === versionPickerMod?.version}
              on:click={() => changeVersion(v.id)}
              disabled={versionPickerChanging}
            >
              <div class="version-main">
                <strong>{v.versionNumber}</strong>
                <span class="version-loaders">{v.loaders.join(", ")} · MC {v.gameVersions.slice(0, 3).join(", ")}</span>
              </div>
              {#if v.versionNumber === versionPickerMod?.version}
                <span class="current-badge">Current</span>
              {:else}
                <span class="install-badge">Install</span>
              {/if}
            </button>
          {/each}
        </div>
      {/if}
    </div>
  </div>
{/if}

<!-- Post-bulk dependency resolution dialog -->
{#if dependencyDialogOpen}
  <div class="modal-backdrop" role="button" tabindex="-1" on:click={() => (dependencyDialogOpen = false)} on:keydown={() => {}}>
    <div class="modal dep-dialog" role="dialog" aria-modal="true" on:click|stopPropagation>
      <div class="modal-header">
        <div>
          <h2>Missing dependencies</h2>
          <p>{dependencyMissingCount} required mod(s) are still missing. How would you like to handle this?</p>
        </div>
        <button class="icon-btn" on:click={() => (dependencyDialogOpen = false)} aria-label="Close"><X size={18} /></button>
      </div>
      <div class="dep-dialog-actions">
        <button class="secondary" on:click={resolveDepsViaGraph}>
          <GitGraph size={18} /> Open in Graph
          <span>See which mods need which dependencies and install them one by one.</span>
        </button>
        <button on:click={autoResolveDeps} disabled={dependencyResolving}>
          <Zap size={18} />
          {dependencyResolving ? "Installing..." : "Auto-download all"}
          <span>Let TuffBox find and install every missing dependency automatically.</span>
        </button>
      </div>
      <div class="dep-dialog-footer">
        <button class="ghost" on:click={() => (dependencyDialogOpen = false)}>Skip for now</button>
      </div>
    </div>
  </div>
{/if}

<!-- Change plan preview modal -->
{#if planPreviewOpen && planPreviewMod}
  <div class="modal-backdrop" role="button" tabindex="-1" on:click={() => (planPreviewOpen = false)} on:keydown={() => {}}>
    <div class="modal plan-modal" role="dialog" aria-modal="true" on:click|stopPropagation>
      <div class="modal-header">
        <div>
          <h2>Install plan: {planPreviewMod.name}</h2>
          <p>Auto-snapshot will be created before applying changes.</p>
        </div>
        <button class="icon-btn" on:click={() => (planPreviewOpen = false)} aria-label="Close"><X size={18} /></button>
      </div>

      {#if planPreviewLoading}
        <div class="loading compact"><Loader2 size={20} class="spin" /> Loading version info...</div>
      {:else}
        <div class="plan-details">
          <div class="plan-summary">
            <div class="plan-item">
              <strong>Mod</strong>
              <span>{planPreviewMod.name} ({planPreviewMod.slug})</span>
            </div>
            <div class="plan-item">
              <strong>Version to install</strong>
              <span>{planPreviewDeps?.version ?? "latest compatible"}</span>
            </div>
            <div class="plan-item">
              <strong>Side</strong>
              <span class="side-tag">{selectedSide}</span>
            </div>
            <div class="plan-item">
              <strong>File</strong>
              <span class="mono">{planPreviewDeps?.fileName ?? "downloaded from Modrinth"}</span>
            </div>
          </div>

          {#if planPreviewDeps && requiredDeps(planPreviewDeps).length > 0}
            <div class="plan-deps-section">
              <strong>Required dependencies ({requiredDeps(planPreviewDeps).length})</strong>
              <div class="plan-dep-list">
                {#each requiredDeps(planPreviewDeps) as dep}
                  <div class="plan-dep-row">
                    <code>{dep.target}</code>
                    {#if dep.versionConstraint}<span>{dep.versionConstraint}</span>{/if}
                  </div>
                {/each}
              </div>
            </div>
          {:else if planPreviewDeps}
            <div class="plan-no-deps">No required dependencies.</div>
          {/if}

          {#if planPreviewDeps && conflictDeps(planPreviewDeps).length > 0}
            <div class="plan-conflicts">
              <strong>⚠ Conflicts detected ({conflictDeps(planPreviewDeps).length})</strong>
              <div class="plan-dep-list">
                {#each conflictDeps(planPreviewDeps) as dep}
                  <div class="plan-dep-row conflict">
                    <code>{dep.target}</code>
                    <span>incompatible</span>
                  </div>
                {/each}
              </div>
            </div>
          {/if}
        </div>
      {/if}

      <div class="plan-modal-actions">
        <button class="ghost" on:click={() => { planPreviewOpen = false; if (planPreviewMod) startInstallPlan(planPreviewMod); }}>See raw details</button>
        <button class="secondary" on:click={() => confirmFromPlan(false)} disabled={mutating}>
          <Download size={16} /> Install mod only
        </button>
        <button on:click={() => confirmFromPlan(true)} disabled={mutating}>
          <Zap size={16} /> Install with dependencies
        </button>
      </div>
    </div>
  </div>
{/if}

<style>
  .mods {
    max-width: none;
    width: 100%;
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

  .installed-list {
    display: grid;
    gap: 10px;
  }

  .installed-card {
    min-height: 72px;
    display: grid;
    grid-template-columns: 52px minmax(0, 1fr) auto auto;
    gap: 14px;
    align-items: center;
    padding: 12px 14px;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
    transition: border-color .15s ease, background .15s ease;
  }

  .installed-card:hover {
    border-color: rgba(27, 217, 106, 0.28);
    background: rgba(255,255,255,0.025);
  }

  .mod-icon,
  .result-icon {
    width: 52px;
    height: 52px;
    border-radius: 14px;
    overflow: hidden;
    background: linear-gradient(135deg, var(--accent-secondary), var(--accent-primary));
    display: flex;
    align-items: center;
    justify-content: center;
    color: #fff;
    font-weight: 900;
    flex-shrink: 0;
  }

  .mod-icon img,
  .result-icon img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .installed-main {
    min-width: 0;
  }

  .installed-title {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }

  .installed-title strong {
    color: var(--text-primary);
    font-size: 15px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .installed-meta,
  .installed-tags,
  .card-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .installed-meta {
    margin-top: 5px;
    color: var(--text-muted);
    font-size: 12px;
    min-width: 0;
  }

  .installed-meta span:last-child {
    overflow: hidden;
    text-overflow: ellipsis;
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
    width: min(1560px, calc(100vw - 28px));
    max-height: min(940px, calc(100vh - 28px));
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

  .sort-select {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text-muted);
    font-size: 12px;
    white-space: nowrap;
  }

  .browser-layout {
    display: grid;
    grid-template-columns: 250px minmax(0, 1fr);
    gap: 16px;
    min-height: 650px;
  }

  .filter-panel {
    overflow: auto;
    max-height: calc(100vh - 190px);
    padding: 14px;
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
    background: rgba(255,255,255,0.018);
  }

  .filter-panel h3 {
    margin: 16px 0 8px;
    color: var(--text-muted);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: .08em;
  }

  .filter-panel h3:first-child {
    margin-top: 0;
  }

  .filter-panel button,
  .check-row {
    width: 100%;
    justify-content: flex-start;
    text-align: left;
    padding: 7px 9px;
    margin-bottom: 3px;
    background: transparent;
    color: var(--text-secondary);
    border: 1px solid transparent;
    transform: none;
  }

  .filter-panel button:hover,
  .filter-panel button.active {
    background: var(--bg-tertiary);
    border-color: rgba(27,217,106,.28);
    color: var(--text-primary);
  }

  .muted-filter {
    color: var(--text-muted) !important;
  }

  .check-row {
    display: flex;
    align-items: center;
    gap: 8px;
    cursor: pointer;
  }

  .check-row input {
    width: auto;
  }

  .browser-results {
    min-width: 0;
  }

  .bulk-bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    margin-bottom: 12px;
    padding: 12px;
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
    background: rgba(255,255,255,.018);
  }

  .bulk-bar strong { color: var(--accent-primary); font-size: 20px; }
  .bulk-bar span { color: var(--text-muted); margin-left: 6px; }
  .bulk-actions { display: flex; gap: 8px; flex-wrap: wrap; justify-content: flex-end; }

  .results {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(320px, 1fr));
    gap: 12px;
  }

  .result-card {
    position: relative;
    min-height: 168px;
    display: grid;
    grid-template-columns: 64px minmax(0, 1fr);
    grid-template-rows: 1fr auto;
    gap: 12px;
    align-items: start;
    padding: 16px;
    border-radius: var(--border-radius-lg);
    background: var(--bg-tertiary);
    border: 1px solid var(--border-color);
  }

  .result-card.installed {
    border-color: rgba(27, 217, 106, 0.35);
    background: rgba(27, 217, 106, 0.07);
  }

  .result-card.selected {
    border-color: rgba(139, 92, 246, 0.65);
    box-shadow: 0 0 0 1px rgba(139, 92, 246, 0.18) inset;
  }

  .select-result {
    position: absolute;
    top: 12px;
    right: 12px;
    z-index: 2;
  }

  .select-result input { width: 16px; height: 16px; }

  .result-icon {
    width: 64px;
    height: 64px;
    border-radius: 18px;
  }

  .result-card button {
    grid-column: 1 / -1;
    width: 100%;
  }

  .install-preview {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 8px 0;
    color: var(--text-muted);
    font-size: 11px;
  }

  .install-preview > span {
    background: var(--bg-elevated);
    border-radius: 999px;
    padding: 3px 7px;
  }

  .install-preview.muted {
    color: var(--text-muted);
  }

  .deps {
    width: 100%;
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
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

  .install-plan-panel {
    position: sticky;
    bottom: -22px;
    margin: 16px -22px -22px;
    padding: 16px 22px;
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 16px;
    align-items: center;
    background: linear-gradient(180deg, rgba(24,24,27,.96), rgba(9,9,11,.98));
    border-top: 1px solid rgba(27,217,106,.28);
  }

  .plan-eyebrow {
    color: var(--accent-primary);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: .1em;
    font-weight: 900;
  }

  .install-plan-panel h3 { margin: 3px 0 4px; }
  .install-plan-panel p { margin: 0; color: var(--text-muted); }
  .plan-actions { display: flex; gap: 10px; align-items: center; flex-wrap: wrap; justify-content: flex-end; }
  .plan-deps { margin-top: 8px; max-height: 80px; overflow: auto; }
  .conflict-warning { margin-top: 10px; padding: 10px; border: 1px solid rgba(239,68,68,.32); border-radius: 12px; background: rgba(239,68,68,.08); display: grid; gap: 6px; }
  .conflict-warning strong { color: #fecaca; }
  .conflict-warning span { color: var(--text-muted); font-size: 12px; }
  .dependency-tree { margin-top: 10px; display: grid; gap: 6px; max-height: 150px; overflow: auto; }
  .dependency-tree > strong { color: var(--text-secondary); }
  .dep-node { position: relative; display: flex; gap: 8px; align-items: center; margin-left: 14px; padding-left: 14px; color: var(--text-muted); font-size: 12px; }
  .dep-node::before { content: ""; position: absolute; left: 0; top: -6px; bottom: 50%; width: 10px; border-left: 1px solid rgba(27,217,106,.35); border-bottom: 1px solid rgba(27,217,106,.35); }
  .dep-node.muted::before { border-color: var(--border-color); }

  .plan-modal { max-width: 540px; }
  .plan-details { padding: 12px 0; display: grid; gap: 16px; }
  .plan-summary { display: grid; gap: 8px; }
  .plan-item { display: flex; justify-content: space-between; align-items: center; gap: 12px; padding: 8px 0; border-bottom: 1px solid var(--border-color); }
  .plan-item strong { color: var(--text-primary); font-size: 13px; }
  .plan-item span { color: var(--text-muted); font-size: 13px; text-align: right; }
  .plan-item .side-tag { text-transform: uppercase; font-weight: 700; }
  .plan-item .mono { font-family: ui-monospace,monospace; font-size: 11px; }
  .plan-deps-section, .plan-conflicts { padding: 12px; border-radius: 10px; border: 1px solid var(--border-color); background: var(--bg-tertiary); }
  .plan-deps-section > strong { color: var(--accent-primary); font-size: 13px; display: block; margin-bottom: 8px; }
  .plan-conflicts > strong { color: #fca5a5; font-size: 13px; display: block; margin-bottom: 8px; }
  .plan-dep-list { display: grid; gap: 4px; }
  .plan-dep-row { display: flex; justify-content: space-between; gap: 8px; padding: 6px 8px; border-radius: 6px; background: var(--bg-secondary); }
  .plan-dep-row code { font-size: 12px; }
  .plan-dep-row span { color: var(--text-muted); font-size: 11px; }
  .plan-dep-row.conflict { border-left: 3px solid rgba(239,68,68,.6); }
  .plan-no-deps { color: var(--text-muted); font-size: 12px; padding: 8px; }
  .plan-modal-actions { display: flex; justify-content: flex-end; gap: 10px; padding-top: 14px; border-top: 1px solid var(--border-color); margin-top: 8px; }

  .recs-panel { margin-bottom: 16px; padding: 14px; border: 1px solid rgba(139,92,246,.25); border-radius: var(--border-radius-lg); background: rgba(139,92,246,.02); }
  .recs-header h3 { display: flex; align-items: center; gap: 8px; color: var(--accent-secondary); margin: 0 0 10px; font-size: 14px; }
  .recs-list { display: grid; gap: 6px; }
  .recs-row { display: flex; justify-content: space-between; align-items: center; gap: 12px; padding: 10px 12px; border-radius: 10px; background: var(--bg-tertiary); border: 1px solid var(--border-color); }
  .recs-main { display: grid; grid-template-columns: auto 1fr; gap: 2px 8px; align-items: center; }
  .recs-main strong { color: var(--text-primary); font-size: 13px; }
  .recs-main span { color: var(--text-muted); font-size: 11px; grid-column: 2; }
  .recs-prio { font-size: 9px; text-transform: uppercase; font-weight: 800; padding: 2px 6px; border-radius: 4px; }
  .recs-prio.critical { background: rgba(239,68,68,.15); color: #fca5a5; }
  .recs-prio.high { background: rgba(27,217,106,.12); color: var(--accent-primary); }
  .recs-prio.medium { background: rgba(96,165,250,.12); color: #93c5fd; }
  .recs-prio.low { background: var(--bg-elevated); color: var(--text-muted); }

  .update-panel { margin-bottom: 16px; padding: 16px; border: 1px solid rgba(27,217,106,.3); border-radius: var(--border-radius-lg); background: radial-gradient(circle at top right, rgba(27,217,106,.08), transparent 40%), var(--bg-secondary); }
  .update-panel-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px; }
  .update-panel-header h3 { display: flex; align-items: center; gap: 8px; color: var(--accent-primary); margin: 0; font-size: 15px; }
  .update-list { display: grid; gap: 6px; max-height: 240px; overflow: auto; }
  .update-row { display: flex; justify-content: space-between; align-items: center; gap: 12px; padding: 10px 12px; border-radius: 10px; background: var(--bg-tertiary); border: 1px solid var(--border-color); }
  .update-main { display: grid; gap: 2px; }
  .update-main strong { color: var(--text-primary); font-size: 13px; }
  .update-main span { color: var(--text-muted); font-size: 12px; }
  .update-main code { color: var(--accent-primary); font-size: 12px; font-weight: 700; }

  :global(.spin) {
    animation: spin 900ms linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .notice.success {
    padding: 12px 14px;
    border-radius: var(--border-radius-lg);
    margin-bottom: 14px;
    border: 1px solid rgba(27, 217, 106, 0.25);
    color: var(--accent-primary);
    background: rgba(27, 217, 106, 0.08);
  }

  .version-modal { max-width: 520px; }
  .version-list { display: grid; gap: 6px; max-height: 460px; overflow: auto; padding: 8px 0; }
  .version-row {
    display: flex; align-items: center; justify-content: space-between; gap: 12px;
    padding: 12px 14px; border-radius: 12px; border: 1px solid var(--border-color);
    background: var(--bg-tertiary); color: var(--text-secondary); text-align: left;
    width: 100%; transform: none;
  }
  .version-row:hover, .version-row.current { border-color: rgba(27,217,106,.35); background: rgba(27,217,106,.06); }
  .version-row:disabled { opacity: .5; cursor: wait; }
  .version-main { display: grid; gap: 4px; }
  .version-main strong { color: var(--text-primary); }
  .version-loaders { color: var(--text-muted); font-size: 12px; }
  .current-badge { font-size: 11px; font-weight: 800; color: var(--accent-primary); background: rgba(27,217,106,.15); padding: 4px 10px; border-radius: 999px; }
  .install-badge { font-size: 11px; font-weight: 700; color: var(--accent-secondary); background: rgba(139,92,246,.12); padding: 4px 10px; border-radius: 999px; }

  .dep-dialog { max-width: 520px; }
  .dep-dialog-actions { display: grid; gap: 14px; padding: 8px 0 18px; }
  .dep-dialog-actions button {
    display: grid; grid-template-columns: auto 1fr; gap: 4px 12px; align-items: center;
    width: 100%; padding: 16px 18px; border-radius: 14px; text-align: left; border: 1px solid var(--border-color);
    background: var(--bg-tertiary); color: var(--text-primary); transform: none;
  }
  .dep-dialog-actions button:hover { border-color: rgba(27,217,106,.4); }
  .dep-dialog-actions button span { grid-column: 2; color: var(--text-muted); font-size: 12px; line-height: 1.4; }
  .dep-dialog-footer { display: flex; justify-content: flex-end; padding-top: 8px; border-top: 1px solid var(--border-color); }
</style>
