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
  let projectDir = "";
  let exporting = false;
  let result: ExportResult | null = null;
  let error: string | null = null;
  let issues: { severity: "error" | "warning"; code: string; message: string; target?: string | null }[] = [];

  async function refreshDefaultPath() {
    if (!$projectPath) return;
    projectDir = await invoke("get_project_dir", { path: $projectPath });
    issues = await invoke("validate_modrinth_export", { path: $projectPath });
    const id = $projectInfo?.id ?? "modpack";
    const version = $projectInfo?.version ?? "1.0.0";
    targetPath = `${projectDir}/${id}-${version}.mrpack`;
  }

  async function exportMrpack() {
    if (!$projectPath) return;
    exporting = true;
    error = null;
    result = null;
    try {
      result = await invoke("export_modrinth_pack", {
        path: $projectPath,
        targetPath: targetPath || null,
      });
    } catch (e) {
      error = String(e);
    } finally {
      exporting = false;
    }
  }

  $: if ($projectPath && !targetPath) refreshDefaultPath();
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
      <div class="format-card active">
        <PackageOpen size={28} />
        <div>
          <h2>Modrinth .mrpack</h2>
          <p>Creates `modrinth.index.json`, external mod downloads and `overrides/` for configs/scripts/resource packs.</p>
        </div>
      </div>

      <label>
        Output path
        <input bind:value={targetPath} placeholder=".../my-pack-1.0.0.mrpack" />
      </label>

      <div class="checks">
        <div><strong>Dependencies</strong><span>Minecraft + selected loader are written to the index.</span></div>
        <div><strong>Mods</strong><span>Modrinth/direct URL mods are exported as remote downloads.</span></div>
        <div><strong>Overrides</strong><span>config/defaultconfigs/kubejs/scripts/resourcepacks/shaderpacks are embedded.</span></div>
      </div>

      {#if issues.length > 0}
        <div class="issues">
          {#each issues as issue}
            <div class="issue {issue.severity}">
              <strong>{issue.code}</strong>
              <span>{issue.message}</span>
              {#if issue.target}<code>{issue.target}</code>{/if}
            </div>
          {/each}
        </div>
      {/if}

      <button class="export" on:click={exportMrpack} disabled={exporting || issues.some((i) => i.severity === "error")}>
        <UploadCloud size={16} />
        {exporting ? "Exporting..." : "Export .mrpack"}
      </button>
    </section>
  {/if}
</div>

<style>
  .export-builder { max-width: 1100px; }
  .toolbar, .title, .notice, .format-card { display: flex; align-items: center; }
  .toolbar { justify-content: space-between; margin-bottom: 16px; }
  .title { gap: 10px; color: var(--text-secondary); font-weight: 700; }
  .notice { gap: 10px; padding: 12px 14px; border-radius: var(--border-radius-lg); margin-bottom: 14px; border: 1px solid var(--border-color); }
  .notice.error { color: #fecaca; background: rgba(239, 68, 68, 0.08); border-color: rgba(239, 68, 68, 0.28); }
  .notice.success { color: var(--accent-primary); background: rgba(27, 217, 106, 0.08); border-color: rgba(27, 217, 106, 0.25); }
  .panel, .empty { background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); }
  .panel { padding: 22px; display: grid; gap: 18px; }
  .format-card { gap: 14px; padding: 18px; border-radius: var(--border-radius-lg); background: radial-gradient(circle at top left, rgba(27, 217, 106, 0.12), transparent 45%), var(--bg-tertiary); border: 1px solid rgba(27, 217, 106, 0.28); }
  .format-card h2 { margin: 0 0 4px; }
  .format-card p, .checks span { color: var(--text-muted); }
  label { display: grid; gap: 8px; color: var(--text-secondary); font-weight: 700; }
  input { width: 100%; }
  .checks { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 12px; }
  .checks div { background: var(--bg-tertiary); border: 1px solid var(--border-color); border-radius: 14px; padding: 14px; display: grid; gap: 4px; }
  .issues { display: grid; gap: 8px; }
  .issue { display: grid; gap: 4px; padding: 12px; border-radius: 12px; background: var(--bg-tertiary); border: 1px solid var(--border-color); }
  .issue.warning { border-color: rgba(245, 158, 11, 0.3); }
  .issue.error { border-color: rgba(239, 68, 68, 0.3); }
  .issue span { color: var(--text-muted); }
  code { color: var(--text-secondary); font-family: ui-monospace, monospace; }
  .export { justify-self: start; }
  .empty { color: var(--text-muted); padding: 80px; text-align: center; }
  @media (max-width: 900px) { .checks { grid-template-columns: 1fr; } }
</style>
