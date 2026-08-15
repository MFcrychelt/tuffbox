<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { fade, fly, slide } from "svelte/transition";
  import { quintOut } from "svelte/easing";
  import {
    Plus,
    Folder,
    FolderOpen,
    Settings,
    HelpCircle,
    RefreshCw,
    Play,
    Square,
    Tags,
    Share2,
    Copy,
    Trash2,
    Link2,
    ChevronDown,
    ChevronRight,
    Package,
    Wrench,
    Minus,
    Compass,
  } from "@lucide/svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { open as openDialog, confirm } from "@tauri-apps/plugin-dialog";
  import { open as openShell } from "@tauri-apps/plugin-shell";
  import {
    recentProjects,
    projectPath,
    projectInfo,
    ideStageRequest,
    openAddInstance,
    runningInstances,
    isProjectRunning,
    formatPlaytime,
    authState,
    skinPath,
    loginTypeLabel,
    loginModalOpen,
    type RecentProject,
  } from "../lib/store";
  import { toasts } from "../lib/toast";
  import { api } from "../lib/api";
  import { copyText } from "../lib/clipboard";
  import { launchWithFeedback, killWithFeedback } from "../lib/launch";
  import {
    DEFAULT_GROUP,
    loadGroupMap,
    setGroup,
    getGroup,
    loadCollapsedGroups,
    toggleCollapsed,
    listGroupNames,
    folderFromDrop,
    type GroupMap,
  } from "../lib/libraryGroups";
  import { portal } from "../lib/portal";
  import PromptDialog from "./PromptDialog.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import HeadAvatar from "./HeadAvatar.svelte";

  let {
    currentView = $bindable(),
    onDiscover = undefined,
    onCreate = undefined,
  }: {
    currentView: "dashboard" | "ide" | "mods" | "graph" | "diagnostics" | "snapshots" | "configs" | "settings" | "project-settings" | "ore-gen" | "recipes" | "quests" | "library" | "chats" | "me" | "world";
    onDiscover?: () => void;
    onCreate?: () => void;
  } = $props();

  const LONG_PRESS_MS = 420;
  const MOVE_CANCEL_PX = 10;

  let selectedPath = $state<string | null>($projectPath);
  let launching = $state<string | null>(null);
  let actionBusy = $state(false);
  let exportMenuOpen = $state(false);
  let addMenuOpen = $state(false);
  let githubImportOpen = $state(false);
  let githubConfirmOpen = $state(false);
  let githubPendingSource = $state("");
  let githubInspectSummary = $state("");
  let foldersMenuOpen = $state(false);
  let groupMap = $state<GroupMap>(loadGroupMap());
  let collapsed = $state(loadCollapsedGroups());
  let projectStats = $state<Record<string, { playtime: number }>>({});
  let refreshing = $state(false);

  let showClonePrompt = $state(false);
  let cloneTarget = $state<RecentProject | null>(null);
  let clonePromptName = $state("");

  let showGroupPrompt = $state(false);
  let groupTarget = $state<RecentProject | null>(null);
  let groupPromptName = $state(DEFAULT_GROUP);

  let ctxMenu = $state<{ x: number; y: number; project: RecentProject } | null>(null);

  /** Android-style long-press → drag onto another tile to make a folder. */
  let dragSource = $state<RecentProject | null>(null);
  let dropTargetPath = $state<string | null>(null);
  let dropTargetGroup = $state<string | null>(null);
  let dragGhost = $state<{ x: number; y: number; letter: string; colorA: string; colorB: string } | null>(null);
  let suppressNextClick = $state(false);
  let longPressTimer = $state<ReturnType<typeof setTimeout> | null>(null);
  let pressOrigin = $state<{ x: number; y: number; project: RecentProject } | null>(null);
  let dragging = $state(false);
  let holdingPath = $state<string | null>(null);

  function prefersReducedMotion(): boolean {
    if (typeof document === "undefined") return true;
    if (document.documentElement.classList.contains("potato-pc")) return true;
    return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  }

  function tileIntro(node: Element) {
    if (prefersReducedMotion()) return { duration: 0 };
    return fade(node, { duration: 160 });
  }

  function sideIntro(node: Element) {
    if (prefersReducedMotion()) return { duration: 0 };
    return fly(node, { x: 12, duration: 200, opacity: 0, easing: quintOut });
  }

  function groupBodyIntro(node: Element) {
    if (prefersReducedMotion()) return { duration: 0 };
    return slide(node, { duration: 200, easing: quintOut });
  }

  const selected = $derived($recentProjects.find((p) => p.path === selectedPath) ?? null);
  const selectedRunning = $derived(
    isProjectRunning(selectedPath, $runningInstances),
  );
  let selectingPath = false;
  $effect(() => {
    const recent = $recentProjects;
    const current = $projectPath;
    if (selectingPath) return;
    if (selectedPath && !recent.some((p) => p.path === selectedPath)) {
      selectedPath = recent[0]?.path ?? null;
      return;
    }
    if (!selectedPath && recent.length) {
      selectedPath =
        current && recent.some((p) => p.path === current) ? current : recent[0].path;
    }
  });

  const grouped = $derived((() => {
    const byGroup = new Map<string, RecentProject[]>();
    for (const p of $recentProjects) {
      const g = getGroup(groupMap, p.path);
      const list = byGroup.get(g) ?? [];
      list.push(p);
      byGroup.set(g, list);
    }
    const names = listGroupNames(
      groupMap,
      $recentProjects.map((p) => p.path),
    ).filter((n) => byGroup.has(n));
    return names.map((name) => ({
      name,
      projects: byGroup.get(name) ?? [],
      collapsed: collapsed.has(name),
    }));
  })());

  const existingGroups = $derived(listGroupNames(
    groupMap,
    $recentProjects.map((p) => p.path),
  ));

  function gradientFrom(name: string) {
    const colors = ["var(--accent-primary)", "var(--accent-secondary)", "#3b82f6", "#f59e0b", "#ec4899", "#06b6d4", "#ef4444"];
    let hash = 0;
    for (let i = 0; i < name.length; i++) hash = name.charCodeAt(i) + ((hash << 5) - hash);
    return colors[Math.abs(hash) % colors.length];
  }

  async function loadStats(path: string) {
    try {
      const s = await api.stats.get(path);
      projectStats[path] = { playtime: s.totalPlaytimeSeconds ?? 0 };
      projectStats = { ...projectStats };
    } catch {
      projectStats[path] = { playtime: 0 };
      projectStats = { ...projectStats };
    }
  }

  function ensureStats(paths: string[]) {
    for (const path of paths) {
      if (projectStats[path] !== undefined) continue;
      void loadStats(path);
    }
  }

  $effect(() => {
    ensureStats($recentProjects.map((p) => p.path));
  });

  async function selectInstance(
    project: RecentProject,
    opts?: { keepMenus?: boolean },
  ): Promise<string> {
    if (!opts?.keepMenus) closeMenus();
    selectingPath = true;
    // Optimistic select so tiles feel clickable even while validate runs.
    selectedPath = project.path;
    try {
      const info = (await invoke("validate_project", {
        path: project.path,
      })) as RecentProject["info"] & { manifestPath?: string };
      const manifestPath = info.manifestPath || project.path;
      recentProjects.add(
        { path: manifestPath, info },
        {
          reorder: false,
          ...(manifestPath !== project.path
            ? { replacePath: project.path }
            : {}),
        },
      );
      selectedPath = manifestPath;
      projectPath.set(manifestPath);
      projectInfo.set(info);
      return manifestPath;
    } catch {
      selectedPath = project.path;
      projectPath.set(project.path);
      projectInfo.set(project.info);
      return project.path;
    } finally {
      selectingPath = false;
    }
  }

  function openInIde(project: RecentProject) {
    void selectInstance(project);
    ideStageRequest.set("content");
    currentView = "ide";
  }

  function openEdit(project: RecentProject) {
    // Edit = open the pack in IDE Content (mods), not Setup.
    openInIde(project);
  }

  async function launchInstance(project: RecentProject) {
    closeMenus();
    if (
      isProjectRunning(project.path, $runningInstances)
    ) {
      return;
    }
    launching = project.path;
    try {
      const path = await selectInstance(project);
      if (
        isProjectRunning(path, $runningInstances) ||
        isProjectRunning(project.path, $runningInstances)
      ) {
        return;
      }
      await invoke("set_last_opened_project", { path });
      await launchWithFeedback({ path, profile: "client" });
    } finally {
      launching = null;
      void loadStats(selectedPath ?? project.path);
    }
  }

  async function stopInstance(project: RecentProject) {
    closeMenus();
    if (!isProjectRunning(project.path, $runningInstances)) return;
    await killWithFeedback(project.path);
  }

  function closeMenus() {
    ctxMenu = null;
    exportMenuOpen = false;
    addMenuOpen = false;
    foldersMenuOpen = false;
  }

  function clearLongPressTimer() {
    if (longPressTimer != null) {
      clearTimeout(longPressTimer);
      longPressTimer = null;
    }
  }

  function endDrag() {
    clearLongPressTimer();
    pressOrigin = null;
    holdingPath = null;
    dragging = false;
    dragSource = null;
    dropTargetPath = null;
    dropTargetGroup = null;
    dragGhost = null;
  }

  function beginDrag(
    project: RecentProject,
    x: number,
    y: number,
    target?: EventTarget | null,
    pointerId?: number,
  ) {
    dragging = true;
    holdingPath = null;
    dragSource = project;
    closeMenus();
    selectInstance(project);
    dragGhost = {
      x,
      y,
      letter: project.info.name[0]?.toUpperCase() ?? "?",
      colorA: gradientFrom(project.info.name),
      colorB: gradientFrom(project.info.id),
    };
    if (target && pointerId != null && target instanceof HTMLElement) {
      try {
        target.setPointerCapture(pointerId);
      } catch {
        /* ignore */
      }
    }
    try {
      navigator.vibrate?.(12);
    } catch {
      /* ignore */
    }
  }

  function hitTestDrop(clientX: number, clientY: number) {
    dropTargetPath = null;
    dropTargetGroup = null;
    if (!dragSource) return;

    // Prefer rect hit-testing over elementFromPoint — CSS `zoom` on `.app-shell`
    // can desync the latter from the visual cursor in Chromium/Electron.
    const tiles = document.querySelectorAll<HTMLElement>(".prism-lib .inst-tile[data-path]");
    for (const tile of tiles) {
      const r = tile.getBoundingClientRect();
      if (clientX < r.left || clientX > r.right || clientY < r.top || clientY > r.bottom) continue;
      const path = tile.dataset.path ?? null;
      if (path && path !== dragSource.path) {
        dropTargetPath = path;
        return;
      }
    }

    const headers = document.querySelectorAll<HTMLElement>(".prism-lib .group-header[data-group]");
    for (const header of headers) {
      const r = header.getBoundingClientRect();
      if (clientX < r.left || clientX > r.right || clientY < r.top || clientY > r.bottom) continue;
      const name = header.dataset.group ?? null;
      if (name && name !== getGroup(groupMap, dragSource.path)) {
        dropTargetGroup = name;
      }
      return;
    }
  }

  function applyDrop() {
    if (!dragSource) return;
    if (dropTargetPath) {
      const target = $recentProjects.find((p) => p.path === dropTargetPath);
      if (!target) return;
      const result = folderFromDrop(
        groupMap,
        dragSource.path,
        target.path,
        target.info.name,
      );
      if (!result) return;
      groupMap = result.map;
      // Ensure the new/merged folder is expanded so the user sees the result.
      if (collapsed.has(result.groupName)) {
        collapsed = toggleCollapsed(collapsed, result.groupName);
      }
      toasts.success(
        result.created
          ? `Folder “${result.groupName}” created`
          : `Moved into “${result.groupName}”`,
      );
      return;
    }
    if (dropTargetGroup) {
      const name = dropTargetGroup;
      groupMap = setGroup(groupMap, dragSource.path, name);
      if (collapsed.has(name)) {
        collapsed = toggleCollapsed(collapsed, name);
      }
      toasts.success(
        name === DEFAULT_GROUP ? "Removed from folder" : `Moved into “${name}”`,
      );
    }
  }

  function onTilePointerDown(e: PointerEvent, project: RecentProject) {
    if (e.button !== 0) return;
    clearLongPressTimer();
    holdingPath = project.path;
    pressOrigin = { x: e.clientX, y: e.clientY, project };
    const origin = pressOrigin;
    const target = e.currentTarget;
    const pointerId = e.pointerId;
    longPressTimer = setTimeout(() => {
      longPressTimer = null;
      if (!pressOrigin || pressOrigin.project.path !== origin.project.path) return;
      beginDrag(origin.project, origin.x, origin.y, target, pointerId);
      suppressNextClick = true;
    }, LONG_PRESS_MS);
  }

  function onTilePointerMove(e: PointerEvent) {
    if (pressOrigin && !dragging) {
      const dx = e.clientX - pressOrigin.x;
      const dy = e.clientY - pressOrigin.y;
      if (dx * dx + dy * dy > MOVE_CANCEL_PX * MOVE_CANCEL_PX) {
        clearLongPressTimer();
        pressOrigin = null;
        holdingPath = null;
      }
      return;
    }
    if (!dragging || !dragSource || !dragGhost) return;
    dragGhost = { ...dragGhost, x: e.clientX, y: e.clientY };
    hitTestDrop(e.clientX, e.clientY);
  }

  function onTilePointerUp(e: PointerEvent) {
    clearLongPressTimer();
    if (dragging) {
      hitTestDrop(e.clientX, e.clientY);
      applyDrop();
      suppressNextClick = true;
      endDrag();
      setTimeout(() => {
        suppressNextClick = false;
      }, 80);
      return;
    }
    pressOrigin = null;
  }

  function onTilePointerCancel() {
    clearLongPressTimer();
    if (dragging) {
      suppressNextClick = true;
      endDrag();
      setTimeout(() => {
        suppressNextClick = false;
      }, 80);
    }
    pressOrigin = null;
  }

  function onTileClick(project: RecentProject) {
    if (suppressNextClick || dragging) {
      suppressNextClick = false;
      return;
    }
    selectInstance(project);
  }

  function openCtxMenu(e: MouseEvent, project: RecentProject) {
    if (dragging) {
      e.preventDefault();
      return;
    }
    e.preventDefault();
    e.stopPropagation();
    const pad = 8;
    const menuW = 230;
    const menuH = 420;
    // Zoom is on <html> — clientX/Y and position:fixed share one frame.
    let x = e.clientX;
    let y = e.clientY;
    if (x + menuW > window.innerWidth - pad) x = window.innerWidth - menuW - pad;
    if (y + menuH > window.innerHeight - pad) y = window.innerHeight - menuH - pad;
    void selectInstance(project, { keepMenus: true });
    ctxMenu = { x: Math.max(pad, x), y: Math.max(pad, y), project };
  }

  function onGlobalPointerDown(e: MouseEvent) {
    const t = e.target as HTMLElement | null;
    if (addMenuOpen && !t?.closest?.(".tb-add-wrap")) addMenuOpen = false;
    if (foldersMenuOpen && !t?.closest?.(".tb-folders-wrap")) foldersMenuOpen = false;
    if (exportMenuOpen && !t?.closest?.(".tb-export-wrap")) exportMenuOpen = false;
    if (!ctxMenu || e.button === 2) return;
    if (t?.closest?.(".pack-ctx-menu")) return;
    ctxMenu = null;
  }

  function onGlobalKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      if (dragging) {
        endDrag();
        return;
      }
      closeMenus();
    }
  }

  function toggleGroup(name: string) {
    collapsed = toggleCollapsed(collapsed, name);
  }

  async function openInstancesFolder() {
    foldersMenuOpen = false;
    try {
      const info = await api.launcher.instancesPathInfo();
      const settings = await api.launcher.get();
      const dir = (settings.instancesPath?.trim() || info.current || info.default || "").replace(
        /[\\/]+$/,
        "",
      );
      if (!dir) {
        toasts.error("Instances folder is not set.");
        return;
      }
      await openShell(dir);
    } catch (e) {
      toasts.error(String(e));
    }
  }

  async function openSelectedFolder(project: RecentProject) {
    try {
      await invoke("open_project_folder", { path: project.path });
    } catch (e) {
      toasts.error(String(e));
    }
  }

  async function refreshAll() {
    refreshing = true;
    try {
      const next: RecentProject[] = [];
      for (const p of $recentProjects) {
        try {
          const info = (await invoke("validate_project", { path: p.path })) as RecentProject["info"] & {
            manifestPath?: string;
          };
          const manifestPath = (info as { manifestPath?: string }).manifestPath || p.path;
          next.push({ path: manifestPath, info: info as RecentProject["info"] });
        } catch {
          next.push(p);
        }
      }
      recentProjects.set(next);
      projectStats = {};
      ensureStats(next.map((p) => p.path));
      if (selectedPath) {
        const sel = next.find((p) => p.path === selectedPath);
        if (sel) projectInfo.set(sel.info);
      }
      toasts.info("Library refreshed");
    } catch (e) {
      toasts.error(String(e));
    } finally {
      refreshing = false;
    }
  }

  async function importPackFile() {
    addMenuOpen = false;
    const selected = await openDialog({
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
    addMenuOpen = false;
    const selected = await openDialog({
      directory: true,
      multiple: false,
      title: "Import Prism / MultiMC / CurseForge / mods folder",
    });
    if (typeof selected !== "string" || !selected) return;
    await importFromSource(selected);
  }

  async function importGithubRepo() {
    addMenuOpen = false;
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
      githubInspectSummary = `${info.fullName || trimmed}${version} · ${ready}. Install anonymously?`;
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

  async function resolveImportTargetDir(): Promise<string> {
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

  async function importFromSource(source: string) {
    actionBusy = true;
    try {
      const targetDir = await resolveImportTargetDir();
      if (!targetDir) {
        toasts.error("Set an instances folder in Settings first.");
        return;
      }
      const result: { path?: string; name?: string; modCount?: number } = await invoke(
        "install_modpack",
        { source, targetDir, instanceName: null },
      );
      const path = result.path;
      if (!path) throw new Error("Import returned no path");
      const info = (await invoke("validate_project", { path })) as RecentProject["info"] & {
        manifestPath?: string;
      };
      const manifestPath = info.manifestPath || path;
      recentProjects.add({ path: manifestPath, info: info as RecentProject["info"] });
      selectInstance({ path: manifestPath, info: info as RecentProject["info"] });
      toasts.success(`Imported "${result.name ?? info.name ?? "pack"}"`);
    } catch (e) {
      toasts.error(String(e));
    } finally {
      actionBusy = false;
    }
  }

  async function runAction(action: string, project: RecentProject) {
    closeMenus();
    switch (action) {
      case "launch":
        await launchInstance(project);
        break;
      case "stop":
        await stopInstance(project);
        break;
      case "edit":
        openEdit(project);
        break;
      case "change-group":
        groupPromptName = getGroup(groupMap, project.path);
        groupTarget = project;
        showGroupPrompt = true;
        break;
      case "folder":
        await openSelectedFolder(project);
        break;
      case "export-mrpack":
        actionBusy = true;
        try {
          const exported = await api.export.modrinthPack(null, project.path);
          try {
            await copyText(exported.path);
            toasts.success(`Exported .mrpack — path copied: ${exported.path}`);
          } catch {
            toasts.success(`Exported .mrpack: ${exported.path}`);
          }
        } catch (e) {
          toasts.error(String(e));
        } finally {
          actionBusy = false;
        }
        break;
      case "export-prism":
        actionBusy = true;
        try {
          const exported = await api.export.prismInstance(null, project.path);
          try {
            await copyText(exported.path);
            toasts.success(`Exported Prism zip — path copied: ${exported.path}`);
          } catch {
            toasts.success(`Exported Prism zip: ${exported.path}`);
          }
        } catch (e) {
          toasts.error(String(e));
        } finally {
          actionBusy = false;
        }
        break;
      case "copy":
        clonePromptName = `${project.info.name} copy`;
        cloneTarget = project;
        showClonePrompt = true;
        break;
      case "shortcut":
        actionBusy = true;
        try {
          const path = await api.files.createDesktopShortcut(project.path);
          toasts.success(`Desktop shortcut created — double-click to launch: ${path}`);
        } catch (e) {
          toasts.error(String(e));
        } finally {
          actionBusy = false;
        }
        break;
      case "open-ide":
        openInIde(project);
        break;
      case "copy-path":
        try {
          const dir = await api.project.getDir(project.path);
          await copyText(dir);
          toasts.success("Instance folder path copied");
        } catch (e) {
          toasts.error(String(e));
        }
        break;
      case "repair":
        actionBusy = true;
        try {
          const report: { downloaded?: unknown[]; failed?: unknown[] } = await invoke(
            "repair_project",
            { path: project.path },
          );
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
        if (selectedPath === project.path) {
          selectedPath = $recentProjects[0]?.path ?? null;
          projectPath.set(selectedPath);
          projectInfo.set($recentProjects[0]?.info ?? null);
        }
        toasts.info(`Removed "${project.info.name}" from library`);
        break;
      case "delete": {
        const ok = await confirm(`Delete "${project.info.name}" from disk?`, {
          title: "Delete instance",
          kind: "warning",
        });
        if (!ok) break;
        try {
          await invoke("delete_project", { path: project.path });
          recentProjects.remove(project.path);
          if (selectedPath === project.path) {
            selectedPath = $recentProjects[0]?.path ?? null;
            projectPath.set(selectedPath);
            projectInfo.set($recentProjects[0]?.info ?? null);
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
      const info = (await invoke("validate_project", { path: clonedPath })) as RecentProject["info"] & {
        manifestPath?: string;
      };
      const manifestPath = info.manifestPath || clonedPath;
      recentProjects.add({ path: manifestPath, info: info as RecentProject["info"] });
      selectInstance({ path: manifestPath, info: info as RecentProject["info"] });
      toasts.success(`Copied to: ${manifestPath}`);
    } catch (e) {
      toasts.error(String(e));
    } finally {
      actionBusy = false;
      cloneTarget = null;
    }
  }

  function confirmGroup(name: string) {
    showGroupPrompt = false;
    if (!groupTarget) return;
    groupMap = setGroup(groupMap, groupTarget.path, name);
    groupTarget = null;
  }

  function applyExistingGroup(name: string) {
    if (!groupTarget) return;
    groupMap = setGroup(groupMap, groupTarget.path, name);
    showGroupPrompt = false;
    groupTarget = null;
  }

  onMount(() => {
    if ($recentProjects.length && !selectedPath) {
      selectedPath = $projectPath ?? $recentProjects[0].path;
    }
  });

  onDestroy(() => {
    clearLongPressTimer();
    endDrag();
  });
</script>

<div class="prism-lib lib-motion" class:drag-mode={dragging}>
  <div class="prism-toolbar lib-toolbar-enter">
    <div class="tb-left">
      <div class="tb-add-wrap">
        <button
          type="button"
          class="tb-btn primary"
          title="Add instance"
          onclick={(e) => { e.stopPropagation(); (addMenuOpen = !addMenuOpen);  }}
        >
          <Plus size={16} />
          <span>Add Instance</span>
          <ChevronDown size={14} />
        </button>
        {#if addMenuOpen}
          <div class="tb-menu" role="menu">
            <button type="button" role="menuitem" onclick={() => { addMenuOpen = false; openAddInstance("blank"); }}>
              <Plus size={14} /> Create new…
            </button>
            <button type="button" role="menuitem" onclick={importPackFile} disabled={actionBusy}>
              <FolderOpen size={14} /> Import file (.mrpack / .zip)
            </button>
            <button type="button" role="menuitem" onclick={importInstanceFolder} disabled={actionBusy}>
              <Folder size={14} /> Import instance folder
            </button>
            <button type="button" role="menuitem" onclick={importGithubRepo} disabled={actionBusy}>
              <Link2 size={14} /> Import GitHub repository
            </button>
          </div>
        {/if}
      </div>

      <div class="tb-folders-wrap">
        <button
          type="button"
          class="tb-btn"
          title="Folders"
          onclick={(e) => { e.stopPropagation(); (foldersMenuOpen = !foldersMenuOpen);  }}
        >
          <Folder size={16} />
          <span>Folders</span>
        </button>
        {#if foldersMenuOpen}
          <div class="tb-menu" role="menu">
            <button type="button" role="menuitem" onclick={openInstancesFolder}>
              <FolderOpen size={14} /> Instances folder
            </button>
            {#if selected}
              <button type="button" role="menuitem" onclick={() => { foldersMenuOpen = false; void openSelectedFolder(selected); }}>
                <Folder size={14} /> Selected instance
              </button>
            {/if}
          </div>
        {/if}
      </div>

      {#if onDiscover}
        <button type="button" class="tb-btn" title="Discover" onclick={() => onDiscover()}>
          <Compass size={16} />
          <span>Discover</span>
        </button>
      {/if}
      {#if onCreate}
        <button type="button" class="tb-btn" title="Create" onclick={() => onCreate()}>
          <Plus size={16} />
          <span>Create</span>
        </button>
      {/if}

      <button type="button" class="tb-btn" title="Settings" onclick={() => (currentView = "settings")}>
        <Settings size={16} />
        <span>Settings</span>
      </button>
      <button
        type="button"
        class="tb-btn"
        title="Help"
        onclick={() => window.dispatchEvent(new CustomEvent("tuffbox:show-shortcuts"))}
      >
        <HelpCircle size={16} />
        <span>Help</span>
      </button>
      <button
        type="button"
        class="tb-btn"
        title="Refresh"
        disabled={refreshing}
        onclick={() => void refreshAll()}
      >
        <span class:spinning={refreshing}><RefreshCw size={16} /></span>
        <span>Update</span>
      </button>
    </div>

    <div class="tb-right">
      {#if $authState.loggedIn && $authState.profile}
        <button
          type="button"
          class="tb-account"
          title="Account"
          onclick={() => (currentView = "me")}
        >
          <HeadAvatar skinSrc={$skinPath} size={28} alt={$authState.profile.name} />
          <span class="tb-account-name">{$authState.profile.name}</span>
          <span class="tb-account-badge">
            {loginTypeLabel(
              $authState.loginType,
              $authState.accounts.find((a) => a.uuid === $authState.activeAccountUuid)?.authority,
            )}
          </span>
        </button>
      {:else}
        <button type="button" class="tb-btn" onclick={() => loginModalOpen.set(true)}>
          Sign in
        </button>
      {/if}
    </div>
  </div>

  <div class="prism-body" class:is-dragging={dragging}>
    <div class="prism-grid-pane">
      {#if $recentProjects.length === 0}
        <div class="empty-state">
          <h3>No instances yet</h3>
          <p>Create or import a pack to build your library.</p>
          <button type="button" class="empty-cta" onclick={() => openAddInstance("blank")}>
            <Plus size={16} /> Add Instance
          </button>
        </div>
      {:else}
        <p class="drag-hint" class:visible={dragging}>
          Drop on another instance to make a folder
        </p>
        {#each grouped as group (group.name)}
          <section class="inst-group">
            <button
              type="button"
              class="group-header"
              class:drop-target={dragging && dropTargetGroup === group.name}
              data-group={group.name}
              onclick={() => toggleGroup(group.name)}
              aria-expanded={!group.collapsed}
            >
              {#if group.collapsed}
                <ChevronRight size={16} />
              {:else}
                <ChevronDown size={16} />
              {/if}
              <span>{group.name}</span>
              <span class="group-count">{group.projects.length}</span>
            </button>
            {#if !group.collapsed}
              <div class="inst-grid" transition:groupBodyIntro>
                {#each group.projects as project (project.path)}
                  <div
                    class="inst-tile"
                    class:selected={selectedPath === project.path}
                    class:running={isProjectRunning(project.path, $runningInstances)}
                    class:dragging={dragSource?.path === project.path}
                    class:drop-target={dropTargetPath === project.path}
                    class:holding={holdingPath === project.path && !dragging}
                    data-path={project.path}
                    role="button"
                    tabindex="0"
                    aria-label={`${project.info.name}. Hold and drag onto another instance to create a folder`}
                    in:tileIntro
                    onclick={() => onTileClick(project)}
                    ondblclick={() => !dragging && void launchInstance(project)}
                    onkeydown={(e) => e.key === "Enter" && selectInstance(project)}
                    oncontextmenu={(e) => openCtxMenu(e, project)}
                    onpointerdown={(e) => onTilePointerDown(e, project)}
                    onpointermove={onTilePointerMove}
                    onpointerup={onTilePointerUp}
                    onpointercancel={onTilePointerCancel}
                  >
                    <div class="hold-ring" aria-hidden="true"></div>
                    <div
                      class="inst-icon"
                      class:folder-preview={dropTargetPath === project.path}
                      style={`background: linear-gradient(135deg, ${gradientFrom(project.info.name)}, ${gradientFrom(project.info.id)})`}
                    >
                      {#if dropTargetPath === project.path && dragSource}
                        <span class="folder-stack" aria-hidden="true">
                          <span class="stack-a">{dragSource.info.name[0]?.toUpperCase()}</span>
                          <span class="stack-b">{project.info.name[0]?.toUpperCase()}</span>
                        </span>
                      {:else}
                        {project.info.name[0]?.toUpperCase() ?? "?"}
                      {/if}
                    </div>
                    <span
                      class="inst-name"
                      title={project.info.name}
                    >{project.info.name}</span>
                  </div>
                {/each}
              </div>
            {/if}
          </section>
        {/each}
      {/if}
      <div class="lib-footer" aria-live="polite">
        <span>
          {#if selected}
            Minecraft {selected.info.minecraftVersion} · {selected.info.loaderKind}
            {#if projectStats[selected.path]?.playtime}
              · {formatPlaytime(projectStats[selected.path].playtime)} played
            {/if}
          {:else}
            —
          {/if}
        </span>
        <span>
          Total playtime: {formatPlaytime(
            Object.values(projectStats).reduce((s, p) => s + (p?.playtime ?? 0), 0),
          )}
        </span>
      </div>
    </div>

    <aside class="prism-side lib-side-enter" aria-label="Instance actions">
      {#if selected}
        {#key selected.path}
          <div class="side-panel" in:sideIntro>
            <div class="side-hero">
              <div
                class="side-icon"
                style={`background: linear-gradient(135deg, ${gradientFrom(selected.info.name)}, ${gradientFrom(selected.info.id)})`}
              >
                {selected.info.name[0]?.toUpperCase() ?? "?"}
              </div>
              <div class="side-title" title={selected.info.name}>{selected.info.name}</div>
              <div class="side-meta">
                {selected.info.minecraftVersion} · {selected.info.loaderKind}
              </div>
            </div>

            <div class="side-actions">
              <button
                type="button"
                class={["side-btn", "launch", { stop: selectedRunning }]}
                disabled={actionBusy || launching === selected.path}
                onclick={() => void runAction(selectedRunning ? "stop" : "launch", selected)}
              >
                {#if launching === selected.path}
                  <span class="mini-spinner"></span> Launching…
                {:else if selectedRunning}
                  <Square size={14} /> Stop
                {:else}
                  <Play size={16} fill="currentColor" /> Play
                {/if}
              </button>
              <div class="side-sep" aria-hidden="true"></div>
              <button type="button" class="side-btn" disabled={actionBusy} onclick={() => runAction("open-ide", selected)}>
                <Package size={14} /> Open IDE
              </button>
              <button
                type="button"
                class="side-btn"
                disabled={actionBusy}
                onclick={() => void runAction("change-group", selected)}
              >
                <Tags size={14} /> Change Group
              </button>
              <button
                type="button"
                class="side-btn"
                disabled={actionBusy}
                onclick={() => void runAction("folder", selected)}
              >
                <Folder size={14} /> Folder
              </button>

              <div class="tb-export-wrap">
                <button
                  type="button"
                  class="side-btn"
                  disabled={actionBusy}
                  onclick={(e) => { e.stopPropagation(); (exportMenuOpen = !exportMenuOpen);  }}
                >
                  <Share2 size={14} /> Export <ChevronDown size={12} />
                </button>
                {#if exportMenuOpen}
                  <div class="tb-menu side-menu" role="menu" transition:fade={{ duration: prefersReducedMotion() ? 0 : 120 }}>
                    <button type="button" role="menuitem" onclick={() => void runAction("export-mrpack", selected)}>
                      Export .mrpack
                    </button>
                    <button type="button" role="menuitem" onclick={() => void runAction("export-prism", selected)}>
                      Export Prism zip
                    </button>
                  </div>
                {/if}
              </div>

              <button
                type="button"
                class="side-btn"
                disabled={actionBusy}
                onclick={() => void runAction("copy", selected)}
              >
                <Copy size={14} /> Copy
              </button>
              <button
                type="button"
                class="side-btn danger"
                disabled={actionBusy}
                onclick={() => void runAction("delete", selected)}
              >
                <Trash2 size={14} /> Delete
              </button>
              <button
                type="button"
                class="side-btn"
                disabled={actionBusy}
                onclick={() => void runAction("shortcut", selected)}
              >
                <Link2 size={14} /> Create Shortcut
              </button>
            </div>
          </div>
        {/key}
      {:else}
        <div class="side-empty" in:fade={{ duration: prefersReducedMotion() ? 0 : 160 }}>Select an instance</div>
      {/if}
    </aside>
  </div>
</div>

{#if dragGhost}
  <div
    class="drag-ghost"
    use:portal
    style={`position:fixed; left:${dragGhost.x}px; top:${dragGhost.y}px; z-index:10000; background: linear-gradient(135deg, ${dragGhost.colorA}, ${dragGhost.colorB})`}
    aria-hidden="true"
  >
    <span class="ghost-letter">{dragGhost.letter}</span>
    <span class="ghost-ring"></span>
  </div>
{/if}

{#if ctxMenu}
  {@const menuProject = ctxMenu.project}
  <div
    class="pack-ctx-menu"
    use:portal
    style={`position:fixed; left:${ctxMenu.x}px; top:${ctxMenu.y}px; z-index:10000`}
    role="menu"
  >
    <button
      type="button"
      role="menuitem"
      onclick={() =>
        void runAction(
          isProjectRunning(menuProject.path, $runningInstances) ? "stop" : "launch",
          menuProject,
        )}
      disabled={actionBusy}
    >
      {#if isProjectRunning(menuProject.path, $runningInstances)}
        <Square size={14} /> Stop
      {:else}
        <Play size={14} /> Play
      {/if}
    </button>
    <button type="button" role="menuitem" onclick={() => runAction("open-ide", menuProject)}>
      <Package size={14} /> Open IDE
    </button>
    <button type="button" role="menuitem" onclick={() => void runAction("change-group", menuProject)}>
      <Tags size={14} /> Change Group
    </button>
    <button type="button" role="menuitem" onclick={() => void runAction("folder", menuProject)}>
      <Folder size={14} /> Folder
    </button>
    <button type="button" role="menuitem" onclick={() => void runAction("copy", menuProject)}>
      <Copy size={14} /> Copy
    </button>
    <button type="button" role="menuitem" onclick={() => void runAction("shortcut", menuProject)}>
      <Link2 size={14} /> Create Shortcut
    </button>
    <div class="menu-sep"></div>
    <button type="button" role="menuitem" onclick={() => void runAction("copy-path", menuProject)}>
      <Copy size={14} /> Copy path
    </button>
    <button type="button" role="menuitem" onclick={() => void runAction("repair", menuProject)} disabled={actionBusy}>
      <Wrench size={14} /> Repair
    </button>
    <div class="menu-sep"></div>
    <button type="button" role="menuitem" onclick={() => void runAction("remove", menuProject)}>
      <Minus size={14} /> Remove from library
    </button>
    <button type="button" role="menuitem" class="danger" onclick={() => void runAction("delete", menuProject)}>
      <Trash2 size={14} /> Delete from disk
    </button>
  </div>
{/if}

{#if showClonePrompt && cloneTarget}
  <PromptDialog
    title="Copy instance"
    message={`Create a copy of "${cloneTarget.info.name}"`}
    mode="text"
    defaultValue={clonePromptName}
    confirmLabel="Copy"
    onconfirm={(v) => confirmClone(v)}
    oncancel={() => {
      showClonePrompt = false;
      cloneTarget = null;
    }}
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

{#if showGroupPrompt && groupTarget}
  <div
    class="group-dialog-backdrop"
    use:portal
    style="position:fixed; inset:0; z-index:10000;"
    role="presentation"
    onclick={() => { showGroupPrompt = false; groupTarget = null; }}
  >
    <div
      class="group-dialog"
      role="dialog"
      aria-labelledby="group-dlg-title"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <h3 id="group-dlg-title">Change Group</h3>
      <p>Move “{groupTarget.info.name}” into a group.</p>
      <div class="group-chips">
        {#each existingGroups as g (g)}
          <button type="button" class="chip" class:active={groupPromptName === g} onclick={() => applyExistingGroup(g)}>
            {g}
          </button>
        {/each}
      </div>
      <label class="group-new-label" for="group-new-input">Or type a new name</label>
      <input id="group-new-input" bind:value={groupPromptName} onkeydown={(e) => e.key === "Enter" && confirmGroup(groupPromptName)} />
      <div class="group-dlg-actions">
        <button type="button" class="ghost" onclick={() => { showGroupPrompt = false; groupTarget = null; }}>Cancel</button>
        <button type="button" class="accent" onclick={() => confirmGroup(groupPromptName)}>Apply</button>
      </div>
    </div>
  </div>
{/if}

<svelte:window onmousedown={onGlobalPointerDown} onkeydown={onGlobalKeydown} />

<style>
  .prism-lib {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-height: 0;
    height: 100%;
    border: none;
    border-radius: 0;
    background: transparent;
    overflow: hidden;
    position: relative;
  }
  .prism-lib.lib-motion::before {
    display: none;
  }

  .lib-toolbar-enter {
    animation: lib-toolbar-in 160ms var(--ease-out) both;
  }
  .lib-side-enter {
    animation: lib-side-in 160ms var(--ease-out) both;
  }

  .prism-toolbar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 4px 8px;
    border-bottom: 1px solid var(--border-color);
    background: var(--bg-tertiary);
    flex-wrap: wrap;
    position: relative;
    z-index: 2;
  }
  .tb-left {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }
  .tb-right { margin-left: auto; }
  .tb-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 7px 10px;
    border-radius: var(--border-radius-sm);
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-secondary);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    transition:
      transform var(--motion-fast) var(--ease-spring),
      background var(--motion-fast) var(--ease-out),
      border-color var(--motion-fast) var(--ease-out),
      color var(--motion-fast) var(--ease-out),
      box-shadow var(--motion-fast) var(--ease-out);
  }
  .tb-btn:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
  .tb-btn:active:not(:disabled) {
    transform: scale(0.94);
  }
  .tb-btn.primary {
    background: color-mix(in srgb, var(--accent-primary) 14%, transparent);
    border-color: color-mix(in srgb, var(--accent-primary) 35%, transparent);
    color: var(--accent-primary);
  }
  .tb-btn.primary:hover {
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--accent-primary) 25%, transparent), 0 6px 16px color-mix(in srgb, var(--accent-primary) 12%, transparent);
  }
  .tb-btn:disabled { opacity: 0.5; cursor: default; }
  .tb-add-wrap,
  .tb-folders-wrap,
  .tb-export-wrap {
    position: relative;
  }
  .tb-menu {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
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
  .tb-menu.side-menu { left: auto; right: 0; }
  .tb-menu button {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    text-align: left;
    padding: 8px 10px;
    border: none;
    border-radius: var(--border-radius-sm);
    background: transparent;
    color: var(--text-primary);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }
  .tb-menu button:hover {
    background: color-mix(in srgb, var(--accent-primary) 12%, transparent);
    color: var(--accent-primary);
  }
  .tb-account {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 4px 10px 4px 4px;
    border-radius: 999px;
    border: 1px solid var(--border-color);
    background: var(--bg-secondary);
    color: var(--text-primary);
    cursor: pointer;
  }
  .tb-account-name { font-size: 12px; font-weight: 700; max-width: 120px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .tb-account-badge {
    font-size: 10px;
    font-weight: 700;
    color: var(--text-muted);
    text-transform: uppercase;
  }

  .prism-body {
    display: grid;
    grid-template-columns: 1fr 220px;
    flex: 1;
    min-height: 0;
  }
  .prism-body.is-dragging {
    cursor: grabbing;
    user-select: none;
  }
  .prism-body.is-dragging .inst-tile {
    cursor: grabbing;
  }
  @media (max-width: 720px) {
    .prism-body { grid-template-columns: 1fr; }
    .prism-side { border-left: none; border-top: 1px solid var(--border-color); }
  }

  .prism-grid-pane {
    padding: 8px 10px 12px;
    overflow: auto;
    min-height: 0;
    height: 100%;
    display: flex;
    flex-direction: column;
  }
  .lib-footer {
    margin-top: auto;
    padding-top: 10px;
    display: flex;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
    font-size: 11px;
    color: var(--text-muted);
  }
  .drag-hint {
    margin: 0 0 10px;
    font-size: 12px;
    font-weight: 600;
    color: var(--accent-primary);
    opacity: 0;
    max-height: 0;
    overflow: hidden;
    transition: opacity 0.15s ease, max-height 0.15s ease;
  }
  .drag-hint.visible {
    opacity: 1;
    max-height: 24px;
  }
  .inst-group {
    margin-bottom: 14px;
  }
  .group-header {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 6px;
    margin-bottom: 8px;
    border: none;
    background: transparent;
    color: var(--text-secondary);
    font-size: 13px;
    font-weight: 700;
    cursor: pointer;
    border-radius: var(--border-radius-sm);
    transition:
      color var(--motion-fast) var(--ease-out),
      background var(--motion-fast) var(--ease-out),
      box-shadow var(--motion-fast) var(--ease-out);
  }
  .group-header:hover { color: var(--text-primary); }
  .group-header.drop-target {
    background: color-mix(in srgb, var(--accent-primary) 18%, transparent);
    color: var(--accent-primary);
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--accent-primary) 40%, transparent);
  }
  .group-count {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-muted);
  }
  .inst-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(108px, 1fr));
    gap: 10px;
  }
  .inst-tile {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 12px 8px 10px;
    border-radius: var(--border-radius-md);
    border: 1px solid transparent;
    background: transparent;
    cursor: pointer;
    text-align: center;
    outline: none;
    touch-action: manipulation;
    position: relative;
    transition:
      transform var(--motion-fast) var(--ease-out),
      background var(--motion-fast) var(--ease-out),
      border-color var(--motion-fast) var(--ease-out),
      opacity var(--motion-fast) var(--ease-out),
      box-shadow var(--motion-fast) var(--ease-out);
  }
  .inst-tile:hover .inst-icon {
    transform: translateY(-1px);
    filter: brightness(1.04);
  }
  .inst-tile:hover:not(.selected) .inst-name {
    background: color-mix(in srgb, var(--bg-hover) 90%, transparent);
  }
  .inst-tile.selected .inst-icon {
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent-primary) 55%, transparent);
  }
  .inst-tile.selected .inst-name {
    background: color-mix(in srgb, var(--accent-primary) 16%, transparent);
    border-color: color-mix(in srgb, var(--accent-primary) 35%, transparent);
    color: var(--accent-primary);
  }
  .inst-tile.running .inst-icon {
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent-primary) 55%, transparent);
  }
  .inst-tile.running::after {
    content: "";
    position: absolute;
    top: 10px;
    right: 14px;
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--accent-primary);
    box-shadow: 0 0 0 2px var(--bg-primary, transparent);
    z-index: 3;
  }
  .inst-tile.dragging {
    opacity: 0.28;
    filter: saturate(0.7);
  }
  .drag-mode .inst-tile:not(.dragging):not(.drop-target) {
    opacity: 0.7;
  }
  .inst-tile.drop-target {
    background: color-mix(in srgb, var(--accent-primary) 22%, transparent);
    border-color: color-mix(in srgb, var(--accent-primary) 55%, transparent);
    z-index: 2;
  }
  .inst-tile.holding .inst-icon {
    filter: brightness(0.96);
  }
  .inst-tile.holding .hold-ring {
    opacity: 1;
    animation: lib-hold-ring 420ms linear forwards;
  }
  .inst-tile:focus-visible {
    outline: none;
  }
  .inst-tile:focus-visible .inst-icon {
    box-shadow:
      0 0 0 2px color-mix(in srgb, var(--accent-primary) 70%, transparent),
      0 0 0 4px color-mix(in srgb, var(--accent-primary) 22%, transparent);
  }
  .inst-tile:focus-visible .inst-name {
    outline: 2px solid color-mix(in srgb, var(--accent-primary) 55%, transparent);
    outline-offset: 1px;
  }

  .hold-ring {
    --hold: 0deg;
    position: absolute;
    top: 8px;
    left: 50%;
    width: 82px;
    height: 82px;
    margin-left: -41px;
    border-radius: 50%;
    opacity: 0;
    pointer-events: none;
    background: conic-gradient(
      from -90deg,
      var(--accent-primary) var(--hold),
      transparent 0
    );
    -webkit-mask: radial-gradient(farthest-side, transparent calc(100% - 2px), #000 calc(100% - 1px));
    mask: radial-gradient(farthest-side, transparent calc(100% - 2px), #000 calc(100% - 1px));
    z-index: 1;
  }

  .inst-icon {
    width: 72px;
    height: 72px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 28px;
    font-weight: 900;
    color: #fff;
    text-shadow: 0 2px 8px rgba(0, 0, 0, 0.35);
    flex-shrink: 0;
    position: relative;
    z-index: 2;
    transition:
      transform var(--motion-fast) var(--ease-out),
      border-radius var(--motion-fast) var(--ease-out),
      box-shadow var(--motion-fast) var(--ease-out),
      filter var(--motion-fast) var(--ease-out);
  }
  .inst-icon.folder-preview {
    border-radius: var(--border-radius-lg);
  }
  .folder-stack {
    position: relative;
    width: 100%;
    height: 100%;
  }
  .folder-stack .stack-a,
  .folder-stack .stack-b {
    position: absolute;
    width: 34px;
    height: 34px;
    border-radius: var(--border-radius-sm);
    background: rgba(0, 0, 0, 0.28);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 16px;
    font-weight: 800;
  }
  .folder-stack .stack-a { top: 10px; left: 10px; }
  .folder-stack .stack-b { bottom: 10px; right: 10px; }
  .inst-name {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-primary);
    max-width: 100%;
    width: fit-content;
    line-height: 1.3;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    white-space: normal;
    padding: 2px 8px;
    border-radius: 999px;
    border: 1px solid transparent;
    box-sizing: border-box;
    transition:
      color var(--motion-fast) var(--ease-out),
      background var(--motion-fast) var(--ease-out),
      border-color var(--motion-fast) var(--ease-out);
  }
  .inst-tile.drop-target .inst-name { color: var(--accent-primary); }

  .drag-ghost {
    position: fixed;
    z-index: 200;
    width: 64px;
    height: 64px;
    margin: -32px 0 0 -32px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 24px;
    font-weight: 900;
    color: #fff;
    pointer-events: none;
    box-shadow: 0 10px 24px rgba(0, 0, 0, 0.45), 0 0 0 1px color-mix(in srgb, var(--accent-primary) 30%, transparent);
    opacity: 0.95;
    will-change: left, top;
  }
  .ghost-letter { position: relative; z-index: 1; }
  .ghost-ring {
    position: absolute;
    inset: -5px;
    border-radius: 50%;
    border: 1px solid color-mix(in srgb, var(--accent-primary) 40%, transparent);
    opacity: 0.7;
  }

  .prism-side {
    border-left: 1px solid var(--border-color);
    background: var(--bg-tertiary);
    padding: 14px 12px;
    display: flex;
    flex-direction: column;
    gap: 12px;
    overflow: auto;
  }
  .side-panel { display: flex; flex-direction: column; gap: 12px; }
  .side-hero { text-align: center; min-width: 0; }
  .side-icon {
    width: 64px;
    height: 64px;
    margin: 0 auto 10px;
    border-radius: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 24px;
    font-weight: 900;
    color: #fff;
    box-shadow: 0 6px 16px rgba(0, 0, 0, 0.3);
  }
  .side-title {
    font-size: 13px;
    font-weight: 700;
    color: var(--text-primary);
    margin-bottom: 4px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 100%;
  }
  .side-meta {
    font-size: 11px;
    color: var(--text-muted);
    text-transform: capitalize;
  }
  .side-actions {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .side-btn {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 8px 10px;
    border-radius: var(--border-radius-sm);
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-primary);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    text-align: left;
    transition:
      background var(--motion-fast) var(--ease-out),
      color var(--motion-fast) var(--ease-out),
      border-color var(--motion-fast) var(--ease-out);
  }
  .side-btn:hover:not(:disabled) {
    background: color-mix(in srgb, var(--accent-primary) 10%, transparent);
    color: var(--accent-primary);
  }
  .side-btn:active:not(:disabled) { opacity: 0.9; }
  .side-btn:disabled { opacity: 0.4; cursor: default; }
  .side-btn.launch {
    background: color-mix(in srgb, var(--accent-primary) 16%, transparent);
    border-color: color-mix(in srgb, var(--accent-primary) 30%, transparent);
    color: var(--accent-primary);
    margin-bottom: 4px;
  }
  .side-btn.launch:hover:not(:disabled) {
    box-shadow: 0 6px 16px color-mix(in srgb, var(--accent-primary) 18%, transparent);
  }
  .side-btn.launch.stop {
    background: color-mix(in srgb, var(--accent-danger, #ef4444) 16%, transparent);
    border-color: color-mix(in srgb, var(--accent-danger, #ef4444) 30%, transparent);
    color: var(--accent-danger, #f87171);
  }
  .side-btn.launch.stop:hover:not(:disabled) {
    box-shadow: 0 6px 16px color-mix(in srgb, var(--accent-danger, #ef4444) 18%, transparent);
  }
  .side-btn.danger:hover:not(:disabled) {
    background: rgba(239, 68, 68, 0.12);
    color: #f87171;
  }
  .side-empty {
    padding: 24px 8px;
    text-align: center;
    color: var(--text-muted);
    font-size: 13px;
  }

  .side-sep {
    height: 1px;
    margin: 4px 2px;
    background: var(--border-color);
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 48px 24px;
    text-align: center;
    color: var(--text-muted);
  }
  .empty-state h3 { margin: 0; color: var(--text-primary); font-size: 16px; }
  .empty-state p { margin: 0; font-size: 13px; }
  .empty-cta {
    margin-top: 6px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 10px 14px;
    border: none;
    border-radius: 10px;
    background: var(--accent-primary);
    color: #000;
    font-weight: 700;
    font-size: 13px;
    cursor: pointer;
  }

  .pack-ctx-menu {
    position: fixed;
    z-index: 200;
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
    border-radius: var(--border-radius-sm);
    background: transparent;
    color: var(--text-primary);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    text-align: left;
  }
  .pack-ctx-menu button:hover:not(:disabled) {
    background: color-mix(in srgb, var(--accent-primary) 12%, transparent);
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

  .group-dialog-backdrop {
    position: fixed;
    inset: 0;
    z-index: 100;
    background: rgba(0, 0, 0, 0.55);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 16px;
  }
  .group-dialog {
    width: min(400px, 100%);
    padding: 18px;
    border-radius: var(--border-radius-lg);
    border: 1px solid var(--border-color);
    background: var(--bg-elevated, #1a1f28);
    box-shadow: 0 20px 48px rgba(0, 0, 0, 0.5);
  }
  .group-dialog h3 { margin: 0 0 6px; font-size: 16px; color: var(--text-primary); }
  .group-dialog p { margin: 0 0 12px; font-size: 13px; color: var(--text-muted); }
  .group-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-bottom: 12px;
  }
  .chip {
    padding: 6px 10px;
    border-radius: 999px;
    border: 1px solid var(--border-color);
    background: var(--bg-secondary);
    color: var(--text-secondary);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }
  .chip.active,
  .chip:hover {
    border-color: color-mix(in srgb, var(--accent-primary) 40%, transparent);
    color: var(--accent-primary);
    background: color-mix(in srgb, var(--accent-primary) 10%, transparent);
  }
  .group-new-label {
    display: block;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-muted);
    margin-bottom: 4px;
  }
  .group-dialog input {
    width: 100%;
    height: 40px;
    padding: 0 12px;
    border-radius: 10px;
    border: 1px solid var(--border-color);
    background: var(--bg-tertiary);
    color: var(--text-primary);
    font-size: 13px;
    box-sizing: border-box;
  }
  .group-dlg-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 14px;
  }
  .group-dlg-actions .ghost {
    padding: 8px 12px;
    border-radius: var(--border-radius-sm);
    border: 1px solid var(--border-color);
    background: transparent;
    color: var(--text-secondary);
    cursor: pointer;
    font-weight: 600;
    font-size: 12px;
  }
  .group-dlg-actions .accent {
    padding: 8px 14px;
    border-radius: var(--border-radius-sm);
    border: none;
    background: var(--accent-primary);
    color: #000;
    cursor: pointer;
    font-weight: 700;
    font-size: 12px;
  }

  .mini-spinner {
    width: 14px;
    height: 14px;
    border: 2px solid color-mix(in srgb, var(--accent-primary) 25%, transparent);
    border-top-color: var(--accent-primary);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
    display: inline-block;
  }
  .spinning {
    display: inline-flex;
    animation: spin 0.8s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  @property --hold {
    syntax: "<angle>";
    inherits: false;
    initial-value: 0deg;
  }

  @keyframes lib-hold-ring {
    from { --hold: 0deg; opacity: 1; }
    to { --hold: 360deg; opacity: 1; }
  }
  @keyframes lib-toolbar-in {
    from { opacity: 0; }
    to { opacity: 1; }
  }
  @keyframes lib-side-in {
    from { opacity: 0; }
    to { opacity: 1; }
  }

  .drag-mode .prism-grid-pane {
    background: radial-gradient(ellipse at center, color-mix(in srgb, var(--accent-primary) 3%, transparent), transparent 70%);
  }

  :global(.potato-pc) .lib-motion::before,
  :global(.potato-pc) .lib-toolbar-enter,
  :global(.potato-pc) .lib-side-enter,
  :global(.potato-pc) .hold-ring {
    animation: none !important;
  }
  :global(.potato-pc) .inst-tile:hover .inst-icon {
    transform: none !important;
    filter: none !important;
  }
  :global(.potato-pc) .drag-mode .inst-tile:not(.dragging):not(.drop-target) {
    opacity: 1;
  }

  @media (prefers-reduced-motion: reduce) {
    .lib-motion::before,
    .lib-toolbar-enter,
    .lib-side-enter,
    .hold-ring {
      animation: none !important;
    }
    .inst-tile:hover .inst-icon {
      transform: none !important;
      filter: none !important;
    }
    .drag-mode .inst-tile:not(.dragging):not(.drop-target) {
      opacity: 1;
    }
  }
</style>
