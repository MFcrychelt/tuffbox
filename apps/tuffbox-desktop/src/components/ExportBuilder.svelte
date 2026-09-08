<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { open as openShell } from "@tauri-apps/plugin-shell";
  import { save, open as openDialog } from "@tauri-apps/plugin-dialog";
  import {
    PackageOpen,
    RefreshCw,
    UploadCloud,
    CheckCircle2,
    AlertTriangle,
    Server,
    Box,
    FolderTree,
    Layers,
    ExternalLink,
    FileArchive,
    FolderOpen,
    Copy,
  } from "@lucide/svelte";
  import { projectPath, projectInfo, pushWorkTrail } from "../lib/store";
  import EmptyState from "./EmptyState.svelte";
  import ExportFormatSelector from "./ExportFormatSelector.svelte";
  import PreflightWarnings from "./PreflightWarnings.svelte";
  import ArchivePreviewTree from "./ArchivePreviewTree.svelte";
  import { api } from "../lib/api";

  type ExportMode = "mrpack" | "curseforge" | "prism" | "server" | "packwiz";

  type ExportResult = {
    path: string;
    fileCount: number;
    overrideCount: number;
  };

  type ExportIssue = {
    severity: "error" | "warning";
    code: string;
    message: string;
    target?: string | null;
  };

  type FormatDef = {
    id: ExportMode;
    title: string;
    badge: string;
    blurb: string;
    detail: string;
    pathKind: "file" | "dir";
    validation: "mrpack" | "curseforge" | null;
    filters?: { name: string; extensions: string[] }[];
  };

  const FORMATS: FormatDef[] = [
    {
      id: "mrpack",
      title: "Modrinth",
      badge: ".mrpack",
      blurb: "Modrinth App & website",
      detail: "modrinth.index.json, remote downloads, overrides (config / KubeJS / packs).",
      pathKind: "file",
      validation: "mrpack",
      filters: [{ name: "Modrinth pack", extensions: ["mrpack"] }],
    },
    {
      id: "curseforge",
      title: "CurseForge",
      badge: ".zip",
      blurb: "CurseForge / Overwolf",
      detail: "manifest.json + overrides. Non-CF remotes kept in tuffbox.remote-mods.json.",
      pathKind: "file",
      validation: "curseforge",
      filters: [{ name: "CurseForge zip", extensions: ["zip"] }],
    },
    {
      id: "prism",
      title: "Prism / MultiMC",
      badge: ".zip",
      blurb: "Prism · MultiMC · PolyMC",
      detail: "instance.cfg + mmc-pack.json + mods/configs for portable instances.",
      pathKind: "file",
      validation: null,
      filters: [{ name: "Prism zip", extensions: ["zip"] }],
    },
    {
      id: "server",
      title: "Server pack",
      badge: ".zip",
      blurb: "Dedicated servers",
      detail: "Server-safe mods, configs, download manifest, start scripts. Skips client-only.",
      pathKind: "file",
      validation: null,
      filters: [{ name: "Server zip", extensions: ["zip"] }],
    },
    {
      id: "packwiz",
      title: "Packwiz",
      badge: "folder",
      blurb: "Git-friendly metadata",
      detail: "pack.toml + index.toml + metafiles; configs/overrides hashed into the index.",
      pathKind: "dir",
      validation: null,
    },
  ];

  let targetPath = $state("");
  let serverTargetPath = $state("");
  let prismTargetPath = $state("");
  let curseforgeTargetPath = $state("");
  let packwizTargetPath = $state("");
  let projectDir = $state("");
  let exporting = $state(false);
  let batching = $state(false);
  let result = $state<ExportResult | null>(null);
  let batchResults = $state<
    { kind: string; status: string; path?: string; error?: string; files?: number }[]
  >([]);
  let error = $state<string | null>(null);
  let message = $state<string | null>(null);
  let mrIssues = $state<ExportIssue[]>([]);
  let cfIssues = $state<ExportIssue[]>([]);
  let exportMode = $state<ExportMode>("mrpack");
  let copiedFlash = $state(false);
  let includeConfigs = $state(true);
  let clientOnly = $state(false);

  let lastPathForDefaults = $state("");
  let lastInfoReadyForDefaults = $state(false);

  const activeFormat = $derived(FORMATS.find((f) => f.id === exportMode) ?? FORMATS[0]);
  const activePath = $derived(
    exportMode === "mrpack"
      ? targetPath
      : exportMode === "server"
        ? serverTargetPath
        : exportMode === "prism"
          ? prismTargetPath
          : exportMode === "curseforge"
            ? curseforgeTargetPath
            : packwizTargetPath,
  );
  const activeIssues = $derived(
    activeFormat.validation === "mrpack"
      ? mrIssues
      : activeFormat.validation === "curseforge"
        ? cfIssues
        : [],
  );
  const blockingErrors = $derived(activeIssues.filter((i) => i.severity === "error"));
  const warnCount = $derived(activeIssues.filter((i) => i.severity === "warning").length);
  const exportBlocked = $derived(
    exporting || (activeFormat.validation != null && blockingErrors.length > 0),
  );
  // Warnings repeat per-mod (MOD_WITHOUT_HASH, UNKNOWN_MOD_SIDE, ...) — showing
  // them all at once is noise. Group by code, collapse by default, let the user
  // expand only what they care about. Errors stay flat (they block export).
  type IssueGroup = {
    code: string;
    severity: "error" | "warning";
    message: string;
    count: number;
    targets: string[];
  };
  let expandedGroups = $state<Set<string>>(new Set());
  const groupedIssues = $derived.by(() => {
    const groups = new Map<string, IssueGroup>();
    for (const issue of activeIssues) {
      let g = groups.get(issue.code);
      if (!g) {
        g = { code: issue.code, severity: issue.severity, message: issue.message, count: 0, targets: [] };
        groups.set(issue.code, g);
      }
      g.count += 1;
      if (issue.target && !g.targets.includes(issue.target)) g.targets.push(issue.target);
    }
    return [...groups.values()];
  });
  const errorGroups = $derived(groupedIssues.filter((g) => g.severity === "error"));
  const warningGroups = $derived(groupedIssues.filter((g) => g.severity === "warning"));
  const warningCounts = $derived(Object.fromEntries(FORMATS.map((fmt) => [fmt.id, formatWarns(fmt.id)])));
  const errorCounts = $derived(Object.fromEntries(FORMATS.map((fmt) => [fmt.id, formatErrors(fmt.id)])));
  const archiveMods = $derived(activeIssues.map((issue) => issue.target).filter((target): target is string => !!target && /\\.jar$/i.test(target)));
  const archiveEntries = $derived(archiveMods.length + (includeConfigs ? 1 : 0) + 3);

  function toggleGroup(code: string) {
    const next = new Set(expandedGroups);
    if (next.has(code)) next.delete(code);
    else next.add(code);
    expandedGroups = next;
  }

  function expandAllGroups() {
    expandedGroups = new Set(groupedIssues.map((g) => g.code));
  }

  function collapseAllGroups() {
    expandedGroups = new Set();
  }

  const packSummary = $derived.by(() => {
    const info = $projectInfo;
    if (!info) return "";
    const loader =
      info.loaderKind && info.loaderVersion
        ? `${info.loaderKind} ${info.loaderVersion}`
        : info.loaderKind || null;
    const parts = [
      info.name || info.id,
      `v${info.version || "?"}`,
      info.minecraftVersion ? `MC ${info.minecraftVersion}` : null,
      loader,
    ].filter(Boolean);
    return parts.join(" · ");
  });

  async function loadDefaultPaths(path: string) {
    projectDir = await invoke("get_project_dir", { path });
    const [mr, cf] = await Promise.all([
      api.export.validateModrinth(path),
      api.export.validateCurseforge(path),
    ]);
    mrIssues = mr ?? [];
    cfIssues = cf ?? [];
    const id = $projectInfo?.id ?? "modpack";
    const version = $projectInfo?.version ?? "1.0.0";
    targetPath = `${projectDir}/${id}-${version}.mrpack`;
    serverTargetPath = `${projectDir}/${id}-${version}-server.zip`;
    prismTargetPath = `${projectDir}/${id}-${version}-prism.zip`;
    curseforgeTargetPath = `${projectDir}/${id}-${version}-curseforge.zip`;
    packwizTargetPath = `${projectDir}/${id}-${version}-packwiz`;
  }

  function refreshDefaultPath() {
    if (!$projectPath) return;
    void loadDefaultPaths($projectPath);
  }

  function setActivePath(value: string) {
    if (exportMode === "mrpack") targetPath = value;
    else if (exportMode === "server") serverTargetPath = value;
    else if (exportMode === "prism") prismTargetPath = value;
    else if (exportMode === "curseforge") curseforgeTargetPath = value;
    else packwizTargetPath = value;
  }

  function formatWarns(id: ExportMode): number {
    if (id === "mrpack") return mrIssues.filter((i) => i.severity === "warning").length;
    if (id === "curseforge") return cfIssues.filter((i) => i.severity === "warning").length;
    return 0;
  }

  function formatErrors(id: ExportMode): number {
    if (id === "mrpack") return mrIssues.filter((i) => i.severity === "error").length;
    if (id === "curseforge") return cfIssues.filter((i) => i.severity === "error").length;
    return 0;
  }

  async function browseOutput() {
    const fmt = activeFormat;
    if (fmt.pathKind === "dir") {
      const selected = await openDialog({
        directory: true,
        title: `Packwiz output folder`,
        defaultPath: activePath || projectDir || undefined,
      });
      if (typeof selected === "string" && selected) setActivePath(selected);
      return;
    }
    const selected = await save({
      title: `Export ${fmt.title}`,
      defaultPath: activePath || undefined,
      filters: fmt.filters,
    });
    if (typeof selected === "string" && selected) setActivePath(selected);
  }

  async function runSelectedExport() {
    if (!$projectPath) return;
    exporting = true;
    error = null;
    result = null;
    batchResults = [];
    try {
      let out: ExportResult;
      const p = activePath || null;
      if (exportMode === "mrpack") out = await api.export.modrinthPack(p, $projectPath);
      else if (exportMode === "server") out = await api.export.serverPack(p, $projectPath);
      else if (exportMode === "prism") out = await api.export.prismInstance(p, $projectPath);
      else if (exportMode === "curseforge") out = await api.export.curseforgePack(p, $projectPath);
      else out = await api.export.packwizPack(p, $projectPath);
      result = out;
      pushWorkTrail(`Export ready · ${out.path}`, [
        { id: "release", label: "Open Release", kind: "stage", stage: "release" },
        { id: "dismiss", label: "Dismiss", kind: "dismiss" },
      ]);
    } catch (e) {
      error = String(e);
    } finally {
      exporting = false;
    }
  }

  async function exportAllFormats() {
    if (!$projectPath) return;
    batching = true;
    error = null;
    result = null;
    batchResults = [];
    try {
      const rows = await api.export.batchAll($projectPath);
      batchResults = (rows ?? []).map((r) => ({
        kind: String(r.kind ?? ""),
        status: String(r.status ?? ""),
        path: r.path != null ? String(r.path) : undefined,
        error: r.error != null ? String(r.error) : undefined,
        files: typeof r.files === "number" ? r.files : undefined,
      }));
      const failed = batchResults.filter((r) => r.status !== "ok");
      if (failed.length > 0) {
        error = `${failed.length} format(s) failed — see batch results.`;
      }
    } catch (e) {
      error = String(e);
    } finally {
      batching = false;
    }
  }

  async function openPath(path?: string | null) {
    if (!path) return;
    try {
      await openShell(path);
    } catch {
      /* ignore */
    }
  }

  async function openTargetFolder(path?: string | null) {
    if (!path) return;
    const folder = path.replace(/[\\/][^\\/]*$/, "") || path;
    await openPath(folder);
  }

  async function copyPath(path?: string | null) {
    if (!path) return;
    try {
      await navigator.clipboard.writeText(path);
      copiedFlash = true;
      setTimeout(() => (copiedFlash = false), 1200);
    } catch {
      /* ignore */
    }
  }

  $effect(() => {
    const path = $projectPath;
    const infoReady = !!$projectInfo;
    if (!path) return;
    // Recompute defaults when the path changes AND once more once
    // projectInfo resolves, so filenames use real id/version instead of
    // the "modpack"/"1.0.0" fallbacks.
    if (path === lastPathForDefaults && infoReady === lastInfoReadyForDefaults) return;
    lastPathForDefaults = path;
    lastInfoReadyForDefaults = infoReady;
    void loadDefaultPaths(path);
  });
