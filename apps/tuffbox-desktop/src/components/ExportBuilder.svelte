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
  let mrIssues = $state<ExportIssue[]>([]);
  let cfIssues = $state<ExportIssue[]>([]);
  let exportMode = $state<ExportMode>("mrpack");
  let copiedFlash = $state(false);

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
      <div class="grid gap-2 grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-5" role="listbox" aria-label="Export format">
        {#each FORMATS as fmt (fmt.id)}
          {@const errs = formatErrors(fmt.id)}
          {@const warns = formatWarns(fmt.id)}
          <button
            type="button"
            class="format-card grid grid-cols-[auto_minmax(0,1fr)] grid-rows-[auto_auto] gap-x-2 gap-y-0.5 items-start text-left px-2.5 py-2 rounded-[length:var(--border-radius-md)] bg-white/[0.04] border border-white/[0.08] text-[color:var(--text-secondary)] cursor-pointer min-w-0 overflow-hidden hover:border-[color:color-mix(in_srgb,#10b981_40%,transparent)] hover:bg-white/[0.07]"
            class:active={exportMode === fmt.id}
            class:has-error={errs > 0}
            role="option"
            aria-selected={exportMode === fmt.id}
            title="{fmt.title} {fmt.badge} — {fmt.blurb}"
            onclick={() => (exportMode = fmt.id)}
          >
            <span class="row-span-2 flex items-center justify-center w-7 h-7 rounded-md bg-white/[0.06] text-[color:var(--accent-primary)] mt-px" aria-hidden="true">
              {#if fmt.id === "mrpack"}<PackageOpen size={16} />
              {:else if fmt.id === "curseforge"}<FileArchive size={16} />
              {:else if fmt.id === "prism"}<Box size={16} />
              {:else if fmt.id === "server"}<Server size={16} />
              {:else}<FolderTree size={16} />{/if}
            </span>
            <span class="flex items-baseline gap-1.5 min-w-0 max-w-full overflow-hidden">
              <span class="min-w-0 flex-1 text-xs font-bold leading-tight truncate">{fmt.title}</span>
              <span class="text-[10px] font-semibold text-[color:var(--text-muted)] lowercase shrink-0">{fmt.badge}</span>
              {#if errs > 0}
                <span class="shrink-0 min-w-3.5 h-3.5 px-1 rounded-full inline-flex items-center justify-center text-[10px] font-extrabold leading-none text-[#fecaca] bg-[rgba(239,68,68,0.15)] border border-[rgba(239,68,68,0.4)]" title="{errs} blocking error(s)">{errs}</span>
              {:else if warns > 0}
                <span class="shrink-0 min-w-3.5 h-3.5 px-1 rounded-full inline-flex items-center justify-center text-[10px] font-extrabold leading-none text-[#fbbf24] bg-[rgba(245,158,11,0.15)] border border-[rgba(245,158,11,0.35)]" title="{warns} warning(s)">{warns}</span>
              {/if}
            </span>
            <span class="col-start-2 min-w-0 text-[11px] text-[color:var(--text-muted)] leading-tight line-clamp-2 break-words">{fmt.blurb}</span>
          </button>
        {/each}
      </div>

      <div class="grid gap-2.5 p-3 border border-white/[0.08] rounded-[length:var(--border-radius-md)] bg-black/40">
        <div>
          <h2 class="m-0 mb-1 text-sm font-bold text-[color:var(--text-primary)]">{activeFormat.title}</h2>
          <p class="m-0 text-xs text-[color:var(--text-muted)] leading-snug">{activeFormat.detail}</p>
        </div>

        <label class="grid gap-1 text-[11px] font-semibold text-[color:var(--text-secondary)]">
          {activeFormat.pathKind === "dir" ? "Output folder" : "Output file"}
          <div class="flex gap-1.5 items-stretch flex-wrap sm:flex-nowrap">
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

        {#if activeIssues.length > 0}
          <div class="flex flex-col gap-2">
            <div class="flex items-center gap-2 flex-wrap">
              {#if blockingErrors.length > 0}
                <span class="text-[10px] font-bold px-[7px] py-0.5 rounded-full text-[#fecaca] bg-[rgba(239,68,68,0.12)] border border-[rgba(239,68,68,0.3)]">{blockingErrors.length} error{blockingErrors.length === 1 ? "" : "s"}</span>
              {/if}
              {#if warnCount > 0}
                <span class="text-[10px] font-bold px-[7px] py-0.5 rounded-full text-[#fbbf24] bg-[rgba(245,158,11,0.12)] border border-[rgba(245,158,11,0.3)]">{warnCount} warning{warnCount === 1 ? "" : "s"}</span>
              {/if}
              {#if groupedIssues.length > 1}
                <button type="button" class="ml-auto text-[10px] text-[color:var(--text-muted)] hover:text-[color:var(--text-secondary)] cursor-pointer" onclick={expandAllGroups}>Expand all</button>
                <button type="button" class="text-[10px] text-[color:var(--text-muted)] hover:text-[color:var(--text-secondary)] cursor-pointer" onclick={collapseAllGroups}>Collapse all</button>
              {/if}
            </div>

            {#each errorGroups as g (g.code)}
              <div class="grid gap-0.5 p-2 rounded-md bg-red-500/10 border border-red-500/30 text-[11px] backdrop-blur-sm">
                <div class="flex items-baseline gap-2 flex-wrap">
                  <strong class="text-[#fecaca]">{g.code}{#if g.count > 1}&nbsp;× {g.count}{/if}</strong>
                  <span class="text-[color:var(--text-muted)] break-words">{g.message}</span>
                </div>
                {#if g.targets.length > 0}
                  <div class="flex flex-wrap gap-x-2 gap-y-0.5">
                    {#each g.targets.slice(0, 8) as tgt (tgt)}
                      <code class="font-mono text-[10px] text-[color:var(--text-secondary)] break-all">{tgt}</code>
                    {/each}
                    {#if g.targets.length > 8}<span class="text-[10px] text-[color:var(--text-muted)]">+{g.targets.length - 8} more</span>{/if}
                  </div>
                {/if}
              </div>
            {/each}

            {#each warningGroups as g (g.code)}
              <div class="rounded-md bg-amber-500/10 border border-amber-500/30 backdrop-blur-sm text-[11px]">
                <button
                  type="button"
                  class="w-full flex items-center gap-2 px-2 py-2 text-left cursor-pointer hover:bg-amber-500/10 rounded-md"
                  onclick={() => toggleGroup(g.code)}
                  aria-expanded={expandedGroups.has(g.code)}
                >
                  <strong class="text-amber-300 shrink-0">{g.code}{#if g.count > 1}&nbsp;× {g.count}{/if}</strong>
                  <span class="text-[color:var(--text-muted)] truncate">{g.message}</span>
                  <span class="ml-auto text-[10px] text-[color:var(--text-muted)] shrink-0">{expandedGroups.has(g.code) ? "▲" : "▼"}</span>
                </button>
                {#if expandedGroups.has(g.code)}
                  <div class="flex flex-wrap gap-x-2 gap-y-0.5 px-2 pb-2">
                    {#each g.targets as tgt (tgt)}
                      <code class="font-mono text-[10px] text-[color:var(--text-secondary)] break-all">{tgt}</code>
                    {/each}
                  </div>
                {/if}
              </div>
            {/each}
          </div>
        {/if}

        <div class="flex gap-2 flex-wrap items-center">
          <button class="eb-export-btn" onclick={runSelectedExport} disabled={exportBlocked || batching}>
            <UploadCloud size={15} />
            {#if exporting}
              Exporting…
            {:else}
              Export {activeFormat.title}
            {/if}
          </button>
          {#if result}
            <button class="ghost" onclick={() => openPath(result?.path)}>
              <ExternalLink size={14} /> Open
            </button>
            <button class="ghost" onclick={() => copyPath(result?.path)}>
              <Copy size={14} /> {copiedFlash ? "Copied" : "Copy path"}
            </button>
          {/if}
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
