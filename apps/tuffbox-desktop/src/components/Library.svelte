<script lang="ts">
  import { onMount } from "svelte";
  import {
    Library as LibraryIcon,
    Search,
    Play,
    Plus,
    Download,
    FolderOpen,
    Folder,
    Star,
    Compass,
    LayoutGrid,
    ExternalLink,
    MoreVertical,
    Copy,
    GitBranch,
    Share2,
    Wrench,
    Minus,
    Trash2,
    Package,
    Settings,
  } from "lucide-svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { open, confirm } from "@tauri-apps/plugin-dialog";
  import { open as openExternal } from "@tauri-apps/plugin-shell";
  import {
    recentProjects,
    projectPath,
    projectInfo,
    ideStageRequest,
    newProjectOpen,
    type RecentProject,
  } from "../lib/store";
  import { toasts } from "../lib/toast";
  import { api } from "../lib/api";
  import type { SearchResult } from "../lib/api";
  import { launchWithFeedback } from "../lib/launch";
  import CreationTrends from "./CreationTrends.svelte";
  import PromptDialog from "./PromptDialog.svelte";
  import AddInstanceModal from "./AddInstanceModal.svelte";

  export let currentView: "dashboard" | "ide" | "mods" | "graph" | "diagnostics" | "snapshots" | "configs" | "settings" | "project-settings" | "ore-gen" | "recipes" | "quests" | "library" | "chats" | "me" | "world";

  type Tab = "yours" | "discover" | "create";
  let tab: Tab = "yours";
  let swarmEnabled = false;

  async function loadSwarm() {
    try {
      const s = await invoke<{ enabled?: boolean }>("get_swarm_settings");
      swarmEnabled = !!s?.enabled;
    } catch {
      swarmEnabled = false;
    }
  }

  // ── Your packs (local instances) ────────────────────────────────
  let instanceSizes: Record<string, string> = {};
  let launching: string | null = null;
  let actionBusy = false;
  let ctxMenu: { x: number; y: number; project: RecentProject } | null = null;
  let showClonePrompt = false;
  let cloneTarget: RecentProject | null = null;
  let clonePromptName = "";

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
    closeCtxMenu();
    projectPath.set(project.path);
    projectInfo.set(project.info);
    ideStageRequest.set("content");
    currentView = "ide";
  }

  function openPackSettings(project: RecentProject) {
    closeCtxMenu();
    projectPath.set(project.path);
    projectInfo.set(project.info);
    currentView = "project-settings";
  }

  async function launchPack(project: RecentProject) {
    closeCtxMenu();
    launching = project.path;
    try {
      await invoke("set_last_opened_project", { path: project.path });
      await launchWithFeedback({ path: project.path, profile: "client" });
    } finally {
      launching = null;
    }
  }

  let importing = false;
  let importMenuOpen = false;

  function openNewPack() {
    newProjectOpen.set(true);
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
    const info = (await invoke("validate_project", { path })) as any;
    const manifestPath = info.manifestPath || path;
    recentProjects.add({ path: manifestPath, info });
    projectPath.set(manifestPath);
    projectInfo.set(info);
    toasts.success(
      `Imported "${result.name ?? info.name ?? "pack"}"${
        result.modCount != null ? ` · ${result.modCount} mods` : ""
      }`,
    );
    tab = "yours";
    ideStageRequest.set("content");
    currentView = "ide";
  }

  async function importFromSource(source: string) {
    importing = true;
    importMenuOpen = false;
    try {
      const targetDir = await resolveImportTargetDir();
      if (!targetDir) {
        toasts.error("Set a download/instances folder in Discover or Settings first.");
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

  async function onPackCreated(e: CustomEvent<string>) {
    newProjectOpen.set(false);
    const path = e.detail;
    try {
      const info = (await invoke("validate_project", { path })) as any;
      const manifestPath = info.manifestPath || path;
      recentProjects.add({ path: manifestPath, info });
      projectPath.set(manifestPath);
      projectInfo.set(info);
      toasts.success(`Created "${info.name ?? "pack"}"`);
      tab = "yours";
      ideStageRequest.set("content");
      currentView = "ide";
    } catch (err) {
      toasts.error(String(err));
    }
  }

  function openCtxMenu(e: MouseEvent, project: RecentProject) {
    e.preventDefault();
    e.stopPropagation();
    const pad = 8;
    const menuW = 220;
    const menuH = 360;
    let x = e.clientX;
    let y = e.clientY;
    if (x + menuW > window.innerWidth - pad) x = window.innerWidth - menuW - pad;
    if (y + menuH > window.innerHeight - pad) y = window.innerHeight - menuH - pad;
    ctxMenu = { x: Math.max(pad, x), y: Math.max(pad, y), project };
  }

  function closeCtxMenu() {
    ctxMenu = null;
  }

  function onGlobalPointerDown(e: MouseEvent) {
    if (importMenuOpen) {
      const t = e.target as HTMLElement | null;
      if (!t?.closest?.(".import-wrap") && !t?.closest?.(".import-menu")) {
        importMenuOpen = false;
      }
    }
    if (!ctxMenu || e.button === 2) return;
    const t = e.target as HTMLElement | null;
    if (t?.closest?.(".pack-ctx-menu")) return;
    closeCtxMenu();
  }

  function onGlobalKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      closeCtxMenu();
      importMenuOpen = false;
    }
  }

  async function runPackAction(action: string, project: RecentProject) {
    closeCtxMenu();
    switch (action) {
      case "open":
        openPack(project);
        break;
      case "play":
        await launchPack(project);
        break;
      case "settings":
        openPackSettings(project);
        break;
      case "open-folder":
        try {
          await invoke("open_project_folder", { path: project.path });
        } catch (e) {
          toasts.error(String(e));
        }
        break;
      case "copy-path":
        try {
          await navigator.clipboard.writeText(project.path);
          toasts.success("Path copied");
        } catch (e) {
          toasts.error(String(e));
        }
        break;
      case "clone":
        clonePromptName = `${project.info.name} copy`;
        cloneTarget = project;
        showClonePrompt = true;
        break;
      case "export":
        actionBusy = true;
        try {
          const exported: any = await api.export.modrinthPack(null, project.path);
          await navigator.clipboard.writeText(exported.path);
          toasts.success(`Exported .mrpack: ${exported.path}`);
        } catch (e) {
          toasts.error(String(e));
        } finally {
          actionBusy = false;
        }
        break;
      case "repair":
        actionBusy = true;
        try {
          const report: any = await invoke("repair_project", { path: project.path });
          const downloaded = report.downloaded?.length ?? 0;
          const failed = report.failed?.length ?? 0;
          toasts.success(
            downloaded === 0 && failed === 0
              ? "All mod files present and valid."
              : `Repaired: ${downloaded} file(s) re-downloaded${failed ? `, ${failed} failed` : ""}.`,
          );
        } catch (e) {
          toasts.error(String(e));
        } finally {
          actionBusy = false;
        }
        break;
      case "remove":
        recentProjects.remove(project.path);
        if ($projectPath === project.path) {
          const next = $recentProjects[0];
          projectPath.set(next?.path ?? null);
          projectInfo.set(next?.info ?? null);
        }
        toasts.info(`Removed "${project.info.name}" from library`);
        break;
      case "delete": {
        const ok = await confirm(`Delete "${project.info.name}" from disk?`, {
          title: "Delete pack",
          kind: "warning",
        });
        if (!ok) break;
        try {
          await invoke("delete_project", { path: project.path });
          recentProjects.remove(project.path);
          if ($projectPath === project.path) {
            const next = $recentProjects[0];
            projectPath.set(next?.path ?? null);
            projectInfo.set(next?.info ?? null);
          }
          toasts.success(`Deleted "${project.info.name}"`);
        } catch (e) {
          toasts.error(String(e));
        }
        break;
      }
    }
  }

  async function confirmClone(newName: string) {
    showClonePrompt = false;
    if (!cloneTarget || !newName.trim()) return;
    actionBusy = true;
    try {
      const clonedPath = await invoke<string>("clone_project", {
        path: cloneTarget.path,
        newName: newName.trim(),
      });
      const info = (await invoke("validate_project", { path: clonedPath })) as any;
      const manifestPath = info.manifestPath || clonedPath;
      recentProjects.add({ path: manifestPath, info });
      toasts.success(`Cloned to: ${manifestPath}`);
    } catch (e) {
      toasts.error(String(e));
    } finally {
      actionBusy = false;
      cloneTarget = null;
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
  let downloadDir = "";
  let defaultDownloadDir = "";

  async function loadDownloadDir() {
    try {
      const info = await api.launcher.instancesPathInfo();
      defaultDownloadDir = info.default;
      const settings = await api.launcher.get();
      downloadDir = (settings.instancesPath?.trim() || info.current || info.default).replace(/[\\/]+$/, "");
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

  async function openModpackPage(result: DiscoverResult) {
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
      if (!downloadDir && !defaultDownloadDir) {
        await loadDownloadDir();
      }
      const parent = (downloadDir || defaultDownloadDir).replace(/[\\/]+$/, "");
      if (!parent) {
        throw new Error("Choose a download folder first (Download to).");
      }
      // install_modpack creates `<targetDir>/<instanceId>` — pass the parent folder only.
      const targetDir = parent;
      let source: string;
      if (result.provider === "curseforge") {
        toasts.info(`Resolving CurseForge files for ${result.name}…`);
        const files = await invoke<Array<{ id: number; fileName?: string }>>("get_curseforge_modpack_files", {
          modId: Number(result.id),
          gameVersion: null,
        });
        const fileId = files?.[0]?.id;
        if (fileId == null) throw new Error("No CurseForge files available for this modpack.");
        source = `cf:${result.id}:${fileId}`;
        toasts.info(`Downloading ${files[0]?.fileName || result.name}…`);
      } else {
        source = await api.modpacks.getModpackUrl(result.id);
      }
      const res: any = await api.modpacks.install(source, targetDir, result.name);
      const info = await invoke("validate_project", { path: res.path }) as import("../lib/api").ProjectSummary;
      const manifestPath = info.manifestPath || res.path;
      recentProjects.add({ path: manifestPath, info: info as any });
      toasts.success(`Added "${result.name}" to ${targetDir}.`);
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
    void loadSwarm();
    void loadDownloadDir();
    if (tab === "discover") search();
  });

  function switchTab(t: Tab) {
    tab = t;
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

  $: discoverPlaceholder =
    discoverProvider === "curseforge"
      ? "Search CurseForge modpacks…"
      : discoverProvider === "both"
        ? "Search modpacks…"
        : "Search Modrinth modpacks…";
</script>

<div class="library fade-slide-in">
  <div class="library-header">
    <div class="title-row">
      <LibraryIcon size={22} />
      <h1>Library</h1>
    </div>
    <div class="header-actions">
      <div class="import-wrap">
        <button
          type="button"
          class="header-btn"
          class:busy={importing}
          disabled={importing}
          on:click|stopPropagation={() => (importMenuOpen = !importMenuOpen)}
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
            <button type="button" role="menuitem" on:click={importPackFile}>
              File (.mrpack / .zip)
            </button>
            <button type="button" role="menuitem" on:click={importInstanceFolder}>
              Instance folder
            </button>
          </div>
        {/if}
      </div>
      <div class="tabs">
        <button class:active={tab === "yours"} on:click={() => switchTab("yours")}>
          <LayoutGrid size={15} /> Your packs
        </button>
        <button class:active={tab === "discover"} on:click={() => switchTab("discover")}>
          <Compass size={15} /> Discover
        </button>
        <button class:active={tab === "create"} on:click={() => switchTab("create")} title="Create a new modpack">
          <Plus size={15} /> Create
        </button>
      </div>
    </div>
  </div>

  {#if tab === "yours"}
    {#if $recentProjects.length === 0}
      <div class="empty-state">
        <div class="empty-icon"><LibraryIcon size={40} /></div>
        <h3>No packs yet</h3>
        <p>Create or import a modpack to build your library.</p>
        <div class="empty-actions">
          <button class="empty-cta" type="button" on:click={openNewPack}>
            <Plus size={16} /> New pack
          </button>
          <button class="empty-cta ghost" type="button" on:click={() => (importMenuOpen = true)} disabled={importing}>
            <Download size={16} /> Import
          </button>
        </div>
      </div>
    {:else}
      <div class="pack-grid tb-stagger">
        {#each $recentProjects as project, i (project.path)}
          <div
            class="pack-card yours tb-card"
            class:active={$projectPath === project.path}
            style={`--i: ${i}`}
            role="button"
            tabindex="0"
            on:click={() => openPack(project)}
            on:keydown={(e) => e.key === "Enter" && openPack(project)}
            on:contextmenu={(e) => openCtxMenu(e, project)}
          >
            <div
              class="pack-cover"
              style={`background: linear-gradient(135deg, ${gradientFrom(project.info.name)}, ${gradientFrom(project.info.id)})`}
            >
              <span class="pack-cover-letter tb-cover-media">{project.info.name[0]}</span>
              <button
                class="pack-play"
                class:busy={launching === project.path}
                type="button"
                on:click|stopPropagation={() => launchPack(project)}
                title="Play"
                aria-label="Play {project.info.name}"
              >
                {#if launching === project.path}
                  <span class="mini-spinner"></span>
                {:else}
                  <Play size={20} fill="currentColor" />
                {/if}
              </button>
            </div>
            <div class="pack-body">
              <span class="pack-name" title={project.info.name}>{project.info.name}</span>
              <span class="pack-meta">{project.info.minecraftVersion} · {project.info.loaderKind}</span>
              <div class="pack-footer">
                <span class="pack-size">{instanceSizes[project.path] || "…"}</span>
                <div class="pack-btns">
                  <button
                    type="button"
                    class="icon-act"
                    title="Open in IDE → Mods"
                    on:click|stopPropagation={() => openPack(project)}
                  >
                    <Package size={14} />
                  </button>
                  <button
                    type="button"
                    class="icon-act"
                    title="Open folder"
                    on:click|stopPropagation={() => runPackAction("open-folder", project)}
                  >
                    <Folder size={14} />
                  </button>
                  <button
                    type="button"
                    class="icon-act"
                    title="Settings"
                    on:click|stopPropagation={() => openPackSettings(project)}
                  >
                    <Settings size={14} />
                  </button>
                  <button
                    type="button"
                    class="icon-act more"
                    title="More actions"
                    on:click|stopPropagation={(e) => openCtxMenu(e, project)}
                  >
                    <MoreVertical size={14} />
                  </button>
                </div>
              </div>
            </div>
          </div>
        {/each}

        <button class="pack-card add-card tb-card" style={`--i: ${$recentProjects.length}`} type="button" on:click={openNewPack}>
          <div class="pack-cover add-cover"><Plus size={28} class="tb-cover-media" /></div>
          <div class="pack-body"><span class="pack-name">New pack</span></div>
        </button>
      </div>
    {/if}
  {:else if tab === "discover"}
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

    <div class="download-path">
      <label for="lib-download-dir">Download to</label>
      <div class="path-row">
        <input
          id="lib-download-dir"
          bind:value={downloadDir}
          placeholder={defaultDownloadDir || "Choose a folder for modpacks"}
        />
        <button type="button" class="path-btn" on:click={browseDownloadDir} title="Browse">
          <FolderOpen size={15} />
        </button>
        <button type="button" class="path-btn save" on:click={applyDownloadDir}>Save</button>
      </div>
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
      <div class="pack-grid tb-stagger">
        {#each results as result, i (resultKey(result))}
          <div class="pack-card discover-card tb-card" style={`--i: ${i}`}>
            <div class="pack-cover" style={result.iconUrl ? `background: #18181b` : `background: linear-gradient(135deg, ${gradientFrom(result.name)}, ${gradientFrom(result.slug)})`}>
              {#if result.iconUrl}
                <img class="pack-cover-img tb-cover-media" src={result.iconUrl} alt="" />
              {:else}
                <span class="pack-cover-letter tb-cover-media">{result.name[0]}</span>
              {/if}
            </div>
            <div class="pack-body">
              <div class="pack-title-row">
                <button
                  type="button"
                  class="pack-name linkish"
                  title="Open on {result.provider === 'curseforge' ? 'CurseForge' : 'Modrinth'}"
                  on:click={() => openModpackPage(result)}
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
                  title="Open catalog page"
                  on:click={() => openModpackPage(result)}
                >
                  <ExternalLink size={14} /> Page
                </button>
                <button class="pack-add" disabled={adding.has(resultKey(result))} on:click={() => addModpack(result)}>
                  {#if adding.has(resultKey(result))}<span class="mini-spinner"></span> Adding…{:else}<Plus size={14} /> Add to TuffBox{/if}
                </button>
              </div>
            </div>
          </div>
        {/each}
      </div>
    {/if}
  {:else if tab === "create"}
    <div class="create-pane">
      <div class="create-actions">
        <button type="button" class="create-plus" on:click={openNewPack}>
          <span class="plus-ring"><Plus size={40} strokeWidth={2.25} /></span>
          <strong>Create modpack</strong>
          <span>Blank instance or CurseForge browse</span>
        </button>
        <button type="button" class="create-plus import" on:click={() => (importMenuOpen = true)} disabled={importing}>
          <span class="plus-ring"><Download size={36} strokeWidth={2.25} /></span>
          <strong>{importing ? "Importing…" : "Import pack"}</strong>
          <span>.mrpack · zip · Prism · MultiMC · CurseForge · mods folder</span>
        </button>
      </div>
      <div class="create-trends">
        <CreationTrends {swarmEnabled} />
      </div>
    </div>
  {/if}
</div>

{#if ctxMenu}
  {@const menuProject = ctxMenu.project}
  <div
    class="pack-ctx-menu"
    style={`left:${ctxMenu.x}px; top:${ctxMenu.y}px`}
    role="menu"
  >
    <button type="button" role="menuitem" on:click={() => runPackAction("open", menuProject)} disabled={actionBusy}>
      <Package size={14} /> Open in IDE → Mods
    </button>
    <button type="button" role="menuitem" on:click={() => runPackAction("play", menuProject)} disabled={actionBusy || launching === menuProject.path}>
      <Play size={14} /> Play
    </button>
    <button type="button" role="menuitem" on:click={() => runPackAction("open-folder", menuProject)} disabled={actionBusy}>
      <Folder size={14} /> Open folder
    </button>
    <button type="button" role="menuitem" on:click={() => runPackAction("settings", menuProject)} disabled={actionBusy}>
      <Settings size={14} /> Settings
    </button>
    <div class="menu-sep"></div>
    <button type="button" role="menuitem" on:click={() => runPackAction("copy-path", menuProject)} disabled={actionBusy}>
      <Copy size={14} /> Copy path
    </button>
    <button type="button" role="menuitem" on:click={() => runPackAction("clone", menuProject)} disabled={actionBusy}>
      <GitBranch size={14} /> Clone
    </button>
    <button type="button" role="menuitem" on:click={() => runPackAction("export", menuProject)} disabled={actionBusy}>
      <Share2 size={14} /> Export .mrpack
    </button>
    <button type="button" role="menuitem" on:click={() => runPackAction("repair", menuProject)} disabled={actionBusy}>
      <Wrench size={14} /> Repair
    </button>
    <div class="menu-sep"></div>
    <button type="button" role="menuitem" on:click={() => runPackAction("remove", menuProject)} disabled={actionBusy}>
      <Minus size={14} /> Remove from library
    </button>
    <button type="button" role="menuitem" class="danger" on:click={() => runPackAction("delete", menuProject)} disabled={actionBusy}>
      <Trash2 size={14} /> Delete from disk
    </button>
  </div>
{/if}

{#if showClonePrompt && cloneTarget}
  <PromptDialog
    title="Clone pack"
    message={`Create a copy of "${cloneTarget.info.name}"`}
    mode="text"
    defaultValue={clonePromptName}
    confirmLabel="Clone"
    on:confirm={(e) => confirmClone(e.detail)}
    on:cancel={() => { showClonePrompt = false; cloneTarget = null; }}
  />
{/if}

{#if $newProjectOpen}
  <AddInstanceModal
    on:close={() => newProjectOpen.set(false)}
    on:created={onPackCreated}
  />
{/if}

<svelte:window on:mousedown={onGlobalPointerDown} on:keydown={onGlobalKeydown} />

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
  .header-actions {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }
  .import-wrap { position: relative; }
  .header-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 8px 14px;
    border-radius: 999px;
    background: rgba(27, 217, 106, 0.12);
    border: 1px solid rgba(27, 217, 106, 0.35);
    color: var(--accent-primary);
    font-size: 13px;
    font-weight: 700;
    cursor: pointer;
  }
  .header-btn:disabled { opacity: 0.6; cursor: default; }
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
    border-radius: 8px;
    background: transparent;
    color: var(--text-primary);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }
  .import-menu button:hover {
    background: rgba(27, 217, 106, 0.12);
    color: var(--accent-primary);
  }
  .title-row { display: flex; align-items: center; gap: 10px; color: var(--accent-primary); }
  .title-row h1 { margin: 0; font-size: 22px; color: var(--text-primary); }

  .tabs { display: flex; gap: 6px; }
  .tabs button {
    display: flex; align-items: center; gap: 6px;
    padding: 8px 14px; border-radius: 999px;
    background: var(--bg-secondary); border: 1px solid var(--border-color);
    color: var(--text-secondary); font-size: 13px; font-weight: 600; cursor: pointer;
    transition:
      transform var(--motion-fast) var(--ease-spring),
      background var(--motion-fast) var(--ease-out),
      border-color var(--motion-fast) var(--ease-out),
      color var(--motion-fast) var(--ease-out);
  }
  .tabs button:hover { background: var(--bg-hover); color: var(--text-primary); }
  .tabs button:active:not(:disabled) { transform: scale(0.96); }
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
    overflow: visible;
    text-align: left;
    cursor: pointer;
    display: flex;
    flex-direction: column;
    position: relative;
    transition:
      transform var(--motion-fast) var(--ease-spring),
      border-color var(--motion-fast) var(--ease-out),
      background var(--motion-fast) var(--ease-out);
  }
  .pack-card:hover { background: var(--bg-tertiary); border-color: rgba(27, 217, 106, 0.28); }
  .pack-card.yours:focus-visible {
    outline: 2px solid var(--accent-primary);
    outline-offset: 2px;
  }
  .pack-card.active {
    border-color: rgba(27, 217, 106, 0.45);
    box-shadow: 0 0 0 1px rgba(27, 217, 106, 0.2);
  }

  .pack-cover {
    position: relative;
    height: 120px;
    display: flex; align-items: center; justify-content: center;
    overflow: hidden;
    border-radius: var(--border-radius-lg) var(--border-radius-lg) 0 0;
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
    transition: transform var(--motion-fast) var(--ease-spring);
  }
  .pack-play:hover { transform: scale(1.1); }
  .pack-play.busy { opacity: 0.8; cursor: default; }

  .pack-body { padding: 12px 14px 14px; display: flex; flex-direction: column; gap: 4px; flex: 1; }
  .pack-name {
    font-weight: 700; font-size: 14px; color: var(--text-primary);
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  button.pack-name.linkish {
    background: none; border: none; padding: 0; margin: 0;
    cursor: pointer; text-align: left; font: inherit; font-weight: 700; font-size: 14px;
    color: var(--text-primary); max-width: 100%;
  }
  button.pack-name.linkish:hover {
    color: var(--accent-primary); text-decoration: underline;
  }
  .pack-meta { font-size: 12px; color: var(--text-muted); text-transform: capitalize; }
  .pack-desc {
    margin: 4px 0 0; font-size: 12px; color: var(--text-muted); line-height: 1.4;
    display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;
    min-height: 34px;
  }
  .pack-footer { display: flex; align-items: center; justify-content: space-between; margin-top: auto; padding-top: 8px; gap: 8px; }
  .pack-size { font-size: 12px; color: var(--text-muted); flex-shrink: 0; }
  .pack-btns {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .icon-act {
    width: 28px;
    height: 28px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 8px;
    border: 1px solid var(--border-color);
    background: var(--bg-tertiary);
    color: var(--text-secondary);
    cursor: pointer;
    padding: 0;
  }
  .icon-act:hover {
    border-color: rgba(27, 217, 106, 0.4);
    color: var(--accent-primary);
    background: rgba(27, 217, 106, 0.08);
  }
  .icon-act.more:hover {
    border-color: var(--border-color);
    color: var(--text-primary);
    background: var(--bg-hover);
  }

  .pack-ctx-menu {
    position: fixed;
    z-index: 90;
    min-width: 210px;
    padding: 6px;
    border-radius: 10px;
    border: 1px solid var(--border-color);
    background: var(--bg-elevated, #1a1f28);
    box-shadow: 0 16px 40px rgba(0, 0, 0, 0.45);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .pack-ctx-menu button {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 8px 10px;
    border: none;
    border-radius: 8px;
    background: transparent;
    color: var(--text-primary);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    text-align: left;
  }
  .pack-ctx-menu button:hover:not(:disabled) {
    background: rgba(27, 217, 106, 0.12);
    color: var(--accent-primary);
  }
  .pack-ctx-menu button:disabled { opacity: 0.45; cursor: default; }
  .pack-ctx-menu button.danger:hover:not(:disabled) {
    background: rgba(239, 68, 68, 0.12);
    color: #f87171;
  }
  .pack-ctx-menu .menu-sep {
    height: 1px;
    background: var(--border-color);
    margin: 4px 2px;
  }

  .empty-actions {
    display: flex;
    gap: 10px;
    flex-wrap: wrap;
    justify-content: center;
  }
  .empty-cta {
    margin-top: 4px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 10px 16px;
    border-radius: 10px;
    border: none;
    background: var(--accent-primary);
    color: #000;
    font-weight: 700;
    font-size: 13px;
    cursor: pointer;
  }
  .empty-cta:hover { background: var(--accent-hover); }
  .empty-cta.ghost {
    background: transparent;
    border: 1px solid var(--border-color);
    color: var(--text-secondary);
  }
  .empty-cta.ghost:hover {
    border-color: rgba(27, 217, 106, 0.4);
    color: var(--accent-primary);
  }

  .create-pane {
    display: flex;
    flex-direction: column;
    gap: 22px;
  }
  .create-actions {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
    gap: 14px;
  }
  .create-plus {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    min-height: 200px;
    padding: 28px 20px;
    border-radius: var(--border-radius-xl);
    border: 2px dashed rgba(27, 217, 106, 0.35);
    background:
      radial-gradient(ellipse at 50% 0%, rgba(27, 217, 106, 0.12), transparent 60%),
      var(--bg-secondary);
    color: var(--text-secondary);
    cursor: pointer;
    transition:
      border-color var(--motion-fast) var(--ease-out),
      background var(--motion-fast) var(--ease-out),
      transform var(--motion-fast) var(--ease-spring);
  }
  .create-plus.import {
    border-color: rgba(59, 130, 246, 0.35);
    background:
      radial-gradient(ellipse at 50% 0%, rgba(59, 130, 246, 0.12), transparent 60%),
      var(--bg-secondary);
  }
  .create-plus.import .plus-ring {
    background: rgba(59, 130, 246, 0.14);
    color: #60a5fa;
    border-color: rgba(59, 130, 246, 0.35);
  }
  .create-plus:hover {
    border-color: rgba(27, 217, 106, 0.6);
    color: var(--text-primary);
    transform: translateY(-1px);
  }
  .create-plus.import:hover {
    border-color: rgba(59, 130, 246, 0.6);
  }
  .create-plus:disabled { opacity: 0.6; cursor: default; transform: none; }
  .create-plus strong {
    font-size: 18px;
    color: var(--text-primary);
  }
  .create-plus > span:last-child {
    font-size: 13px;
    color: var(--text-muted);
    text-align: center;
  }
  .plus-ring {
    width: 72px;
    height: 72px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(27, 217, 106, 0.14);
    color: var(--accent-primary);
    border: 1px solid rgba(27, 217, 106, 0.35);
  }
  .create-trends {
    min-width: 0;
  }
  .mini-spinner.dark {
    border-color: rgba(27, 217, 106, 0.25);
    border-top-color: var(--accent-primary);
  }

  .pack-stats { display: flex; gap: 12px; font-size: 12px; color: var(--text-muted); margin-top: 6px; }
  .pack-stats span { display: inline-flex; align-items: center; gap: 4px; }

  .pack-actions {
    margin-top: 10px;
    display: flex; gap: 8px; align-items: stretch;
  }
  .pack-page {
    display: inline-flex; align-items: center; justify-content: center; gap: 5px;
    padding: 8px 10px; border-radius: 8px; font-size: 12px; font-weight: 600;
    background: var(--bg-tertiary); border: 1px solid var(--border-color);
    color: var(--text-secondary); cursor: pointer; flex-shrink: 0;
  }
  .pack-page:hover { border-color: var(--accent-primary); color: var(--accent-primary); }
  .pack-add {
    flex: 1;
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
    background: rgba(27, 217, 106, 0.12);
    border-color: rgba(27, 217, 106, 0.35);
    color: var(--accent-primary);
  }

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
