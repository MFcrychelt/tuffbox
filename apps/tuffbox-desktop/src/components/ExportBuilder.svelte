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
  const mrBlocking = $derived(mrIssues.filter((i) => i.severity === "error"));
  const warnCount = $derived(activeIssues.filter((i) => i.severity === "warning").length);
  const exportBlocked = $derived(
    exporting || (activeFormat.validation != null && blockingErrors.length > 0),
  );
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

  function onProjectPathChange(path: string | null) {
    if (!path || path === lastPathForDefaults) return;
    lastPathForDefaults = path;
    void loadDefaultPaths(path);
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
    onProjectPathChange($projectPath);
  });
</script>

<div class="export-builder">
  <div class="toolbar">
    <div class="title-block">
      <div class="title"><UploadCloud size={16} /> Export</div>
      {#if packSummary}
        <div class="pack-summary" title={packSummary}>{packSummary}</div>
      {/if}
    </div>
    <div class="toolbar-actions">
      <button class="ghost mini" onclick={refreshDefaultPath} disabled={!$projectPath} title="Reset output paths">
        <RefreshCw size={14} />
        Defaults
      </button>
      <button
        class="ghost mini"
        onclick={exportAllFormats}
        disabled={!$projectPath || batching || exporting}
        title="Build all formats into ./export"
      >
        <Layers size={14} />
        {batching ? "Exporting all…" : "Export all"}
      </button>
    </div>
  </div>

  {#if error}<div class="notice error"><AlertTriangle size={14} /> {error}</div>{/if}
  {#if result}
    <div class="notice success">
      <CheckCircle2 size={14} />
      <span>
        Exported {result.fileCount} entries
        {#if result.overrideCount > 0}· {result.overrideCount} overrides{/if}
        → <code title={result.path}>{result.path}</code>
      </span>
      <button class="ghost mini" onclick={() => copyPath(result?.path)} title="Copy path">
        <Copy size={13} />
        {copiedFlash ? "Copied" : "Copy"}
      </button>
      <button class="ghost mini" onclick={() => openPath(result?.path)} title="Open output">
        <ExternalLink size={13} />
      </button>
    </div>
  {/if}

  {#if !$projectPath}
    <EmptyState icon={PackageOpen} title="No project selected" description="Open a project to export a modpack." />
  {:else}
    <section class="panel">
      <div class="format-grid" role="listbox" aria-label="Export format">
        {#each FORMATS as fmt (fmt.id)}
          {@const errs = formatErrors(fmt.id)}
          {@const warns = formatWarns(fmt.id)}
          <button
            type="button"
            class="format-card"
            class:active={exportMode === fmt.id}
            class:has-error={errs > 0}
            role="option"
            aria-selected={exportMode === fmt.id}
            title="{fmt.title} {fmt.badge} — {fmt.blurb}"
            onclick={() => (exportMode = fmt.id)}
          >
            <span class="fmt-icon" aria-hidden="true">
              {#if fmt.id === "mrpack"}<PackageOpen size={16} />
              {:else if fmt.id === "curseforge"}<FileArchive size={16} />
              {:else if fmt.id === "prism"}<Box size={16} />
              {:else if fmt.id === "server"}<Server size={16} />
              {:else}<FolderTree size={16} />{/if}
            </span>
            <span class="fmt-text">
              <span class="fmt-title">{fmt.title}</span>
              <span class="fmt-badge">{fmt.badge}</span>
              {#if errs > 0}
                <span class="fmt-chip err" title="{errs} blocking error(s)">{errs}</span>
              {:else if warns > 0}
                <span class="fmt-chip warn" title="{warns} warning(s)">{warns}</span>
              {/if}
            </span>
            <span class="fmt-blurb">{fmt.blurb}</span>
          </button>
        {/each}
      </div>

      <div class="detail">
        <div class="detail-head">
          <h2>{activeFormat.title}</h2>
          <p>{activeFormat.detail}</p>
        </div>

        <label class="path-field">
          {activeFormat.pathKind === "dir" ? "Output folder" : "Output file"}
          <div class="path-row">
            <input
              value={activePath}
              oninput={(e) => setActivePath(e.currentTarget.value)}
              placeholder={activeFormat.pathKind === "dir" ? ".../pack-packwiz" : ".../pack.zip"}
            />
            <button type="button" class="ghost mini browse" onclick={browseOutput}>
              <FolderOpen size={14} />
              Browse
            </button>
          </div>
        </label>

        {#if activeIssues.length > 0}
          <div class="issues">
            <div class="issues-head">
              {#if blockingErrors.length > 0}
                <span class="chip err">{blockingErrors.length} error{blockingErrors.length === 1 ? "" : "s"}</span>
              {/if}
              {#if warnCount > 0}
                <span class="chip warn">{warnCount} warning{warnCount === 1 ? "" : "s"}</span>
              {/if}
            </div>
            {#each activeIssues as issue (issue.code + (issue.target ?? "") + issue.message)}
              <div class="issue {issue.severity}">
                <strong>{issue.code}</strong>
                <span>{issue.message}</span>
                {#if issue.target}<code>{issue.target}</code>{/if}
              </div>
            {/each}
          </div>
        {/if}

        <div class="export-actions">
          <button class="export" onclick={runSelectedExport} disabled={exportBlocked || batching}>
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
        <div class="batch">
          <h3>Batch results · ./export</h3>
          <ul>
            {#each batchResults as row (row.kind)}
              <li class:ok={row.status === "ok"} class:err={row.status !== "ok"}>
                <strong>{row.kind}</strong>
                {#if row.status === "ok"}
                  <span>{row.files ?? "?"} files</span>
                  <div class="batch-actions">
                    {#if row.path}
                      <button class="ghost mini" onclick={() => openPath(row.path)} title={row.path}>
                        <ExternalLink size={12} /> Open
                      </button>
                      <button class="ghost mini" onclick={() => copyPath(row.path)}>
                        <Copy size={12} />
                      </button>
                    {/if}
                  </div>
                  {#if row.path}<code title={row.path}>{row.path}</code>{/if}
                {:else}
                  <span>{row.error ?? "failed"}</span>
                {/if}
              </li>
            {/each}
          </ul>
        </div>
      {/if}

      <p class="publish-hint">
        Publish tokens live in Settings · upload artifacts from the Release stage after export.
      </p>
    </section>
  {/if}
</div>

<style>
  .export-builder {
    width: 100%;
    max-width: none;
  }
  .toolbar {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 12px;
  }
  .title-block {
    min-width: 0;
  }
  .title {
    display: flex;
    align-items: center;
    gap: 8px;
    color: var(--text-secondary);
    font-weight: 700;
    font-size: 13px;
  }
  .pack-summary {
    margin-top: 3px;
    font-size: 11px;
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    max-width: min(560px, 55vw);
  }
  .toolbar-actions {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
    flex-shrink: 0;
  }
  .notice {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 8px 10px;
    border-radius: var(--border-radius-md);
    margin-bottom: 10px;
    border: 1px solid var(--border-color);
    font-size: 12px;
    line-height: 1.4;
  }
  .notice.error {
    color: var(--accent-danger);
    background: color-mix(in srgb, var(--accent-danger) 8%, transparent);
    border-color: color-mix(in srgb, var(--accent-danger) 28%, transparent);
  }
  .notice.success {
    color: var(--accent-primary);
    background: color-mix(in srgb, var(--accent-primary) 8%, transparent);
    border-color: color-mix(in srgb, var(--accent-primary) 25%, transparent);
  }
  .notice code {
    font-size: 11px;
    word-break: break-all;
  }
  .panel {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
    padding: 14px;
    display: grid;
    gap: 12px;
  }
  .format-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(168px, 1fr));
    gap: 8px;
  }
  .format-card {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    grid-template-rows: auto auto;
    column-gap: 8px;
    row-gap: 2px;
    align-items: start;
    text-align: left;
    padding: 9px 10px;
    border-radius: var(--border-radius-md);
    background: var(--bg-tertiary);
    border: 1px solid var(--border-color);
    color: var(--text-secondary);
    cursor: pointer;
    min-height: 0;
    min-width: 0;
    overflow: hidden;
  }
  .format-card:hover {
    border-color: color-mix(in srgb, var(--accent-primary) 35%, var(--border-color));
  }
  .format-card.active {
    color: var(--text-primary);
    border-color: color-mix(in srgb, var(--accent-primary) 55%, transparent);
    background: color-mix(in srgb, var(--accent-primary) 8%, var(--bg-tertiary));
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--accent-primary) 18%, transparent);
  }
  .format-card.has-error:not(.active) {
    border-color: rgba(239, 68, 68, 0.35);
  }
  .fmt-icon {
    grid-row: 1 / span 2;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border-radius: 6px;
    background: color-mix(in srgb, var(--bg-secondary) 70%, transparent);
    color: var(--accent-primary);
    margin-top: 1px;
  }
  .fmt-text {
    display: flex;
    align-items: baseline;
    gap: 6px;
    min-width: 0;
    max-width: 100%;
    overflow: hidden;
  }
  .fmt-title {
    min-width: 0;
    flex: 1 1 auto;
    font-size: 12px;
    font-weight: 700;
    line-height: 1.2;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .fmt-badge {
    font-size: 10px;
    font-weight: 600;
    color: var(--text-muted);
    text-transform: lowercase;
    flex-shrink: 0;
  }
  .fmt-chip {
    flex-shrink: 0;
    min-width: 14px;
    height: 14px;
    padding: 0 4px;
    border-radius: 999px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    font-size: 10px;
    font-weight: 800;
    line-height: 1;
  }
  .fmt-chip.warn {
    color: var(--accent-warning);
    background: color-mix(in srgb, var(--accent-warning) 15%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent-warning) 35%, transparent);
  }
  .fmt-chip.err {
    color: var(--accent-danger);
    background: color-mix(in srgb, var(--accent-danger) 15%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent-danger) 40%, transparent);
  }
  .fmt-blurb {
    grid-column: 2;
    min-width: 0;
    font-size: 11px;
    color: var(--text-muted);
    line-height: 1.3;
    overflow: hidden;
    display: -webkit-box;
    -webkit-box-orient: vertical;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    overflow-wrap: anywhere;
    word-break: break-word;
  }
  .detail {
    display: grid;
    gap: 10px;
    padding: 12px;
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-md);
    background: var(--bg-tertiary);
  }
  .detail-head h2 {
    margin: 0 0 4px;
    font-size: 14px;
    font-weight: 700;
    color: var(--text-primary);
  }
  .detail-head p {
    margin: 0;
    font-size: 12px;
    color: var(--text-muted);
    line-height: 1.4;
  }
  .path-field {
    display: grid;
    gap: 5px;
    font-size: 11px;
    font-weight: 600;
    color: var(--text-secondary);
  }
  .path-row {
    display: flex;
    gap: 6px;
    align-items: stretch;
  }
  .path-row input {
    flex: 1;
    min-width: 0;
    font-size: 12px;
    padding: 7px 9px;
  }
  .browse {
    flex-shrink: 0;
  }
  .issues {
    display: grid;
    gap: 6px;
    max-height: 160px;
    overflow: auto;
  }
  .issues-head {
    display: flex;
    gap: 6px;
  }
  .chip {
    font-size: 10px;
    font-weight: 700;
    padding: 2px 7px;
    border-radius: 999px;
  }
  .chip.err {
    color: var(--accent-danger);
    background: color-mix(in srgb, var(--accent-danger) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent-danger) 30%, transparent);
  }
  .chip.warn {
    color: var(--accent-warning);
    background: color-mix(in srgb, var(--accent-warning) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent-warning) 30%, transparent);
  }
  .issue {
    display: grid;
    gap: 2px;
    padding: 8px 9px;
    border-radius: 6px;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    font-size: 11px;
  }
  .issue.warning {
    border-color: rgba(245, 158, 11, 0.35);
  }
  .issue.error {
    border-color: rgba(239, 68, 68, 0.35);
  }
  .issue span {
    color: var(--text-muted);
    overflow-wrap: anywhere;
    word-break: break-word;
  }
  .issue code {
    font-family: ui-monospace, monospace;
    color: var(--text-secondary);
    font-size: 10px;
    overflow-wrap: anywhere;
    word-break: break-all;
  }
  .export-actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    align-items: center;
  }
  .export {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .batch {
    display: grid;
    gap: 6px;
  }
  .batch h3 {
    margin: 0;
    font-size: 12px;
    font-weight: 700;
    color: var(--text-secondary);
  }
  .batch ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 4px;
  }
  .batch li {
    display: grid;
    grid-template-columns: 88px minmax(0, 1fr) auto;
    gap: 6px 10px;
    padding: 6px 8px;
    border-radius: 6px;
    border: 1px solid var(--border-color);
    background: var(--bg-tertiary);
    font-size: 11px;
    align-items: center;
    min-width: 0;
    overflow: hidden;
  }
  .batch li.ok {
    border-color: color-mix(in srgb, var(--accent-primary) 30%, transparent);
  }
  .batch li.err {
    border-color: rgba(239, 68, 68, 0.35);
  }
  .batch-actions {
    display: flex;
    gap: 4px;
  }
  .batch code {
    grid-column: 2 / -1;
    font-size: 10px;
    color: var(--text-muted);
    word-break: break-all;
  }
  .publish-hint {
    margin: 0;
    font-size: 11px;
    color: var(--text-muted);
    line-height: 1.4;
  }
  @media (max-width: 720px) {
    .batch li {
      grid-template-columns: 72px 1fr;
    }
    .batch-actions {
      grid-column: 2;
    }
  }
</style>
