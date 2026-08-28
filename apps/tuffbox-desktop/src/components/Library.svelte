<script lang="ts">
  import { onMount } from "svelte";
  import {
    Search,
    Plus,
    Download,
    FolderOpen,
    Star,
    Compass,
    LayoutGrid,
    ExternalLink,
  } from "@lucide/svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { open } from "@tauri-apps/plugin-dialog";
  import { open as openExternal } from "@tauri-apps/plugin-shell";
  import {
    recentProjects,
    projectPath,
    projectInfo,
    newProjectOpen,
    libraryTabRequest,
    addInstanceMode,
    openAddInstance,
  } from "../lib/store";
  import { toasts } from "../lib/toast";
  import { api, githubInspectMeta, onInstallLink } from "../lib/api";
  import type { SearchResult } from "../lib/api";
  import CreationTrends from "./CreationTrends.svelte";
  import AddInstanceModal from "./AddInstanceModal.svelte";
  import LibraryInstancesPane from "./LibraryInstancesPane.svelte";
  import PromptDialog from "./PromptDialog.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import GithubPackInstallProgress from "./GithubPackInstallProgress.svelte";
  import CatalogProjectView from "./CatalogProjectView.svelte";
  import KudosBalanceStrip from "./KudosBalanceStrip.svelte";

  let { currentView = $bindable() }: { currentView: "dashboard" | "ide" | "mods" | "graph" | "diagnostics" | "snapshots" | "configs" | "settings" | "project-settings" | "ore-gen" | "recipes" | "quests" | "library" | "chats" | "me" | "world" } = $props();

  type Tab = "yours" | "discover" | "create";

  let tab = $state<Tab>("yours");
  let swarmEnabled = $state(false);
  let p2pEnabled = $state(false);
  let kudosBalance = $state<{ totalKudos?: number; rac?: number } | null>(null);
  let kudosLoading = $state(false);
  let importing = $state(false);
  let importMenuOpen = $state(false);
  let githubImportOpen = $state(false);
  let githubConfirmOpen = $state(false);
  let githubInstallActive = $state(false);
  let githubPendingSource = $state("");
  let githubInspectSummary = $state("");

  async function loadSwarm() {
    try {
      const s = await invoke<{ enabled?: boolean; p2pEnabled?: boolean }>("get_swarm_settings");
      swarmEnabled = !!s?.enabled;
      p2pEnabled = !!s?.p2pEnabled;
    } catch {
      swarmEnabled = false;
      p2pEnabled = false;
    }
    if (swarmEnabled) {
      await loadKudos();
    } else {
      kudosBalance = null;
    }
  }

  async function loadKudos() {
    if (!swarmEnabled) {
      kudosBalance = null;
      return;
    }
    kudosLoading = true;
    try {
      kudosBalance = await invoke<{ totalKudos?: number; rac?: number }>("get_local_kudos_balance");
    } catch {
      kudosBalance = null;
    } finally {
      kudosLoading = false;
    }
  }

  function focusCreationPeerGen() {
    tab = "create";
    queueMicrotask(() => {
      document.querySelector(".create-trends .peer-gen")?.scrollIntoView({ behavior: "smooth", block: "start" });
    });
  }

  function openNewPack() {
    openAddInstance("blank");
  }

  async function resolveImportTargetDir(): Promise<string> {
    if (!downloadDir && !defaultDownloadDir) {
      await loadDownloadDir();
    }
    const local = (downloadDir || defaultDownloadDir).replace(/[\\/]+$/, "");
    if (local) return local;
    try {
      const info = await api.launcher.instancesPathInfo();
      const settings = await api.launcher.get();
      return (settings.instancesPath?.trim() || info.current || info.default || "").replace(
        /[\\/]+$/,
        "",
      );
    } catch {
      return "";
    }
  }

  async function finishImportedPack(result: { path?: string; name?: string; modCount?: number }) {
    const path = result.path;
    if (!path) throw new Error("Import returned no path");
    const info = (await invoke("validate_project", { path })) as {
      name?: string;
      manifestPath?: string;
    } & import("../lib/api").ProjectSummary;
    const manifestPath = info.manifestPath || path;
    recentProjects.add({ path: manifestPath, info: info as any });
    projectPath.set(manifestPath);
    projectInfo.set(info as any);
    toasts.success(
      `Imported "${result.name ?? info.name ?? "pack"}"${
        result.modCount != null ? ` · ${result.modCount} mods` : ""
      }`,
    );
    tab = "yours";
  }

  async function importFromSource(source: string) {
    importing = true;
    importMenuOpen = false;
    const isGithub = /^(gh:|https:\/\/github\.com\/|[A-Za-z0-9_.-]+\/[A-Za-z0-9_.-]+$)/.test(source.trim()) && !/\.(mrpack|zip)$/i.test(source.trim());
    if (isGithub) githubInstallActive = true;
    try {
      const targetDir = await resolveImportTargetDir();
      if (!targetDir) {
        toasts.error("Set an instances folder in Settings first.");
        return;
      }
      const result: any = await invoke("install_modpack", {
        source,
        targetDir,
        instanceName: null,
      });
      await finishImportedPack(result);
    } catch (e) {
      toasts.error(String(e));
    } finally {
      importing = false;
      githubInstallActive = false;
    }
  }

  async function importPackFile() {
    importMenuOpen = false;
    const selected = await open({
      multiple: false,
      title: "Import .mrpack or .zip",
      filters: [
        { name: "Modpacks", extensions: ["mrpack", "zip"] },
        { name: "All", extensions: ["*"] },
      ],
    });
    if (typeof selected !== "string" || !selected) return;
    await importFromSource(selected);
  }

  async function importInstanceFolder() {
    importMenuOpen = false;
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Import Prism / MultiMC / CurseForge / mods folder",
    });
    if (typeof selected !== "string" || !selected) return;
    await importFromSource(selected);
  }

  async function importGithubRepo() {
    importMenuOpen = false;
    githubImportOpen = true;
  }

  async function confirmGithubImport(source: string) {
    githubImportOpen = false;
    const trimmed = source.trim();
    if (!trimmed) return;
    try {
      const info = await api.transport.github.inspectSource(trimmed);
      if (info.status === "publishing") {
        toasts.error("This pack is still publishing oversized assets. Try again when the author finishes.");
        return;
      }
      githubPendingSource = trimmed;
      const version = info.packVersion ? ` v${info.packVersion}` : "";
      const ready = info.ready
        ? "ready"
        : info.status
          ? String(info.status)
          : "packwiz pack";
      const meta = githubInspectMeta(info);
      githubInspectSummary = `${info.fullName || trimmed}${version} · ${ready}${
        meta ? ` (${meta})` : ""
      }. Install anonymously?`;
      githubConfirmOpen = true;
    } catch (e) {
      toasts.error(String(e));
    }
  }

  async function confirmGithubInstall() {
    githubConfirmOpen = false;
    const source = githubPendingSource;
    githubPendingSource = "";
    if (source) await importFromSource(source);
  }

  async function onPackCreated(path: string) {
    newProjectOpen.set(false);
    try {
      const info = (await invoke("validate_project", { path })) as any;
      const manifestPath = info.manifestPath || path;
      recentProjects.add({ path: manifestPath, info });
      projectPath.set(manifestPath);
      projectInfo.set(info);
      toasts.success(`Created "${info.name ?? "pack"}"`);
      tab = "yours";
    } catch (err) {
      toasts.error(String(err));
    }
  }

  function onGlobalPointerDown(e: MouseEvent) {
    if (!importMenuOpen) return;
    const t = e.target as HTMLElement | null;
    if (!t?.closest?.(".import-wrap") && !t?.closest?.(".import-menu")) {
      importMenuOpen = false;
    }
  }

  function onGlobalKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      importMenuOpen = false;
      githubImportOpen = false;
    }
  }

  // ── Discover (Modrinth / CurseForge modpacks) ───────────────────
  type DiscoverResult = SearchResult & { provider?: "modrinth" | "curseforge" };
  type DiscoverProvider = "modrinth" | "curseforge" | "both";

  let query = $state("");
  let results = $state<DiscoverResult[]>([]);
  let loadingDiscover = $state(false);
  let discoverError = $state("");
  let adding = $state(new Set<string>());
  let discoverProvider = $state<DiscoverProvider>("modrinth");
  let downloadDir = $state("");
  let defaultDownloadDir = $state("");
  let brokenIcons = $state<string[]>([]);
  let catalogViewResult = $state<DiscoverResult | null>(null);
  let searchRequestId = 0;

  async function loadDownloadDir() {
    try {
      const info = await api.launcher.instancesPathInfo();
      defaultDownloadDir = info.default;
      const settings = await api.launcher.get();
      downloadDir = (settings.instancesPath?.trim() || info.current || info.default).replace(
        /[\\/]+$/,
        "",
      );
    } catch {
      downloadDir = "";
    }
  }

  async function browseDownloadDir() {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Select folder for downloaded modpacks",
    });
    if (typeof selected !== "string" || !selected) return;
    downloadDir = selected;
    try {
      const settings = await api.launcher.get();
      await api.launcher.save({ ...settings, instancesPath: selected });
      toasts.success("Download folder saved.");
    } catch (e) {
      toasts.error(String(e));
    }
  }

  async function applyDownloadDir() {
    const path = downloadDir.trim();
    if (!path) {
      toasts.error("Pick a download folder first.");
      return;
    }
    try {
      await api.launcher.validateInstancesPath(path);
      const settings = await api.launcher.get();
      await api.launcher.save({ ...settings, instancesPath: path });
      toasts.success("Download folder saved.");
    } catch (e) {
      toasts.error(String(e));
    }
  }

  function resultKey(result: DiscoverResult): string {
    return `${result.provider ?? "modrinth"}:${result.id}`;
  }

  function modpackPageUrl(result: DiscoverResult): string {
    const slugOrId = (result.slug || result.id || "").trim();
    if (!slugOrId) return "";
    if (result.provider === "curseforge") {
      if (/^\d+$/.test(slugOrId) && (!result.slug || result.slug === result.id)) {
        return `https://www.curseforge.com/projects/${slugOrId}`;
      }
      return `https://www.curseforge.com/minecraft/modpacks/${slugOrId}`;
    }
    return `https://modrinth.com/modpack/${slugOrId}`;
  }

  function openCatalogInApp(result: DiscoverResult) {
    catalogViewResult = result;
  }

  function closeCatalogInApp() {
    catalogViewResult = null;
  }

  async function openModpackExternal(result: DiscoverResult) {
    const url = modpackPageUrl(result);
    if (!url) {
      toasts.error("No catalog page for this modpack.");
      return;
    }
    try {
      await openExternal(url);
    } catch (e) {
      toasts.error(`Could not open link: ${e}`);
    }
  }

  function markIconBroken(key: string) {
    if (!brokenIcons.includes(key)) {
      brokenIcons = [...brokenIcons, key];
    }
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

  function gradientFrom(name: string) {
    const colors = ["var(--accent-primary)", "var(--accent-secondary)", "#3b82f6", "#f59e0b", "#ec4899", "#06b6d4", "#ef4444"];
    let hash = 0;
    for (let i = 0; i < name.length; i++) hash = name.charCodeAt(i) + ((hash << 5) - hash);
    return colors[Math.abs(hash) % colors.length];
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
    const hits = await invoke<
      Array<{
        id: number | string;
        slug: string;
        name: string;
        summary?: string | null;
        iconUrl?: string | null;
        authors?: string[] | null;
        downloadCount?: number | null;
        categories?: string[] | null;
      }>
    >("search_curseforge_modpacks", {
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

  async function search() {
    const requestId = ++searchRequestId;
    loadingDiscover = true;
    discoverError = "";
    try {
      let next: DiscoverResult[];
      if (discoverProvider === "modrinth") {
        next = await searchModrinth();
      } else if (discoverProvider === "curseforge") {
        next = await searchCurseForge();
      } else {
        const settled = await Promise.allSettled([searchModrinth(), searchCurseForge()]);
        if (requestId !== searchRequestId) return;
        const mr = settled[0].status === "fulfilled" ? settled[0].value : [];
        const cf = settled[1].status === "fulfilled" ? settled[1].value : [];
        const errors = settled
          .filter((s): s is PromiseRejectedResult => s.status === "rejected")
          .map((s) => String(s.reason));
        if (mr.length === 0 && cf.length === 0 && errors.length > 0) {
          throw new Error(errors.join("; "));
        }
        if (errors.length > 0) {
          const mrFailed = settled[0].status === "rejected";
          const cfFailed = settled[1].status === "rejected";
          if (mrFailed && !cfFailed) {
            discoverError = "Modrinth unavailable — showing CurseForge results.";
          } else if (cfFailed && !mrFailed) {
            discoverError = "CurseForge unavailable — showing Modrinth results.";
          } else {
            discoverError = errors.join("; ");
          }
        }
        next = interleaveResults(mr, cf);
      }
      if (requestId !== searchRequestId) return;
      results = next;
      brokenIcons = brokenIcons.filter((id) => next.some((r) => resultKey(r) === id));
    } catch (e) {
      if (requestId !== searchRequestId) return;
      discoverError = String(e);
      results = [];
    } finally {
      if (requestId === searchRequestId) {
        loadingDiscover = false;
      }
    }
  }

  function setDiscoverProvider(provider: DiscoverProvider) {
    if (discoverProvider === provider) return;
    discoverProvider = provider;
    catalogViewResult = null;
    search();
  }

  async function addModpack(result: DiscoverResult) {
    const key = resultKey(result);
    adding = new Set([...adding, key]);
    try {
      if (!downloadDir && !defaultDownloadDir) {
        await loadDownloadDir();
      }
      const parent = (downloadDir || defaultDownloadDir).replace(/[\\/]+$/, "");
      if (!parent) {
        throw new Error("Choose a download folder first (Download to).");
      }
      const targetDir = parent;
      let source: string;
      if (result.provider === "curseforge") {
        toasts.info(`Resolving CurseForge files for ${result.name}…`);
        const files = await invoke<Array<{ id: number; fileName?: string }>>(
          "get_curseforge_modpack_files",
          {
            modId: Number(result.id),
            gameVersion: null,
          },
        );
        const fileId = files?.[0]?.id;
        if (fileId == null) throw new Error("No CurseForge files available for this modpack.");
        source = `cf:${result.id}:${fileId}`;
        toasts.info(`Downloading ${files[0]?.fileName || result.name}…`);
      } else {
        source = await api.modpacks.getModpackUrl(result.id);
      }
      const res: any = await api.modpacks.install(source, targetDir, result.name);
      const info = (await invoke("validate_project", {
        path: res.path,
      })) as import("../lib/api").ProjectSummary;
      const manifestPath = info.manifestPath || res.path;
      recentProjects.add({ path: manifestPath, info: info as any });
      toasts.success(`Added "${result.name}" to ${targetDir}.`);
    } catch (e) {
      toasts.error(`Could not add ${result.name}: ${e}`);
    } finally {
      const next = new Set(adding);
      next.delete(key);
      adding = next;
    }
  }

  onMount(() => {
    void loadSwarm();
    void loadDownloadDir();
    if (tab === "discover") search();
    // `tuffbox://install?repo=…` links land here: valid repos go straight to
    // the existing confirm dialog, garbage becomes a toast.
    return onInstallLink((link) => {
      if (link.status === "valid") {
        void confirmGithubImport(link.repo);
      } else {
        toasts.error(`Install link rejected: "${link.raw}" is not a GitHub owner/repo.`);
      }
    });
  });

  $effect(() => {
    const req = $libraryTabRequest;
    if (!req) return;
    libraryTabRequest.set(null);
    switchTab(req);
  });

  function switchTab(t: Tab) {
    tab = t;
    if (t !== "discover") catalogViewResult = null;
    if (t === "discover") {
      void loadDownloadDir();
      if (results.length === 0) search();
    }
    if (t === "create") void loadSwarm();
  }

  function formatCount(n?: number | null): string {
    if (!n) return "0";
    if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
    if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
    return String(n);
  }

  const discoverPlaceholder = $derived(
    discoverProvider === "curseforge"
      ? "Search CurseForge modpacks…"
      : discoverProvider === "both"
        ? "Search modpacks…"
        : "Search Modrinth modpacks…",
  );
</script>

<div class="library fade-slide-in">
  {#snippet tabButtons()}
    <div class="tabs" role="tablist" aria-label="Library">
      <button type="button" class:active={tab === "yours"} onclick={() => switchTab("yours")}>
        <LayoutGrid size={15} /> Your packs
      </button>
      <button type="button" class:active={tab === "discover"} onclick={() => switchTab("discover")}>
        <Compass size={15} /> Discover
      </button>
      <button
        type="button"
        class:active={tab === "create"}
        onclick={() => switchTab("create")}
        title="Create a new instance"
      >
        <Plus size={15} /> Create
      </button>
    </div>
  {/snippet}

  {#if tab !== "yours"}
    <div class="library-subnav lib-header-enter">
      {@render tabButtons()}
      <div class="import-wrap">
        <button
          type="button"
          class="header-btn"
          class:busy={importing}
          disabled={importing}
          onclick={(e) => { e.stopPropagation(); (importMenuOpen = !importMenuOpen);  }}
          title="Import .mrpack, .zip, or Prism/MultiMC/CurseForge instance"
        >
          {#if importing}
            <span class="mini-spinner dark"></span> Importing…
          {:else}
            <Download size={15} /> Import
          {/if}
        </button>
        {#if importMenuOpen}
          <div class="import-menu" role="menu">
            <button type="button" role="menuitem" onclick={importPackFile}>
              File (.mrpack / .zip)
            </button>
            <button type="button" role="menuitem" onclick={importInstanceFolder}>
              Instance folder
            </button>
            <button type="button" role="menuitem" onclick={importGithubRepo}>
              GitHub repository
            </button>
          </div>
        {/if}
      </div>
    </div>
  {/if}

  {#if tab === "yours"}
    <div class="yours-wrap">
      <LibraryInstancesPane bind:currentView>
        {#snippet toolbarLeading()}{@render tabButtons()}{/snippet}
      </LibraryInstancesPane>
    </div>
  {:else if tab === "discover"}
  <div class="tab-scroll">
    {#if catalogViewResult}
      <CatalogProjectView
        result={catalogViewResult}
        installing={adding.has(resultKey(catalogViewResult))}
        onback={closeCatalogInApp}
        oninstall={() => {
          if (catalogViewResult) void addModpack(catalogViewResult);
        }}
        onopenexternal={() => {
          if (catalogViewResult) void openModpackExternal(catalogViewResult);
        }}
      />
    {:else}
    <div class="discover-bar">
      <div class="provider-toggle" role="group" aria-label="Catalog provider">
        <button
          type="button"
          class:active={discoverProvider === "modrinth"}
          onclick={() => setDiscoverProvider("modrinth")}
        >Modrinth</button>
        <button
          type="button"
          class:active={discoverProvider === "curseforge"}
          onclick={() => setDiscoverProvider("curseforge")}
        >CurseForge</button>
        <button
          type="button"
          class:active={discoverProvider === "both"}
          onclick={() => setDiscoverProvider("both")}
          title="Search both catalogs at once"
        >Both</button>
      </div>
      <div class="search">
        <Search size={16} />
        <input
          aria-label="Search modpacks"
          bind:value={query}
          placeholder={discoverPlaceholder}
          onkeydown={(e) => e.key === "Enter" && search()}
        />
      </div>
      <button class="search-btn" onclick={() => search()} disabled={loadingDiscover}>
        {loadingDiscover ? "Searching…" : "Search"}
      </button>
    </div>

    <div class="download-path">
      <label for="lib-download-dir">Download to</label>
      <div class="path-row">
        <input
          id="lib-download-dir"
          bind:value={downloadDir}
          placeholder={defaultDownloadDir || "Choose a folder for modpacks"}
        />
        <button type="button" class="path-btn" onclick={browseDownloadDir} title="Browse">
          <FolderOpen size={15} />
        </button>
        <button type="button" class="path-btn save" onclick={applyDownloadDir}>Save</button>
      </div>
    </div>

    {#if discoverError}
      <div class={results.length > 0 ? "catalog-warn" : "error"}>{discoverError}</div>
    {/if}

    {#if loadingDiscover && results.length === 0}
      <div class="loading-state">Loading modpacks…</div>
    {:else if results.length === 0}
      <div class="empty-state">
        <div class="empty-icon"><Compass size={40} /></div>
        <h3>No packs found</h3>
        <p>Try a different search.</p>
      </div>
    {:else}
      <div class="pack-grid tb-stagger">
        {#each results as result, i (resultKey(result))}
          {@const key = resultKey(result)}
          {@const showIcon = !!result.iconUrl && !brokenIcons.includes(key)}
          <div
            class="pack-card discover-card tb-card"
            style={`--i: ${Math.min(i, 8)}`}
            role="button"
            tabindex="0"
            onclick={() => openCatalogInApp(result)}
            onkeydown={(e) => {
              if (e.key === "Enter" || e.key === " ") {
                e.preventDefault();
                openCatalogInApp(result);
              }
            }}
          >
            <div
              class="pack-cover"
              style={`background: linear-gradient(135deg, ${gradientFrom(result.name)}, ${gradientFrom(result.slug || result.id)})`}
            >
              {#if showIcon}
                <img
                  class="pack-cover-img tb-cover-media"
                  src={result.iconUrl}
                  alt=""
                  loading="lazy"
                  decoding="async"
                  onerror={() => markIconBroken(key)}
                />
              {:else}
                <span class="pack-cover-letter tb-cover-media">{result.name[0]?.toUpperCase() ?? "?"}</span>
              {/if}
            </div>
            <div class="pack-body">
              <div class="pack-title-row">
                <button
                  type="button"
                  class="pack-name linkish"
                  title={result.name}
                  onclick={(e) => {
                    e.stopPropagation();
                    openCatalogInApp(result);
                  }}
                >{result.name}</button>
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
              <div class="pack-actions">
                <button
                  type="button"
                  class="pack-page"
                  title="Open catalog page in TuffBox"
                  onclick={(e) => {
                    e.stopPropagation();
                    openCatalogInApp(result);
                  }}
                >
                  <ExternalLink size={14} /> Page
                </button>
                <button
                  class="pack-add"
                  disabled={adding.has(key)}
                  onclick={(e) => {
                    e.stopPropagation();
                    void addModpack(result);
                  }}
                >
                  {#if adding.has(key)}
                    <span class="mini-spinner"></span> Adding…
                  {:else}
                    <Plus size={14} /> Add to TuffBox
                  {/if}
                </button>
              </div>
            </div>
          </div>
        {/each}
      </div>
    {/if}
    {/if}
  </div>
  {:else if tab === "create"}
  <div class="tab-scroll">
    <div class="create-pane">
      <header class="create-hero">
        <div class="create-hero-top">
          <div>
            <h2>Start a pack</h2>
            <p>Blank instance, import a pack file, or browse Modrinth / CurseForge in Discover.</p>
          </div>
          {#if swarmEnabled && (kudosLoading || kudosBalance)}
            <KudosBalanceStrip
              compact
              title="Kudos"
              total={Number(kudosBalance?.totalKudos ?? 0)}
              rac={Number(kudosBalance?.rac ?? 0)}
              loading={kudosLoading && !kudosBalance}
              onclick={focusCreationPeerGen}
            />
          {/if}
        </div>
      </header>
      <div class="create-actions">
        <button type="button" class="create-plus" onclick={openNewPack}>
          <span class="plus-ring"><Plus size={28} strokeWidth={2.25} /></span>
          <div class="create-copy">
            <strong>Create modpack</strong>
            <span>Blank · Fabric / Forge / NeoForge / Quilt</span>
          </div>
        </button>
        <button
          type="button"
          class="create-plus import"
          onclick={() => openAddInstance("import")}
          disabled={importing}
        >
          <span class="plus-ring"><Download size={26} strokeWidth={2.25} /></span>
          <div class="create-copy">
            <strong>{importing ? "Importing…" : "Import pack"}</strong>
            <span>.mrpack · zip · Prism · MultiMC · CurseForge</span>
          </div>
        </button>
        <button
          type="button"
          class="create-plus browse"
          onclick={() => switchTab("discover")}
        >
          <span class="plus-ring"><Compass size={26} strokeWidth={2.25} /></span>
          <div class="create-copy">
            <strong>Browse packs</strong>
            <span>Modrinth · CurseForge — Library Discover</span>
          </div>
        </button>
      </div>
      <div class="create-trends">
        <CreationTrends {swarmEnabled} {p2pEnabled} />
      </div>
    </div>
  </div>
  {/if}
</div>

{#if $newProjectOpen}
  <AddInstanceModal
    initialMode={$addInstanceMode}
    onclose={() => newProjectOpen.set(false)}
    oncreated={onPackCreated}
  />
{/if}

{#if githubImportOpen}
  <PromptDialog
    title="Import from GitHub"
    message="Public repo only. Paste owner/repo or a github.com URL. No login needed."
    mode="text"
    defaultValue=""
    confirmLabel="Preview"
    onconfirm={(v) => void confirmGithubImport(v)}
    oncancel={() => (githubImportOpen = false)}
  />
{/if}

{#if githubConfirmOpen}
  <ConfirmDialog
    title="Install GitHub pack"
    message={githubInspectSummary}
    confirmLabel="Install"
    onconfirm={() => void confirmGithubInstall()}
    oncancel={() => (githubConfirmOpen = false)}
  />
{/if}

<GithubPackInstallProgress active={githubInstallActive} onclose={() => (githubInstallActive = false)} />

<svelte:window onmousedown={onGlobalPointerDown} onkeydown={onGlobalKeydown} />

<style>
  .library {
    max-width: none;
    margin: 0;
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    height: 100%;
  }
  .library .yours-wrap {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  .library .tab-scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
    display: flex;
    flex-direction: column;
  }

  .lib-header-enter {
    animation: lib-page-header var(--motion-enter) var(--ease-spring) both;
  }

  .library-subnav {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: 4px;
    margin-bottom: 14px;
    gap: 16px;
    flex-wrap: wrap;
  }
  .import-wrap {
    position: relative;
  }
  .header-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 8px 14px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--accent-primary) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent-primary) 35%, transparent);
    color: var(--accent-primary);
    font-size: 13px;
    font-weight: 700;
    cursor: pointer;
  }
  .header-btn:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .import-menu {
    position: absolute;
    top: calc(100% + 6px);
    right: 0;
    z-index: 40;
    min-width: 220px;
    padding: 6px;
    border-radius: 10px;
    border: 1px solid var(--border-color);
    background: var(--bg-elevated, #1a1f28);
    box-shadow: 0 12px 28px rgba(0, 0, 0, 0.4);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .import-menu button {
    width: 100%;
    text-align: left;
    padding: 9px 10px;
    border: none;
    border-radius: var(--border-radius-sm);
    background: transparent;
    color: var(--text-primary);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }
  .import-menu button:hover {
    background: color-mix(in srgb, var(--accent-primary) 12%, transparent);
    color: var(--accent-primary);
  }

  .tabs {
    display: flex;
    gap: 6px;
  }
  .tabs button {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 14px;
    border-radius: var(--border-radius-md);
    background: #39393b;
    border: 1px solid #39393b;
    border-bottom-color: #232425;
    color: var(--text-secondary);
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    transition:
      background var(--motion-fast) var(--motion-ease),
      border-color var(--motion-fast) var(--motion-ease),
      color var(--motion-fast) var(--motion-ease);
  }
  .tabs button:hover {
    background: #47484a;
    color: var(--text-primary);
  }
  .tabs button:active:not(:disabled) {
    background: #2a2b2c;
    border-bottom-width: 1px;
    transform: translateY(1px);
  }
  .tabs button.active {
    background: #491ac0;
    border-color: #491ac0;
    border-bottom-color: #32127f;
    color: #ffffff;
  }
  .tabs button.active:hover {
    background: #5c2dd5;
    border-color: #5c2dd5;
    border-bottom-color: #3f1a96;
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
    overflow: visible;
    text-align: left;
    cursor: pointer;
    display: flex;
    flex-direction: column;
    position: relative;
    transition:
      border-color var(--motion-fast) var(--motion-ease),
      background var(--motion-fast) var(--motion-ease);
  }
  .pack-card:hover {
    background: var(--bg-tertiary);
    border-color: color-mix(in srgb, var(--accent-primary) 28%, transparent);
  }

  .pack-cover {
    position: relative;
    aspect-ratio: 1;
    width: 100%;
    height: auto;
    display: flex;
    align-items: center;
    justify-content: center;
    overflow: hidden;
    border-radius: var(--border-radius-lg) var(--border-radius-lg) 0 0;
  }
  .pack-cover-letter {
    font-size: 44px;
    font-weight: 900;
    color: #fff;
    text-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
  }
  .pack-cover-img {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  .pack-body {
    padding: 12px 14px 14px;
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex: 1;
  }
  .pack-name {
    font-weight: 700;
    font-size: 14px;
    color: var(--text-primary);
    white-space: normal;
    word-break: break-word;
    line-height: 1.3;
  }
  button.pack-name.linkish {
    background: none;
    border: none;
    padding: 0;
    margin: 0;
    cursor: pointer;
    text-align: left;
    font: inherit;
    font-weight: 700;
    font-size: 14px;
    color: var(--text-primary);
    max-width: 100%;
  }
  button.pack-name.linkish:hover {
    color: var(--accent-primary);
    text-decoration: underline;
  }
  .pack-meta {
    font-size: 12px;
    color: var(--text-muted);
    text-transform: capitalize;
  }
  .pack-desc {
    margin: 4px 0 0;
    font-size: 12px;
    color: var(--text-muted);
    line-height: 1.4;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    min-height: 34px;
  }

  .create-pane {
    display: flex;
    flex-direction: column;
    gap: 20px;
  }
  .create-hero-top {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }
  .create-hero h2 {
    margin: 0 0 4px;
    font-size: 20px;
    color: var(--text-primary);
  }
  .create-hero p {
    margin: 0;
    font-size: 13px;
    color: var(--text-muted);
    max-width: 52ch;
  }
  .create-actions {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 12px;
  }
  .create-plus {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 14px;
    min-height: 0;
    padding: 18px 16px;
    border-radius: var(--border-radius-xl);
    border: 1px solid color-mix(in srgb, var(--accent-primary) 28%, transparent);
    background:
      linear-gradient(135deg, color-mix(in srgb, var(--accent-primary) 10%, transparent), transparent 55%),
      var(--bg-secondary);
    color: var(--text-secondary);
    cursor: pointer;
    text-align: left;
    transition:
      border-color var(--motion-fast) var(--motion-ease),
      background var(--motion-fast) var(--motion-ease);
  }
  .create-plus.import {
    border-color: rgba(59, 130, 246, 0.28);
    background:
      linear-gradient(135deg, rgba(59, 130, 246, 0.1), transparent 55%),
      var(--bg-secondary);
  }
  .create-plus.import .plus-ring {
    background: rgba(59, 130, 246, 0.14);
    color: #60a5fa;
    border-color: rgba(59, 130, 246, 0.35);
  }
  .create-plus.browse {
    border-color: rgba(245, 158, 11, 0.28);
    background:
      linear-gradient(135deg, rgba(245, 158, 11, 0.1), transparent 55%),
      var(--bg-secondary);
  }
  .create-plus.browse .plus-ring {
    background: rgba(245, 158, 11, 0.14);
    color: #fbbf24;
    border-color: rgba(245, 158, 11, 0.35);
  }
  .create-plus:hover {
    border-color: color-mix(in srgb, var(--accent-primary) 55%, transparent);
    color: var(--text-primary);
  }
  .create-plus.import:hover {
    border-color: rgba(59, 130, 246, 0.55);
  }
  .create-plus.browse:hover {
    border-color: rgba(245, 158, 11, 0.55);
  }
  .create-plus:disabled {
    opacity: 0.6;
    cursor: default;
    transform: none;
  }
  .create-copy {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .create-plus strong {
    font-size: 15px;
    color: var(--text-primary);
  }
  .create-plus .create-copy > span {
    font-size: 12px;
    color: var(--text-muted);
  }
  .plus-ring {
    width: 48px;
    height: 48px;
    border-radius: 14px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: color-mix(in srgb, var(--accent-primary) 14%, transparent);
    color: var(--accent-primary);
    border: 1px solid color-mix(in srgb, var(--accent-primary) 35%, transparent);
    flex-shrink: 0;
  }
  .create-trends {
    min-width: 0;
  }
  @media (max-width: 720px) {
    .create-actions {
      grid-template-columns: 1fr;
    }
  }
  .mini-spinner.dark {
    border-color: color-mix(in srgb, var(--accent-primary) 25%, transparent);
    border-top-color: var(--accent-primary);
  }

  .pack-stats {
    display: flex;
    gap: 12px;
    font-size: 12px;
    color: var(--text-muted);
    margin-top: 6px;
  }
  .pack-stats span {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  .pack-actions {
    margin-top: 10px;
    display: flex;
    gap: 8px;
    align-items: stretch;
  }
  .pack-page {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    padding: 8px 10px;
    border-radius: var(--border-radius-sm);
    font-size: 12px;
    font-weight: 600;
    background: var(--bg-tertiary);
    border: 1px solid var(--border-color);
    color: var(--text-secondary);
    cursor: pointer;
    flex-shrink: 0;
  }
  .pack-page:hover {
    border-color: var(--accent-primary);
    color: var(--accent-primary);
  }
  .pack-add {
    flex: 1;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 8px 12px;
    border-radius: var(--border-radius-sm);
    font-size: 12px;
    font-weight: 700;
    background: var(--accent-primary);
    color: var(--on-accent);
    border: none;
    cursor: pointer;
    transition: background 0.15s ease;
  }
  .pack-add:hover {
    background: var(--accent-hover);
  }
  .pack-add:disabled {
    opacity: 0.7;
    cursor: default;
  }

  .discover-bar {
    display: flex;
    gap: 10px;
    margin-bottom: 20px;
    align-items: center;
    flex-wrap: wrap;
    /* Task #61: the bar floated on the page bg with no surface — give it a
       proper panel like other toolbars. */
    padding: 10px 12px;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-md);
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
    border-radius: var(--border-radius-sm);
    border: 1px solid transparent;
    background: #39393b;
    border-bottom-color: #232425;
    color: var(--text-secondary);
    font-size: 12px;
    font-weight: 700;
    cursor: pointer;
    transition: background var(--motion-fast) var(--motion-ease),
      border-color var(--motion-fast) var(--motion-ease), color var(--motion-fast) var(--motion-ease);
  }
  .provider-toggle button:hover {
    background: #47484a;
    color: var(--text-primary);
  }
  .provider-toggle button.active {
    background: #491ac0;
    border-color: #491ac0;
    border-bottom-color: #32127f;
    color: #ffffff;
  }
  .search {
    flex: 1;
    min-width: 180px;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 14px;
    border-radius: 10px;
    border: 1px solid var(--border-color);
    background: var(--bg-tertiary);
  }
  .search input {
    border: 0;
    background: transparent;
    color: var(--text-primary);
    width: 100%;
    padding: 12px 0;
    font-size: 14px;
  }
  .search-btn {
    padding: 0 18px;
    height: 44px;
    border-radius: 10px;
    font-weight: 700;
    font-size: 13px;
    background: var(--accent-primary);
    color: var(--on-accent);
    border: none;
    cursor: pointer;
  }
  .search-btn:disabled {
    opacity: 0.6;
  }

  .download-path {
    display: grid;
    gap: 6px;
    margin: -8px 0 18px;
  }
  .download-path label {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-muted);
  }
  .path-row {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .path-row input {
    flex: 1;
    min-width: 0;
    height: 40px;
    padding: 0 12px;
    border-radius: 10px;
    border: 1px solid var(--border-color);
    background: var(--bg-tertiary);
    color: var(--text-primary);
    font-size: 13px;
  }
  .path-btn {
    height: 40px;
    padding: 0 12px;
    border-radius: 10px;
    border: 1px solid var(--border-color);
    background: var(--bg-secondary);
    color: var(--text-secondary);
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    font-weight: 600;
  }
  .path-btn.save {
    background: color-mix(in srgb, var(--accent-primary) 12%, transparent);
    border-color: color-mix(in srgb, var(--accent-primary) 35%, transparent);
    color: var(--accent-primary);
  }

  .discover-card {
    cursor: default;
  }
  .pack-title-row {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .pack-title-row .pack-name {
    flex: 1;
    min-width: 0;
  }
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
    background: color-mix(in srgb, var(--accent-primary) 18%, transparent);
    color: var(--accent-primary);
  }
  .provider-badge.curseforge {
    background: rgba(241, 100, 54, 0.18);
    color: #f16436;
  }

  .mini-spinner {
    width: 14px;
    height: 14px;
    border: 2px solid rgba(0, 0, 0, 0.25);
    border-top-color: #000;
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
    display: inline-block;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .empty-state,
  .loading-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 64px 32px;
    text-align: center;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-xl);
    color: var(--text-muted);
  }
  .empty-icon {
    width: 72px;
    height: 72px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: var(--bg-elevated);
    color: var(--text-muted);
  }
  .empty-state h3 {
    margin: 0;
    font-size: 16px;
    color: var(--text-primary);
  }
  .empty-state p {
    margin: 0;
    font-size: 13px;
    max-width: 320px;
  }

  .error,
  .catalog-warn {
    padding: 10px 12px;
    border-radius: 10px;
    margin-bottom: 16px;
  }
  .error {
    background: rgba(239, 68, 68, 0.12);
    border: 1px solid rgba(239, 68, 68, 0.35);
    color: #fca5a5;
  }
  .catalog-warn {
    background: color-mix(in srgb, var(--accent-primary) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent-primary) 28%, transparent);
    color: var(--text-secondary);
  }

  @keyframes lib-page-header {
    from { opacity: 0; transform: translateY(-8px); }
    to { opacity: 1; transform: none; }
  }

  :global(.potato-pc) .lib-header-enter {
    animation: none !important;
  }
  @media (prefers-reduced-motion: reduce) {
    .lib-header-enter {
      animation: none !important;
    }
  }
</style>
