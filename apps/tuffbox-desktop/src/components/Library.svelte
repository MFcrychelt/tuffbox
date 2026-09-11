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
  import { Grid, Stack } from "@tuffbox/layout-lib";

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
  // Import menu a11y: trigger element ref — Escape-close must return focus.
  let importBtnEl: HTMLButtonElement | null = $state(null);
  // Download-dir dirty tracking: testers must SEE that a typed path is not
  // saved yet (Enter / Save apply it; Browse saves immediately).
  // (`downloadDirDirty` is derived next to `downloadDir`'s declaration below.)
  let lastSavedDir = $state("");
  // Tab keyboard navigation (Left/Right/Home/End) needs element refs.
  let tabYoursEl: HTMLButtonElement | null = null;
  let tabDiscoverEl: HTMLButtonElement | null = null;
  let tabCreateEl: HTMLButtonElement | null = null;

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
      if (importMenuOpen) {
        importMenuOpen = false;
        importBtnEl?.focus();
      }
      githubImportOpen = false;
    }
  }

  function toggleImportMenu() {
    importMenuOpen = !importMenuOpen;
  }

  /** Roving focus over the section tabs: Left/Right/Home/End, per WAI-ARIA.
   *  Automation and screen readers get a real tablist, not three buttons. */
  function onTablistKeydown(e: KeyboardEvent) {
    const order: Tab[] = ["yours", "discover", "create"];
    const idx = order.indexOf(tab);
    let next: Tab | null = null;
    if (e.key === "ArrowRight") next = order[(idx + 1) % order.length];
    else if (e.key === "ArrowLeft") next = order[(idx - 1 + order.length) % order.length];
    else if (e.key === "Home") next = order[0];
    else if (e.key === "End") next = order[order.length - 1];
    if (!next) return;
    e.preventDefault();
    switchTab(next);
    const el = next === "yours" ? tabYoursEl : next === "discover" ? tabDiscoverEl : tabCreateEl;
    el?.focus();
  }

  /** Cover gradient; letter-fallback mode adds a dark scrim so the white
   *  initial stays readable over ANY theme accent (Solar amber, Frost cyan,
   *  light themes — the old bare gradient failed contrast on ~half of them). */
  function coverBackground(name: string, slugOrId: string, withScrim: boolean): string {
    const grad = `linear-gradient(135deg, ${gradientFrom(name)}, ${gradientFrom(slugOrId || name)})`;
    return withScrim
      ? `linear-gradient(rgba(0, 0, 0, 0.32), rgba(0, 0, 0, 0.32)), ${grad}`
      : grad;
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
  // Dirty indicator for the "Download to" row: a typed-but-unsaved path must
  // be visible to the user (and to QA) instead of silently lost on Enter.
  const downloadDirDirty = $derived(
    downloadDir.trim() !== "" && downloadDir.trim() !== lastSavedDir,
  );
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
      lastSavedDir = downloadDir;
    } catch {
      downloadDir = "";
      lastSavedDir = "";
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
      lastSavedDir = selected;
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
      lastSavedDir = downloadDir.trim();
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

  /** Observable search outcome: total + per-provider split + echoed query.
   *  QA asserted "did the search actually run" before by counting cards —
   *  now the state is announced in one place (and to screen readers). */
  const discoverStatus = $derived.by(() => {
    if (loadingDiscover && results.length === 0) return "Searching catalogs…";
    if (results.length === 0) return "";
    const mr = results.filter((r) => (r.provider ?? "modrinth") !== "curseforge").length;
    const cf = results.length - mr;
    const parts: string[] = [`${results.length} packs`];
    if (discoverProvider === "both" && mr > 0 && cf > 0) {
      parts.push(`Modrinth ${mr} · CurseForge ${cf}`);
    }
    const q = query.trim();
    if (q) parts.push(`for “${q}”`);
    return parts.join(" · ");
  });
</script>

<div class="library fade-slide-in">
  {#snippet tabButtons()}
    <!-- svelte-ignore a11y_interactive_supports_focus -->
    <!-- Keydown lands here by bubbling from the focused tab button; the
         container itself is intentionally not tabbable. -->
    <div
      class="tabs"
      role="tablist"
      aria-label="Library sections"
      data-testid="library-tabs"
      onkeydown={onTablistKeydown}
    >
      <button
        bind:this={tabYoursEl}
        type="button"
        role="tab"
        id="library-tab-yours"
        aria-selected={tab === "yours"}
        aria-controls="library-panel-yours"
        class:active={tab === "yours"}
        onclick={() => switchTab("yours")}
        data-testid="library-tab-yours"
      >
        <LayoutGrid size={15} /> Your packs
      </button>
      <button
        bind:this={tabDiscoverEl}
        type="button"
        role="tab"
        id="library-tab-discover"
        aria-selected={tab === "discover"}
        aria-controls="library-panel-discover"
        class:active={tab === "discover"}
        onclick={() => switchTab("discover")}
        data-testid="library-tab-discover"
      >
        <Compass size={15} /> Discover
      </button>
      <button
        bind:this={tabCreateEl}
        type="button"
        role="tab"
        id="library-tab-create"
        aria-selected={tab === "create"}
        aria-controls="library-panel-create"
        class:active={tab === "create"}
        onclick={() => switchTab("create")}
        title="Create a new instance"
        data-testid="library-tab-create"
      >
        <Plus size={15} /> Create
      </button>
    </div>
  {/snippet}

  <!-- Stable section rail: identical position on every tab. Previously the
       tab strip jumped (it lived inside the instances toolbar on "yours"
       and in this row elsewhere) — every tab switch visibly relocated the
       control, and Import was unreachable from the packs list. -->
  <div class="library-subnav lib-header-enter" data-testid="library-subnav">
    {@render tabButtons()}
    <div class="import-wrap" data-testid="library-import-wrap">
      <button
        bind:this={importBtnEl}
        type="button"
        class="header-btn"
        class:busy={importing}
        disabled={importing}
        aria-haspopup="menu"
        aria-expanded={importMenuOpen}
        onclick={(e) => { e.stopPropagation(); toggleImportMenu(); }}
        title="Import .mrpack, .zip, or Prism/MultiMC/CurseForge instance"
        data-testid="library-import-btn"
      >
        {#if importing}
          <span class="mini-spinner" aria-hidden="true"></span> Importing…
        {:else}
          <Download size={15} /> Import
        {/if}
      </button>
      {#if importMenuOpen}
        <div class="import-menu" role="menu" aria-label="Import sources" data-testid="library-import-menu">
          <button type="button" role="menuitem" onclick={importPackFile} data-testid="library-import-file">
            File (.mrpack / .zip)
          </button>
          <button type="button" role="menuitem" onclick={importInstanceFolder} data-testid="library-import-folder">
            Instance folder
          </button>
          <button type="button" role="menuitem" onclick={importGithubRepo} data-testid="library-import-github">
            GitHub repository
          </button>
        </div>
      {/if}
    </div>
  </div>

  {#if tab === "yours"}
    <div
      class="yours-wrap"
      id="library-panel-yours"
      role="tabpanel"
      aria-labelledby="library-tab-yours"
      data-testid="library-panel-yours"
    >
      <LibraryInstancesPane bind:currentView />
    </div>
  {:else if tab === "discover"}
  <div
    class="tab-scroll"
    id="library-panel-discover"
    role="tabpanel"
    aria-labelledby="library-tab-discover"
    data-testid="library-panel-discover"
  >
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
    <Stack direction="row" gap="3" wrap class="discover-bar">
      <div class="provider-toggle" role="group" aria-label="Catalog provider" data-testid="library-provider-toggle">
        <button
          type="button"
          class:active={discoverProvider === "modrinth"}
          onclick={() => setDiscoverProvider("modrinth")}
          data-testid="library-provider-modrinth"
        >Modrinth</button>
        <button
          type="button"
          class:active={discoverProvider === "curseforge"}
          onclick={() => setDiscoverProvider("curseforge")}
          data-testid="library-provider-curseforge"
        >CurseForge</button>
        <button
          type="button"
          class:active={discoverProvider === "both"}
          onclick={() => setDiscoverProvider("both")}
          title="Search both catalogs at once"
          data-testid="library-provider-both"
        >Both</button>
      </div>
      <div class="search">
        <Search size={16} />
        <input
          aria-label="Search modpacks"
          bind:value={query}
          placeholder={discoverPlaceholder}
          onkeydown={(e) => e.key === "Enter" && search()}
          data-testid="library-search-input"
        />
      </div>
      <button
        class="search-btn"
        onclick={() => search()}
        disabled={loadingDiscover}
        data-testid="library-search-btn"
      >
        {#if loadingDiscover}<span class="mini-spinner" aria-hidden="true"></span>{/if}
        Search
      </button>
    </Stack>

    {#if discoverStatus}
      <p class="discover-status" role="status" aria-live="polite" data-testid="library-results-status">
        {discoverStatus}
      </p>
    {/if}

    <form
      class="download-path"
      onsubmit={(e) => { e.preventDefault(); void applyDownloadDir(); }}
      data-testid="library-download-form"
    >
      <label for="lib-download-dir">
        Download to {#if downloadDirDirty}<span class="unsaved">· unsaved</span>{/if}
      </label>
      <div class="path-row">
        <input
          id="lib-download-dir"
          bind:value={downloadDir}
          placeholder={defaultDownloadDir || "Choose a folder for modpacks"}
          data-testid="library-download-dir"
        />
        <button type="button" class="path-btn" onclick={browseDownloadDir} title="Browse" data-testid="library-download-browse">
          <FolderOpen size={15} />
        </button>
        <button type="submit" class="path-btn save" disabled={!downloadDirDirty} data-testid="library-download-save">Save</button>
      </div>
    </form>

    {#if discoverError}
      <div class={results.length > 0 ? "catalog-warn" : "error"}>{discoverError}</div>
    {/if}

    {#if loadingDiscover && results.length === 0}
      <div class="loading-state" data-testid="library-loading" role="status">
        <span class="mini-spinner big" aria-hidden="true"></span>
        Loading modpacks…
      </div>
    {:else if results.length === 0}
      <div class="empty-state" data-testid="library-empty">
        <div class="empty-icon"><Compass size={40} /></div>
        <h3>No packs found</h3>
        <p>
          {#if query.trim()}Nothing matches “{query.trim()}”{:else}Try a different search{/if}{#if discoverProvider !== "both"} in {discoverProvider === "curseforge" ? "CurseForge" : "Modrinth"}{/if}.
        </p>
      </div>
    {:else}
      <Grid autoMin={220} gap="4" class="tb-stagger">
        {#each results as result, i (resultKey(result))}
          {@const key = resultKey(result)}
          {@const showIcon = !!result.iconUrl && !brokenIcons.includes(key)}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
          <!-- Whole-card click is mouse convenience only; the keyboard path
               is the real buttons inside (open / page / add). -->
          <article
            class="pack-card discover-card tb-card"
            style={`--i: ${Math.min(i, 8)}`}
            onclick={() => openCatalogInApp(result)}
            data-testid="library-result-card"
            data-provider={result.provider ?? "modrinth"}
          >
            <div
              class="pack-cover"
              style={`background: ${coverBackground(result.name, result.slug || result.id, !showIcon)}`}
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
                  data-testid="library-result-open"
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
                  data-testid="library-result-page"
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
                  data-testid="library-result-add"
                >
                  {#if adding.has(key)}
                    <span class="mini-spinner"></span> Adding…
                  {:else}
                    <Plus size={14} /> Add to TuffBox
                  {/if}
                </button>
              </div>
            </div>
          </article>
        {/each}
      </Grid>
    {/if}
    {/if}
  </div>
  {:else if tab === "create"}
  <div
    class="tab-scroll"
    id="library-panel-create"
    role="tabpanel"
    aria-labelledby="library-tab-create"
    data-testid="library-panel-create"
  >
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
    /* Responsive: center + cap on 1440p, full width on laptops. */
    max-width: min(1680px, 100%);
    margin: 0 auto;
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    height: 100%;
    /* Pill look for rounded themes; sharp themes (win95/pixelato define 0px
       radius tokens) flatten it — hardcoded 999px used to stay round there
       and looked foreign next to the square toolbar. */
    --lib-pill-radius: 999px;
  }
  :global([data-theme="win95"]) .library,
  :global([data-theme="pixelato"]) .library {
    --lib-pill-radius: 0px;
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
    margin-bottom: 10px;
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
    border-radius: var(--lib-pill-radius);
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
    border-radius: var(--border-radius-md);
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
    border-radius: var(--lib-pill-radius);
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
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
    background: var(--bg-tertiary);
    border-color: color-mix(in srgb, var(--accent-primary) 35%, var(--border-color));
    color: var(--text-primary);
  }
  .tabs button:active:not(:disabled) {
    background: var(--bg-active);
  }
  .tabs button.active {
    background: var(--accent-primary);
    border-color: var(--accent-primary);
    color: var(--on-accent);
  }
  .tabs button.active:hover {
    background: var(--accent-hover);
    border-color: var(--accent-hover);
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
    border-color: color-mix(in srgb, var(--accent-secondary) 28%, transparent);
    background:
      linear-gradient(135deg, color-mix(in srgb, var(--accent-secondary) 10%, transparent), transparent 55%),
      var(--bg-secondary);
  }
  .create-plus.import .plus-ring {
    background: color-mix(in srgb, var(--accent-secondary) 14%, transparent);
    color: var(--accent-secondary);
    border-color: color-mix(in srgb, var(--accent-secondary) 35%, transparent);
  }
  .create-plus.browse {
    border-color: color-mix(in srgb, var(--accent-warning) 28%, transparent);
    background:
      linear-gradient(135deg, color-mix(in srgb, var(--accent-warning) 10%, transparent), transparent 55%),
      var(--bg-secondary);
  }
  .create-plus.browse .plus-ring {
    background: color-mix(in srgb, var(--accent-warning) 14%, transparent);
    color: var(--accent-warning);
    border-color: color-mix(in srgb, var(--accent-warning) 35%, transparent);
  }
  .create-plus:hover {
    border-color: color-mix(in srgb, var(--accent-primary) 55%, transparent);
    color: var(--text-primary);
  }
  .create-plus.import:hover {
    border-color: color-mix(in srgb, var(--accent-secondary) 55%, transparent);
  }
  .create-plus.browse:hover {
    border-color: color-mix(in srgb, var(--accent-warning) 55%, transparent);
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
    border-radius: var(--border-radius-md);
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
  .mini-spinner.big {
    width: 22px;
    height: 22px;
    border-width: 3px;
    color: var(--accent-primary);
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

  /* Panel surface for the discover toolbar; flex layout comes from <Stack>
     in markup — scoped styles on component roots don't apply, so this lives
     on the class passed through to the Stack's div. */
  :global(.discover-bar) {
    margin-bottom: 12px;
    padding: 10px 12px;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-md);
  }
  .discover-status {
    margin: 0 0 12px;
    font-size: 12px;
    color: var(--text-muted);
  }
  .download-path .unsaved {
    color: var(--accent-warning);
    font-weight: 700;
  }
  .provider-toggle {
    display: inline-flex;
    gap: 4px;
    padding: 3px;
    border-radius: var(--border-radius-md);
    border: 1px solid var(--border-color);
    background: var(--bg-tertiary);
    flex-shrink: 0;
  }
  .provider-toggle button {
    padding: 6px 12px;
    border-radius: var(--border-radius-sm);
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-muted);
    font-size: 12px;
    font-weight: 700;
    cursor: pointer;
    transition: background var(--motion-fast) var(--motion-ease),
      border-color var(--motion-fast) var(--motion-ease), color var(--motion-fast) var(--motion-ease);
  }
  .provider-toggle button:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
  .provider-toggle button.active {
    background: var(--accent-primary);
    border-color: var(--accent-primary);
    color: var(--on-accent);
  }
  .search {
    flex: 1;
    min-width: 180px;
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 0 14px;
    border-radius: var(--border-radius-md);
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
    border-radius: var(--border-radius-md);
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
    margin: 0 0 18px;
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
    border-radius: var(--border-radius-md);
    border: 1px solid var(--border-color);
    background: var(--bg-tertiary);
    color: var(--text-primary);
    font-size: 13px;
  }
  .path-btn {
    height: 40px;
    padding: 0 12px;
    border-radius: var(--border-radius-md);
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
    cursor: pointer;
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

  /* Inherits currentColor: on accent buttons it is --on-accent, in the
     header button --accent-primary — visible on every theme (the old
     hardcoded #000 vanished on dark surfaces, #fff-style variants were a
     per-callsite guessing game). */
  .mini-spinner {
    width: 14px;
    height: 14px;
    border: 2px solid color-mix(in srgb, currentColor 30%, transparent);
    border-top-color: currentColor;
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
    border-radius: var(--border-radius-md);
    margin-bottom: 16px;
  }
  .error {
    background: color-mix(in srgb, var(--accent-danger) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent-danger) 35%, transparent);
    color: var(--accent-danger);
  }
  .catalog-warn {
    background: color-mix(in srgb, var(--accent-primary) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent-primary) 28%, transparent);
    color: var(--text-secondary);
  }

  /* ── Glass transparency toggle support ─────────────────────────────
     themes.css glassifies .tb-card only; without this the Discover pane
     answered the transparency toggle half-way (translucent cards over an
     opaque toolbar/menu). Mirror the same recipe for the page chrome.
     Sharp themes keep solid fills; potato-pc keeps translucency, no blur. */
  :global(html[data-glass="on"] .discover-bar),
  :global(html[data-glass="on"] .empty-state),
  :global(html[data-glass="on"] .loading-state),
  :global(html[data-glass="on"] .import-menu) {
    background: color-mix(in srgb, var(--bg-secondary) 55%, transparent);
    -webkit-backdrop-filter: blur(16px) saturate(140%);
    backdrop-filter: blur(16px) saturate(140%);
  }
  :global(html[data-glass="on"] .import-menu) {
    background: color-mix(in srgb, var(--bg-elevated) 72%, transparent);
    -webkit-backdrop-filter: blur(22px) saturate(140%);
    backdrop-filter: blur(22px) saturate(140%);
  }
  :global(html[data-glass="on"]:is([data-theme="win95"], [data-theme="pixelato"]) .discover-bar),
  :global(html[data-glass="on"]:is([data-theme="win95"], [data-theme="pixelato"]) .empty-state),
  :global(html[data-glass="on"]:is([data-theme="win95"], [data-theme="pixelato"]) .loading-state),
  :global(html[data-glass="on"]:is([data-theme="win95"], [data-theme="pixelato"]) .import-menu) {
    background: var(--bg-secondary);
    -webkit-backdrop-filter: none;
    backdrop-filter: none;
  }
  :global(html[data-glass="on"].potato-pc .discover-bar),
  :global(html[data-glass="on"].potato-pc .empty-state),
  :global(html[data-glass="on"].potato-pc .loading-state),
  :global(html[data-glass="on"].potato-pc .import-menu) {
    -webkit-backdrop-filter: none;
    backdrop-filter: none;
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
