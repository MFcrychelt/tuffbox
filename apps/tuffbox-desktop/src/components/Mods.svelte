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
    Power,
    PowerOff,
    ExternalLink,
    Users,
  } from "lucide-svelte";
  import { projectPath, projectInfo } from "../lib/store";
  import EmptyState from "./EmptyState.svelte";
  import CatalogProjectView from "./CatalogProjectView.svelte";
  import { toasts } from "../lib/toast";
import { confirm, open } from "@tauri-apps/plugin-dialog";
import { open as openExternal } from "@tauri-apps/plugin-shell";
import PromptDialog from "./PromptDialog.svelte";
import ConfirmDialog from "./ConfirmDialog.svelte";
import { trapFocus } from "../lib/focusTrap";

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
    disabled?: boolean;
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
    provider?: string;
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
    error?: string | null;
  };

  type ModUpdateProgress = {
    phase: string;
    message: string;
    current: number;
    total: number;
    percent: number;
    modId?: string | null;
  };

  type DownloadBatch = {
    phase: string;
    items?: DownloadItem[];
    downloaded?: string[];
    failed?: { modId: string; error: string }[];
    alreadyPresent?: string[];
    skipped?: string[];
    scopeModIds?: string[];
    batchComplete?: boolean;
  };

  let mods: ModRow[] = [];
  let loading = false;
  let mutating = false;
  let steamBridgeInstalling = false;
  let filter = "";
  let sideFilter = "all";
  let contentFilter = "mod"; // mod, resourcepack, datapack, shader, favorites, list:<name>
  let error: string | null = null;
  let lastLoadedPath: string | null = null;
  let brokenIcons: string[] = [];
  let savedMods: SearchResult[] = [];
  let savedModsLoading = false;
  let renameTarget = "";
  let showRenamePrompt = false;
  let deleteTarget = "";
  let showDeleteConfirm = false;

  // Download progress overlay
  let downloadOpen = false;
  let downloadTitle = "Downloading content";
  let downloadItems: DownloadItem[] = [];
  let downloadDone = false;
  let unlistenProgress: UnlistenFn | null = null;
  let unlistenBatch: UnlistenFn | null = null;
  let unlistenUpdateProgress: UnlistenFn | null = null;
  let downloadScopeModIds: Set<string> | null = null;
  let downloadStageMessage = "Preparing downloads…";
  let downloadStagePercent = 0;
  let downloadError: string | null = null;

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
      downloadItems = downloadItems.map((item, itemIdx) =>
        itemIdx === idx ? { ...item, ...payload } : item
      );
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

  function openDownloadOverlay(title: string, scopeModIds: string[] | null = null) {
    downloadTitle = title;
    downloadItems = [];
    downloadDone = false;
    downloadScopeModIds = scopeModIds?.length ? new Set(scopeModIds) : null;
    downloadStageMessage = "Preparing downloads…";
    downloadStagePercent = 0;
    downloadError = null;
    downloadOpen = true;
  }

  function closeDownloadOverlay() {
    if (!downloadDone && downloadActiveCount > 0) return;
    downloadOpen = false;
  }

  async function retryFailedDownloads() {
    if (!$projectPath) return;
    const failedIds = downloadItems.filter((i) => i.status === "failed").map((i) => i.id);
    if (failedIds.length === 0) return;
    downloadDone = false;
    downloadTitle = `Retrying ${failedIds.length} failed download${failedIds.length > 1 ? "s" : ""}`;
    downloadScopeModIds = new Set(failedIds);
    downloadStageMessage = "Retrying failed downloads…";
    downloadStagePercent = 0;
    downloadError = null;
    downloadItems = downloadItems.map((item) =>
      failedIds.includes(item.id)
        ? { ...item, status: "queued", percent: 0, downloaded: 0, total: 0 }
        : item
    );
    try {
      const result: any = await invoke("retry_failed_mod_downloads", {
        path: $projectPath,
        modIds: failedIds,
      });
      const stillFailed = result?.download?.failed?.length ?? 0;
      if (stillFailed === 0) {
        message = "Retry succeeded — all files downloaded.";
      } else {
        error = `${stillFailed} download(s) still failed.`;
      }
    } catch (e) {
      downloadError = String(e);
      error = downloadError;
    } finally {
      downloadDone = true;
    }
  }

  async function retrySingleDownload(modId: string) {
    if (!$projectPath) return;
    downloadDone = false;
    downloadScopeModIds = new Set([modId]);
    downloadStageMessage = "Retrying download…";
    downloadStagePercent = 0;
    downloadError = null;
    downloadItems = downloadItems.map((item) =>
      item.id === modId
        ? { ...item, status: "queued", percent: 0, downloaded: 0, total: 0 }
        : item
    );
    try {
      await invoke("retry_failed_mod_downloads", {
        path: $projectPath,
        modIds: [modId],
      });
    } catch (e) {
      downloadError = String(e);
      error = downloadError;
    } finally {
      downloadDone = true;
    }
  }

  onMount(() => {
    const unsub = projectPath.subscribe((path) => {
      if (path && lastLoadedPath !== path) {
        void load(true);
      } else if (!path) {
        mods = [];
        lastLoadedPath = null;
        clearSelection();
      }
    });
    return unsub;
  });

  onMount(async () => {
    unlistenBatch = await listen<DownloadBatch>(
      "mod-download-batch",
      (event) => {
        const payload = event.payload;
        if (payload.phase === "start") {
          downloadOpen = true;
          downloadDone = false;
          downloadStageMessage = "Preparing downloads…";
          downloadStagePercent = 0;
          const scoped = payload.scopeModIds?.length ? new Set(payload.scopeModIds) : null;
          if (scoped) {
            downloadScopeModIds = scoped;
          }
          downloadItems = (payload.items ?? []).map((item) => ({
            id: item.id,
            name: item.name,
            downloaded: 0,
            total: 0,
            percent: 0,
            status: "queued",
          }));
        } else if (payload.phase === "done") {
          const downloadedIds = new Set(payload.downloaded ?? []);
          const alreadyPresentIds = new Set(payload.alreadyPresent ?? []);
          const skippedIds = new Set(payload.skipped ?? []);
          const failedIds = new Set((payload.failed ?? []).map((failure) => failure.modId));
          const failureById = new Map((payload.failed ?? []).map((failure) => [failure.modId, failure.error]));
          const successfulIds = new Set([...downloadedIds, ...alreadyPresentIds, ...skippedIds]);

          downloadItems = downloadItems.map((item) => {
            if (skippedIds.has(item.id)) {
              return { ...item, status: "skipped", percent: 100 };
            }
            if (downloadedIds.has(item.id) || alreadyPresentIds.has(item.id)) {
              return { ...item, status: "done", percent: 100 };
            }
            if (
              failedIds.has(item.id) ||
              ((item.status === "queued" || item.status === "downloading") && !successfulIds.has(item.id))
            ) {
              return {
                ...item,
                status: "failed",
                percent: 0,
                error: failureById.get(item.id) ?? "The download did not complete.",
              };
            }
            return item;
          });

          if (payload.batchComplete !== false) {
            downloadDone = true;
            downloadStagePercent = 100;
            const failed = downloadItems.filter((item) => item.status === "failed").length;
            downloadStageMessage = failed > 0
              ? `Downloads finished with ${failed} failure${failed > 1 ? "s" : ""}.`
              : "Downloads complete.";
            downloadError = failed > 0
              ? (payload.failed ?? []).map((failure) => `${failure.modId}: ${failure.error}`).join("\n")
              : null;
            if (failed === 0) {
              setTimeout(() => {
                if (downloadDone) downloadOpen = false;
              }, 900);
            }
          }
        }
      },
    );

    unlistenUpdateProgress = await listen<ModUpdateProgress>("mod-update-progress", (event) => {
      const payload = event.payload;
      downloadStageMessage = payload.message;
      downloadStagePercent = Math.max(0, Math.min(100, payload.percent));
      if (!downloadOpen) {
        downloadOpen = true;
        downloadDone = payload.phase === "done";
      }
    });

    unlistenProgress = await listen<DownloadItem>("mod-download-progress", (event) => {
      if (downloadScopeModIds && !downloadScopeModIds.has(event.payload.id)) {
        return;
      }
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
    unlistenUpdateProgress?.();
    if (searchDebounceTimer) clearTimeout(searchDebounceTimer);
  });

  let addOpen = false;
  /** In-launcher catalog page (GDLauncher-style) opened from a search card. */
  let catalogViewResult: SearchResult | null = null;
  let catalogProvider: "modrinth" | "curseforge" | "both" = "modrinth";
  let searchQuery = "";
  let searchResults: SearchResult[] = [];
  let searchTotal = 0;
  let searchLoading = false;
  let searchRequestId = 0;
  let searchDebounceTimer: ReturnType<typeof setTimeout> | null = null;
  let selectedSide = "auto";
  let pendingInstallOptional = true;
  let filterGameVersion = "";
  let filterLoader = "fabric";
  let filterCategory = "";
  let filterEnvironment = "";
  let filterLicense = "";
  let sortBy = "relevance";
  let cfSortField = 2;
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
    cfSort: true,
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
      provider: (p.provider as string | undefined) ?? undefined,
    };
  }

  function isSavedViewFilter(filter: string): boolean {
    return filter === "favorites" || filter.startsWith("list:");
  }

  function canUpdateMod(mod: ModRow): boolean {
    return mod.source === "modrinth" && !!mod.updateAvailable;
  }

  function canChangeVersion(mod: ModRow): boolean {
    return mod.source === "modrinth" && !!mod.projectId;
  }

  function isCurseForgeResult(result: SearchResult | null | undefined): boolean {
    return result?.provider === "curseforge" || catalogProvider === "curseforge";
  }

  function setCatalogProvider(provider: "modrinth" | "curseforge" | "both") {
    if (catalogProvider === provider) return;
    catalogProvider = provider;
    searchQuery = "";
    searchResults = [];
    searchTotal = 0;
    selectedResultIds = {};
    pendingInstall = null;
    pendingInstallOptional = provider !== "curseforge";
    searchMods(1);
  }

  function savedViewLabel(filter: string): string {
    if (filter === "favorites") return "Favorites";
    if (filter.startsWith("list:")) return filter.slice(5);
    return "Saved";
  }

  function modIconLookupKey(mod: ModRow): string | null {
    if (mod.source === "curseforge") return null;
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
        brokenIcons = brokenIcons.filter((id) => id !== mod.id);
      }
    } catch {
      // keep letter-avatar fallback
    }
  }

  async function handleIconError(mod: ModRow) {
    if (!brokenIcons.includes(mod.id)) {
      brokenIcons = [...brokenIcons, mod.id];
    }
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

  $: totalPages = Math.max(1, Math.ceil(searchTotal / pageSize));
  $: pagedResults = searchResults.slice((page - 1) * pageSize, page * pageSize);
  $: if (page > totalPages) page = totalPages;

  function goToPage(p: number) {
    const target = Math.min(totalPages, Math.max(1, p));
    if (target === page && searchResults.length > 0) return;
    searchMods(target);
  }

  let previews: Record<string, InstallPreview | null> = {};
  let pendingInstall: SearchResult | null = null;
  let selectedResultIds: Record<string, boolean> = {};

  // --- Version picker (change mod version) ---
  type ModVersion = {
    id: string;
    versionNumber: string;
    gameVersions: string[];
    loaders: string[];
    name?: string;
    changelog?: string;
    datePublished?: string;
    versionType?: string;
    compatible?: boolean;
    compatibleMinecraft?: boolean;
    compatibleLoader?: boolean;
  };
  let versionPickerMod: ModRow | null = null;
  let availableVersions: ModVersion[] = [];
  let versionPickerLoading = false;
  let versionPickerError: string | null = null;
  let versionPickerChanging = false;
  let versionPickerQuery = "";
  let hideIncompatible = true;
  let selectedVersion: ModVersion | null = null;
  let versionPickerMc = "";
  let versionPickerLoader = "";

  $: versionPickerFiltered = availableVersions.filter((v) => {
    if (hideIncompatible && v.compatible === false && v.versionNumber !== versionPickerMod?.version) {
      return false;
    }
    const q = versionPickerQuery.trim().toLowerCase();
    if (!q) return true;
    return (
      v.versionNumber.toLowerCase().includes(q) ||
      (v.name ?? "").toLowerCase().includes(q) ||
      v.gameVersions.some((gv) => gv.toLowerCase().includes(q)) ||
      v.loaders.some((l) => l.toLowerCase().includes(q)) ||
      (v.versionType ?? "").toLowerCase().includes(q)
    );
  });

  $: compatibleVersionCount = availableVersions.filter((v) => v.compatible !== false).length;

  async function openVersionPicker(mod: ModRow) {
    if (!$projectPath) return;
    if (!mod.projectId) {
      toasts.error("Cannot change version: missing Modrinth project ID");
      return;
    }
    versionPickerMod = mod;
    versionPickerLoading = true;
    versionPickerError = null;
    availableVersions = [];
    selectedVersion = null;
    versionPickerQuery = "";
    hideIncompatible = true;
    try {
      const info: any = $projectInfo ?? await invoke("validate_project", { path: $projectPath });
      versionPickerMc = info.minecraftVersion ?? "";
      versionPickerLoader = (info.loaderKind ?? "").toLowerCase();
      availableVersions = await invoke("get_mod_versions", {
        modId: mod.projectId,
        minecraftVersion: versionPickerMc,
        loader: versionPickerLoader || null,
      });
      selectedVersion =
        availableVersions.find((v) => v.versionNumber === mod.version) ??
        availableVersions.find((v) => v.compatible !== false) ??
        availableVersions[0] ??
        null;
    } catch (e) {
      versionPickerError = String(e);
    } finally {
      versionPickerLoading = false;
    }
  }

  async function changeVersion(versionId: string) {
    if (!$projectPath || !versionPickerMod) return;
    const target = availableVersions.find((v) => v.id === versionId);
    if (target && target.compatible === false) {
      const ok = confirm(
        `Version ${target.versionNumber} is not marked compatible with ${versionPickerLoader} ${versionPickerMc}. Install anyway?`
      );
      if (!ok) return;
    }
    versionPickerChanging = true;
    versionPickerError = null;
    const targetModId = versionPickerMod.id;
    openDownloadOverlay(`Switching ${versionPickerMod.name}`);
    try {
      await invoke("change_mod_version", {
        path: $projectPath,
        modId: versionPickerMod.id,
        newVersionId: versionId,
      });
      versionPickerMod = null;
      availableVersions = [];
      selectedVersion = null;
      await refreshSingleMod(targetModId);
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
    const target = confirmMod;
    confirmOpen = false;
    mutating = true;
    error = null;
    // Optimistic local remove so the list stays interactive (no full reload).
    removeModLocally(target.id);
    try {
      await invoke("remove_project_mod", { path: $projectPath, modId: target.id });
      confirmMod = null;
    } catch (e) {
      error = `Failed to remove ${target.name}: ${String(e)}`;
      confirmMod = null;
      await reloadModsSilent();
    } finally {
      mutating = false;
    }
  }

  // Mod recommendations
  let recommendations: any[] = [];
  let recsLoading = false;
  let recsError: string | null = null;

  $: hasSteamBridge = mods.some((m) => {
    const id = (m.id ?? "").toLowerCase();
    const file = (m.fileName ?? "").toLowerCase();
    return (
      id === "steambridge" ||
      id === "steam-bridge" ||
      id === "steam_bridge" ||
      file.startsWith("steambridge")
    );
  });

  async function installSteamBridge() {
    if (!$projectPath || steamBridgeInstalling || hasSteamBridge) return;
    steamBridgeInstalling = true;
    error = null;
    try {
      const result = await api.mods.installSteamBridge($projectPath);
      const note =
        result.matchKind && !result.matchKind.includes("exact")
          ? ` (${result.matchKind})`
          : "";
      toasts.success(
        `Steam Bridge ${result.tag} installed for ${result.loader} ${result.mcVersion}${note}. Host a LAN world — friends join via Steam.`,
        10000,
        [
          {
            label: "Repo",
            run: () => {
              void openExternal(`https://github.com/${result.repo}`);
            },
          },
        ],
      );
      await load(true);
    } catch (e) {
      const msg = String(e);
      error = msg;
      toasts.error(msg, 10000);
    } finally {
      steamBridgeInstalling = false;
    }
  }

  async function loadRecommendations() {
    if (!$projectPath) return;
    recsLoading = true;
    recsError = null;
    try {
      recommendations = await api.mods.recommend($projectPath);
      if (!recommendations.length) {
        message = "No missing optimization suggestions for this loader/version.";
      }
    } catch (e) {
      recommendations = [];
      recsError = String(e);
      error = `Suggestions failed: ${String(e)}`;
    } finally {
      recsLoading = false;
    }
  }

  // Multi-select (right-click enters selection mode; then LMB/RMB toggle)
  let selectionMode = false;
  let selectedModIds: Record<string, boolean> = {};

  $: selectedMods = filtered.filter((m) => selectedModIds[m.id]);
  $: selectedCount = selectedMods.length;

  function clearSelection() {
    selectionMode = false;
    selectedModIds = {};
  }

  function toggleModSelected(modId: string) {
    selectedModIds = { ...selectedModIds, [modId]: !selectedModIds[modId] };
    if (!Object.values(selectedModIds).some(Boolean)) {
      selectionMode = false;
      selectedModIds = {};
    }
  }

  function onCardContextMenu(e: MouseEvent, mod: ModRow) {
    e.preventDefault();
    e.stopPropagation();
    if (!selectionMode) {
      selectionMode = true;
      selectedModIds = { [mod.id]: true };
      return;
    }
    toggleModSelected(mod.id);
  }

  function onCardClick(e: MouseEvent, mod: ModRow) {
    if (!selectionMode) return;
    // Ignore clicks on action buttons — they use stopPropagation.
    const target = e.target as HTMLElement | null;
    if (target?.closest("button")) return;
    e.preventDefault();
    toggleModSelected(mod.id);
  }

  async function toggleDisabled(mod: ModRow) {
    if (!$projectPath || mutating) return;
    mutating = true;
    error = null;
    try {
      if (mod.disabled) {
        await api.mods.enable(mod.id, $projectPath);
      } else {
        await api.mods.disable(mod.id, $projectPath);
      }
      await refreshSingleMod(mod.id);
      // Ensure disabled flag is reflected even if list_mods is briefly stale.
      mods = mods.map((m) =>
        m.id === mod.id ? { ...m, disabled: !mod.disabled } : m
      );
    } catch (e) {
      error = String(e);
      await reloadModsSilent();
    } finally {
      mutating = false;
    }
  }

  async function bulkDisableSelected() {
    if (!$projectPath || selectedMods.length === 0) return;
    mutating = true;
    error = null;
    const targets = selectedMods.filter((m) => !m.disabled);
    try {
      for (const mod of targets) {
        await api.mods.disable(mod.id, $projectPath);
      }
      await reloadModsSilent();
      message = `Disabled ${targets.length} item${targets.length === 1 ? "" : "s"}.`;
      clearSelection();
    } catch (e) {
      error = String(e);
      await reloadModsSilent();
    } finally {
      mutating = false;
    }
  }

  async function bulkEnableSelected() {
    if (!$projectPath || selectedMods.length === 0) return;
    mutating = true;
    error = null;
    const targets = selectedMods.filter((m) => m.disabled);
    try {
      for (const mod of targets) {
        await api.mods.enable(mod.id, $projectPath);
      }
      await reloadModsSilent();
      message = `Enabled ${targets.length} item${targets.length === 1 ? "" : "s"}.`;
      clearSelection();
    } catch (e) {
      error = String(e);
      await reloadModsSilent();
    } finally {
      mutating = false;
    }
  }

  async function bulkDeleteSelected() {
    if (!$projectPath || selectedMods.length === 0) return;
    const ok = await confirm(
      `Remove ${selectedMods.length} selected item${selectedMods.length === 1 ? "" : "s"}? A snapshot is taken first.`
    );
    if (!ok) return;
    mutating = true;
    error = null;
    const ids = selectedMods.map((m) => m.id);
    try {
      for (const id of ids) {
        removeModLocally(id);
        await invoke("remove_project_mod", { path: $projectPath, modId: id });
      }
      message = `Removed ${ids.length} item${ids.length === 1 ? "" : "s"}.`;
      clearSelection();
    } catch (e) {
      error = String(e);
      await reloadModsSilent();
    } finally {
      mutating = false;
    }
  }

  async function bulkUpdateSelected() {
    if (!$projectPath || selectedMods.length === 0) return;
    const updatable = selectedMods.filter((m) => m.updateAvailable && canUpdateMod(m));
    if (updatable.length === 0) {
      message = "No selected items have updates available.";
      return;
    }
    mutating = true;
    error = null;
    openDownloadOverlay(`Updating ${updatable.length} selected`, updatable.map((m) => m.id));
    try {
      for (const mod of updatable) {
        await invoke("update_project_mod", { path: $projectPath, modId: mod.id, versionId: null });
      }
      await reloadModsSilent();
      message = `Updated ${updatable.length} item${updatable.length === 1 ? "" : "s"}.`;
      clearSelection();
    } catch (e) {
      error = String(e);
      downloadDone = true;
      await reloadModsSilent();
    } finally {
      mutating = false;
      downloadDone = true;
    }
  }

  // Batch update state (no separate update panel — badges + toolbar only)
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
    if (!$projectPath) return;
    updateApplying = true;
    error = null;
    message = null;
    if (updateList.length === 0) {
      await checkForUpdates();
    }
    if (updateList.length === 0) {
      message = "All mods are up to date for this Minecraft version.";
      updateApplying = false;
      return;
    }
    const pendingIds = updateList.map((u) => u.modId);
    openDownloadOverlay(
      `Updating ${updateList.length} mod${updateList.length > 1 ? "s" : ""}`,
      pendingIds
    );
    try {
      const result: any = await invoke("update_all_mods", { path: $projectPath });
      const updated: string[] = Array.isArray(result) ? result : (result?.updated ?? []);
      const errs: string[] = result?.errors ?? [];
      const failedDownloads = result?.download?.failed?.length ?? 0;
      message = updated.length
        ? `Updated ${updated.length} mod${updated.length > 1 ? "s" : ""}: ${updated.join(", ")}`
        : "No mods were updated.";
      if (errs.length) {
        error = errs.slice(0, 3).join("; ");
      } else if (failedDownloads > 0) {
        error = `${failedDownloads} download(s) failed — check the progress window.`;
      }
      updateList = [];
      await reloadModsSilent();
    } catch (e) {
      error = String(e);
      downloadStageMessage = "Update failed.";
      downloadDone = true;
    } finally {
      updateApplying = false;
      downloadDone = true;
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
      await reloadModsSilent();
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
      if (isSavedViewFilter(contentFilter)) {
        await loadSavedModsView();
      }
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

  function modrinthTypePath(projectType?: string | null): string {
    const t = (projectType || "mod").toLowerCase();
    if (t === "resourcepack" || t === "resourcepacks") return "resourcepack";
    if (t === "datapack" || t === "datapacks") return "datapack";
    if (t === "shader" || t === "shaders" || t === "shaderpack") return "shader";
    if (t === "modpack" || t === "modpacks") return "modpack";
    return "mod";
  }

  function curseforgeTypePath(projectType?: string | null): string {
    const t = (projectType || "mod").toLowerCase();
    if (t === "resourcepack" || t === "resourcepacks") return "texture-packs";
    if (t === "datapack" || t === "datapacks") return "data-packs";
    if (t === "shader" || t === "shaders" || t === "shaderpack") return "shaders";
    if (t === "modpack" || t === "modpacks") return "modpacks";
    return "mc-mods";
  }

  /** Public catalog page for a search hit (Modrinth or CurseForge). */
  function projectPageUrl(result: SearchResult): string {
    const slugOrId = (result.slug || result.id || "").trim();
    if (!slugOrId) return "";
    if (isCurseForgeResult(result)) {
      // Prefer slug path; numeric id still works via /projects/{id} fallback.
      if (/^\d+$/.test(slugOrId) && (!result.slug || result.slug === result.id)) {
        return `https://www.curseforge.com/projects/${slugOrId}`;
      }
      return `https://www.curseforge.com/minecraft/${curseforgeTypePath(result.projectType)}/${slugOrId}`;
    }
    return `https://modrinth.com/${modrinthTypePath(result.projectType)}/${slugOrId}`;
  }

  function installedModPageUrl(mod: ModRow): string | null {
    const source = (mod.source || "").toLowerCase();
    if (source === "modrinth") {
      const id = (mod.projectId || mod.id || "").trim();
      if (!id) return null;
      return `https://modrinth.com/${modrinthTypePath(mod.contentType)}/${id}`;
    }
    if (source === "curseforge") {
      const id = (mod.projectId || "").trim() || (mod.id || "").trim();
      if (!id) return null;
      if (/^\d+$/.test(id)) return `https://www.curseforge.com/projects/${id}`;
      return `https://www.curseforge.com/minecraft/${curseforgeTypePath(mod.contentType)}/${id}`;
    }
    return null;
  }

  async function openExternalUrl(url: string) {
    if (!url) return;
    try {
      await openExternal(url);
    } catch (e) {
      toasts.error(`Could not open link: ${e}`);
    }
  }

  async function openProjectPage(result: SearchResult) {
    const url = projectPageUrl(result);
    if (!url) {
      toasts.error("No catalog page for this project.");
      return;
    }
    await openExternalUrl(url);
  }

  function openCatalogInApp(result: SearchResult) {
    catalogViewResult = result;
  }

  function closeCatalogInApp() {
    catalogViewResult = null;
  }

  async function openInstalledModPage(mod: ModRow) {
    const url = installedModPageUrl(mod);
    if (!url) {
      toasts.error("This mod has no Modrinth/CurseForge page.");
      return;
    }
    await openExternalUrl(url);
  }

  async function copyProjectLink(result: SearchResult) {
    const url = projectPageUrl(result);
    if (!url) return;
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
      await refreshSavedViewIfActive();
    } catch (e) {
      error = String(e);
    }
  }

  async function addToList(listName: string, modId: string) {
    if (!$projectPath) return;
    try {
      userState = await api.mods.addToList(listName, modId, $projectPath);
      if (contentFilter === `list:${listName}`) {
        await loadSavedModsView();
      }
    } catch (e) {
      error = String(e);
    }
  }

  async function removeFromList(listName: string, modId: string) {
    if (!$projectPath) return;
    try {
      userState = await api.mods.removeFromList(listName, modId, $projectPath);
      if (contentFilter === `list:${listName}`) {
        await loadSavedModsView();
      }
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
      await reloadModsSilent();
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
      await reloadModsSilent();
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

  function modsFingerprint(list: ModRow[]): string {
    return list
      .map((m) => `${m.id}|${m.version}|${m.projectId ?? ""}|${m.source}|${m.disabled ? 1 : 0}|${m.fileName ?? ""}`)
      .join(";");
  }

  async function load(force = false) {
    if (!$projectPath) return;
    if (!force && lastLoadedPath === $projectPath && mods.length > 0) return;
    const path = $projectPath;
    const showSpinner = mods.length === 0 || lastLoadedPath !== path;
    if (showSpinner) loading = true;
    error = null;
    try {
      // Fast path: list known mods from disk/index and paint immediately.
      const listed: ModRow[] = await invoke("list_mods", { path });
      if ($projectPath !== path) return;
      mods = listed;
      lastLoadedPath = path;
      brokenIcons = [];
      await loadUserState();
    } catch (e) {
      error = String(e);
      loading = false;
      return;
    }
    // Don't block the spinner on Modrinth indexing / hash lookup.
    loading = false;
    if (isSavedViewFilter(contentFilter)) {
      loadSavedModsView().catch(() => {});
    }
    (async () => {
      try {
        const synced: ModRow[] = await invoke("sync_mods_folder", { path });
        if ($projectPath !== path) return;
        // Don't clobber the list mid Update All — sync can reintroduce leftover jars.
        if (updateApplying) return;
        if (modsFingerprint(synced) !== modsFingerprint(mods)) {
          // Preserve disabled/update flags for rows that only change icon metadata.
          const prev = new Map(mods.map((m) => [m.id, m]));
          mods = synced.map((m) => {
            const old = prev.get(m.id);
            return old
              ? { ...m, updateAvailable: old.updateAvailable, disabled: m.disabled ?? old.disabled }
              : m;
          });
        }
      } catch {
        // Keep the fast list; offline or sync failures shouldn't wipe the UI.
      }
      if ($projectPath !== path) return;
      hydrateMissingIcons().catch(() => {});
      refreshUpdateDots().catch(() => {});
    })();
  }

  // Updates a single installed mod row in place (no full-list spinner), so
  // changing a version or updating one mod doesn't flash the entire list.
  async function refreshSingleMod(modId: string) {
    if (!$projectPath) return;
    try {
      const fresh: ModRow[] = await invoke("list_mods", { path: $projectPath });
      const found = fresh.find((m) => m.id === modId);
      if (found) {
        const existing = mods.find((m) => m.id === modId);
        mods = mods.map((m) =>
          m.id === modId
            ? { ...found, updateAvailable: existing?.updateAvailable && found.version === existing.version }
            : m
        );
      } else {
        mods = mods.filter((m) => m.id !== modId);
        updateList = updateList.filter((u) => u.modId !== modId);
      }
      // Only re-check update flag for this mod — avoid remapping the whole list
      // when check_mod_updates is slow.
      refreshUpdateDotsFor([modId]).catch(() => {});
    } catch {
      await reloadModsSilent();
    }
  }

  /** Fetch installed mods without the full-page "Loading mods..." spinner. */
  async function reloadModsSilent() {
    if (!$projectPath) return;
    const path = $projectPath;
    try {
      const fresh: ModRow[] = await invoke("list_mods", { path });
      if ($projectPath !== path) return;
      const prevFlags = new Map(mods.map((m) => [m.id, m.updateAvailable]));
      mods = fresh.map((m) => ({
        ...m,
        updateAvailable: prevFlags.get(m.id) ?? m.updateAvailable,
      }));
      lastLoadedPath = path;
      brokenIcons = brokenIcons.filter((id) => mods.some((m) => m.id === id));
      hydrateMissingIcons().catch(() => {});
      refreshUpdateDots().catch(() => {});
    } catch (e) {
      error = String(e);
    }
  }

  function removeModLocally(modId: string) {
    mods = mods.filter((m) => m.id !== modId);
    updateList = updateList.filter((u) => u.modId !== modId);
  }

  // Cross-references the latest available Modrinth versions with the installed
  // ones and flags each mod row that has an update pending (drives the dot).
  async function refreshUpdateDots(scopeIds?: string[]) {
    if (!$projectPath) return;
    try {
      const updates: any[] = await invoke("check_mod_updates", { path: $projectPath });
      updateList = updates;
      const ids = new Set(updates.map((u) => u.modId));
      if (scopeIds?.length) {
        const scope = new Set(scopeIds);
        mods = mods.map((m) =>
          scope.has(m.id) ? { ...m, updateAvailable: ids.has(m.id) } : m
        );
      } else {
        mods = mods.map((m) => ({ ...m, updateAvailable: ids.has(m.id) }));
      }
    } catch {
      // leave existing flags in place
    }
  }

  async function refreshUpdateDotsFor(modIds: string[]) {
    return refreshUpdateDots(modIds);
  }

  // Some mods (e.g. local jars with a known Modrinth project id, or entries
  // whose CDN icon failed to resolve) have no iconUrl. Try to fetch a real
  // icon so the list isn't all letter-avatars.
  async function hydrateMissingIcons() {
    if (!$projectPath) return;
    const missing = mods.filter((m) => {
      if (brokenIcons.includes(m.id)) return !!modIconLookupKey(m);
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

  async function refreshSavedViewIfActive() {
    if (isSavedViewFilter(contentFilter)) {
      await loadSavedModsView();
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
    filter = "";
    clearSelection();
    if (addOpen) searchMods(1);
    if (isSavedViewFilter(next)) {
      loadUserState().then(() => loadSavedModsView()).catch(() => {});
    }
  }

  async function openAddModal() {
    addOpen = true;
    catalogViewResult = null;
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
    await searchMods(1);
  }

  async function openProjectFolder() {
    const selected = await open({ directory: true, title: "Select Minecraft instance" });
    if (!selected || typeof selected !== "string") return;
    try {
      const info = await api.project.validate(selected);
      const manifestPath = info.manifestPath || selected;
      projectPath.set(manifestPath);
      projectInfo.set(info);
    } catch (e) {
      toasts.error(String(e));
    }
  }

  function scheduleSearchMods(targetPage: number = 1, delayMs = 280) {
    if (searchDebounceTimer) clearTimeout(searchDebounceTimer);
    searchDebounceTimer = setTimeout(() => {
      searchDebounceTimer = null;
      void searchMods(targetPage);
    }, delayMs);
  }

  function onSearchQueryInput() {
    // Typeahead for Add-mod catalog; Enter still forces an immediate search.
    scheduleSearchMods(1);
  }

  async function searchMods(targetPage: number = 1) {
    if (!$projectPath) return;
    if (searchDebounceTimer) {
      clearTimeout(searchDebounceTimer);
      searchDebounceTimer = null;
    }
    const requestId = ++searchRequestId;
    searchLoading = true;
    error = null;
    try {
      const loader =
        contentFilter === "mod" && filterLoader ? filterLoader.toLowerCase() : null;
      const contentType = contentTypeForFilter(contentFilter);
      const args = {
        path: $projectPath,
        query: searchQuery.trim(),
        gameVersion: filterGameVersion || null,
        loader,
        contentType,
        page: targetPage,
        pageSize,
      };
      let payload: { results: SearchResult[]; total: number };
      if (catalogProvider === "curseforge") {
        payload = await invoke("search_curseforge_mods", {
          ...args,
          sortField: cfSortField,
        });
      } else if (catalogProvider === "both") {
        payload = await invoke("search_unified_mods", {
          ...args,
          sortField: cfSortField,
        });
      } else {
        // Resourcepacks/datapacks/shaders aren't tied to a mod loader on
        // Modrinth; sending a loader facet for them would return zero
        // results, so only apply it for the "mod" tab.
        payload = await invoke("search_modrinth_mods", {
          ...args,
          category: filterCategory || null,
          environment: filterEnvironment || null,
          license: filterLicense || null,
          sort: sortBy,
        });
      }
      if (requestId !== searchRequestId) return;
      searchResults = payload.results.map((r) => ({
        ...r,
        provider: r.provider ?? (catalogProvider === "curseforge" ? "curseforge" : "modrinth"),
      }));
      searchTotal = payload.total;
      page = targetPage;
    } catch (e) {
      if (requestId !== searchRequestId) return;
      error = String(e);
      searchResults = [];
      searchTotal = 0;
    } finally {
      if (requestId === searchRequestId) {
        searchLoading = false;
      }
    }
  }

  async function loadInstallPreview(result: SearchResult) {
    if (!$projectPath) return;
    if (isCurseForgeResult(result)) return;
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
      await reloadModsSilent();
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
    const installTarget = pendingInstall;
    const curseforge = isCurseForgeResult(installTarget);
    mutating = true;
    error = null;
    openDownloadOverlay(
      !curseforge && withDependencies
        ? `Installing ${pendingInstall.name} + deps`
        : `Installing ${pendingInstall.name}`,
    );
    try {
      if (curseforge) {
        await invoke("add_curseforge_mod", {
          path: $projectPath,
          modId: pendingInstall.id,
          side: selectedSide,
        });
      } else if (withDependencies) {
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
      await reloadModsSilent();
    } catch (e) {
      downloadError = String(e);
      error = downloadError;
      downloadStageMessage = `Installation failed for ${installTarget.name}.`;
      upsertDownloadItem({
        id: installTarget.id,
        name: installTarget.name,
        status: "failed",
        percent: 0,
        error: downloadError,
      });
      downloadDone = true;
    } finally {
      mutating = false;
    }
  }

  async function removeMod(mod: ModRow) {
    showRemoveConfirm(mod);
  }

  async function updateMod(mod: ModRow, versionId?: string | null) {
    if (!$projectPath || !canUpdateMod(mod)) return;
    mutating = true;
    error = null;
    message = null;
    openDownloadOverlay(`Updating ${mod.name}`, [mod.id]);
    try {
      let targetVersionId = versionId ?? null;
      if (!targetVersionId) {
        const pending = updateList.find((u) => u.modId === mod.id);
        targetVersionId = pending?.versionId ?? null;
      }
      const result: any = await invoke("update_project_mod", {
        path: $projectPath,
        modId: mod.id,
        versionId: targetVersionId,
      });
      const failures = result?.download?.failed ?? [];
      if (failures.length) {
        throw new Error(failures.map((failure: any) => failure.error).join("; "));
      }
      updateList = updateList.filter((u) => u.modId !== mod.id);
      mods = mods.map((m) => (m.id === mod.id ? { ...m, updateAvailable: false } : m));
      message = `Updated ${mod.name}.`;
      await refreshSingleMod(mod.id);
    } catch (e) {
      error = String(e);
      downloadStageMessage = "Update failed.";
      downloadDone = true;
    } finally {
      mutating = false;
      downloadDone = true;
      downloadScopeModIds = null;
    }
  }

  function modIconUrl(mod: ModRow) {
    return mod.iconUrl;
  }

  function isInstalled(result: SearchResult) {
    return mods.some(
      (m) =>
        m.id === result.slug ||
        m.id === result.id ||
        m.projectId === result.id ||
        m.projectId === String(result.id),
    );
  }

  function iconFallback(name: string) {
    return name?.[0]?.toUpperCase() ?? "?";
  }

  async function syncModsFolderFromUi() {
    if (!$projectPath) return;
    loading = true;
    try {
      const synced: ModRow[] = await invoke("sync_mods_folder", { path: $projectPath });
      mods = synced;
      brokenIcons = [];
      hydrateMissingIcons().catch(() => {});
      refreshUpdateDots().catch(() => {});
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
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

  function matchesInstalledQuery(m: ModRow, q: string): boolean {
    if (!q) return true;
    const haystacks = [m.name, m.id, m.version, m.fileName, m.projectId];
    return haystacks.some((value) => (value ?? "").toLowerCase().includes(q));
  }

  $: contentScopedMods = isSavedViewFilter(contentFilter)
    ? []
    : mods.filter((m) => (m.contentType ?? "mod") === contentFilter);

  $: filtered = isSavedViewFilter(contentFilter)
    ? []
    : contentScopedMods.filter((m) => {
        const q = filter.trim().toLowerCase();
        const matchesText = matchesInstalledQuery(m, q);
        const matchesSide =
          contentFilter !== "mod" || sideFilter === "all" || m.side === sideFilter;
        return matchesText && matchesSide;
      });

  $: listTabNames = Object.keys(userState.lists).sort((a, b) => a.localeCompare(b));

  $: savedViewKey = isSavedViewFilter(contentFilter)
    ? `${contentFilter}:${Object.keys(userState.favorites).length}:${JSON.stringify(userState.lists)}`
    : "";
  $: if (savedViewKey) {
    void loadSavedModsView();
  }

  $: filteredSavedMods = savedMods.filter((result) => {
    const q = filter.trim().toLowerCase();
    if (!q) return true;
    return (
      result.name.toLowerCase().includes(q) ||
      result.slug.toLowerCase().includes(q) ||
      result.id.toLowerCase().includes(q) ||
      (result.description ?? "").toLowerCase().includes(q)
    );
  });

  $: searchPlaceholder = isSavedViewFilter(contentFilter)
    ? `Filter ${savedViewLabel(contentFilter).toLowerCase()}...`
    : `Search ${contentFilter}s...`;

  $: selectedResults = searchResults.filter((result) => selectedResultIds[result.id] && !isInstalled(result));

  $: counts = {
    all: contentScopedMods.length,
    client: contentScopedMods.filter((m) => m.side === "client").length,
    server: contentScopedMods.filter((m) => m.side === "server").length,
    both: contentScopedMods.filter((m) => m.side === "both").length,
  };

  $: contentNoun =
    contentFilter === "resourcepack"
      ? "resource packs"
      : contentFilter === "datapack"
        ? "datapacks"
        : contentFilter === "shader"
          ? "shaders"
          : "mods";

  $: tabCounts = {
    mod: mods.filter((m) => (m.contentType ?? "mod") === "mod").length,
    resourcepack: mods.filter((m) => m.contentType === "resourcepack").length,
    datapack: mods.filter((m) => m.contentType === "datapack").length,
    shader: mods.filter((m) => m.contentType === "shader").length,
  };

</script>

<div class="mods fade-slide-in">
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
      <button class={contentFilter === "mod" ? "primary" : "secondary"} on:click={() => switchContentFilter("mod")}>Mods <span class="tab-count">{tabCounts.mod}</span></button>
      <button class={contentFilter === "resourcepack" ? "primary" : "secondary"} on:click={() => switchContentFilter("resourcepack")}>Resourcepacks <span class="tab-count">{tabCounts.resourcepack}</span></button>
      <button class={contentFilter === "datapack" ? "primary" : "secondary"} on:click={() => switchContentFilter("datapack")}>Datapacks <span class="tab-count">{tabCounts.datapack}</span></button>
      <button class={contentFilter === "shader" ? "primary" : "secondary"} on:click={() => switchContentFilter("shader")}>Shaders <span class="tab-count">{tabCounts.shader}</span></button>
      <button class={contentFilter === "favorites" ? "primary" : "secondary"} on:click={() => switchContentFilter("favorites")} title="Favorite Modrinth projects">
        <Heart size={14} /> Favorites
      </button>
      {#each listTabNames as listName (listName)}
        <button class={contentFilter === `list:${listName}` ? "primary" : "secondary"} on:click={() => switchContentFilter(`list:${listName}`)} title="Saved build list">
          <Bookmark size={14} /> {listName}
          <span class="tab-count">{userState.lists[listName]?.length ?? 0}</span>
        </button>
      {/each}
    </div>
    <div class="toolbar-row">
      <div class="search wide">
        <Search size={16} />
        <input bind:value={filter} placeholder={searchPlaceholder} />
      </div>
      <div class="actions">
        <button on:click={openAddModal} disabled={!$projectPath || mutating}>
          <Plus size={16} />
          Add {isSavedViewFilter(contentFilter) ? "mod" : contentFilter}
        </button>
        <button class="secondary" on:click={syncModsFolderFromUi} disabled={!$projectPath || loading} title="Scan all content folders (mods/, resourcepacks/, shaderpacks/, datapacks/)">
          <RotateCw size={16} /> Sync
        </button>
        <button class="secondary glow-btn" on:click={applyAllUpdates} disabled={!$projectPath || updateApplying || updateCheckLoading || contentFilter !== "mod"} title="Update all mods to the latest build for this Minecraft version">
          <Sparkles size={16} />
          {#if updateApplying}
            Updating...
          {:else if updateCheckLoading}
            Checking...
          {:else if updateList.length > 0}
            Update all ({updateList.length})
          {:else}
            Update all
          {/if}
        </button>
        <button class="secondary" on:click={loadRecommendations} disabled={!$projectPath || recsLoading || contentFilter !== "mod"} title="Suggest optimization mods for this loader, Minecraft version, and pack">
          <Lightbulb size={16} />
          {recsLoading ? "Analyzing..." : "Suggestions"}
        </button>
        <button
          class="secondary"
          on:click={installSteamBridge}
          disabled={!$projectPath || steamBridgeInstalling || mutating || contentFilter !== "mod" || hasSteamBridge}
          title={hasSteamBridge
            ? "Steam Bridge is already installed"
            : "Play LAN worlds with Steam friends — no Radmin/VPN. Downloads the jar for this pack's Minecraft + loader from github.com/Ragalikx/steam-bridge-mc"}
        >
          {#if steamBridgeInstalling}
            <Loader2 size={16} class="spin" />
            Steam Bridge…
          {:else if hasSteamBridge}
            <Check size={16} />
            Steam Bridge
          {:else}
            <Users size={16} />
            Steam Bridge
          {/if}
        </button>
      </div>
    </div>
  </div>

  <div class="quick-filters" aria-label="Side filters">
    {#if contentFilter === "mod"}
    <button class:active={sideFilter === "all"} on:click={() => (sideFilter = "all")}>All <span>{counts.all}</span></button>
    <button class:active={sideFilter === "both"} on:click={() => (sideFilter = "both")}>Both <span>{counts.both}</span></button>
    <button class:active={sideFilter === "client"} on:click={() => (sideFilter = "client")}>Client <span>{counts.client}</span></button>
    <button class:active={sideFilter === "server"} on:click={() => (sideFilter = "server")}>Server <span>{counts.server}</span></button>
    {/if}
  </div>

  {#if selectionMode}
    <div class="selection-bar">
      <span class="selection-count">{selectedCount} selected</span>
      <div class="selection-actions">
        <button class="secondary mini" on:click={bulkUpdateSelected} disabled={mutating || !selectedMods.some((m) => m.updateAvailable)}>
          <RotateCw size={14} /> Update
        </button>
        <button class="secondary mini" on:click={bulkDisableSelected} disabled={mutating || !selectedMods.some((m) => !m.disabled)}>
          <PowerOff size={14} /> Disable
        </button>
        <button class="secondary mini" on:click={bulkEnableSelected} disabled={mutating || !selectedMods.some((m) => m.disabled)}>
          <Power size={14} /> Enable
        </button>
        <button class="secondary mini danger" on:click={bulkDeleteSelected} disabled={mutating}>
          <Trash2 size={14} /> Delete
        </button>
        <button class="ghost mini" on:click={clearSelection}>Cancel</button>
      </div>
    </div>
  {/if}

  {#if recommendations.length > 0}
    <div class="recs-panel">
      <div class="recs-header">
        <h3><Lightbulb size={16} /> Suggestions ({recommendations.length})</h3>
        <button class="ghost mini" on:click={() => (recommendations = [])}><X size={14} /></button>
      </div>
      <div class="recs-list">
        {#each recommendations as rec (rec.slug)}
          <div class="recs-row">
            <div class="recs-main">
              <span class="recs-prio {rec.priority}">{rec.priority}</span>
              {#if rec.source}<span class="recs-source">{rec.source}</span>{/if}
              <strong>{rec.name}</strong>
              <span>{rec.description}</span>
              {#if rec.loader || rec.minecraftVersion}
                <span class="recs-meta">{[rec.loader, rec.minecraftVersion].filter(Boolean).join(" · ")}</span>
              {/if}
            </div>
            <button class="secondary mini" on:click={async () => {
              if (!$projectPath) return;
              mutating = true;
              openDownloadOverlay(`Installing ${rec.name}`);
              try {
                await invoke("add_modrinth_mod_with_dependencies", { path: $projectPath, modId: rec.slug, side: "auto" });
                recommendations = recommendations.filter((r) => r.slug !== rec.slug);
                await reloadModsSilent();
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

  {#if error}
    <div class="error">{error}</div>
  {/if}
  {#if message}
    <div class="notice success">{message}</div>
  {/if}

  {#if loading}
    <div class="loading">Loading {contentNoun}...</div>
  {:else if !$projectPath}
    <EmptyState icon={Package} title="No project selected" description="Open a project to manage content." actionLabel="Open project" on:action={openProjectFolder} />
  {:else if isSavedViewFilter(contentFilter)}
    {#if savedModsLoading}
      <div class="loading">Loading {savedViewLabel(contentFilter).toLowerCase()}...</div>
    {:else if savedMods.length === 0}
      <div class="empty">
        {#if contentFilter === "favorites"}
          No favorites yet. Open <strong>Add mod</strong> and use the heart icon on Modrinth projects.
        {:else}
          List <strong>{savedViewLabel(contentFilter)}</strong> is empty. Bookmark projects from the Modrinth browser.
        {/if}
        <button class="secondary" style="margin-top: 12px" on:click={openAddModal} disabled={!$projectPath}>
          <Plus size={16} /> Browse Modrinth
        </button>
      </div>
    {:else if filteredSavedMods.length === 0}
      <div class="empty">No projects match your filter.</div>
    {:else}
      <div class="saved-toolbar">
        <span class="saved-count">{filteredSavedMods.length} of {savedMods.length} saved</span>
        {#if contentFilter.startsWith("list:")}
          {@const listName = contentFilter.slice(5)}
          <button on:click={() => installList(listName)} disabled={!$projectPath || installingFromList === listName || mutating}>
            <ArrowDown size={16} /> {installingFromList === listName ? "Installing..." : `Install all from "${listName}"`}
          </button>
          <button class="secondary" on:click={() => { renameTarget = listName; showRenamePrompt = true; }}>Rename</button>
          <button class="secondary danger" on:click={() => { deleteTarget = listName; showDeleteConfirm = true; }}>Delete list</button>
        {/if}
        <button class="secondary" on:click={openAddModal} disabled={!$projectPath}><Plus size={16} /> Browse Modrinth</button>
      </div>
      <div class="results list saved-results tb-stagger">
        {#each filteredSavedMods as result, i (result.id)}
          <article
            class="result-card tb-card"
            style={`--i: ${i}`}
            class:installed={isInstalled(result)}
            class:list={true}
            role="button"
            tabindex="0"
            on:click={() => openCatalogInApp(result)}
            on:keydown={(e) => (e.key === "Enter" || e.key === " ") && openCatalogInApp(result)}
          >
            <div class="result-icon">
              {#if result.iconUrl}
                <img class="tb-cover-media" src={result.iconUrl} alt="" loading="lazy" />
              {:else}
                <span class="tb-cover-media">{iconFallback(result.name)}</span>
              {/if}
            </div>
            <div class="result-main">
              <div class="result-title">
                <button type="button" class="result-name linkish" title="Open in launcher" on:click|stopPropagation={() => openCatalogInApp(result)}>{result.name}</button>
                {#if result.author}<span class="result-author">by {result.author}</span>{/if}
                {#if isInstalled(result)}<span class="installed-pill">Installed</span>{/if}
              </div>
              <p class="result-desc">{result.description}</p>
            </div>
            <div class="result-actions">
              <button class="download-btn" on:click|stopPropagation={() => startInstallPlan(result)} disabled={mutating || isInstalled(result)}>
                <Download size={16} /> {isInstalled(result) ? "Installed" : "Install"}
              </button>
              <div class="quick-actions">
                <button class="qa" class:active={userState.favorites[result.id]} title="Favorite" on:click|stopPropagation={() => toggleFavorite(result.id)}>
                  <Heart size={15} fill={userState.favorites[result.id] ? "currentColor" : "none"} />
                </button>
                {#if contentFilter.startsWith("list:")}
                  <button class="qa danger" title="Remove from list" on:click|stopPropagation={() => removeFromList(contentFilter.slice(5), result.id)}><X size={15} /></button>
                {/if}
                <button class="qa" title="Open on site" on:click|stopPropagation={() => openProjectPage(result)}>
                  <ExternalLink size={15} />
                </button>
                <button class="qa" title={copiedLinkId === result.id ? "Copied!" : "Copy link"} on:click|stopPropagation={() => copyProjectLink(result)}>
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
              <span class="footer-updated"><Clock size={13} />{formatRelative(result.dateModified)}</span>
            </div>
          </article>
        {/each}
      </div>
    {/if}
  {:else if filtered.length === 0}
    <EmptyState icon={Package} title={`No ${contentNoun} found`} description="Try Sync, adjust filters, or add content from Modrinth." actionLabel={`Add ${contentFilter}`} on:action={openAddModal} />
  {:else}
    <div class="installed-list tb-stagger" class:selecting={selectionMode}>
      {#each filtered as mod, i (mod.id)}
        <article
          class="installed-card tb-card"
          class:has-update={mod.updateAvailable}
          class:disabled={mod.disabled}
          class:selected={!!selectedModIds[mod.id]}
          style="--i: {i}"
          role={selectionMode ? "button" : undefined}
          tabindex={selectionMode ? 0 : undefined}
          on:contextmenu={(e) => onCardContextMenu(e, mod)}
          on:click={(e) => onCardClick(e, mod)}
          on:keydown={(e) => {
            if (!selectionMode) return;
            if (e.key === "Enter" || e.key === " ") {
              e.preventDefault();
              toggleModSelected(mod.id);
            }
          }}
        >
          <div class="mod-icon">
            {#if selectionMode}
              <span class="select-check" class:on={!!selectedModIds[mod.id]} aria-hidden="true">
                {#if selectedModIds[mod.id]}<Check size={14} />{/if}
              </span>
            {/if}
            {#if mod.updateAvailable}
              <span class="update-dot" title="Update available"></span>
            {/if}
            {#if mod.iconUrl && !brokenIcons.includes(mod.id)}
              <img class="tb-cover-media" src={mod.iconUrl} alt="" loading="lazy" on:error={() => handleIconError(mod)} />
            {:else}
              <span class="tb-cover-media">{iconFallback(mod.name)}</span>
            {/if}
          </div>
          <div class="installed-main">
            <div class="installed-title">
              {#if installedModPageUrl(mod)}
                <button type="button" class="installed-name linkish" title="Open catalog page" on:click|stopPropagation={() => openInstalledModPage(mod)}>{mod.name}</button>
              {:else}
                <strong>{mod.name}</strong>
              {/if}
              {#if mod.disabled}
                <span class="disabled-badge">Disabled</span>
              {/if}
              {#if mod.updateAvailable}
                <span class="update-badge">Update</span>
              {/if}
              <code>{mod.id}</code>
            </div>
            <div class="installed-meta">
              <span class="version">{mod.version}</span>
              {#if mod.fileName}<span class="filename">{mod.fileName}{mod.disabled && !String(mod.fileName).endsWith('.disabled') ? '.disabled' : ''}</span>{/if}
            </div>
          </div>
          <div class="installed-tags">
            <span class="tag side-{mod.side}">{mod.side}</span>
            <span class="tag source">{mod.source}</span>
          </div>
          <div class="card-actions">
            {#if installedModPageUrl(mod)}
              <button
                class="icon-btn"
                on:click|stopPropagation={() => openInstalledModPage(mod)}
                title="Open on {(mod.source || '').toLowerCase() === 'curseforge' ? 'CurseForge' : 'Modrinth'}"
              >
                <ExternalLink size={16} />
              </button>
            {/if}
            <button
              class="icon-btn"
              class:warn={mod.disabled}
              on:click|stopPropagation={() => toggleDisabled(mod)}
              disabled={mutating}
              title={mod.disabled ? "Enable (remove .disabled)" : "Disable (rename to *.disabled)"}
            >
              {#if mod.disabled}
                <Power size={16} />
              {:else}
                <PowerOff size={16} />
              {/if}
            </button>
            <button class="icon-btn" on:click|stopPropagation={() => openVersionPicker(mod)} disabled={mutating || !canChangeVersion(mod) || selectionMode} title="Change version">
              <ArrowUpDown size={16} />
            </button>
            {#if mod.updateAvailable}
              <button class="icon-btn update-btn hot" on:click|stopPropagation={() => updateMod(mod)} disabled={mutating || selectionMode} title="Update to latest from Modrinth">
                <RotateCw size={16} />
                <span class="update-text">Update</span>
              </button>
            {/if}
            <button class="icon-btn danger" on:click|stopPropagation={() => showRemoveConfirm(mod)} disabled={mutating || selectionMode} title="Remove with snapshot">
              <Trash2 size={16} />
            </button>
          </div>
        </article>
      {/each}
    </div>
  {/if}
</div>

{#if confirmOpen && confirmMod}
  <div
    class="modal-backdrop"
    role="button"
    tabindex="-1"
    aria-label="Confirm remove mod"
    on:click|self={() => { confirmOpen = false; confirmMod = null; }}
    on:keydown={() => {}}
  >
    <div class="modal confirm-modal" role="dialog" aria-modal="true" use:trapFocus={{ onEscape: () => { confirmOpen = false; confirmMod = null; } }}>
      <div class="modal-header">
        <div>
          <h2>Remove {confirmMod.name}?</h2>
          <p>Deletes the jar from disk and removes the Modrinth index entry. A snapshot is taken first.</p>
        </div>
        <button class="icon-btn" on:click={() => { confirmOpen = false; confirmMod = null; }}><X size={18} /></button>
      </div>
      <div class="plan-actions">
        <button class="ghost" on:click={() => { confirmOpen = false; confirmMod = null; }}>Cancel</button>
        <button class="danger" on:click={doRemove} disabled={mutating}>
          <Trash2 size={16} /> Remove
        </button>
      </div>
    </div>
  </div>
{/if}

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

      <div class="download-stage" aria-live="polite">
        <div class="download-stage-top">
          <span>{downloadStageMessage}</span>
          <strong>{downloadStagePercent}%</strong>
        </div>
        <div
          class="download-overall-bar"
          role="progressbar"
          aria-label="Overall update progress"
          aria-valuemin="0"
          aria-valuemax="100"
          aria-valuenow={downloadStagePercent}
        >
          <div class="download-overall-fill" style="width: {downloadStagePercent}%"></div>
        </div>
      </div>

      {#if downloadError}
        <div class="download-error" role="alert">
          <AlertTriangle size={16} />
          <pre>{downloadError}</pre>
        </div>
      {/if}

      <div class="download-list">
        {#if downloadItems.length === 0}
          <div class="download-empty">Preparing downloads…</div>
        {:else}
          {#each downloadItems as item (item.id)}
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
                  <span class="download-item-error">{item.error ?? "Download failed"}</span>
                  <button class="mini ghost retry-one" on:click={() => retrySingleDownload(item.id)} disabled={!downloadDone}>Retry</button>
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
          {#if downloadFailedCount > 0}
            <button class="secondary" on:click={retryFailedDownloads}>
              <RotateCw size={16} /> Retry failed ({downloadFailedCount})
            </button>
          {/if}
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
    on:click|self={() => { catalogViewResult = null; addOpen = false; }}
    on:keydown={() => {}}
  >
    <div class="modal add-mods-modal" role="dialog" aria-modal="true" use:trapFocus={{ onEscape: () => { if (catalogViewResult) closeCatalogInApp(); else addOpen = false; } }}>
      {#if catalogViewResult}
        <div class="modal-body catalog-body">
          <CatalogProjectView
            result={catalogViewResult}
            minecraftVersion={$projectInfo?.minecraftVersion ?? null}
            loaderKind={$projectInfo?.loaderKind ?? null}
            installed={isInstalled(catalogViewResult)}
            installing={mutating && pendingInstall?.id === catalogViewResult.id}
            on:back={closeCatalogInApp}
            on:install={() => { if (catalogViewResult) void startInstallPlan(catalogViewResult); }}
            on:openExternal={() => { if (catalogViewResult) void openProjectPage(catalogViewResult); }}
          />
        </div>
      {:else}
      <div class="modal-header">
        <div>
          <h2>Add {catalogProvider === "both" ? "" : (catalogProvider === "curseforge" ? "CurseForge " : "Modrinth ")}{contentFilter}</h2>
          <p>
            {contentFilter === "mod"
              ? "Search is filtered by the current Minecraft version and loader."
              : "Search is filtered by the current Minecraft version."}
          </p>
          <div class="provider-toggle" role="group" aria-label="Catalog provider">
            <button
              type="button"
              class:active={catalogProvider === "modrinth"}
              on:click={() => setCatalogProvider("modrinth")}
            >Modrinth</button>
            <button
              type="button"
              class:active={catalogProvider === "curseforge"}
              on:click={() => setCatalogProvider("curseforge")}
            >CurseForge</button>
            <button
              type="button"
              class:active={catalogProvider === "both"}
              on:click={() => setCatalogProvider("both")}
              title="Search both catalogs at once"
            >Both</button>
          </div>
        </div>
        <button class="icon-btn" on:click={() => (addOpen = false)}><X size={18} /></button>
      </div>

      <div class="modal-tabs">
        <button class:active={contentFilter === "mod"} on:click={() => switchContentFilter("mod")}>Mods</button>
        <button class:active={contentFilter === "resourcepack"} on:click={() => switchContentFilter("resourcepack")}>Resourcepacks</button>
        <button class:active={contentFilter === "datapack"} on:click={() => switchContentFilter("datapack")}>Datapacks</button>
        <button class:active={contentFilter === "shader"} on:click={() => switchContentFilter("shader")}>Shaders</button>
        <button class:active={contentFilter === "favorites"} on:click={() => switchContentFilter("favorites")}>Favorites</button>
        {#each listTabNames as listName (listName)}
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
                  {#each filteredVersions as version (version)}
                    <button class:active={filterGameVersion === version} on:click={() => { filterGameVersion = version; searchMods(1); }}>{version}</button>
                  {/each}
                </div>
                <label class="check-row">
                  <input type="checkbox" checked={filterGameVersion === ""} on:change={() => { filterGameVersion = ""; searchMods(1); }} /> Show all versions
                </label>
              </div>
            {/if}
          </section>

          <section class="filter-block" class:closed={!accordionOpen.loader} hidden={contentFilter !== "mod"}>
            <button class="filter-head" on:click={() => toggleAccordion("loader")}>
              <span>Loader</span>
              <ChevronDown size={16} class={!accordionOpen.loader ? "rot" : ""} />
            </button>
            {#if accordionOpen.loader}
              <div class="filter-body">
                <div class="filter-list loader-list">
                  {#each shownLoaders as loaderName (loaderName)}
                    <button class="loader-row" class:active={filterLoader === loaderName.toLowerCase()} on:click={() => { filterLoader = loaderName.toLowerCase(); searchMods(1); }}>
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

          <section class="filter-block" class:closed={!accordionOpen.category} hidden={catalogProvider === "curseforge"}>
            <button class="filter-head" on:click={() => toggleAccordion("category")}>
              <span>Category</span>
              <ChevronDown size={16} class={!accordionOpen.category ? "rot" : ""} />
            </button>
            {#if accordionOpen.category}
              <div class="filter-body">
                <div class="filter-list">
                  <button class:active={!filterCategory} on:click={() => { filterCategory = ""; searchMods(1); }}>All categories</button>
                  {#each categories as category (category)}
                    <button class="cat-row" class:active={filterCategory === category} on:click={() => { filterCategory = category; searchMods(1); }}>
                      <Tag size={14} />
                      <span>{humanize(category)}</span>
                    </button>
                  {/each}
                </div>
              </div>
            {/if}
          </section>

          <section class="filter-block" class:closed={!accordionOpen.cfSort} hidden={catalogProvider !== "curseforge"}>
            <button class="filter-head" on:click={() => toggleAccordion("cfSort")}>
              <span>Sort (CurseForge)</span>
              <ChevronDown size={16} class={!accordionOpen.cfSort ? "rot" : ""} />
            </button>
            {#if accordionOpen.cfSort}
              <div class="filter-body">
                <div class="filter-list">
                  {#each [{ id: 1, label: "Featured" }, { id: 2, label: "Popularity" }, { id: 3, label: "Last Updated" }, { id: 4, label: "Name" }, { id: 5, label: "Total Downloads" }, { id: 6, label: "Views" }] as opt (opt.id)}
                    <button class:active={cfSortField === opt.id} on:click={() => { cfSortField = opt.id; searchMods(1); }}>{opt.label}</button>
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
              <input
                bind:value={searchQuery}
                placeholder="Search mods..."
                on:input={onSearchQueryInput}
                on:keydown={(e) => e.key === "Enter" && searchMods(1)}
              />
              {#if searchLoading}
                <Loader2 size={16} class="spin search-spinner" />
              {/if}
            </div>
            <div class="topbar-controls">
              <label class="sort-select">Sort by:
                <select bind:value={sortBy} on:change={() => searchMods(1)}>
                  {#each sortOptions as option (option.id)}<option value={option.id}>{option.label}</option>{/each}
                </select>
              </label>
              <label class="sort-select">View:
                <select bind:value={pageSize} on:change={() => searchMods(1)}>
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
              {#each Array.from({ length: Math.min(totalPages, 5) }, (_, i) => i + 1) as p (p)}
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
              <button on:click={bulkInstallSelected} disabled={selectedResults.length === 0 || mutating || catalogProvider === "curseforge"} title={catalogProvider === "curseforge" ? "Bulk install with dependencies is Modrinth-only" : undefined}>Install selected + dependencies</button>
            </div>
          </div>

          {#if searchLoading && searchResults.length === 0 && !isSavedViewFilter(contentFilter)}
            <div class="loading compact">Loading {catalogProvider === "both" ? "Modrinth & CurseForge" : (catalogProvider === "curseforge" ? "CurseForge" : "Modrinth")} projects...</div>
          {:else if isSavedViewFilter(contentFilter)}
            {#if savedModsLoading}
              <div class="loading compact">Loading saved projects...</div>
            {:else if savedMods.length === 0}
              {#if contentFilter === "favorites"}
                <EmptyState icon={Bookmark} compact={true} title="No favorites yet" description="Favorite mods will appear here." />
              {:else}
                <EmptyState icon={Bookmark} compact={true} title="This list is empty" description="Saved projects will appear here." />
              {/if}
            {:else}
              <div class="results {viewMode} tb-stagger">
                {#each savedMods as result, i (result.id)}
                  <article
                    class="result-card tb-card"
                    style={`--i: ${i}`}
                    class:installed={isInstalled(result)}
                    class:list={viewMode === "list"}
                    role="button"
                    tabindex="0"
                    on:click={() => openCatalogInApp(result)}
                    on:keydown={(e) => (e.key === "Enter" || e.key === " ") && openCatalogInApp(result)}
                  >
                    <div class="result-icon">
                      {#if result.iconUrl}
                        <img class="tb-cover-media" src={result.iconUrl} alt="" loading="lazy" />
                      {:else}
                        <span class="tb-cover-media">{iconFallback(result.name)}</span>
                      {/if}
                    </div>
                    <div class="result-main">
                      <div class="result-title">
                        <button type="button" class="result-name linkish" title="Open in launcher" on:click|stopPropagation={() => openCatalogInApp(result)}>{result.name}</button>
                        {#if catalogProvider === "both"}
                          <span
                            class="provider-badge"
                            class:modrinth={(result.provider ?? "modrinth") !== "curseforge"}
                            class:curseforge={result.provider === "curseforge"}
                            title={result.provider === "curseforge" ? "CurseForge" : "Modrinth"}
                          >{result.provider === "curseforge" ? "CF" : "MR"}</span>
                        {/if}
                        {#if result.author}<span class="result-author">by {result.author}</span>{/if}
                      </div>
                      <p class="result-desc">{result.description}</p>
                    </div>
                    <div class="result-actions">
                      <button class="download-btn" on:click|stopPropagation={() => startInstallPlan(result)} disabled={mutating || isInstalled(result)}>
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
                              {#each listTabNames as listName (listName)}
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
                        <button class="qa" title="Open on site" on:click|stopPropagation={() => openProjectPage(result)}>
                          <ExternalLink size={15} />
                        </button>
                        <button class="qa" title={copiedLinkId === result.id ? "Copied!" : "Copy link"} on:click|stopPropagation={() => copyProjectLink(result)}>
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
            <EmptyState icon={Search} compact={true} title="No results" description="Adjust filters or search text." />
          {:else}
            <div class="results {viewMode} tb-stagger">
          {#each pagedResults as result, i (result.id)}
            <article
              class="result-card tb-card"
              style={`--i: ${i}`}
              class:installed={isInstalled(result)}
              class:selected={selectedResultIds[result.id]}
              class:list={viewMode === "list"}
              role="button"
              tabindex="0"
              on:click={() => openCatalogInApp(result)}
              on:keydown={(e) => (e.key === "Enter" || e.key === " ") && openCatalogInApp(result)}
            >
              <label class="select-result" title="Select for bulk install" on:click|stopPropagation>
                <input type="checkbox" checked={!!selectedResultIds[result.id]} disabled={isInstalled(result)} on:change={() => toggleResultSelection(result)} />
              </label>
              <div class="result-icon">
                {#if result.iconUrl}
                  <img class="tb-cover-media" src={result.iconUrl} alt="" loading="lazy" />
                {:else}
                  <span class="tb-cover-media">{iconFallback(result.name)}</span>
                {/if}
              </div>
              <div class="result-main">
                <div class="result-title">
                  <button type="button" class="result-name linkish" title="Open in launcher" on:click|stopPropagation={() => openCatalogInApp(result)}>{result.name}</button>
                  {#if catalogProvider === "both"}
                    <span
                      class="provider-badge"
                      class:modrinth={(result.provider ?? "modrinth") !== "curseforge"}
                      class:curseforge={result.provider === "curseforge"}
                      title={result.provider === "curseforge" ? "CurseForge" : "Modrinth"}
                    >{result.provider === "curseforge" ? "CF" : "MR"}</span>
                  {/if}
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
                  {#each resultBadges(result) as b (b.label)}
                    <span class="badge"><Tag size={12} />{b.label}</span>
                  {/each}
                </div>
              </div>
              <div class="result-actions">
                <button class="download-btn" on:click|stopPropagation={() => startInstallPlan(result)} disabled={mutating || isInstalled(result)}>
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
                        {#each Object.keys(userState.lists) as listName (listName)}
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
                  <button class="qa" title="Open on site" on:click|stopPropagation={() => openProjectPage(result)}>
                    <ExternalLink size={15} />
                  </button>
                  <button class="qa" title={copiedLinkId === result.id ? "Copied!" : "Copy link"} on:click|stopPropagation={() => copyProjectLink(result)}>
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
          {#if totalPages > 1 && !isSavedViewFilter(contentFilter)}
            <div class="pagination bottom">
              <button class="page-btn" disabled={page <= 1} on:click={() => goToPage(page - 1)}>‹ Prev</button>
              {#each Array.from({ length: Math.min(totalPages, 7) }, (_, i) => i + 1) as p (p)}
                <button class="page-btn" class:active={p === page} on:click={() => goToPage(p)}>{p}</button>
              {/each}
              {#if totalPages > 7}<span class="page-ellipsis">…</span><button class="page-btn" on:click={() => goToPage(totalPages)}>{totalPages}</button>{/if}
              <span class="page-info">{page} / {totalPages}</span>
              <button class="page-btn" disabled={page >= totalPages} on:click={() => goToPage(page + 1)}>Next ›</button>
            </div>
          {/if}
        </section>
      </div>

      {#if pendingInstall}
        <div class="install-plan-panel">
          <div>
            <span class="plan-eyebrow">Install plan</span>
            <h3>
              <button type="button" class="plan-title-link" title="Open catalog page" on:click={() => openProjectPage(pendingInstall)}>
                {pendingInstall.name}
                <ExternalLink size={14} />
              </button>
              <span class="plan-slug">({previews[pendingInstall.id]?.slug ?? pendingInstall.slug})</span>
            </h3>
            {#if isCurseForgeResult(pendingInstall)}
              <p class="muted">CurseForge installs the selected project directly (no dependency resolution).</p>
            {:else if previews[pendingInstall.id]}
              <div class="dep-list">
                <h4>Required ({requiredDeps(previews[pendingInstall.id]).length})</h4>
                {#if requiredDeps(previews[pendingInstall.id]).length === 0}
                  <p class="muted">No hard dependencies.</p>
                {:else}
                  {#each requiredDeps(previews[pendingInstall.id]) as dep (`${dep.type}:${dep.target}`)}
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
                  {#each optionalDeps(previews[pendingInstall.id]) as dep (`${dep.type}:${dep.target}`)}
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
                  {#each conflictDeps(previews[pendingInstall.id]) as dep (`${dep.type}:${dep.target}`)}
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
            <button
              on:click={() => confirmInstall(isCurseForgeResult(pendingInstall) ? false : pendingInstallOptional)}
              disabled={mutating}
            >
              <Download size={16} />
              {#if isCurseForgeResult(pendingInstall)}
                Install
              {:else}
                Install{pendingInstallOptional ? " with dependencies" : ""}
              {/if}
            </button>
          </div>
        </div>
      {/if}
      {/if}
    </div>
  </div>
{/if}

<!-- Version picker modal — Modrinth-style: search, filter compatible, channel + confirm -->
{#if versionPickerMod}
  <div class="modal-backdrop" role="button" tabindex="-1" on:click={(e) => e.target === e.currentTarget && (versionPickerMod = null)} on:keydown={() => {}}>
    <div class="modal version-modal" role="dialog" aria-modal="true" use:trapFocus={{ onEscape: () => (versionPickerMod = null) }}>
      <div class="modal-header">
        <div>
          <h2>Change version: {versionPickerMod.name}</h2>
          <p>
            Current: <code>{versionPickerMod.version}</code>
            · target <strong>{versionPickerLoader || "loader"}</strong>
            <strong>{versionPickerMc || "Minecraft"}</strong>
            · {compatibleVersionCount} compatible
          </p>
        </div>
        <button class="icon-btn" on:click={() => (versionPickerMod = null)} aria-label="Close"><X size={18} /></button>
      </div>
      {#if versionPickerError}<div class="error compact">{versionPickerError}</div>{/if}
      {#if versionPickerLoading}
        <div class="loading compact"><Loader2 size={20} class="spin" /> Loading versions...</div>
      {:else if availableVersions.length === 0}
        <EmptyState icon={Package} compact={true} title="No versions found" description="No versions found for this mod on Modrinth." />
      {:else}
        <div class="version-toolbar">
          <div class="search wide">
            <Search size={16} />
            <input bind:value={versionPickerQuery} placeholder="Search version, channel, MC…" />
          </div>
          <button
            class="secondary mini"
            class:active={!hideIncompatible}
            on:click={() => (hideIncompatible = !hideIncompatible)}
            title="Show versions for other Minecraft versions / loaders"
          >
            {hideIncompatible ? "Show all" : "Hide incompatible"}
          </button>
        </div>
        <div class="version-picker-body">
          <div class="version-list" role="listbox">
            {#each versionPickerFiltered as v (v.id)}
              <button
                class="version-row"
                class:current={v.versionNumber === versionPickerMod?.version}
                class:selected={selectedVersion?.id === v.id}
                class:incompatible={v.compatible === false}
                role="option"
                aria-selected={selectedVersion?.id === v.id}
                on:click={() => (selectedVersion = v)}
                disabled={versionPickerChanging}
              >
                <div class="version-main">
                  <div class="version-title-row">
                    <span class="channel-dot channel-{v.versionType ?? 'release'}" title={v.versionType ?? "release"}></span>
                    <strong>{v.versionNumber}</strong>
                    {#if v.compatible === false}
                      <span class="incompat-badge" title="Not for {versionPickerLoader} {versionPickerMc}"><AlertTriangle size={12} /></span>
                    {/if}
                  </div>
                  {#if v.name && v.name !== v.versionNumber}
                    <span class="version-name">{v.name}</span>
                  {/if}
                  <span class="version-loaders">
                    {(v.versionType ?? "release")} · {v.loaders.join(", ")} · MC {v.gameVersions.slice(0, 4).join(", ")}{#if v.gameVersions.length > 4}…{/if}{#if v.datePublished} · {formatDate(v.datePublished)}{/if}
                  </span>
                </div>
                {#if v.versionNumber === versionPickerMod?.version}
                  <span class="current-badge">Current</span>
                {:else if selectedVersion?.id === v.id}
                  <span class="install-badge">Selected</span>
                {/if}
              </button>
            {:else}
              <EmptyState icon={Package} compact={true} title="No matching versions" description="No versions match this filter." />
            {/each}
            {#if selectedVersion}
              <div class="version-switch-footer">
                <button
                  class="primary block"
                  on:click={() => selectedVersion && changeVersion(selectedVersion.id)}
                  disabled={versionPickerChanging || selectedVersion.versionNumber === versionPickerMod?.version}
                >
                  {#if versionPickerChanging}
                    <Loader2 size={16} class="spin" /> Switching...
                  {:else if selectedVersion.versionNumber === versionPickerMod?.version}
                    Already installed
                  {:else}
                    <Download size={16} /> Switch to {selectedVersion.versionNumber}
                  {/if}
                </button>
              </div>
            {/if}
          </div>
          <div class="version-detail">
            {#if selectedVersion}
              <div class="version-detail-header">
                <strong>{selectedVersion.versionNumber}</strong>
                <span class="channel-pill channel-{selectedVersion.versionType ?? 'release'}">{selectedVersion.versionType ?? "release"}</span>
              </div>
              <p class="muted">
                {selectedVersion.loaders.join(", ")} · MC {selectedVersion.gameVersions.join(", ")}
                {#if selectedVersion.datePublished} · {formatDate(selectedVersion.datePublished)}{/if}
              </p>
              {#if selectedVersion.compatible === false}
                <div class="notice warn compact">
                  This build is not listed for {versionPickerLoader} {versionPickerMc}.
                </div>
              {/if}
              <div class="version-changelog-full">
                {#if selectedVersion.changelog}
                  {stripHtml(selectedVersion.changelog).slice(0, 1200)}{stripHtml(selectedVersion.changelog).length > 1200 ? "…" : ""}
                {:else}
                  <span class="muted">No changelog for this version.</span>
                {/if}
              </div>
              <div class="version-detail-actions">
                <button
                  on:click={() => selectedVersion && changeVersion(selectedVersion.id)}
                  disabled={versionPickerChanging || selectedVersion.versionNumber === versionPickerMod?.version}
                >
                  {#if versionPickerChanging}
                    <Loader2 size={16} class="spin" /> Switching...
                  {:else if selectedVersion.versionNumber === versionPickerMod?.version}
                    Already installed
                  {:else}
                    <Download size={16} /> Switch to this version
                  {/if}
                </button>
              </div>
            {:else}
              <EmptyState icon={Package} compact={true} title="Select a version" description="Select a version to preview its changelog." />
            {/if}
          </div>
        </div>
      {/if}
    </div>
  </div>
{/if}

<!-- Post-bulk dependency resolution dialog -->
{#if dependencyDialogOpen}
  <div class="modal-backdrop" role="button" tabindex="-1" on:click={(e) => e.target === e.currentTarget && (dependencyDialogOpen = false)} on:keydown={() => {}}>
    <div class="modal dep-dialog" role="dialog" aria-modal="true" use:trapFocus={{ onEscape: () => (dependencyDialogOpen = false) }}>
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
    <div class="modal plan-modal" role="dialog" aria-modal="true" use:trapFocus={{ onEscape: () => (planPreviewOpen = false) }}>
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
                {#each requiredDeps(planPreviewDeps) as dep (`${dep.type}:${dep.target}`)}
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
                {#each conflictDeps(planPreviewDeps) as dep (`${dep.type}:${dep.target}`)}
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

{#if showRenamePrompt}
  <PromptDialog
    title="Rename list"
    message="Enter a new name for the list."
    mode="text"
    defaultValue={renameTarget}
    confirmLabel="Rename"
    on:confirm={(e) => { if (e.detail.trim() && renameTarget) { renameList(renameTarget, e.detail.trim()); } showRenamePrompt = false; }}
    on:cancel={() => (showRenamePrompt = false)}
  />
{/if}

{#if showDeleteConfirm}
  <ConfirmDialog
    title="Delete list"
    message={`Delete list "${deleteTarget}"? This cannot be undone.`}
    danger
    confirmLabel="Delete"
    on:confirm={() => { if (deleteTarget) deleteList(deleteTarget); showDeleteConfirm = false; }}
    on:cancel={() => (showDeleteConfirm = false)}
  />
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
    pointer-events: none;
  }

  .search input {
    width: 100%;
    padding-left: 40px;
  }

  .search :global(.search-spinner) {
    position: absolute;
    right: 12px;
    color: var(--text-muted);
    pointer-events: none;
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
    align-items: center;
    margin-bottom: 14px;
  }

  .saved-count {
    font-size: 13px;
    color: var(--text-muted);
    margin-right: auto;
  }

  .saved-results {
    margin-top: 4px;
  }

  .installed-pill {
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 999px;
    background: rgba(27, 217, 106, 0.15);
    color: #1bd96a;
    border: 1px solid rgba(27, 217, 106, 0.35);
  }

  .installed-list {
    display: grid;
    gap: 10px;
  }

  .installed-list.selecting .installed-card {
    cursor: pointer;
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
  }

  .installed-card:hover {
    background: rgba(255,255,255,0.03);
  }

  .installed-card.has-update {
    border-color: rgba(245, 166, 35, 0.35);
    background:
      linear-gradient(90deg, rgba(245, 166, 35, 0.08), transparent 28%),
      var(--bg-secondary);
  }

  .installed-card.disabled {
    opacity: 0.72;
    border-style: dashed;
  }

  .installed-card.selected {
    border-color: var(--accent-primary);
    background:
      linear-gradient(90deg, rgba(27, 217, 106, 0.12), transparent 40%),
      var(--bg-secondary);
    box-shadow: 0 0 0 1px rgba(27, 217, 106, 0.25);
  }

  .select-check {
    position: absolute;
    top: 4px;
    left: 4px;
    width: 18px;
    height: 18px;
    border-radius: 5px;
    border: 1px solid rgba(255,255,255,0.35);
    background: rgba(0,0,0,0.45);
    display: flex;
    align-items: center;
    justify-content: center;
    color: #fff;
    z-index: 2;
  }
  .select-check.on {
    background: var(--accent-primary);
    border-color: var(--accent-primary);
    color: #04140a;
  }

  .disabled-badge {
    font-size: 10px;
    font-weight: 800;
    text-transform: uppercase;
    padding: 2px 6px;
    border-radius: 999px;
    color: #fca5a5;
    background: rgba(239, 68, 68, 0.12);
    border: 1px solid rgba(239, 68, 68, 0.3);
  }

  .selection-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
    margin-bottom: 12px;
    padding: 10px 12px;
    border-radius: 12px;
    border: 1px solid rgba(27, 217, 106, 0.35);
    background: rgba(27, 217, 106, 0.08);
  }
  .selection-count { font-weight: 700; font-size: 13px; color: var(--accent-primary); }
  .selection-actions { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; }
  .selection-actions .mini {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 10px;
  }
  .icon-btn.warn { color: #fbbf24; }

  .mod-icon,
  .result-icon {
    width: 52px;
    height: 52px;
    border-radius: 14px;
    overflow: hidden;
    background: linear-gradient(135deg, var(--accent-secondary), var(--accent-primary));
    display: flex;
    position: relative;
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

  .download-error {
    display: flex;
    align-items: flex-start;
    gap: 9px;
    margin-bottom: 12px;
    padding: 10px 12px;
    border: 1px solid rgba(239, 68, 68, 0.35);
    border-radius: 10px;
    color: #fca5a5;
    background: rgba(239, 68, 68, 0.08);
  }

  .download-error pre {
    margin: 0;
    min-width: 0;
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    color: inherit;
    font: inherit;
    font-size: 12px;
  }

  .download-item-error {
    min-width: 0;
    overflow-wrap: anywhere;
    color: #fca5a5;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .download-stage {
    margin-bottom: 14px;
    padding: 12px 14px;
    border: 1px solid rgba(27, 217, 106, 0.24);
    border-radius: 14px;
    background:
      linear-gradient(135deg, rgba(27, 217, 106, 0.1), rgba(110, 231, 168, 0.025)),
      rgba(255, 255, 255, 0.02);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.04);
  }

  .download-stage-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 9px;
    color: var(--text-muted);
    font-size: 12px;
  }

  .download-stage-top span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .download-stage-top strong {
    flex-shrink: 0;
    color: var(--accent-primary);
    font-variant-numeric: tabular-nums;
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
    gap: 10px;
  }

  .retry-one {
    margin-left: auto;
    font-size: 11px;
    padding: 2px 8px !important;
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

  .update-btn {
    width: auto;
    padding: 0 10px;
    gap: 5px;
    display: inline-flex;
    align-items: center;
    color: #1bd96a;
  }
  .update-btn .update-text { font-size: 12px; font-weight: 700; }
  .update-btn.hot { background: rgba(27, 217, 106, 0.12); border-radius: 8px; }

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

  /* Add content browser: nearly full viewport so filters + grid aren't cramped. */
  .modal.add-mods-modal {
    width: min(1760px, calc(100vw - 16px));
    height: min(1000px, calc(100vh - 16px));
    max-height: calc(100vh - 16px);
    display: flex;
    flex-direction: column;
    overflow: hidden;
    padding: 18px 20px 16px;
  }

  .modal.add-mods-modal .modal-header {
    flex-shrink: 0;
    margin-bottom: 12px;
  }

  .modal.add-mods-modal .modal-tabs {
    flex-shrink: 0;
    padding: 0 0 10px;
  }

  .modal.add-mods-modal .browser-layout {
    flex: 1;
    min-height: 0;
    height: auto;
    align-items: stretch;
  }

  .modal.add-mods-modal .catalog-body {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }

  .modal.add-mods-modal .filter-sidebar {
    max-height: none;
    height: 100%;
  }

  .modal.add-mods-modal .browser-results {
    min-height: 0;
    height: 100%;
    overflow: auto;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .catalog-body {
    min-height: min(720px, calc(100vh - 80px));
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

  .provider-toggle {
    display: inline-flex;
    gap: 4px;
    margin-top: 10px;
    padding: 3px;
    border-radius: 10px;
    border: 1px solid var(--border-color);
    background: var(--bg-tertiary);
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

  .confirm-modal {
    width: min(480px, calc(100vw - 28px));
    max-height: none;
  }

  .confirm-modal .plan-actions {
    margin-top: 18px;
  }

  .confirm-modal button.danger {
    background: #ef4444;
    color: #fff;
  }

  .confirm-modal button.danger:hover:not(:disabled) {
    background: #dc2626;
  }

  .confirm-modal button.danger:disabled {
    opacity: 0.55;
  }

  .modal-search {
    margin-bottom: 16px;
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

  .pagination.bottom {
    margin: 16px auto 8px;
    justify-content: center;
    flex-wrap: wrap;
  }
  .pagination .page-info {
    color: var(--text-muted);
    font-size: 13px;
    padding: 0 8px;
    align-self: center;
  }

  .version-switch-footer {
    position: sticky;
    bottom: 0;
    padding: 10px 12px;
    background: var(--bg-secondary);
    border-top: 1px solid var(--border-color);
  }
  .primary.block,
  button.primary.block {
    width: 100%;
    justify-content: center;
  }

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
    grid-template-columns: repeat(auto-fill, minmax(460px, 1fr));
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
    cursor: pointer;
  }

  .result-card:hover {
    background: #333;
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
  .result-icon img,
  .result-icon .tb-cover-media {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .result-icon span.tb-cover-media {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    height: 100%;
  }

  .result-main { grid-area: main; min-width: 0; }

  .result-title {
    display: flex;
    align-items: baseline;
    flex-wrap: wrap;
    gap: 8px;
  }
  .result-name { color: var(--text-primary); font-weight: 800; font-size: 15px; }
  button.result-name.linkish,
  button.installed-name.linkish {
    background: none;
    border: none;
    padding: 0;
    margin: 0;
    cursor: pointer;
    text-align: left;
    font: inherit;
    font-weight: 800;
    font-size: 15px;
    color: var(--text-primary);
  }
  button.result-name.linkish:hover,
  button.installed-name.linkish:hover {
    color: var(--accent-primary);
    text-decoration: underline;
  }
  .plan-title-link {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    background: none;
    border: none;
    padding: 0;
    margin: 0;
    cursor: pointer;
    color: inherit;
    font: inherit;
    font-weight: inherit;
  }
  .plan-title-link:hover { color: var(--accent-primary); }
  .plan-slug { color: var(--text-muted); font-weight: 500; margin-left: 6px; }
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
  .recs-header { display: flex; align-items: center; justify-content: space-between; gap: 8px; margin-bottom: 10px; }
  .recs-header h3 { display: flex; align-items: center; gap: 8px; color: var(--accent-secondary); margin: 0; font-size: 14px; }
  .recs-list { display: grid; gap: 6px; }
  .recs-row { display: flex; justify-content: space-between; align-items: center; gap: 12px; padding: 10px 12px; border-radius: 10px; background: var(--bg-tertiary); border: 1px solid var(--border-color); }
  .recs-main { display: flex; flex-wrap: wrap; gap: 4px 8px; align-items: center; }
  .recs-main strong { color: var(--text-primary); font-size: 13px; }
  .recs-main span { color: var(--text-muted); font-size: 11px; }
  .recs-meta { opacity: 0.8; }
  .recs-source { font-size: 9px; text-transform: uppercase; font-weight: 800; padding: 2px 6px; border-radius: 4px; background: rgba(139,92,246,.12); color: #c4b5fd; }
  .recs-prio { font-size: 9px; text-transform: uppercase; font-weight: 800; padding: 2px 6px; border-radius: 4px; }
  .recs-prio.critical { background: rgba(239,68,68,.15); color: #fca5a5; }
  .recs-prio.high { background: rgba(27,217,106,.12); color: var(--accent-primary); }
  .recs-prio.medium { background: rgba(96,165,250,.12); color: #93c5fd; }
  .recs-prio.low { background: var(--bg-elevated); color: var(--text-muted); }

  .notice.warn {
    border: 1px solid rgba(245, 158, 11, 0.35);
    color: #fbbf24;
    background: rgba(245, 158, 11, 0.08);
    padding: 10px 12px;
    border-radius: 10px;
    margin: 8px 0;
    font-size: 13px;
  }
  .notice.warn.compact { margin: 6px 0 10px; }

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

  .version-modal { max-width: min(920px, 94vw); width: 920px; }
  .version-toolbar {
    display: flex;
    gap: 10px;
    align-items: center;
    margin-bottom: 12px;
  }
  .version-toolbar .search { flex: 1; }
  .version-toolbar .secondary.mini.active {
    border-color: rgba(27,217,106,.4);
    color: var(--accent-primary);
  }
  .version-picker-body {
    display: grid;
    grid-template-columns: minmax(0, 1.05fr) minmax(0, 1fr);
    gap: 14px;
    min-height: 360px;
    max-height: min(70vh, 560px);
  }
  .version-list {
    display: grid;
    gap: 6px;
    overflow: auto;
    padding: 4px 2px 8px 0;
    align-content: start;
  }
  .version-row {
    display: flex; align-items: center; justify-content: space-between; gap: 12px;
    padding: 10px 12px; border-radius: 12px; border: 1px solid var(--border-color);
    background: var(--bg-tertiary); color: var(--text-secondary); text-align: left;
    width: 100%; transform: none;
  }
  .version-row:hover, .version-row.current, .version-row.selected {
    border-color: rgba(27,217,106,.35);
    background: rgba(27,217,106,.06);
  }
  .version-row.incompatible { opacity: 0.78; }
  .version-row:disabled { opacity: .5; cursor: wait; }
  .version-main { display: grid; gap: 3px; min-width: 0; flex: 1; }
  .version-title-row { display: flex; align-items: center; gap: 8px; min-width: 0; }
  .version-title-row strong { color: var(--text-primary); }
  .version-name { color: var(--text-secondary); font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .version-loaders { color: var(--text-muted); font-size: 12px; }
  .channel-dot {
    width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0;
    background: #22c55e;
  }
  .channel-dot.channel-beta { background: #3b82f6; }
  .channel-dot.channel-alpha { background: #f59e0b; }
  .channel-pill {
    font-size: 11px; font-weight: 700; text-transform: capitalize;
    padding: 2px 8px; border-radius: 999px; border: 1px solid var(--border-color);
  }
  .channel-pill.channel-release { color: #86efac; border-color: rgba(34,197,94,.35); }
  .channel-pill.channel-beta { color: #93c5fd; border-color: rgba(59,130,246,.35); }
  .channel-pill.channel-alpha { color: #fcd34d; border-color: rgba(245,158,11,.35); }
  .incompat-badge { color: #fbbf24; display: inline-flex; }
  .version-detail {
    display: flex; flex-direction: column; gap: 8px;
    padding: 12px 14px; border-radius: 14px; border: 1px solid var(--border-color);
    background: var(--bg-secondary); min-height: 0; overflow: hidden;
  }
  .version-detail-header { display: flex; align-items: center; gap: 10px; }
  .version-detail-header strong { font-size: 18px; color: var(--text-primary); }
  .version-changelog-full {
    flex: 1; overflow: auto; white-space: pre-wrap; font-size: 13px;
    line-height: 1.45; color: var(--text-secondary); padding-right: 4px;
  }
  .version-detail-actions { display: flex; justify-content: flex-end; padding-top: 8px; }
  .current-badge { font-size: 11px; font-weight: 800; color: var(--accent-primary); background: rgba(27,217,106,.15); padding: 4px 10px; border-radius: 999px; flex-shrink: 0; }
  .install-badge { font-size: 11px; font-weight: 700; color: var(--accent-secondary); background: rgba(139,92,246,.12); padding: 4px 10px; border-radius: 999px; flex-shrink: 0; }

  @media (max-width: 820px) {
    .version-picker-body { grid-template-columns: 1fr; max-height: none; }
    .version-list { max-height: 240px; }
  }

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
