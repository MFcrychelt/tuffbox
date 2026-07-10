<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { api } from "../lib/api";
  import {
    Search,
    Plus,
    Trash2,
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
    AlertTriangle,
    ChevronDown,
    ChevronRight,
    Heart,
    Bookmark,
    LayoutGrid,
    List,
    ArrowRight,
    ArrowDown,
    Scroll,
    Hammer,
    Anvil,
    Tag,
    Clock,
    Link,
    Check,
    Package,
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
    contentType?: "mod" | "resourcepack" | "datapack" | "shader" | string;
    updateAvailable?: boolean;
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
    author?: string | null;
    downloads?: number | null;
    follows?: number | null;
    dateModified?: string | null;
    categories?: string[];
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

  type DownloadItem = {
    id: string;
    name: string;
    downloaded: number;
    total: number;
    percent: number;
    status: "queued" | "downloading" | "done" | "failed" | "skipped" | string;
  };

  let mods: ModRow[] = [];
  let loading = false;
  let mutating = false;
  let filter = "";
  let sideFilter = "all";
  let contentFilter = "mod"; // mod, resourcepack, datapack, shader, favorites, list:<name>
  let error: string | null = null;
  let lastLoadedPath: string | null = null;
  let brokenIcons = new Set<string>();
  let savedMods: SearchResult[] = [];
  let savedModsLoading = false;

  // Download progress overlay
  let downloadOpen = false;
  let downloadTitle = "Downloading content";
  let downloadItems: DownloadItem[] = [];
  let downloadDone = false;
  let unlistenProgress: UnlistenFn | null = null;
  let unlistenBatch: UnlistenFn | null = null;

  function formatBytes(n: number): string {
    if (!n || n <= 0) return "0 B";
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    return `${(n / (1024 * 1024)).toFixed(1)} MB`;
  }

  $: downloadActiveCount = downloadItems.filter((i) => i.status === "downloading" || i.status === "queued").length;
  $: downloadDoneCount = downloadItems.filter((i) => i.status === "done" || i.status === "skipped").length;
  $: downloadFailedCount = downloadItems.filter((i) => i.status === "failed").length;
  $: downloadOverallPercent = downloadItems.length === 0
    ? 0
    : Math.round(downloadItems.reduce((sum, i) => sum + (i.percent || 0), 0) / downloadItems.length);

  function upsertDownloadItem(payload: Partial<DownloadItem> & { id: string }) {
    const idx = downloadItems.findIndex((i) => i.id === payload.id);
    if (idx >= 0) {
      downloadItems[idx] = { ...downloadItems[idx], ...payload };
      downloadItems = downloadItems;
    } else {
      downloadItems = [
        ...downloadItems,
        {
          id: payload.id,
          name: payload.name ?? payload.id,
          downloaded: payload.downloaded ?? 0,
          total: payload.total ?? 0,
          percent: payload.percent ?? 0,
          status: payload.status ?? "queued",
        },
      ];
    }
  }

  function openDownloadOverlay(title: string) {
    downloadTitle = title;
    downloadItems = [];
    downloadDone = false;
    downloadOpen = true;
  }

  function closeDownloadOverlay() {
    if (!downloadDone && downloadActiveCount > 0) return;
    downloadOpen = false;
  }

  onMount(async () => {
    unlistenBatch = await listen<{ phase: string; items?: DownloadItem[]; failed?: { modId: string; error: string }[] }>(
      "mod-download-batch",
      (event) => {
        const payload = event.payload;
        if (payload.phase === "start") {
          downloadOpen = true;
          downloadDone = false;
          downloadItems = (payload.items ?? []).map((item) => ({
            id: item.id,
            name: item.name,
            downloaded: 0,
            total: 0,
            percent: 0,
            status: "queued",
          }));
        } else if (payload.phase === "done") {
          downloadDone = true;
          // Auto-close shortly after success if nothing failed
          const failed = payload.failed?.length ?? downloadFailedCount;
          if (failed === 0) {
            setTimeout(() => {
              if (downloadDone) downloadOpen = false;
            }, 900);
          }
        }
      },
    );

    unlistenProgress = await listen<DownloadItem>("mod-download-progress", (event) => {
      upsertDownloadItem(event.payload);
      if (!downloadOpen) {
        downloadOpen = true;
        downloadDone = false;
      }
    });

    try {
      const versions: { id: string; popular: boolean }[] = await invoke("get_minecraft_versions");
      const popular = versions.filter((v) => v.popular).map((v) => v.id);
      const latest = versions.filter((v) => !v.popular).slice(0, 8).map((v) => v.id);
      gameVersions = [...new Set([...latest, ...popular])];
    } catch {
      // Network unavailable at startup — filter list stays empty.
    }
  });

  onDestroy(() => {
    unlistenProgress?.();
    unlistenBatch?.();
  });

  let addOpen = false;
  let searchQuery = "";
  let searchResults: SearchResult[] = [];
  let searchLoading = false;
  let selectedSide = "auto";
  let pendingInstallOptional = true;
  let filterGameVersion = "";
  let filterLoader = "fabric";
  let filterCategory = "";
  let filterEnvironment = "";
  let filterLicense = "";
  let sortBy = "relevance";
  let previewLoadingId = "";

  // --- Add-mods browser chrome ---
  let versionSearch = "";
  let loaderExpanded = false;
  let viewMode: "grid" | "list" = "grid";
  let page = 1;
  let pageSize = 20;
  let accordionOpen: Record<string, boolean> = {
    gameVersion: true,
    loader: true,
    category: true,
  };

  function toggleAccordion(key: string) {
    accordionOpen[key] = !accordionOpen[key];
  }

  function formatCount(n: number | null | undefined): string {
    if (n == null) return "0";
    if (n >= 1_000_000_000) return (n / 1_000_000_000).toFixed(2) + "B";
    if (n >= 1_000_000) return (n / 1_000_000).toFixed(2) + "M";
    if (n >= 1_000) return (n / 1_000).toFixed(1) + "K";
    return String(n);
  }

  function formatRelative(iso: string | null | undefined): string {
    if (!iso) return "unknown";
    const then = new Date(iso).getTime();
    if (Number.isNaN(then)) return iso;
    const diffMs = Date.now() - then;
    const minutes = Math.floor(diffMs / 60_000);
    if (minutes < 1) return "just now";
    if (minutes < 60) return `${minutes} minute${minutes === 1 ? "" : "s"} ago`;
    const hours = Math.floor(minutes / 60);
    if (hours < 24) return `${hours} hour${hours === 1 ? "" : "s"} ago`;
    const days = Math.floor(hours / 24);
    if (days === 1) return "1 day ago";
    if (days < 30) return `${days} days ago`;
    const months = Math.floor(days / 30);
    if (months < 12) return `${months} month${months > 1 ? "s" : ""} ago`;
    const years = Math.floor(months / 12);
    return `${years} year${years === 1 ? "" : "s"} ago`;
  }

  function projectToSearchResult(p: Record<string, unknown>): SearchResult {
    return {
      id: String(p.id ?? ""),
      slug: String(p.slug ?? ""),
      name: String(p.name ?? ""),
      description: String(p.description ?? ""),
      projectType: String(p.projectType ?? "mod"),
      iconUrl: (p.iconUrl as string | null | undefined) ?? null,
      clientSide: (p.clientSide as string | null | undefined) ?? null,
      serverSide: (p.serverSide as string | null | undefined) ?? null,
      author: (p.author as string | null | undefined) ?? null,
      downloads: (p.downloads as number | null | undefined) ?? null,
      follows: (p.follows as number | null | undefined) ?? null,
      dateModified: (p.dateModified as string | null | undefined) ?? null,
      categories: (p.categories as string[] | undefined) ?? [],
    };
  }

  function isSavedViewFilter(filter: string): boolean {
    return filter === "favorites" || filter.startsWith("list:");
  }

  function modIconLookupKey(mod: ModRow): string | null {
    if (mod.projectId) return mod.projectId;
    if (mod.source === "modrinth" && mod.id) return mod.id;
    return null;
  }

  async function resolveIconForMod(mod: ModRow) {
    const key = modIconLookupKey(mod);
    if (!key) return;
    try {
      const url: string | null = await invoke("get_modrinth_project_icon", { projectId: key });
      if (url) {
        mods = mods.map((x) => (x.id === mod.id ? { ...x, iconUrl: url } : x));
        brokenIcons.delete(mod.id);
        brokenIcons = brokenIcons;
      }
    } catch {
      // keep letter-avatar fallback
    }
  }

  async function handleIconError(mod: ModRow) {
    brokenIcons.add(mod.id);
    brokenIcons = brokenIcons;
    await resolveIconForMod(mod);
  }

  function humanize(s: string): string {
    return s
      .replace(/[-_]/g, " ")
      .replace(/\b\w/g, (c) => c.toUpperCase());
  }

  function resultBadges(result: SearchResult): { icon: string; label: string }[] {
    const badges: { icon: string; label: string }[] = [];
    const env =
      result.clientSide === "required" || result.clientSide === "optional"
        ? "client"
        : result.serverSide === "required" || result.serverSide === "optional"
          ? "server"
          : result.clientSide ?? result.serverSide ?? null;
    if (env) badges.push({ icon: "side", label: humanize(env) });
    for (const c of result.categories ?? []) badges.push({ icon: "tag", label: humanize(c) });
    return badges;
  }

  $: filteredVersions = gameVersions.filter((v) =>
    v.toLowerCase().includes(versionSearch.trim().toLowerCase())
  );

  $: shownLoaders = loaderExpanded
    ? loaders
    : loaders.slice(0, 3);

  $: totalPages = Math.max(1, Math.ceil(searchResults.length / pageSize));
  $: pagedResults = searchResults.slice((page - 1) * pageSize, page * pageSize);
  $: if (page > totalPages) page = totalPages;

  function goToPage(p: number) {
    page = Math.min(totalPages, Math.max(1, p));
  }

  let previews: Record<string, InstallPreview | null> = {};
  let pendingInstall: SearchResult | null = null;
  let selectedResultIds: Record<string, boolean> = {};

  // --- Version picker (change mod version) ---
  type ModVersion = { id: string; versionNumber: string; gameVersions: string[]; loaders: string[]; name?: string; changelog?: string; datePublished?: string };
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
    openDownloadOverlay(`Switching ${versionPickerMod.name}`);
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
      downloadDone = true;
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
      const ids = new Set(updateList.map((u) => u.modId));
      mods = mods.map((m) => ({ ...m, updateAvailable: ids.has(m.id) }));
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
    openDownloadOverlay(`Updating ${updateList.length} mod${updateList.length > 1 ? "s" : ""}`);
    try {
      const result: any = await invoke("update_all_mods", { path: $projectPath });
      const updated: string[] = Array.isArray(result) ? result : (result?.updated ?? []);
      const errs: string[] = result?.errors ?? [];
      const failedDownloads = result?.download?.failed?.length ?? 0;
      message = updated.length
        ? `Updated ${updated.length} mods: ${updated.join(", ")}`
        : "No mods were updated.";
      if (errs.length) {
        error = errs.slice(0, 3).join("; ");
      } else if (failedDownloads > 0) {
        error = `${failedDownloads} download(s) failed — check the progress window.`;
      }
      updateList = [];
      await load(true);
    } catch (e) {
      error = String(e);
      downloadDone = true;
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

  // User state for mods (favorites, named build lists, ratings)
  let userState: { favorites: Record<string, boolean>; lists: Record<string, string[]>; ratings: Record<string, number> } = {
    favorites: {},
    lists: {},
    ratings: {},
  };

  // Which list the user is currently viewing in the Lists panel
  // Dropdown open state for the save button (per-mod)
  let saveDropdownFor: string | null = null;
  // New list name input
  let newListName = "";

  async function loadUserState() {
    if (!$projectPath) return;
    try {
      userState = await api.mods.getUserState($projectPath);
    } catch {
      userState = { favorites: {}, lists: {}, ratings: {} };
    }
  }

  async function patchUserState(modId: string, patch: { favorite?: boolean; saved?: boolean; rating?: number }) {
    if (!$projectPath) return;
    try {
      userState = await api.mods.setUserState(modId, patch, $projectPath);
    } catch (e) {
      error = String(e);
    }
  }

  async function toggleFavorite(modId: string) {
    const current = userState.favorites[modId] ?? false;
    await patchUserState(modId, { favorite: !current });
  }

  function toggleSaved(modId: string) {
    // Quick toggle: add to / remove from a default "Saved" list
    const inDefault = (userState.lists["Saved"] ?? []).includes(modId);
    patchUserState(modId, { saved: !inDefault });
  }

  let copiedLinkId: string | null = null;
  let copiedLinkTimer: ReturnType<typeof setTimeout> | null = null;

  function modrinthProjectUrl(result: SearchResult): string {
    const type = result.projectType || "mod";
    return `https://modrinth.com/${type}/${result.slug || result.id}`;
  }

  async function copyProjectLink(result: SearchResult) {
    const url = modrinthProjectUrl(result);
    try {
      await navigator.clipboard.writeText(url);
    } catch {
      // Clipboard may be unavailable in some environments
      return;
    }
    copiedLinkId = result.id;
    if (copiedLinkTimer) clearTimeout(copiedLinkTimer);
    copiedLinkTimer = setTimeout(() => {
      copiedLinkId = null;
      copiedLinkTimer = null;
    }, 2000);
  }

  // Returns true if the mod is in at least one list
  function modInAnyList(modId: string): boolean {
    return Object.values(userState.lists).some((ids) => ids.includes(modId));
  }

  function modInList(modId: string, listName: string): boolean {
    return (userState.lists[listName] ?? []).includes(modId);
  }

  async function createList(name: string) {
    if (!$projectPath || !name.trim()) return;
    try {
      userState = await api.mods.createList(name.trim(), $projectPath);
    } catch (e) {
      error = String(e);
    }
  }

  async function deleteList(name: string) {
    if (!$projectPath) return;
    try {
      userState = await api.mods.deleteList(name, $projectPath);
      if (contentFilter === `list:${name}`) {
        contentFilter = "mod";
      }
    } catch (e) {
      error = String(e);
    }
  }

  async function renameList(oldName: string, newName: string) {
    if (!$projectPath || !newName.trim() || oldName === newName) return;
    try {
      userState = await api.mods.renameList(oldName, newName.trim(), $projectPath);
      if (contentFilter === `list:${oldName}`) {
        contentFilter = `list:${newName.trim()}`;
      }
    } catch (e) {
      error = String(e);
    }
  }

  async function createListAndAdd(name: string, modId: string) {
    const trimmed = name.trim();
    if (!$projectPath || !trimmed) return;
    try {
      userState = await api.mods.createList(trimmed, $projectPath);
      userState = await api.mods.addToList(trimmed, modId, $projectPath);
      newListName = "";
      saveDropdownFor = null;
    } catch (e) {
      error = String(e);
    }
  }

  async function addToList(listName: string, modId: string) {
    if (!$projectPath) return;
    try {
      userState = await api.mods.addToList(listName, modId, $projectPath);
    } catch (e) {
      error = String(e);
    }
  }

  async function removeFromList(listName: string, modId: string) {
    if (!$projectPath) return;
    try {
      userState = await api.mods.removeFromList(listName, modId, $projectPath);
    } catch (e) {
      error = String(e);
    }
  }

  // Install all mods from a list (one click)
  let installingFromList: string | null = null;
  async function installList(listName: string) {
    if (!$projectPath) return;
    const modIds = userState.lists[listName] ?? [];
    if (modIds.length === 0) return;
    installingFromList = listName;
    openDownloadOverlay(`Installing list "${listName}"`);
    try {
      await invoke("add_modrinth_mods_with_dependencies", {
        path: $projectPath,
        modIds,
        side: "both",
      });
      message = `Installed ${modIds.length} mods from "${listName}"`;
      await load(true);
    } catch (e) {
      error = String(e);
      downloadDone = true;
    } finally {
      installingFromList = null;
    }
  }

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
    openDownloadOverlay(withDeps ? `Installing ${planPreviewMod.name} + deps` : `Installing ${planPreviewMod.name}`);
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
      downloadDone = true;
    } finally {
      mutating = false;
    }
  }

  // Populated from the real Mojang version manifest via get_minecraft_versions
  // instead of a hand-maintained list, so it never goes stale as new
  // Minecraft versions ship.
  let gameVersions: string[] = [];
  const loaders = ["Fabric", "Forge", "NeoForge", "Quilt"];
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

  async function load(force = false) {
    if (!$projectPath) return;
    if (!force && lastLoadedPath === $projectPath && mods.length > 0) return;
    loading = true;
    error = null;
    try {
      // Sync folders first so manually dropped jars get identified via Modrinth
      // hash lookup and receive projectId/icon metadata before we render.
      mods = await invoke("sync_mods_folder", { path: $projectPath });
      lastLoadedPath = $projectPath;
      brokenIcons = new Set();
      await loadUserState();
    } catch (e) {
      error = String(e);
      loading = false;
      return;
    }
    // Render the list immediately. The Modrinth calls below (icon hydration +
    // update checks) can be slow or fail offline, so they run in the
    // background instead of blocking the spinner forever.
    loading = false;
    hydrateMissingIcons().catch(() => {});
    refreshUpdateDots().catch(() => {});
    if (isSavedViewFilter(contentFilter)) {
      loadSavedModsView().catch(() => {});
    }
  }

  // Cross-references the latest available Modrinth versions with the installed
  // ones and flags each mod row that has an update pending (drives the dot).
  async function refreshUpdateDots() {
    if (!$projectPath) return;
    try {
      const updates: any[] = await invoke("check_mod_updates", { path: $projectPath });
      updateList = updates;
      const ids = new Set(updates.map((u) => u.modId));
      mods = mods.map((m) => ({ ...m, updateAvailable: ids.has(m.id) }));
    } catch {
      // leave existing flags in place
    }
  }

  // Some mods (e.g. local jars with a known Modrinth project id, or entries
  // whose CDN icon failed to resolve) have no iconUrl. Try to fetch a real
  // icon so the list isn't all letter-avatars.
  async function hydrateMissingIcons() {
    if (!$projectPath) return;
    const missing = mods.filter((m) => {
      if (brokenIcons.has(m.id)) return !!modIconLookupKey(m);
      if (m.iconUrl) return false;
      return !!modIconLookupKey(m);
    });
    if (missing.length === 0) return;
    await Promise.all(missing.map((m) => resolveIconForMod(m)));
  }

  async function loadSavedModsView() {
    const ids =
      contentFilter === "favorites"
        ? Object.entries(userState.favorites)
            .filter(([, v]) => v)
            .map(([k]) => k)
        : contentFilter.startsWith("list:")
          ? (userState.lists[contentFilter.slice(5)] ?? [])
          : [];
    if (ids.length === 0) {
      savedMods = [];
      savedModsLoading = false;
      return;
    }
    savedModsLoading = true;
    try {
      const results = await Promise.all(
        ids.map(async (id) => {
          try {
            const project = await invoke<Record<string, unknown>>("get_modrinth_project", { projectId: id });
            return projectToSearchResult(project);
          } catch {
            return null;
          }
        })
      );
      savedMods = results.filter((r): r is SearchResult => r !== null);
    } finally {
      savedModsLoading = false;
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
    if (isSavedViewFilter(next)) {
      loadSavedModsView().catch(() => {});
    }
  }

  async function openAddModal() {
    addOpen = true;
    error = null;
    await initAddFilters();
  }

  async function initAddFilters() {
    if (!$projectPath) return;
    try {
      const info: any = await invoke("validate_project", { path: $projectPath });
      filterLoader = info.loaderKind;
      filterGameVersion = info.minecraftVersion;
    } catch {
      // keep defaults
    }
    await loadUserState();
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
    openDownloadOverlay(`Installing ${selectedResults.length} projects`);
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
      downloadDone = true;
    } finally {
      mutating = false;
    }
  }

  async function confirmInstall(withDependencies = false) {
    if (!$projectPath || !pendingInstall) return;
    mutating = true;
    error = null;
    openDownloadOverlay(
      withDependencies
        ? `Installing ${pendingInstall.name} + deps`
        : `Installing ${pendingInstall.name}`,
    );
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
      downloadDone = true;
    } finally {
      mutating = false;
    }
  }

  async function removeMod(mod: ModRow) {
    showRemoveConfirm(mod);
  }

  async function updateMod(mod: ModRow, versionId?: string | null) {
    if (!$projectPath) return;
    mutating = true;
    error = null;
    openDownloadOverlay(`Updating ${mod.name}`);
    try {
      const result: any = await invoke("update_project_mod", {
        path: $projectPath,
        modId: mod.id,
        versionId: versionId ?? null,
      });
      const failures = result?.download?.failed ?? [];
      if (failures.length) {
        throw new Error(failures.map((failure: any) => failure.error).join("; "));
      }
      updateList = updateList.filter((u) => u.modId !== mod.id);
      await load(true);
    } catch (e) {
      error = String(e);
      downloadDone = true;
    } finally {
      mutating = false;
    }
  }

  async function updateFromPanel(update: any) {
    if (!$projectPath) return;
    mutating = true;
    error = null;
    openDownloadOverlay(`Updating ${update.name}`);
    try {
      const result: any = await invoke("update_project_mod", {
        path: $projectPath,
        modId: update.modId,
        versionId: update.versionId ?? null,
      });
      const failures = result?.download?.failed ?? [];
      if (failures.length) {
        throw new Error(failures.map((failure: any) => failure.error).join("; "));
      }
      updateList = updateList.filter((u) => u.modId !== update.modId);
      await load(true);
    } catch (e) {
      error = String(e);
      downloadDone = true;
    } finally {
      mutating = false;
    }
  }

  function modIconUrl(mod: ModRow) {
    return mod.iconUrl;
  }

  function isInstalled(result: SearchResult) {
    return mods.some((m) => m.id === result.slug || m.id === result.id || m.projectId === result.id);
  }

  function iconFallback(name: string) {
    return name?.[0]?.toUpperCase() ?? "?";
  }

  function stripHtml(html: string) {
    return html?.replace(/<[^>]*>/g, "").replace(/&[a-z]+;/gi, " ").trim() ?? "";
  }

  function formatDate(iso: string) {
    if (!iso) return "";
    try {
      const d = new Date(iso);
      return d.toLocaleDateString(undefined, { year: "numeric", month: "short", day: "numeric" });
    } catch {
      return iso.slice(0, 10);
    }
  }

  function depKind(dep: { type: string }) {
    return String(dep.type ?? "").toLowerCase();
  }

  function requiredDeps(preview: InstallPreview | null | undefined) {
    return (preview?.dependencies ?? []).filter((dep) => {
      const kind = depKind(dep);
      return kind.includes("required") || kind.includes("requires");
    });
  }

  function conflictDeps(preview: InstallPreview | null | undefined) {
    return (preview?.dependencies ?? []).filter((dep) => {
      const kind = depKind(dep);
      return kind.includes("conflict") || kind.includes("break") || kind.includes("incompatible");
    });
  }

  function optionalDeps(preview: InstallPreview | null | undefined) {
    return (preview?.dependencies ?? []).filter((dep) => depKind(dep).includes("optional"));
  }

  $: filtered = isSavedViewFilter(contentFilter)
    ? []
    : mods.filter((m) => {
        const q = filter.toLowerCase();
        const matchesText =
          m.name.toLowerCase().includes(q) ||
          m.id.toLowerCase().includes(q) ||
          m.version.toLowerCase().includes(q);
        const matchesSide = sideFilter === "all" || m.side === sideFilter;
        const matchesContentType = (m.contentType ?? "mod") === contentFilter;
        return matchesText && matchesSide && matchesContentType;
      });

  $: listTabNames = Object.keys(userState.lists).sort((a, b) => a.localeCompare(b));

  $: if (isSavedViewFilter(contentFilter)) {
    void loadSavedModsView();
  }

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
  <header class="content-hero">
    <div class="content-hero-copy">
      <div class="content-kicker"><Package size={14} /> Content library</div>
      <h1>Your pack, sharpened</h1>
      <p>Install, update and curate mods with live download feedback — no guessing, no orphan jars.</p>
    </div>
    <div class="content-hero-stats">
      <div class="stat-pill">
        <strong>{filtered.length}</strong>
        <span>shown</span>
      </div>
      <div class="stat-pill accent" class:pulse={updateList.length > 0}>
        <strong>{updateList.length}</strong>
        <span>updates</span>
      </div>
      <div class="stat-pill">
        <strong>{counts.all}</strong>
        <span>total</span>
      </div>
    </div>
  </header>

  <div class="toolbar">
    <div class="tabs content-tabs">
      <button class={contentFilter === "mod" ? "primary" : "secondary"} on:click={() => switchContentFilter("mod")}>Mods</button>
      <button class={contentFilter === "resourcepack" ? "primary" : "secondary"} on:click={() => switchContentFilter("resourcepack")}>Resourcepacks</button>
      <button class={contentFilter === "datapack" ? "primary" : "secondary"} on:click={() => switchContentFilter("datapack")}>Datapacks</button>
      <button class={contentFilter === "shader" ? "primary" : "secondary"} on:click={() => switchContentFilter("shader")}>Shaders</button>
      <button class={contentFilter === "favorites" ? "primary" : "secondary"} on:click={() => switchContentFilter("favorites")} title="Favorite Modrinth projects">
        <Heart size={14} /> Favorites
      </button>
      {#each listTabNames as listName}
        <button class={contentFilter === `list:${listName}` ? "primary" : "secondary"} on:click={() => switchContentFilter(`list:${listName}`)} title="Saved build list">
          <Bookmark size={14} /> {listName}
          <span class="tab-count">{userState.lists[listName]?.length ?? 0}</span>
        </button>
      {/each}
    </div>
    <div class="toolbar-row">
      <div class="search wide">
        <Search size={16} />
        <input bind:value={filter} placeholder="Search {contentFilter}s..." />
      </div>
      <div class="actions">
        <button on:click={openAddModal} disabled={!$projectPath || mutating}>
          <Plus size={16} />
          Add {isSavedViewFilter(contentFilter) ? "mod" : contentFilter}
        </button>
        <button class="secondary" on:click={async () => {
          loading = true;
          try {
            mods = await invoke("sync_mods_folder", { path: $projectPath });
            brokenIcons = new Set();
            hydrateMissingIcons().catch(() => {});
          } catch(e) {
            error = String(e);
          } finally {
            loading = false;
          }
        }} disabled={!$projectPath || loading} title="Scan all content folders (mods/, resourcepacks/, shaderpacks/, datapacks/)">
          <RotateCw size={16} /> Sync
        </button>
        <button class="secondary glow-btn" on:click={checkForUpdates} disabled={!$projectPath || updateCheckLoading} title="Check all mods for updates">
          <Sparkles size={16} />
          {updateCheckLoading ? "Checking..." : updateList.length > 0 ? `${updateList.length} updates` : "Check updates"}
        </button>
        <button class="secondary" on:click={loadRecommendations} disabled={!$projectPath || recsLoading} title="Get mod recommendations">
          <Lightbulb size={16} />
          {recsLoading ? "..." : "Suggestions"}
        </button>
      </div>
    </div>
  </div>

  <div class="quick-filters" aria-label="Side filters">
    {#if !isSavedViewFilter(contentFilter)}
    <button class:active={sideFilter === "all"} on:click={() => (sideFilter = "all")}>All <span>{counts.all}</span></button>
    <button class:active={sideFilter === "both"} on:click={() => (sideFilter = "both")}>Both <span>{counts.both}</span></button>
    <button class:active={sideFilter === "client"} on:click={() => (sideFilter = "client")}>Client <span>{counts.client}</span></button>
    <button class:active={sideFilter === "server"} on:click={() => (sideFilter = "server")}>Server <span>{counts.server}</span></button>
    {/if}
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
              openDownloadOverlay(`Installing ${rec.name}`);
              try {
                await invoke("add_modrinth_mod_with_dependencies", { path: $projectPath, modId: rec.slug, side: "auto" });
                recommendations = recommendations.filter((r) => r.slug !== rec.slug);
                await load(true);
                checkMissingDepsAfterInstall();
              } catch(e) { error = String(e); downloadDone = true; }
              finally { mutating = false; }
            }} disabled={mutating}>
              <Plus size={12} /> Install
            </button>
          </div>
        {/each}
      </div>
    </div>
  {/if}

  <!-- Build Lists panel removed: lists now appear as tabs next to Shaders -->

  {#if updateList.length > 0}
    <div class="update-panel">
      <div class="update-panel-header">
        <h3><ArrowUpCircle size={18} /> {updateList.length} update{updateList.length > 1 ? "s" : ""} ready</h3>
        <button class="update-all-btn" on:click={applyAllUpdates} disabled={updateApplying}>
          <Sparkles size={16} /> {updateApplying ? "Updating..." : "Update all"}
        </button>
      </div>
      <div class="update-list">
        {#each updateList as update}
          <div class="update-row">
            <div class="update-icon">
              {#if update.iconUrl}
                <img src={update.iconUrl} alt="" loading="lazy" />
              {:else}
                <span>{iconFallback(update.name)}</span>
              {/if}
            </div>
            <div class="update-main">
              <strong>{update.name}</strong>
              <span class="update-versions">
                <code class="ver-old">{update.currentVersion}</code>
                <ArrowRight size={12} />
                <code class="ver-new">{update.latestVersion}</code>
              </span>
            </div>
            <button class="secondary mini" on:click={() => updateFromPanel(update)} disabled={mutating}>
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
  {:else if isSavedViewFilter(contentFilter)}
    {#if savedModsLoading}
      <div class="loading">Loading saved projects...</div>
    {:else if savedMods.length === 0}
      <div class="empty">
        {#if contentFilter === "favorites"}
          No favorites yet. Use the heart icon in Add Modrinth to save projects here.
        {:else}
          This list is empty. Use the bookmark icon in Add Modrinth to add projects.
        {/if}
      </div>
    {:else}
      <div class="saved-toolbar">
        {#if contentFilter.startsWith("list:")}
          {@const listName = contentFilter.slice(5)}
          <button on:click={() => installList(listName)} disabled={!$projectPath || installingFromList === listName || mutating}>
            <ArrowDown size={16} /> {installingFromList === listName ? "Installing..." : `Install all from "${listName}"`}
          </button>
          <button class="secondary" on:click={() => { const n = prompt("Rename list:", listName); if (n) renameList(listName, n); }}>Rename</button>
          <button class="secondary danger" on:click={() => { if (confirm(`Delete list "${listName}"?`)) deleteList(listName); }}>Delete list</button>
        {/if}
      </div>
      <div class="saved-list">
        {#each savedMods as result}
          <article class="saved-card" class:installed={isInstalled(result)}>
            <div class="result-icon">
              {#if result.iconUrl}
                <img src={result.iconUrl} alt="" loading="lazy" />
              {:else}
                <span>{iconFallback(result.name)}</span>
              {/if}
            </div>
            <div class="saved-main">
              <div class="installed-title">
                <strong>{result.name}</strong>
                <code>{result.slug}</code>
              </div>
              <p class="result-desc">{result.description}</p>
            </div>
            <div class="saved-actions">
              <button on:click={() => startInstallPlan(result)} disabled={mutating || isInstalled(result)}>
                <ArrowDown size={16} /> {isInstalled(result) ? "Installed" : "Install"}
              </button>
              <button class="qa" class:active={userState.favorites[result.id]} title="Favorite" on:click={() => toggleFavorite(result.id)}>
                <Heart size={15} fill={userState.favorites[result.id] ? "currentColor" : "none"} />
              </button>
              {#if contentFilter.startsWith("list:")}
                <button class="qa danger" title="Remove from list" on:click={() => removeFromList(contentFilter.slice(5), result.id)}><X size={15} /></button>
              {/if}
            </div>
          </article>
        {/each}
      </div>
    {/if}
  {:else if filtered.length === 0}
    <div class="empty">No mods found.</div>
  {:else}
    <div class="installed-list">
      {#each filtered as mod, i}
        <article class="installed-card" class:has-update={mod.updateAvailable} style="--i: {i}">
          <div class="mod-icon">
            {#if mod.updateAvailable}
              <span class="update-dot" title="Update available"></span>
            {/if}
            {#if mod.iconUrl && !brokenIcons.has(mod.id)}
              <img src={mod.iconUrl} alt="" loading="lazy" on:error={() => handleIconError(mod)} />
            {:else}
              <span>{iconFallback(mod.name)}</span>
            {/if}
          </div>
          <div class="installed-main">
            <div class="installed-title">
              <strong>{mod.name}</strong>
              {#if mod.updateAvailable}
                <span class="update-badge">Update</span>
              {/if}
              <code>{mod.id}</code>
            </div>
            <div class="installed-meta">
              <span class="version">{mod.version}</span>
              {#if mod.fileName}<span class="filename">{mod.fileName}</span>{/if}
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
            <button class="icon-btn" class:hot={mod.updateAvailable} on:click={() => {
              const pending = updateList.find((u) => u.modId === mod.id);
              updateMod(mod, pending?.versionId ?? null);
            }} disabled={mutating || mod.source !== "modrinth"} title="Update to latest from Modrinth">
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

{#if downloadOpen}
  <div
    class="modal-backdrop download-backdrop"
    role="dialog"
    aria-modal="true"
    aria-label="Download progress"
  >
    <div class="download-modal">
      <div class="download-modal-header">
        <div>
          <div class="content-kicker"><Download size={14} /> Live transfer</div>
          <h2>{downloadTitle}</h2>
          <p>
            {#if downloadDone}
              {downloadFailedCount > 0
                ? `Finished with ${downloadFailedCount} failure${downloadFailedCount > 1 ? "s" : ""}.`
                : "All transfers complete."}
            {:else}
              {downloadDoneCount}/{downloadItems.length || "…"} finished · {downloadOverallPercent}%
            {/if}
          </p>
        </div>
        {#if downloadDone}
          <button class="icon-btn" on:click={closeDownloadOverlay} title="Close"><X size={18} /></button>
        {:else}
          <span class="spin-wrap"><Loader2 size={22} /></span>
        {/if}
      </div>

      <div class="download-overall">
        <div class="download-overall-bar">
          <div class="download-overall-fill" style="width: {downloadOverallPercent}%"></div>
        </div>
      </div>

      <div class="download-list">
        {#if downloadItems.length === 0}
          <div class="download-empty">Preparing downloads…</div>
        {:else}
          {#each downloadItems as item}
            <div class="download-row" class:done={item.status === "done" || item.status === "skipped"} class:failed={item.status === "failed"} class:active={item.status === "downloading"}>
              <div class="download-row-top">
                <strong>{item.name}</strong>
                <span class="download-status">{item.status}</span>
              </div>
              <div class="download-bar">
                <div class="download-fill" style="width: {item.percent || 0}%"></div>
              </div>
              <div class="download-row-meta">
                {#if item.total > 0}
                  <span>{formatBytes(item.downloaded)} / {formatBytes(item.total)}</span>
                  <span>{item.percent}%</span>
                {:else if item.status === "queued"}
                  <span>Waiting…</span>
                {:else if item.status === "failed"}
                  <span>Failed</span>
                {:else}
                  <span>{formatBytes(item.downloaded)}</span>
                {/if}
              </div>
            </div>
          {/each}
        {/if}
      </div>

      {#if downloadDone}
        <div class="download-modal-actions">
          <button on:click={closeDownloadOverlay}>Done</button>
        </div>
      {/if}
    </div>
  </div>
{/if}

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
          <h2>Add Modrinth {contentFilter}</h2>
          <p>
            {contentFilter === "mod"
              ? "Search is filtered by the current Minecraft version and loader."
              : "Search is filtered by the current Minecraft version."}
          </p>
        </div>
        <button class="icon-btn" on:click={() => (addOpen = false)}><X size={18} /></button>
      </div>

      <div class="modal-tabs">
        <button class:active={contentFilter === "mod"} on:click={() => switchContentFilter("mod")}>Mods</button>
        <button class:active={contentFilter === "resourcepack"} on:click={() => switchContentFilter("resourcepack")}>Resourcepacks</button>
        <button class:active={contentFilter === "datapack"} on:click={() => switchContentFilter("datapack")}>Datapacks</button>
        <button class:active={contentFilter === "shader"} on:click={() => switchContentFilter("shader")}>Shaders</button>
        <button class:active={contentFilter === "favorites"} on:click={() => switchContentFilter("favorites")}>Favorites</button>
        {#each listTabNames as listName}
          <button class:active={contentFilter === `list:${listName}`} on:click={() => switchContentFilter(`list:${listName}`)}>{listName}</button>
        {/each}
      </div>

      <div class="browser-layout">
        <aside class="filter-sidebar">
          <section class="filter-block" class:closed={!accordionOpen.gameVersion}>
            <button class="filter-head" on:click={() => toggleAccordion("gameVersion")}>
              <span>Game version</span>
              <ChevronDown size={16} class={!accordionOpen.gameVersion ? "rot" : ""} />
            </button>
            {#if accordionOpen.gameVersion}
              <div class="filter-body">
                <div class="search mini">
                  <Search size={14} />
                  <input bind:value={versionSearch} placeholder="Search version..." />
                </div>
                <div class="filter-list">
                  {#each filteredVersions as version}
                    <button class:active={filterGameVersion === version} on:click={() => { filterGameVersion = version; searchMods(); }}>{version}</button>
                  {/each}
                </div>
                <label class="check-row">
                  <input type="checkbox" checked={filterGameVersion === ""} on:change={() => { filterGameVersion = ""; searchMods(); }} /> Show all versions
                </label>
              </div>
            {/if}
          </section>

          <section class="filter-block" class:closed={!accordionOpen.loader}>
            <button class="filter-head" on:click={() => toggleAccordion("loader")}>
              <span>Loader</span>
              <ChevronDown size={16} class={!accordionOpen.loader ? "rot" : ""} />
            </button>
            {#if accordionOpen.loader}
              <div class="filter-body">
                <div class="filter-list loader-list">
                  {#each shownLoaders as loaderName}
                    <button class="loader-row" class:active={filterLoader === loaderName.toLowerCase()} on:click={() => { filterLoader = loaderName.toLowerCase(); searchMods(); }}>
                      <span class="loader-ic">
                        {#if loaderName === "Fabric"}<Scroll size={16} />{:else if loaderName === "Forge"}<Hammer size={16} />{:else}<Anvil size={16} />{/if}
                      </span>
                      <span>{loaderName}</span>
                    </button>
                  {/each}
                </div>
                {#if loaders.length > 3}
                  <button class="show-more" on:click={() => (loaderExpanded = !loaderExpanded)}>
                    {loaderExpanded ? "Show less" : "Show more"} <ChevronDown size={14} class={loaderExpanded ? "rot" : ""} />
                  </button>
                {/if}
              </div>
            {/if}
          </section>

          <section class="filter-block" class:closed={!accordionOpen.category}>
            <button class="filter-head" on:click={() => toggleAccordion("category")}>
              <span>Category</span>
              <ChevronDown size={16} class={!accordionOpen.category ? "rot" : ""} />
            </button>
            {#if accordionOpen.category}
              <div class="filter-body">
                <div class="filter-list">
                  <button class:active={!filterCategory} on:click={() => { filterCategory = ""; searchMods(); }}>All categories</button>
                  {#each categories as category}
                    <button class="cat-row" class:active={filterCategory === category} on:click={() => { filterCategory = category; searchMods(); }}>
                      <Tag size={14} />
                      <span>{humanize(category)}</span>
                    </button>
                  {/each}
                </div>
              </div>
            {/if}
          </section>
        </aside>

        <section class="browser-results">
          <div class="browser-topbar">
            <div class="search wide">
              <Search size={16} />
              <input bind:value={searchQuery} placeholder="Search mods..." on:keydown={(e) => e.key === "Enter" && searchMods()} />
            </div>
            <div class="topbar-controls">
              <label class="sort-select">Sort by:
                <select bind:value={sortBy} on:change={() => searchMods()}>
                  {#each sortOptions as option}<option value={option.id}>{option.label}</option>{/each}
                </select>
              </label>
              <label class="sort-select">View:
                <select bind:value={pageSize} on:change={() => (page = 1)}>
                  <option value={20}>20</option>
                  <option value={40}>40</option>
                  <option value={60}>60</option>
                </select>
              </label>
              <button class="view-toggle" class:active={viewMode === "grid"} on:click={() => (viewMode = "grid")} title="Grid view"><LayoutGrid size={16} /></button>
              <button class="view-toggle" class:active={viewMode === "list"} on:click={() => (viewMode = "list")} title="List view"><List size={16} /></button>
            </div>
            <div class="pagination">
              <button class="page-btn" disabled={page <= 1} on:click={() => goToPage(page - 1)}>‹</button>
              {#each Array.from({ length: Math.min(totalPages, 5) }, (_, i) => i + 1) as p}
                <button class="page-btn" class:active={p === page} on:click={() => goToPage(p)}>{p}</button>
              {/each}
              {#if totalPages > 5}<span class="page-ellipsis">…</span><button class="page-btn" on:click={() => goToPage(totalPages)}>{totalPages}</button>{/if}
              <button class="page-btn" disabled={page >= totalPages} on:click={() => goToPage(page + 1)}><ArrowRight size={14} /></button>
            </div>
          </div>

          <div class="bulk-bar">
            <div>
              <strong>{selectedResults.length}</strong>
              <span>selected for bulk install</span>
            </div>
            <div class="bulk-actions">
              <button class="ghost" on:click={selectVisibleResults} disabled={pagedResults.length === 0}>Select visible</button>
              <button class="ghost" on:click={clearResultSelection} disabled={selectedResults.length === 0}>Clear</button>
              <button on:click={bulkInstallSelected} disabled={selectedResults.length === 0 || mutating}>Install selected + dependencies</button>
            </div>
          </div>

          {#if searchLoading}
            <div class="loading compact">Loading Modrinth projects...</div>
          {:else if isSavedViewFilter(contentFilter)}
            {#if savedModsLoading}
              <div class="loading compact">Loading saved projects...</div>
            {:else if savedMods.length === 0}
              <div class="empty compact">
                {#if contentFilter === "favorites"}
                  No favorites yet.
                {:else}
                  This list is empty.
                {/if}
              </div>
            {:else}
              <div class="results {viewMode}">
                {#each savedMods as result}
                  <article class="result-card" class:installed={isInstalled(result)} class:list={viewMode === "list"}>
                    <div class="result-icon">
                      {#if result.iconUrl}
                        <img src={result.iconUrl} alt="" loading="lazy" />
                      {:else}
                        <span>{iconFallback(result.name)}</span>
                      {/if}
                    </div>
                    <div class="result-main">
                      <div class="result-title">
                        <span class="result-name">{result.name}</span>
                        {#if result.author}<span class="result-author">by {result.author}</span>{/if}
                      </div>
                      <p class="result-desc">{result.description}</p>
                    </div>
                    <div class="result-actions">
                      <button class="download-btn" on:click={() => startInstallPlan(result)} disabled={mutating || isInstalled(result)}>
                        <Download size={16} /> {isInstalled(result) ? "Installed" : "Download"}
                      </button>
                      <div class="quick-actions">
                        <button class="qa" class:active={userState.favorites[result.id]} title="Favorite" on:click|stopPropagation={() => toggleFavorite(result.id)}>
                          <Heart size={15} fill={userState.favorites[result.id] ? "currentColor" : "none"} />
                        </button>
                        <div class="save-wrapper">
                          <button class="qa" class:active={modInAnyList(result.id)} title="Add to list" on:click|stopPropagation={() => (saveDropdownFor = saveDropdownFor === result.id ? null : result.id)}>
                            <Bookmark size={15} fill={modInAnyList(result.id) ? "currentColor" : "none"} />
                          </button>
                          {#if saveDropdownFor === result.id}
                            <div class="save-dropdown" role="menu" tabindex="-1" on:click|stopPropagation on:keydown|stopPropagation>
                              <div class="save-dropdown-header">Add to list</div>
                              {#each listTabNames as listName}
                                <button class="save-dropdown-item" on:click={() => { if (modInList(result.id, listName)) removeFromList(listName, result.id); else addToList(listName, result.id); saveDropdownFor = null; }}>
                                  <span class="save-check">{modInList(result.id, listName) ? '✓' : '+'}</span>
                                  <span>{listName}</span>
                                </button>
                              {/each}
                              <div class="save-dropdown-new">
                                <input type="text" placeholder="New list name..." bind:value={newListName} on:keydown={(e) => { if (e.key === 'Enter') { void createListAndAdd(newListName, result.id); }}} />
                                <button on:click={() => createListAndAdd(newListName, result.id)} disabled={!newListName.trim()}>+ Create & add</button>
                              </div>
                            </div>
                          {/if}
                        </div>
                        <button class="qa" title={copiedLinkId === result.id ? "Copied!" : "Copy Modrinth link"} on:click|stopPropagation={() => copyProjectLink(result)}>
                          {#if copiedLinkId === result.id}
                            <Check size={15} />
                          {:else}
                            <Link size={15} />
                          {/if}
                        </button>
                      </div>
                    </div>
                    <div class="result-footer">
                      <span><Download size={13} />{formatCount(result.downloads)}</span>
                      <span><Heart size={13} />{formatCount(result.follows)}</span>
                      <span class="footer-updated"><Clock size={13} />{formatRelative(result.dateModified)}</span>
                    </div>
                  </article>
                {/each}
              </div>
            {/if}
          {:else if pagedResults.length === 0}
            <div class="empty compact">No projects found. Adjust filters or search text.</div>
          {:else}
            <div class="results {viewMode}">
          {#each pagedResults as result}
            <article class="result-card" class:installed={isInstalled(result)} class:selected={selectedResultIds[result.id]} class:list={viewMode === "list"} on:mouseenter={() => loadInstallPreview(result)} on:focusin={() => loadInstallPreview(result)}>
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
                  <span class="result-name">{result.name}</span>
                  {#if result.author}<span class="result-author">by {result.author}</span>{/if}
                </div>
                <p class="result-desc">{result.description}</p>
                {#if previewLoadingId === result.id}
                  <div class="install-preview muted">Loading install preview...</div>
                {:else if previews[result.id]}
                  <div class="install-preview">
                    <span>Version: {previews[result.id]?.version}</span>
                    <span>Side: {previews[result.id]?.side}</span>
                    <span>Deps: {previews[result.id]?.dependencies.length ?? 0}</span>
                  </div>
                {/if}
                <div class="result-badges">
                  {#each resultBadges(result) as b}
                    <span class="badge"><Tag size={12} />{b.label}</span>
                  {/each}
                </div>
              </div>
              <div class="result-actions">
                <button class="download-btn" on:click={() => startInstallPlan(result)} disabled={mutating || isInstalled(result)}>
                  <Download size={16} /> {isInstalled(result) ? "Installed" : "Download"}
                </button>
                <div class="quick-actions">
                  <button class="qa" class:active={userState.favorites[result.id]} title="Favorite" on:click|stopPropagation={() => toggleFavorite(result.id)}>
                    <Heart size={15} fill={userState.favorites[result.id] ? "currentColor" : "none"} />
                  </button>
                  <div class="save-wrapper">
                    <button class="qa" class:active={modInAnyList(result.id)} title="Add to list" on:click|stopPropagation={() => (saveDropdownFor = saveDropdownFor === result.id ? null : result.id)}>
                      <Bookmark size={15} fill={modInAnyList(result.id) ? "currentColor" : "none"} />
                    </button>
                    {#if saveDropdownFor === result.id}
                      <div class="save-dropdown" role="menu" tabindex="-1" on:click|stopPropagation on:keydown|stopPropagation>
                        <div class="save-dropdown-header">Add to list</div>
                        {#each Object.keys(userState.lists) as listName}
                          <button class="save-dropdown-item" on:click={() => { if (modInList(result.id, listName)) removeFromList(listName, result.id); else addToList(listName, result.id); saveDropdownFor = null; }}>
                            <span class="save-check">{modInList(result.id, listName) ? '✓' : '+'}</span>
                            <span>{listName}</span>
                          </button>
                        {/each}
                        <div class="save-dropdown-new">
                          <input type="text" placeholder="New list name..." bind:value={newListName} on:keydown={(e) => { if (e.key === 'Enter') { void createListAndAdd(newListName, result.id); }}} />
                          <button on:click={() => createListAndAdd(newListName, result.id)} disabled={!newListName.trim()}>+ Create & add</button>
                        </div>
                      </div>
                    {/if}
                  </div>
                  <button class="qa" title={copiedLinkId === result.id ? "Copied!" : "Copy Modrinth link"} on:click|stopPropagation={() => copyProjectLink(result)}>
                    {#if copiedLinkId === result.id}
                      <Check size={15} />
                    {:else}
                      <Link size={15} />
                    {/if}
                  </button>
                </div>
              </div>
              <div class="result-footer">
                <span><Download size={13} />{formatCount(result.downloads)}</span>
                <span><Heart size={13} />{formatCount(result.follows)}</span>
                <span class="footer-updated"><Clock size={13} />{formatRelative(result.dateModified)}</span>
              </div>
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
            <h3>{pendingInstall.name} ({previews[pendingInstall.id]?.slug ?? pendingInstall.slug})</h3>
            {#if previews[pendingInstall.id]}
              <div class="dep-list">
                <h4>Required ({requiredDeps(previews[pendingInstall.id]).length})</h4>
                {#if requiredDeps(previews[pendingInstall.id]).length === 0}
                  <p class="muted">No hard dependencies.</p>
                {:else}
                  {#each requiredDeps(previews[pendingInstall.id]) as dep}
                    <div class="dep-entry required">
                      <span class="dep-target">{dep.target}</span>
                      {#if dep.reason}<small>{dep.reason}</small>{/if}
                    </div>
                  {/each}
                {/if}
              </div>
              <div class="dep-list">
                <h4>Optional ({optionalDeps(previews[pendingInstall.id]).length})</h4>
                {#if optionalDeps(previews[pendingInstall.id]).length === 0}
                  <p class="muted">No optional dependencies.</p>
                {:else}
                  {#each optionalDeps(previews[pendingInstall.id]) as dep}
                    <div class="dep-entry optional">
                      <span class="dep-target">{dep.target}</span>
                      {#if dep.reason}<small>{dep.reason}</small>{/if}
                    </div>
                  {/each}
                {/if}
              </div>
              <label class="checkbox-row">
                <input type="checkbox" bind:checked={pendingInstallOptional} />
                <span>Install optional dependencies too</span>
              </label>
              {#if conflictDeps(previews[pendingInstall.id]).length}
                <div class="conflict-warning">
                  <strong><AlertTriangle size={14} /> Conflict warning</strong>
                  <span>This project declares incompatible dependencies. Review before installing.</span>
                  {#each conflictDeps(previews[pendingInstall.id]) as dep}
                    <code>{dep.type}:{dep.target}</code>
                  {/each}
                </div>
              {/if}
            {:else}
              <p class="muted">Preview unavailable; TuffBox will still create a snapshot before installing.</p>
            {/if}
          </div>
          <div class="plan-actions">
            <button class="ghost" on:click={() => (pendingInstall = null)}>Cancel</button>
            <button on:click={() => confirmInstall(pendingInstallOptional)} disabled={mutating}>
              <Download size={16} /> Install{pendingInstallOptional ? " with dependencies" : ""}
            </button>
          </div>
        </div>
      {/if}
    </div>
  </div>
{/if}

<!-- Version picker modal -->
{#if versionPickerMod}
  <div class="modal-backdrop" role="button" tabindex="-1" on:click={(e) => e.target === e.currentTarget && (versionPickerMod = null)} on:keydown={() => {}}>
    <div class="modal version-modal" role="dialog" aria-modal="true">
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
                {#if v.name && v.name !== v.versionNumber}
                  <span class="version-name">{v.name}</span>
                {/if}
                <span class="version-loaders">{v.loaders.join(", ")} · MC {v.gameVersions.slice(0, 3).join(", ")}{#if v.datePublished} · {formatDate(v.datePublished)}{/if}</span>
                {#if v.changelog}
                  <span class="version-changelog">{stripHtml(v.changelog).slice(0, 120)}{stripHtml(v.changelog).length > 120 ? '...' : ''}</span>
                {/if}
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
  <div class="modal-backdrop" role="button" tabindex="-1" on:click={(e) => e.target === e.currentTarget && (dependencyDialogOpen = false)} on:keydown={() => {}}>
    <div class="modal dep-dialog" role="dialog" aria-modal="true">
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
  <div class="modal-backdrop" role="button" tabindex="-1" on:click={(e) => e.target === e.currentTarget && (planPreviewOpen = false)} on:keydown={() => {}}>
    <div class="modal plan-modal" role="dialog" aria-modal="true">
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
    position: relative;
  }

  .content-hero {
    display: flex;
    justify-content: space-between;
    align-items: flex-end;
    gap: 24px;
    margin-bottom: 22px;
    padding: 22px 24px;
    border-radius: 20px;
    border: 1px solid rgba(27, 217, 106, 0.18);
    background:
      radial-gradient(ellipse 80% 120% at 0% 0%, rgba(27, 217, 106, 0.16), transparent 55%),
      radial-gradient(ellipse 60% 100% at 100% 0%, rgba(139, 92, 246, 0.12), transparent 50%),
      linear-gradient(180deg, rgba(255,255,255,0.03), transparent),
      var(--bg-secondary);
    overflow: hidden;
    animation: hero-in 0.45s ease both;
  }

  @keyframes hero-in {
    from { opacity: 0; transform: translateY(8px); }
    to { opacity: 1; transform: none; }
  }

  .content-kicker {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--accent-primary);
    font-size: 11px;
    font-weight: 800;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    margin-bottom: 8px;
  }

  .content-hero h1 {
    margin: 0 0 6px;
    font-size: 28px;
    font-weight: 800;
    letter-spacing: -0.03em;
    background: linear-gradient(120deg, #fff 30%, var(--accent-primary));
    -webkit-background-clip: text;
    background-clip: text;
    color: transparent;
  }

  .content-hero-copy p {
    margin: 0;
    color: var(--text-secondary);
    font-size: 14px;
    max-width: 420px;
    line-height: 1.45;
  }

  .content-hero-stats {
    display: flex;
    gap: 10px;
    flex-shrink: 0;
  }

  .stat-pill {
    min-width: 72px;
    padding: 10px 14px;
    border-radius: 14px;
    background: rgba(0,0,0,0.28);
    border: 1px solid var(--border-color);
    text-align: center;
  }

  .stat-pill strong {
    display: block;
    font-size: 20px;
    font-weight: 800;
    color: var(--text-primary);
  }

  .stat-pill span {
    font-size: 11px;
    color: var(--text-muted);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .stat-pill.accent {
    border-color: rgba(27, 217, 106, 0.4);
    background: rgba(27, 217, 106, 0.1);
  }

  .stat-pill.accent strong { color: var(--accent-primary); }

  .stat-pill.pulse {
    animation: pulse-glow 1.6s ease-in-out infinite;
  }

  @keyframes pulse-glow {
    0%, 100% { box-shadow: 0 0 0 0 rgba(27, 217, 106, 0); }
    50% { box-shadow: 0 0 18px 2px rgba(27, 217, 106, 0.25); }
  }

  .toolbar {
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin-bottom: 14px;
  }

  .content-tabs {
    display: flex;
    gap: 8px;
    overflow-x: auto;
    padding-bottom: 2px;
  }

  .toolbar-row {
    display: flex;
    justify-content: space-between;
    gap: 16px;
    align-items: center;
  }

  .glow-btn {
    border-color: rgba(27, 217, 106, 0.35) !important;
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
    flex-wrap: wrap;
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
    transition: border-color .15s, background .15s, transform .15s;
  }

  .quick-filters button:hover {
    transform: translateY(-1px);
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

  .tabs .tab-count {
    margin-left: 6px;
    font-size: 11px;
    color: var(--text-muted);
    background: var(--bg-elevated);
    padding: 1px 6px;
    border-radius: 999px;
  }

  .saved-toolbar {
    display: flex;
    gap: 10px;
    flex-wrap: wrap;
    margin-bottom: 14px;
  }

  .saved-list {
    display: grid;
    gap: 10px;
  }

  .saved-card {
    display: grid;
    grid-template-columns: 52px minmax(0, 1fr) auto;
    gap: 14px;
    align-items: center;
    padding: 12px 14px;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
  }

  .saved-card.installed {
    border-color: rgba(27, 217, 106, 0.35);
    background: rgba(27, 217, 106, 0.07);
  }

  .saved-main {
    min-width: 0;
  }

  .saved-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .installed-list {
    display: grid;
    gap: 10px;
  }

  .installed-card {
    min-height: 76px;
    display: grid;
    grid-template-columns: 56px minmax(0, 1fr) auto auto;
    gap: 14px;
    align-items: center;
    padding: 12px 14px;
    background: linear-gradient(135deg, rgba(255,255,255,0.02), transparent 40%), var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: 16px;
    transition: border-color .18s ease, background .18s ease, transform .18s ease, box-shadow .18s ease;
    animation: card-in 0.35s ease both;
    animation-delay: calc(var(--i, 0) * 18ms);
  }

  @keyframes card-in {
    from { opacity: 0; transform: translateY(6px); }
    to { opacity: 1; transform: none; }
  }

  .installed-card:hover {
    border-color: rgba(27, 217, 106, 0.35);
    background: rgba(255,255,255,0.03);
    transform: translateY(-1px);
    box-shadow: 0 8px 24px rgba(0,0,0,0.22);
  }

  .installed-card.has-update {
    border-color: rgba(245, 166, 35, 0.35);
    background:
      linear-gradient(90deg, rgba(245, 166, 35, 0.08), transparent 28%),
      var(--bg-secondary);
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
    box-shadow: 0 4px 14px rgba(27, 217, 106, 0.15);
  }

  .mod-icon img,
  .result-icon img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .update-dot {
    position: absolute;
    top: -3px;
    right: -3px;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--accent-warning, #f5a623);
    border: 2px solid var(--bg-card, #1c1f2b);
    box-shadow: 0 0 6px rgba(245, 166, 35, 0.8);
    z-index: 2;
    animation: pulse-glow 1.4s ease-in-out infinite;
  }

  .mod-icon {
    position: relative;
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

  .update-badge {
    font-size: 10px;
    font-weight: 800;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    padding: 2px 7px;
    border-radius: 999px;
    background: rgba(245, 166, 35, 0.18);
    color: #fbbf24;
    border: 1px solid rgba(245, 166, 35, 0.35);
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

  .installed-meta .filename {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 280px;
  }

  .icon-btn.hot {
    color: #fbbf24;
    background: rgba(245, 166, 35, 0.12);
  }

  /* Download progress modal */
  .download-backdrop {
    z-index: 80;
  }

  .download-modal {
    width: min(560px, calc(100vw - 32px));
    max-height: min(720px, calc(100vh - 40px));
    overflow: hidden;
    display: flex;
    flex-direction: column;
    background:
      radial-gradient(ellipse 90% 60% at 50% -10%, rgba(27, 217, 106, 0.18), transparent 55%),
      var(--bg-secondary);
    border: 1px solid rgba(27, 217, 106, 0.28);
    border-radius: 20px;
    box-shadow: 0 30px 100px rgba(0, 0, 0, 0.55);
    padding: 22px;
    animation: hero-in 0.25s ease both;
  }

  .download-modal-header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 16px;
    margin-bottom: 16px;
  }

  .download-modal-header h2 {
    margin: 0 0 4px;
    font-size: 20px;
  }

  .download-modal-header p {
    margin: 0;
    color: var(--text-muted);
    font-size: 13px;
  }

  .download-modal-header .spin-wrap {
    display: inline-flex;
    color: var(--accent-primary);
    animation: spin 0.9s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .download-overall {
    margin-bottom: 14px;
  }

  .download-overall-bar,
  .download-bar {
    height: 8px;
    border-radius: 999px;
    background: rgba(255,255,255,0.06);
    overflow: hidden;
  }

  .download-overall-fill,
  .download-fill {
    height: 100%;
    border-radius: 999px;
    background: linear-gradient(90deg, var(--accent-primary), #6ee7a8);
    box-shadow: 0 0 12px rgba(27, 217, 106, 0.45);
    transition: width 0.12s linear;
  }

  .download-list {
    display: grid;
    gap: 10px;
    overflow: auto;
    padding-right: 4px;
    max-height: 420px;
  }

  .download-empty {
    padding: 28px;
    text-align: center;
    color: var(--text-muted);
  }

  .download-row {
    padding: 12px 14px;
    border-radius: 14px;
    border: 1px solid var(--border-color);
    background: rgba(255,255,255,0.02);
    transition: border-color .15s, background .15s;
  }

  .download-row.active {
    border-color: rgba(27, 217, 106, 0.4);
    background: rgba(27, 217, 106, 0.06);
  }

  .download-row.done {
    border-color: rgba(27, 217, 106, 0.25);
    opacity: 0.85;
  }

  .download-row.failed {
    border-color: rgba(239, 68, 68, 0.4);
    background: rgba(239, 68, 68, 0.06);
  }

  .download-row-top {
    display: flex;
    justify-content: space-between;
    gap: 10px;
    margin-bottom: 8px;
  }

  .download-row-top strong {
    font-size: 13px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .download-status {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-muted);
    flex-shrink: 0;
  }

  .download-row.active .download-status { color: var(--accent-primary); }
  .download-row.failed .download-status { color: #fca5a5; }
  .download-row.done .download-status { color: var(--accent-primary); }

  .download-row-meta {
    display: flex;
    justify-content: space-between;
    margin-top: 6px;
    font-size: 11px;
    color: var(--text-muted);
  }

  .download-modal-actions {
    margin-top: 16px;
    display: flex;
    justify-content: flex-end;
  }

  .update-icon {
    width: 40px;
    height: 40px;
    border-radius: 12px;
    overflow: hidden;
    background: linear-gradient(135deg, var(--accent-secondary), var(--accent-primary));
    display: flex;
    align-items: center;
    justify-content: center;
    color: #fff;
    font-weight: 800;
    flex-shrink: 0;
  }

  .update-icon img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .update-versions {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--text-muted);
    font-size: 12px;
  }

  .ver-old { opacity: 0.7; }
  .ver-new {
    color: var(--accent-primary);
    background: rgba(27, 217, 106, 0.1);
  }

  .update-all-btn {
    background: linear-gradient(135deg, var(--accent-primary), #14b355) !important;
    box-shadow: 0 6px 20px rgba(27, 217, 106, 0.3);
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

  .modal-tabs {
    display: flex;
    gap: 6px;
    padding: 12px 24px 0;
    flex-wrap: wrap;
  }

  .modal-tabs button {
    padding: 8px 14px;
    border-radius: 10px;
    border: 1px solid var(--border-color);
    background: var(--bg-tertiary);
    color: var(--text-secondary);
    font-size: 13px;
    font-weight: 700;
  }

  .modal-tabs button.active {
    border-color: rgba(27, 217, 106, 0.45);
    background: rgba(27, 217, 106, 0.1);
    color: var(--text-primary);
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
    grid-template-columns: 25% minmax(0, 1fr);
    gap: 16px;
    min-height: 650px;
    align-items: start;
  }

  /* ---- Left filter sidebar (accordions) ---- */
  .filter-sidebar {
    position: sticky;
    top: 0;
    max-height: calc(100vh - 170px);
    overflow: auto;
    display: grid;
    gap: 10px;
    padding-right: 4px;
  }

  .filter-block {
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
    background: rgba(255,255,255,0.018);
    overflow: hidden;
  }

  .filter-head {
    width: 100%;
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 14px;
    background: transparent;
    color: var(--text-secondary);
    font-size: 12px;
    font-weight: 800;
    text-transform: uppercase;
    letter-spacing: .06em;
    transform: none;
  }

  .filter-head:hover { color: var(--text-primary); }
  .filter-head :global(svg.rot) { transform: rotate(-90deg); transition: transform .15s; }

  .filter-body {
    display: grid;
    gap: 6px;
    padding: 4px 12px 14px;
  }

  .filter-list {
    display: grid;
    gap: 3px;
    max-height: 280px;
    overflow: auto;
  }

  .filter-list button,
  .loader-row,
  .cat-row {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 8px;
    justify-content: flex-start;
    text-align: left;
    padding: 7px 9px;
    background: transparent;
    color: var(--text-secondary);
    border: 1px solid transparent;
    border-radius: 8px;
    transform: none;
    font-size: 13px;
  }

  .filter-list button:hover,
  .loader-row:hover,
  .cat-row:hover,
  .filter-list button.active,
  .loader-row.active,
  .cat-row.active {
    background: var(--bg-tertiary);
    border-color: rgba(27,217,106,.28);
    color: var(--text-primary);
  }

  .loader-ic { display: inline-flex; color: var(--accent-secondary); }
  .show-more {
    width: 100%;
    text-align: left;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 7px 9px;
    background: transparent;
    color: var(--text-muted);
    border: 1px solid transparent;
    border-radius: 8px;
    transform: none;
    font-size: 12px;
  }
  .show-more:hover { color: var(--text-primary); }

  /* ---- Right content column ---- */
  .browser-results {
    min-width: 0;
    display: grid;
    gap: 14px;
  }

  .browser-topbar {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }

  .browser-topbar .search.wide {
    flex: 1 1 240px;
    min-width: 200px;
  }

  .topbar-controls {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .view-toggle {
    width: 36px; height: 36px;
    padding: 0;
    display: inline-flex; align-items: center; justify-content: center;
    border-radius: 10px;
    background: var(--bg-tertiary);
    border: 1px solid var(--border-color);
    color: var(--text-muted);
    transform: none;
    flex-shrink: 0;
  }
  .view-toggle:hover { color: var(--text-primary); background: var(--bg-elevated); }
  .view-toggle.active { color: var(--accent-primary); border-color: rgba(27,217,106,.4); background: rgba(27,217,106,.08); }
  .view-toggle :global(svg) { width: 16px; height: 16px; flex-shrink: 0; }

  .pagination {
    display: flex;
    align-items: center;
    gap: 4px;
    margin-left: auto;
  }

  .page-btn {
    min-width: 32px; height: 32px;
    padding: 0 8px;
    display: inline-flex; align-items: center; justify-content: center;
    border-radius: 999px;
    background: var(--bg-tertiary);
    border: 1px solid var(--border-color);
    color: var(--text-secondary);
    transform: none;
    font-size: 13px;
  }
  .page-btn:hover:not(:disabled) { color: var(--text-primary); }
  .page-btn.active {
    background: var(--accent-primary);
    color: #fff;
    border-color: transparent;
    font-weight: 800;
  }
  .page-ellipsis { color: var(--text-muted); padding: 0 2px; }

  .bulk-bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
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
    grid-template-columns: repeat(auto-fill, minmax(360px, 1fr));
    gap: 14px;
  }
  .results.list { grid-template-columns: 1fr; }

  .result-card {
    position: relative;
    display: grid;
    grid-template-columns: 64px minmax(0, 1fr);
    grid-template-areas: "icon main" "icon actions" "footer footer";
    gap: 10px 14px;
    align-items: start;
    padding: 16px;
    border-radius: var(--border-radius-lg);
    background: #2d2d2d;
    border: 1px solid var(--border-color);
  }

  .results.list .result-card {
    grid-template-columns: 64px minmax(0,1fr) auto;
    grid-template-areas: "icon main actions" "footer footer footer";
    align-items: center;
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
    grid-area: icon;
    width: 64px;
    height: 64px;
    border-radius: 16px;
    overflow: hidden;
    background: linear-gradient(135deg, var(--accent-secondary), var(--accent-primary));
    display: flex; align-items: center; justify-content: center;
    color: #fff; font-weight: 900; font-size: 22px;
  }
  .result-icon img { width: 100%; height: 100%; object-fit: cover; }

  .result-main { grid-area: main; min-width: 0; }

  .result-title {
    display: flex;
    align-items: baseline;
    flex-wrap: wrap;
    gap: 8px;
  }
  .result-name { color: var(--text-primary); font-weight: 800; font-size: 15px; }
  .result-author { color: #60a5fa; font-size: 12px; cursor: pointer; }
  .result-author:hover { text-decoration: underline; }
  .result-desc {
    margin: 6px 0 0;
    color: var(--text-muted);
    font-size: 13px;
    line-height: 1.45;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .result-badges { display: flex; flex-wrap: wrap; gap: 6px; margin-top: 10px; }
  .badge {
    display: inline-flex; align-items: center; gap: 4px;
    padding: 3px 9px;
    border-radius: 999px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-color);
    color: var(--text-muted);
    font-size: 11px;
    font-weight: 600;
  }

  .result-actions {
    grid-area: actions;
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }
  .download-btn {
    display: inline-flex; align-items: center; gap: 8px;
    padding: 9px 16px;
    border-radius: 10px;
    background: var(--accent-primary);
    color: #fff;
    font-weight: 800;
    border: none;
    transform: none;
  }
  .download-btn:hover:not(:disabled) { filter: brightness(1.08); }
  .download-btn:disabled { opacity: .5; cursor: default; }

  .quick-actions { display: flex; gap: 6px; align-items: center; }
  .qa {
    width: 34px; height: 34px;
    padding: 0;
    display: inline-flex; align-items: center; justify-content: center;
    border-radius: 999px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-color);
    color: var(--text-muted);
    transform: none;
    flex-shrink: 0;
  }
  .qa:hover { color: var(--text-primary); border-color: rgba(27,217,106,.35); background: var(--bg-hover); }
  .qa.active { color: var(--accent-primary); border-color: rgba(27,217,106,.5); background: rgba(27,217,106,.12); }
  .qa :global(svg) { width: 15px; height: 15px; flex-shrink: 0; }

  .save-wrapper { position: relative; }
  .save-dropdown {
    position: absolute; right: 0; top: 100%; margin-top: 4px; z-index: 100;
    min-width: 220px; max-height: 320px; overflow: auto;
    background: var(--bg-card); border: 1px solid var(--border-color);
    border-radius: 10px; box-shadow: 0 8px 24px rgba(0,0,0,0.4);
    padding: 6px; display: flex; flex-direction: column; gap: 2px;
  }
  .save-dropdown-header { padding: 6px 10px; font-size: 11px; font-weight: 700; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.5px; }
  .save-dropdown-item {
    display: flex; align-items: center; gap: 8px; width: 100%;
    padding: 8px 10px; border-radius: 6px; background: transparent; border: none;
    color: var(--text-primary); text-align: left; font-size: 13px; cursor: pointer;
  }
  .save-dropdown-item:hover { background: rgba(27,217,106,0.08); }
  .save-check { width: 16px; text-align: center; color: var(--accent-primary); font-weight: 700; }
  .save-dropdown-new { display: flex; gap: 4px; padding: 6px 4px 2px; border-top: 1px solid var(--border-color); margin-top: 4px; }
  .save-dropdown-new input { flex: 1; min-width: 0; padding: 6px 8px; border-radius: 6px; border: 1px solid var(--border-color); background: var(--bg-tertiary); color: var(--text-primary); font-size: 12px; }
  .save-dropdown-new button { padding: 6px 10px; border-radius: 6px; background: var(--accent-primary); color: #0a0d14; border: none; font-size: 12px; font-weight: 600; cursor: pointer; }
  .save-dropdown-new button:disabled { opacity: 0.4; cursor: not-allowed; }

  .result-footer {
    grid-area: footer;
    display: flex;
    align-items: center;
    gap: 16px;
    color: var(--text-muted);
    font-size: 12px;
    padding-top: 10px;
    border-top: 1px solid var(--border-color);
  }
  .result-footer span { display: inline-flex; align-items: center; gap: 5px; }
  .result-footer .footer-updated { margin-left: auto; }
  .result-footer :global(svg) { width: 13px; height: 13px; flex-shrink: 0; }

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
  .install-plan-panel .dep-list { margin-top: 10px; }
  .install-plan-panel .dep-list h4 { color: var(--text-secondary); font-size: 12px; text-transform: uppercase; letter-spacing: .08em; margin: 0 0 6px; }
  .install-plan-panel .dep-entry { display: flex; align-items: center; gap: 8px; padding: 5px 8px; border-radius: 6px; background: var(--bg-tertiary); margin-bottom: 4px; }
  .install-plan-panel .dep-entry.required { border-left: 3px solid var(--accent-primary); }
  .install-plan-panel .dep-entry.optional { border-left: 3px solid rgba(161,161,170,.4); }
  .install-plan-panel .dep-target { font-family: ui-monospace,monospace; font-size: 12px; }
  .install-plan-panel .dep-entry small { color: var(--text-muted); font-size: 11px; }
  .install-plan-panel .checkbox-row { display: flex; align-items: center; gap: 8px; margin-top: 10px; padding: 8px 10px; border-radius: 8px; background: var(--bg-tertiary); cursor: pointer; }
  .install-plan-panel .checkbox-row span { font-size: 13px; color: var(--text-primary); }
  .plan-deps { margin-top: 8px; max-height: 80px; overflow: auto; }
  .conflict-warning { margin-top: 10px; padding: 10px; border: 1px solid rgba(239,68,68,.32); border-radius: 12px; background: rgba(239,68,68,.08); display: grid; gap: 6px; }
  .conflict-warning strong { color: #fecaca; }
  .conflict-warning span { color: var(--text-muted); font-size: 12px; }
  .dep-node { position: relative; display: flex; gap: 8px; align-items: center; margin-left: 14px; padding-left: 14px; color: var(--text-muted); font-size: 12px; }
  .dep-node::before { content: ""; position: absolute; left: 0; top: -6px; bottom: 50%; width: 10px; border-left: 1px solid rgba(27,217,106,.35); border-bottom: 1px solid rgba(27,217,106,.35); }

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

  .lists-panel { margin-bottom: 16px; padding: 14px; border: 1px solid rgba(245,166,35,.25); border-radius: var(--border-radius-lg); background: rgba(245,166,35,.02); }
  .lists-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 10px; }
  .lists-header h3 { display: flex; align-items: center; gap: 8px; color: var(--accent-warning, #f5a623); margin: 0; font-size: 14px; }
  .lists-empty { color: var(--text-muted); font-size: 12px; margin: 4px 0; font-style: italic; }
  .lists-list { display: grid; gap: 6px; }
  .list-row { display: flex; justify-content: space-between; align-items: center; gap: 8px; padding: 8px 10px; border-radius: 10px; background: var(--bg-tertiary); border: 1px solid var(--border-color); }
  .list-row.active { border-color: rgba(245,166,35,.5); background: rgba(245,166,35,.04); }
  .list-toggle { display: flex; align-items: center; gap: 8px; flex: 1; min-width: 0; background: transparent; border: none; color: var(--text-primary); text-align: left; cursor: pointer; padding: 0; }
  .list-toggle strong { font-size: 13px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .list-count { font-size: 11px; color: var(--text-muted); background: var(--bg-elevated); padding: 2px 8px; border-radius: 999px; }
  .list-actions { display: flex; gap: 4px; flex-shrink: 0; }
  .list-detail { display: grid; gap: 4px; padding: 8px 10px 4px 26px; border-left: 2px solid rgba(245,166,35,.3); margin-left: 14px; }
  .list-mod-row { display: flex; justify-content: space-between; align-items: center; gap: 6px; padding: 4px 6px; border-radius: 6px; background: var(--bg-secondary); }
  .list-mod-id { font-family: monospace; font-size: 11px; color: var(--text-secondary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; flex: 1; }
  .icon-btn.mini { padding: 4px; border-radius: 6px; }
  .icon-btn.mini.danger:hover { background: rgba(239,68,68,.15); color: #fca5a5; }

  .update-panel {
    margin-bottom: 16px;
    padding: 18px;
    border: 1px solid rgba(27,217,106,.35);
    border-radius: 18px;
    background:
      radial-gradient(circle at top right, rgba(27,217,106,.14), transparent 42%),
      radial-gradient(circle at bottom left, rgba(139,92,246,.08), transparent 40%),
      var(--bg-secondary);
    animation: hero-in 0.35s ease both;
  }
  .update-panel-header { display: flex; justify-content: space-between; align-items: center; margin-bottom: 12px; gap: 12px; }
  .update-panel-header h3 { display: flex; align-items: center; gap: 8px; color: var(--accent-primary); margin: 0; font-size: 16px; font-weight: 800; }
  .update-list { display: grid; gap: 8px; max-height: 280px; overflow: auto; }
  .update-row {
    display: grid;
    grid-template-columns: 40px minmax(0, 1fr) auto;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    border-radius: 12px;
    background: rgba(0,0,0,0.2);
    border: 1px solid var(--border-color);
    transition: border-color .15s, transform .15s;
  }
  .update-row:hover {
    border-color: rgba(27,217,106,.35);
    transform: translateX(2px);
  }
  .update-main { display: grid; gap: 4px; min-width: 0; }
  .update-main strong { color: var(--text-primary); font-size: 13px; }
  .update-main span { color: var(--text-muted); font-size: 12px; }
  .update-main code { font-size: 12px; font-weight: 700; }

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

  .version-modal { max-width: 600px; }
  .version-list { display: grid; gap: 6px; max-height: 70vh; overflow: auto; padding: 8px 0; }
  .version-row {
    display: flex; align-items: center; justify-content: space-between; gap: 12px;
    padding: 12px 14px; border-radius: 12px; border: 1px solid var(--border-color);
    background: var(--bg-tertiary); color: var(--text-secondary); text-align: left;
    width: 100%; transform: none;
  }
  .version-row:hover, .version-row.current { border-color: rgba(27,217,106,.35); background: rgba(27,217,106,.06); }
  .version-row:disabled { opacity: .5; cursor: wait; }
  .version-main { display: grid; gap: 3px; min-width: 0; flex: 1; }
  .version-main strong { color: var(--text-primary); }
  .version-name { color: var(--text-secondary); font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .version-loaders { color: var(--text-muted); font-size: 12px; }
  .version-changelog { color: var(--text-muted); font-size: 11px; line-height: 1.4; display: -webkit-box; -webkit-line-clamp: 2; line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden; }
  .current-badge { font-size: 11px; font-weight: 800; color: var(--accent-primary); background: rgba(27,217,106,.15); padding: 4px 10px; border-radius: 999px; flex-shrink: 0; }
  .install-badge { font-size: 11px; font-weight: 700; color: var(--accent-secondary); background: rgba(139,92,246,.12); padding: 4px 10px; border-radius: 999px; flex-shrink: 0; }

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
