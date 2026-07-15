<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { PackageOpen, RefreshCw, UploadCloud, CheckCircle2, AlertTriangle } from "lucide-svelte";
  import { projectPath, projectInfo } from "../lib/store";

  type ExportResult = {
    path: string;
    fileCount: number;
    overrideCount: number;
  };

  let targetPath = "";
  let serverTargetPath = "";
  let prismTargetPath = "";
  let curseforgeTargetPath = "";
  let projectDir = "";
  let exporting = false;
  let result: ExportResult | null = null;
  let error: string | null = null;
  let issues: { severity: "error" | "warning"; code: string; message: string; target?: string | null }[] = [];
  let exportMode: "mrpack" | "server" | "prism" | "curseforge" = "mrpack";

  let lastPathForDefaults = "";

  async function loadDefaultPaths(path: string) {
    projectDir = await invoke("get_project_dir", { path });
    issues = await invoke("validate_modrinth_export", { path });
    const id = $projectInfo?.id ?? "modpack";
    const version = $projectInfo?.version ?? "1.0.0";
    targetPath = `${projectDir}/${id}-${version}.mrpack`;
    serverTargetPath = `${projectDir}/${id}-${version}-server.zip`;
    prismTargetPath = `${projectDir}/${id}-${version}-prism.zip`;
    curseforgeTargetPath = `${projectDir}/${id}-${version}-curseforge.zip`;
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

  async function exportMrpack() {
    await runExport("export_modrinth_pack", targetPath || null);
  }

  async function exportServerPack() {
    await runExport("export_server_pack", serverTargetPath || null);
  }

  async function exportPrismInstance() {
    await runExport("export_prism_instance", prismTargetPath || null);
  }

  async function exportCurseForgePack() {
    await runExport("export_curseforge_pack", curseforgeTargetPath || null);
  }

  async function runExport(command: string, pathValue: string | null) {
    if (!$projectPath) return;
    exporting = true;
    error = null;
    result = null;
    try {
      result = await invoke(command, {
        path: $projectPath,
        targetPath: pathValue,
      });
    } catch (e) {
      error = String(e);
    } finally {
      exporting = false;
    }
  }

  $: onProjectPathChange($projectPath);
</script>

<div class="export-builder">
  <div class="toolbar">
    <div class="title"><UploadCloud size={18} /> Export builder</div>
    <button class="ghost" on:click={refreshDefaultPath} disabled={!$projectPath}>
      <RefreshCw size={16} />
      Default path
    </button>
  </div>

  {#if error}<div class="notice error"><AlertTriangle size={16} /> {error}</div>{/if}
  {#if result}
    <div class="notice success"><CheckCircle2 size={16} /> Exported {result.fileCount} remote mods and {result.overrideCount} override files to {result.path}</div>
  {/if}

  {#if !$projectPath}
    <div class="empty">Open a project to export a modpack.</div>
  {:else}
    <section class="panel">
      <div class="format-grid">
        <button class="format-card" class:active={exportMode === "mrpack"} on:click={() => (exportMode = "mrpack")}>
          <PackageOpen size={28} />
          <div>
            <h2>Modrinth .mrpack</h2>
            <p>modrinth.index.json + remote downloads + overrides.</p>
          </div>
        </button>
        <button class="format-card" class:active={exportMode === "server"} on:click={() => (exportMode = "server")}>
          <PackageOpen size={28} />
          <div>
            <h2>Server pack</h2>
            <p>Server-safe mods, configs, manifest and start scripts.</p>
          </div>
        </button>
        <button class="format-card" class:active={exportMode === "prism"} on:click={() => (exportMode = "prism")}>
          <PackageOpen size={28} />
          <div>
            <h2>Prism instance</h2>
            <p>instance.cfg + mmc-pack.json + mods/configs.</p>
          </div>
        </button>
        <button class="format-card" class:active={exportMode === "curseforge"} on:click={() => (exportMode = "curseforge")}>
          <PackageOpen size={28} />
          <div>
            <h2>CurseForge zip</h2>
            <p>manifest.json + overrides + remote mod manifest.</p>
          </div>
        </button>
      </div>

      <div class="paths-grid">
        <label>
          .mrpack output path
          <input bind:value={targetPath} placeholder=".../my-pack-1.0.0.mrpack" />
        </label>
        <label>
          Server pack output path
          <input bind:value={serverTargetPath} placeholder=".../my-pack-1.0.0-server.zip" />
        </label>
        <label>
          Prism instance output path
          <input bind:value={prismTargetPath} placeholder=".../my-pack-1.0.0-prism.zip" />
        </label>
        <label>
          CurseForge output path
          <input bind:value={curseforgeTargetPath} placeholder=".../my-pack-1.0.0-curseforge.zip" />
        </label>
      </div>

      <div class="checks">
        <div><strong>Dependencies</strong><span>Minecraft + selected loader are written to the index.</span></div>
        <div><strong>Mods</strong><span>Modrinth/direct URL mods are exported as remote downloads.</span></div>
        <div><strong>Overrides</strong><span>config/defaultconfigs/kubejs/scripts/resourcepacks/shaderpacks are embedded.</span></div>
        <div><strong>Server pack</strong><span>Excludes client-only mods, includes local server jars, download manifest, configs and start scripts.</span></div>
      </div>

      {#if issues.length > 0}
        <div class="issues">
          {#each issues as issue (issue.code + (issue.target ?? '') + issue.message)}
            <div class="issue {issue.severity}">
              <strong>{issue.code}</strong>
              <span>{issue.message}</span>
              {#if issue.target}<code>{issue.target}</code>{/if}
            </div>
          {/each}
        </div>
      {/if}

      <div class="publish-section">
        <h3>Publish is in Release</h3>
        <p>
          This stage only builds local artifacts. Configure tokens in Settings, then open the
          Release stage to publish to Modrinth, CurseForge or GitHub Releases.
        </p>
      </div>

      <div class="export-actions">
        {#if exportMode === "mrpack"}
          <button class="export" on:click={exportMrpack} disabled={exporting || issues.some((i) => i.severity === "error")}>
            <UploadCloud size={16} />
            {exporting ? "Exporting..." : "Export .mrpack"}
          </button>
        {:else if exportMode === "server"}
          <button class="secondary export" on:click={exportServerPack} disabled={exporting}>
            <PackageOpen size={16} />
            {exporting ? "Exporting..." : "Export server pack"}
          </button>
        {:else if exportMode === "prism"}
          <button class="secondary export" on:click={exportPrismInstance} disabled={exporting}>
            <PackageOpen size={16} />
            {exporting ? "Exporting..." : "Export Prism instance"}
          </button>
        {:else if exportMode === "curseforge"}
          <button class="secondary export" on:click={exportCurseForgePack} disabled={exporting}>
            <PackageOpen size={16} />
            {exporting ? "Exporting..." : "Export CurseForge zip"}
          </button>
        {/if}
      </div>
    </section>
  {/if}
</div>

<style>
  .export-builder { max-width: none; width: 100%; }
  .toolbar, .title, .notice, .format-card { display: flex; align-items: center; }
  .toolbar { justify-content: space-between; margin-bottom: 16px; }
  .title { gap: 10px; color: var(--text-secondary); font-weight: 700; }
  .notice { gap: 10px; padding: 12px 14px; border-radius: var(--border-radius-lg); margin-bottom: 14px; border: 1px solid var(--border-color); }
  .notice.error { color: #fecaca; background: rgba(239, 68, 68, 0.08); border-color: rgba(239, 68, 68, 0.28); }
  .notice.success { color: var(--accent-primary); background: rgba(27, 217, 106, 0.08); border-color: rgba(27, 217, 106, 0.25); }
  .panel, .empty { background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); }
  .panel { padding: 22px; display: grid; gap: 18px; }
  .format-grid { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 12px; }
  .format-card { gap: 14px; padding: 18px; text-align: left; justify-content: flex-start; color: var(--text-secondary); border-radius: var(--border-radius-lg); background: var(--bg-tertiary); border: 1px solid var(--border-color); transform: none; }
  .format-card.active { background: radial-gradient(circle at top left, rgba(27, 217, 106, 0.12), transparent 45%), var(--bg-tertiary); border-color: rgba(27, 217, 106, 0.45); color: var(--text-primary); }
  .format-card.planned { opacity: .76; }
  .format-card h2 { margin: 0 0 4px; }
  .format-card p, .checks span { color: var(--text-muted); }
  label { display: grid; gap: 8px; color: var(--text-secondary); font-weight: 700; }
  input { width: 100%; }
  .paths-grid { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 12px; }
  .checks { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 12px; }
  .checks div { background: var(--bg-tertiary); border: 1px solid var(--border-color); border-radius: 14px; padding: 14px; display: grid; gap: 4px; }
  .issues { display: grid; gap: 8px; }
  .issue { display: grid; gap: 4px; padding: 12px; border-radius: 12px; background: var(--bg-tertiary); border: 1px solid var(--border-color); }
  .issue.warning { border-color: rgba(245, 158, 11, 0.3); }
  .issue.error { border-color: rgba(239, 68, 68, 0.3); }
  .issue span { color: var(--text-muted); }
  code { color: var(--text-secondary); font-family: ui-monospace, monospace; }
  .publish-section { padding: 16px; border: 1px solid rgba(27,217,106,.25); border-radius: var(--border-radius-lg); background: rgba(27,217,106,.03); }
  .publish-section h3 { color: var(--text-primary); font-size: 14px; margin: 0 0 4px; }
  .publish-section p { color: var(--text-muted); font-size: 12px; margin: 0; line-height: 1.45; }

  .export-actions { display: flex; gap: 10px; flex-wrap: wrap; }
  .export { justify-self: start; }
  .empty { color: var(--text-muted); padding: 80px; text-align: center; }
  @media (max-width: 900px) { .checks, .paths-grid, .format-grid { grid-template-columns: 1fr; } }
</style>