</script>

<div class="export-builder w-full bg-black/30 backdrop-blur-2xl rounded-2xl border border-white/[0.08] shadow-[inset_0_1px_0_rgba(255,255,255,0.1)] p-6">
  <div class="eb-cap">
  <div class="toolbar">
    <div class="title"><UploadCloud size={18} /> Export</div>
    <div class="toolbar-actions">
      <button class="ghost" onclick={refreshDefaultPath} disabled={!$projectPath} title="Reset output paths to defaults">
        <RefreshCw size={16} />
        Defaults
      </button>
      <button
        class="ghost"
        onclick={exportAllFormats}
        disabled={!$projectPath || batching || exporting}
        title="Build all formats into ./export"
      >
        <Layers size={16} />
        {batching ? "Exporting all…" : "Export all"}
      </button>
    </div>
  </div>
  {#if packSummary}
    <p class="pack-summary" title={packSummary}>{packSummary}</p>
  {/if}

  {#if error}<div class="flex items-start gap-2 px-2.5 py-2 rounded-[length:var(--border-radius-md)] mb-2.5 border text-xs leading-snug text-[#fecaca] bg-[rgba(239,68,68,0.08)] border-[rgba(239,68,68,0.28)]"><AlertTriangle size={14} class="shrink-0" /> {error}</div>{/if}
  {#if message}<div class="mb-2.5 flex items-center gap-2 rounded-lg border border-emerald-500/25 bg-emerald-500/10 px-3 py-2 text-xs text-emerald-200"><CheckCircle2 size={14} /> {message}</div>{/if}
  {#if result}
    <div class="flex items-start gap-2 px-2.5 py-2 rounded-[length:var(--border-radius-md)] mb-2.5 border text-xs leading-snug text-[color:var(--accent-primary)] bg-[color-mix(in_srgb,var(--accent-primary)_8%,transparent)] border-[color-mix(in_srgb,var(--accent-primary)_25%,transparent)]">
      <CheckCircle2 size={14} class="shrink-0 mt-0.5" />
      <span class="min-w-0 break-all">
        Exported {result.fileCount} entries
        {#if result.overrideCount > 0}· {result.overrideCount} overrides{/if}
        → <code class="text-[11px] break-all">{result.path}</code>
      </span>
      <button class="ghost mini shrink-0" onclick={() => copyPath(result?.path)} title="Copy path">
        <Copy size={13} />
        {copiedFlash ? "Copied" : "Copy"}
      </button>
      <button class="ghost mini shrink-0" onclick={() => openPath(result?.path)} title="Open output">
        <ExternalLink size={13} />
      </button>
    </div>
  {/if}

  {#if !$projectPath}
    <EmptyState icon={PackageOpen} title="No project selected" description="Open a project to export a modpack." />
  {:else}
    <section class="bg-white/[0.03] border border-white/[0.08] rounded-[length:var(--border-radius-lg)] p-3.5 grid gap-3 shadow-xl backdrop-blur-md">
      <ExportFormatSelector
        formats={FORMATS}
        selected={exportMode}
        warningCounts={warningCounts}
        errorCounts={errorCounts}
        onSelect={(id) => (exportMode = id as ExportMode)}
      />

      <div class="grid gap-2.5 p-3 border border-white/[0.08] rounded-[length:var(--border-radius-md)] bg-black/40">
        <div>
          <h2 class="m-0 mb-1 text-sm font-bold text-[color:var(--text-primary)]">{activeFormat.title}</h2>
          <p class="m-0 text-xs text-[color:var(--text-muted)] leading-snug">{activeFormat.detail}</p>
        </div>

        <label class="grid gap-1 text-[11px] font-semibold text-[color:var(--text-secondary)]">
          {activeFormat.pathKind === "dir" ? "Output folder" : "Output file"}
          <div class="flex items-center gap-1.5 rounded-lg border border-white/10 bg-black/40 p-1.5 focus-within:border-emerald-500/50">
            <FolderOpen size={15} class="ml-1 shrink-0 text-neutral-500" />
            <input
              class="flex-1 min-w-0 text-xs px-2.5 py-[7px] bg-black/40 border-white/10 focus:border-emerald-500/50 focus:ring-1 focus:ring-emerald-500/30 font-mono"
              value={activePath}
              oninput={(e) => setActivePath(e.currentTarget.value)}
              placeholder={activeFormat.pathKind === "dir" ? ".../pack-packwiz" : ".../pack.zip"}
            />
            <button type="button" class="ghost mini shrink-0" onclick={browseOutput}>
              <FolderOpen size={14} />
              Browse
            </button>
          </div>
        </label>

        <PreflightWarnings
          errors={errorGroups}
          warnings={warningGroups}
          expanded={expandedGroups}
          onToggle={toggleGroup}
          onExpandAll={expandAllGroups}
          onCollapseAll={collapseAllGroups}
          onAction={(action, target) => (message = `${action}${target ? ` · ${target}` : ""}`)}
        />

        <ArchivePreviewTree
          modFiles={archiveMods}
          configCount={includeConfigs ? 1 : 0}
          outputName={activePath.split(/[\\/]/).pop() || `${activeFormat.title} export`}
          entryCount={archiveEntries}
        />

        <div class="sticky bottom-3 z-10 flex flex-col gap-4 rounded-xl border border-emerald-500/20 bg-neutral-950/90 p-4 shadow-2xl backdrop-blur-xl lg:flex-row lg:items-center lg:justify-between">
          <div class="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-neutral-300">
            <strong class="text-neutral-100">{activeFormat.badge}</strong>
            <span>{archiveMods.length} mods</span>
            <span>v{$projectInfo?.version ?? "?"}</span>
            <span>{includeConfigs ? "Configs included" : "Manifest only"}</span>
          </div>
          <div class="flex flex-wrap items-center gap-3">
            <label class="inline-flex items-center gap-2 text-xs text-neutral-300"><input type="checkbox" bind:checked={includeConfigs} /> Include configs</label>
            <label class="inline-flex items-center gap-2 text-xs text-neutral-300"><input type="checkbox" bind:checked={clientOnly} /> Client mods only</label>
            {#if result}<button class="ghost mini" onclick={() => openTargetFolder(result?.path)}><FolderOpen size={14} /> Open target folder</button>{/if}
            <button class="eb-export-btn" onclick={runSelectedExport} disabled={exportBlocked || batching}>
              <UploadCloud size={16} />
              {exporting ? "Exporting…" : "Экспортировать сборку"}
            </button>
          </div>
        </div>
      </div>

      {#if batchResults.length > 0}
        <div class="grid gap-1.5">
          <h3 class="m-0 text-xs font-bold text-[color:var(--text-secondary)]">Batch results · ./export</h3>
          <ul class="list-none m-0 p-0 grid gap-1">
            {#each batchResults as row (row.kind)}
              <li
                class="grid grid-cols-[72px_minmax(0,1fr)] sm:grid-cols-[88px_minmax(0,1fr)_auto] gap-x-2.5 gap-y-1 px-2 py-1.5 rounded-md border items-center text-[11px] min-w-0 overflow-hidden"
                class:ok={row.status === "ok"}
                class:err={row.status !== "ok"}
              >
                <strong class="truncate">{row.kind}</strong>
                {#if row.status === "ok"}
                  <span class="min-w-0 truncate">{row.files ?? "?"} files</span>
                  <div class="flex gap-1 sm:row-auto col-span-2 sm:col-span-1 sm:col-start-3">
                    {#if row.path}
                      <button class="ghost mini" onclick={() => openPath(row.path)} title={row.path}>
                        <ExternalLink size={12} /> Open
                      </button>
                      <button class="ghost mini" onclick={() => copyPath(row.path)}>
                        <Copy size={12} />
                      </button>
                    {/if}
                  </div>
                  {#if row.path}<code class="col-span-2 sm:col-span-2 text-[10px] text-[color:var(--text-muted)] break-all" title={row.path}>{row.path}</code>{/if}
                {:else}
                  <span class="min-w-0 break-all sm:col-span-2">{row.error ?? "failed"}</span>
                {/if}
              </li>
            {/each}
          </ul>
        </div>
      {/if}

      <p class="m-0 text-[11px] text-[color:var(--text-muted)] leading-snug">
        Publish tokens live in Settings · upload artifacts from the Release stage after export.
      </p>
    </section>
  {/if}
  </div>
</div>

<style>
  /* Theming/states only — layout lives in Tailwind utilities. */
  .eb-cap {
    max-width: min(1240px, 100%);
    margin: 0 auto;
  }

  /* Stage toolbar — same pattern as Ores / Release / History stages. */
  .toolbar,
  .toolbar-actions {
    display: flex;
    align-items: center;
  }
  .toolbar {
    justify-content: space-between;
    gap: 16px;
    margin-bottom: 14px;
    flex-wrap: wrap;
  }
  .title {
    gap: 10px;
    color: var(--text-secondary);
    font-weight: 700;
  }
  .toolbar-actions {
    gap: 8px;
    flex-wrap: wrap;
  }
  .pack-summary {
    margin: -6px 0 14px;
    font-size: 12px;
    color: var(--text-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: min(720px, 100%);
  }

  /* Primary export action: theme accent, not the global Ore-gray button skin. */
  .eb-export-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 9px 18px;
    border-radius: 999px;
    border: none;
    background: #059669;
    color: #fff;
    font-size: 13px;
    font-weight: 700;
    cursor: pointer;
    transition:
      background var(--motion-fast) var(--motion-ease),
      box-shadow var(--motion-fast) var(--motion-ease);
  }
  .eb-export-btn:hover:not(:disabled) {
    background: #10b981;
    box-shadow: 0 0 20px rgba(16, 185, 129, 0.3);
  }
  .eb-export-btn:active:not(:disabled) {
    background: #047857;
  }
  .eb-export-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .format-card.active {
    color: var(--text-primary);
    border-color: rgba(16, 185, 129, 0.5);
    background: rgba(16, 185, 129, 0.1);
    box-shadow: 0 0 15px rgba(16, 185, 129, 0.15);
  }
  .format-card.has-error:not(.active) {
    border-color: rgba(239, 68, 68, 0.35);
  }
  li.ok {
    border-color: color-mix(in srgb, var(--accent-primary) 30%, transparent);
  }
  li.err {
    border-color: rgba(239, 68, 68, 0.35);
  }
</style>
