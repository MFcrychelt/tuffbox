<script lang="ts">
  import { ArrowLeft, Save, Cpu, Container, Coffee, Terminal, Search, Database, RefreshCw, AlertTriangle } from "@lucide/svelte";
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { projectInfo, projectPath, recentProjects } from "../lib/store";
  import EmptyState from "./EmptyState.svelte";
  import JavaPickerModal from "./JavaPickerModal.svelte";

  let {
    onBack = () => {},
    showBack = true,
    stayAfterSave = false,
  }: {
    onBack?: () => void;
    showBack?: boolean;
    stayAfterSave?: boolean;
  } = $props();

  const memoryMarks = [1024, 2048, 4096, 6144, 8192, 12288, 16384];
  const loaders = [
    { id: "vanilla", label: "Vanilla" },
    { id: "fabric", label: "Fabric" },
    { id: "forge", label: "Forge" },
    { id: "neoforge", label: "NeoForge" },
    { id: "quilt", label: "Quilt" },
  ];

  let memory = $state($projectInfo?.memoryMb ?? 4096);
  let jvmArgs = $state(($projectInfo?.jvmArgs ?? ["-XX:+UseG1GC"]).join(" "));
  let javaPath = $state($projectInfo?.javaPath ?? "Auto-detect");
  let javaVersion = $state("");
  let playerName = $state($projectInfo?.playerName ?? "Player");

  let mcVersion = $state($projectInfo?.minecraftVersion ?? "");
  let loader = $state($projectInfo?.loaderKind ?? "vanilla");
  let loaderVersion = $state($projectInfo?.loaderVersion ?? "");

  let mcVersions = $state<{ id: string; popular: boolean }[]>([]);
  let loaderVersions = $state<{ id: string; stable: boolean }[]>([]);
  let showJavaPicker = $state(false);
  let saving = $state(false);
  let loading = $state(false);
  let error = $state("");

  // Schema status
  let schemaVersion = $state("");
  let schemaNeedsMigration = $state(false);
  let schemaLoading = $state(false);

  async function loadSchemaStatus() {
    if (!$projectPath) return;
    schemaLoading = true;
    try {
      const status: any = await invoke("get_project_schema_status", { path: $projectPath });
      schemaVersion = status.detected ?? "?";
      schemaNeedsMigration = status.needsMigration ?? false;
    } catch {
      schemaVersion = "?";
    } finally {
      schemaLoading = false;
    }
  }

  async function migrateSchema() {
    if (!$projectPath) return;
    saving = true;
    error = "";
    try {
      await invoke("migrate_project_schema", { path: $projectPath });
      await loadSchemaStatus();
    } catch (e) {
      error = `${e}`;
    } finally {
      saving = false;
    }
  }

  onMount(async () => {
    loading = true;
    error = "";
    try {
      // All fetches are independent and run in parallel.
      const [versions] = await Promise.all([
        invoke("get_minecraft_versions"),
        detectJavaPreview(),
        loadSchemaStatus(),
        loadLoaderVersions(),
      ]);
      mcVersions = versions as { id: string; popular: boolean }[];
    } catch (e) {
      error = `${e}`;
    } finally {
      loading = false;
    }
  });

  let loadingLoader = $state(false);
  async function loadLoaderVersions() {
    if (loadingLoader) return;
    if (loader === "vanilla") {
      loaderVersions = [];
      loaderVersion = "";
      return;
    }
    loadingLoader = true;
    try {
      loaderVersions = await invoke("get_loader_versions", {
        loader,
        minecraftVersion: mcVersion,
      });
      if (!loaderVersions.some((v) => v.id === loaderVersion)) {
        loaderVersion = loaderVersions.find((v) => v.stable)?.id ?? loaderVersions[0]?.id ?? "";
      }
    } catch (e) {
      loaderVersions = [];
      loaderVersion = "";
    } finally {
      loadingLoader = false;
    }
  }

  async function detectJavaPreview() {
    if (javaPath && javaPath !== "Auto-detect") {
      javaVersion = (await invoke("get_java_version", { path: javaPath }).catch(() => "")) as string;
    } else {
      javaVersion = (await invoke("get_default_java_version").catch(() => "")) as string;
    }
  }

  async function save() {
    if (!$projectPath) return;
    saving = true;
    error = "";
    try {
      await invoke("update_project_settings", {
        path: $projectPath,
        minecraftVersion: mcVersion,
        loader,
        loaderVersion,
        javaPath: javaPath === "Auto-detect" ? null : javaPath,
        memoryMb: memory,
        jvmArgs: jvmArgs.split(/\s+/).filter(Boolean),
        playerName: playerName.trim() || null,
      });
      const info = await invoke("validate_project", { path: $projectPath });
      projectInfo.set(info as any);
      recentProjects.updateInfo($projectPath, info as any);
      if (!stayAfterSave) onBack();
    } catch (e) {
      error = `${e}`;
    } finally {
      saving = false;
    }
  }

  async function onJavaSelected(path: string) {
    javaPath = path;
    await detectJavaPreview();
  }

  function formatMemory(mb: number) {
    if (mb >= 1024) return `${mb / 1024} GB`;
    return `${mb} MB`;
  }

  $effect(() => {
    if (mcVersions.length > 0 && loader !== "vanilla" && loaderVersions.length === 0) {
      loadLoaderVersions();
    }
  });

  function onLoaderChange() {
    loaderVersions = [];
    loaderVersion = "";
    if (loader !== "vanilla") loadLoaderVersions();
  }

  function onMcVersionChange() {
    loaderVersions = [];
    loaderVersion = "";
    if (loader !== "vanilla") loadLoaderVersions();
  }
</script>

<div class="settings-page">
  <header class="page-header">
    {#if showBack}
      <button class="ghost back" onclick={onBack}>
        <ArrowLeft size={18} />
        Back
      </button>
    {/if}
    <h1>Instance Settings</h1>
  </header>

  {#if $projectPath}
    {#if loading}
      <div class="loading">
        <RefreshCw size={18} class="spin" />
        Loading instance settings…
      </div>
    {/if}
    <div class="settings-groups" class:dimmed={loading}>
      <div class="section-group">
        <h2 class="section-heading">Runtime</h2>
        <div class="settings-grid">
      <section class="card">
        <div class="card-title">
          <Container size={18} />
          <h3>Game</h3>
        </div>
        <div class="field">
          <label for="mc-version">Minecraft version</label>
          <select id="mc-version" bind:value={mcVersion} onchange={onMcVersionChange}>
            {#each mcVersions as v}
              <option value={v.id}>
                {v.id}{#if v.popular} ★{/if}
              </option>
            {/each}
          </select>
        </div>
        <div class="field-row">
          <div class="field">
            <label for="loader-kind">Loader</label>
            <select id="loader-kind" bind:value={loader} onchange={onLoaderChange}>
              {#each loaders as l}
                <option value={l.id}>{l.label}</option>
              {/each}
            </select>
          </div>
          <div class="field">
            <label for="loader-version">Loader version</label>
            {#if loader === "vanilla"}
              <input id="loader-version" value="-" disabled />
            {:else}
              <select id="loader-version" bind:value={loaderVersion}>
                {#each loaderVersions as v}
                  <option value={v.id}>{v.id}{#if v.stable} (stable){/if}</option>
                {/each}
              </select>
            {/if}
          </div>
        </div>
      </section>

      <section class="card">
        <div class="card-title">
          <Coffee size={18} />
          <h3>Java</h3>
        </div>
        <div class="field">
          <label for="java-path">Java executable</label>
          <div class="input-row">
            <input id="java-path" bind:value={javaPath} readonly />
            <button class="icon-btn" onclick={() => (showJavaPicker = true)} aria-label="Search Java">
              <Search size={18} />
            </button>
          </div>
          {#if javaVersion}
            <div class="java-preview">{javaVersion}</div>
          {/if}
        </div>
      </section>

      <section class="card wide">
        <div class="card-title">
          <Cpu size={18} />
          <h3>Memory</h3>
        </div>
        <div class="memory-control">
          <div class="memory-value">{formatMemory(memory)}</div>
          <input
            type="range"
            min={1024}
            max={16384}
            step={512}
            bind:value={memory}
            class="memory-slider"
          />
          <div class="memory-marks">
            {#each memoryMarks as mark}
              <button
                class="mark"
                class:active={memory === mark}
                onclick={() => (memory = mark)}
              >
                {formatMemory(mark)}
              </button>
            {/each}
          </div>
        </div>
      </section>

      <section class="card wide">
        <div class="card-title">
          <Terminal size={18} />
          <h3>JVM Arguments</h3>
        </div>
        <div class="field">
          <textarea bind:value={jvmArgs} rows={4}></textarea>
        </div>
      </section>
        </div>
      </div>

      <div class="section-group">
        <h2 class="section-heading">Project</h2>
        <div class="settings-grid">
      <section class="card">
        <div class="card-title">
          <Terminal size={18} />
          <h3>Player</h3>
        </div>
        <div class="field">
          <label for="player-name">Player name (offline test launches)</label>
          <input id="player-name" bind:value={playerName} placeholder="Player" maxlength="16" />
          <p class="field-hint">
            Used for test runs. TuffBox derives a stable offline UUID from this name
            (same algorithm vanilla uses), so the same name always maps to the same
            in-game identity across launches.
          </p>
        </div>
      </section>

      <section class="card wide">
        <div class="card-title">
          <Database size={18} />
          <h3>Project schema</h3>
        </div>
        <div class="schema-info">
          <div class="schema-row">
            <span>Schema version</span>
            <code>{schemaVersion || "..."}</code>
          </div>
          {#if schemaNeedsMigration}
            <div class="schema-warning">
              <AlertTriangle size={16} />
              <span>Schema migration available. This will normalize your manifest to the current format.</span>
            </div>
            <button class="secondary" onclick={migrateSchema} disabled={saving}>
              <RefreshCw size={16} />
              {saving ? "Migrating..." : "Migrate schema"}
            </button>
          {:else if schemaVersion}
            <div class="schema-ok">✓ Schema is up to date</div>
          {/if}
        </div>
      </section>
        </div>
      </div>
    </div>

    {#if error}
      <div class="error">{error}</div>
    {/if}

    <div class="actions">
      {#if showBack}
        <button class="secondary" onclick={onBack}>Cancel</button>
      {/if}
      <button onclick={save} disabled={saving}>
        <Save size={16} />
        {saving ? "Saving..." : "Save changes"}
      </button>
    </div>
  {:else}
    <EmptyState icon={Cpu} title="No project selected" description="Open a project to edit its settings." />
  {/if}
</div>

{#if showJavaPicker}
  <JavaPickerModal
    current={javaPath === "Auto-detect" ? "" : javaPath}
    onclose={() => (showJavaPicker = false)}
    onselected={onJavaSelected}
  />
{/if}

<style>
  .settings-page {
    max-width: none;
    width: 100%;
  }

  .page-header {
    display: flex;
    align-items: center;
    gap: 16px;
    margin-bottom: 24px;
  }

  .page-header h1 {
    font-size: 24px;
    font-weight: 800;
  }

  .back {
    padding: 8px 12px;
  }

  .settings-groups {
    display: flex;
    flex-direction: column;
    gap: 28px;
    margin-bottom: 24px;
  }

  .settings-groups.dimmed {
    opacity: 0.5;
    pointer-events: none;
  }

  .section-group {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .section-heading {
    font-size: 13px;
    font-weight: 800;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--text-muted);
    margin: 0;
    padding-bottom: 4px;
    border-bottom: 1px solid var(--border-color);
  }

  .settings-grid {
    display: grid;
    grid-template-columns: repeat(2, 1fr);
    gap: 20px;
  }

  .settings-grid.dimmed {
    opacity: 0.5;
    pointer-events: none;
  }

  .loading {
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--text-muted);
    padding: 14px 16px;
    margin-bottom: 16px;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
  }

  .card {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
    padding: 24px;
  }

  .card.wide {
    grid-column: 1 / -1;
  }

  .card-title {
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--text-secondary);
    margin-bottom: 20px;
  }

  .card-title h3 {
    font-size: 16px;
    color: var(--text-primary);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-bottom: 16px;
  }

  .field:last-child {
    margin-bottom: 0;
  }

  .field-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 12px;
  }

  label {
    font-size: 12px;
    color: var(--text-secondary);
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }

  input,
  select {
    padding: 10px 12px;
    border-radius: var(--border-radius-md);
    border: 1px solid var(--border-color);
    background: var(--bg-elevated);
    color: var(--text-primary);
    font-size: 14px;
  }

  input:focus,
  select:focus {
    outline: none;
    border-color: var(--accent-primary);
  }

  input[readonly] {
    background: var(--bg-tertiary);
    color: var(--text-muted);
  }

  input:disabled {
    opacity: 0.6;
  }

  .input-row {
    display: flex;
    gap: 8px;
  }

  .input-row input {
    flex: 1;
  }

  .icon-btn {
    width: 40px;
    height: 40px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--border-radius-md);
    background: var(--bg-elevated);
    border: 1px solid var(--border-color);
    color: var(--text-secondary);
    cursor: pointer;
  }

  .icon-btn:hover {
    border-color: var(--accent-primary);
    color: var(--accent-primary);
  }

  .java-preview {
    font-size: 12px;
    color: var(--text-muted);
    margin-top: 4px;
  }

  .field-hint {
    font-size: 12px;
    color: var(--text-muted);
    margin: 6px 0 0;
    line-height: 1.4;
  }

  textarea {
    font-family: ui-monospace, monospace;
    background: var(--bg-elevated);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-md);
    padding: 12px;
    color: var(--text-primary);
    font-size: 13px;
    resize: vertical;
    outline: none;
  }

  textarea:focus {
    border-color: var(--accent-primary);
  }

  .memory-control {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .memory-value {
    font-size: 32px;
    font-weight: 900;
    text-align: center;
  }

  .memory-slider {
    -webkit-appearance: none;
    appearance: none;
    width: 100%;
    height: 8px;
    background: var(--bg-elevated);
    border-radius: 4px;
    outline: none;
  }

  .memory-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 22px;
    height: 22px;
    background: var(--accent-primary);
    border-radius: 50%;
    cursor: pointer;
    box-shadow: 0 0 12px color-mix(in srgb, var(--accent-primary) 40%, transparent);
  }

  .memory-slider::-moz-range-thumb {
    width: 22px;
    height: 22px;
    background: var(--accent-primary);
    border-radius: 50%;
    cursor: pointer;
    border: none;
  }

  .memory-marks {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    flex-wrap: wrap;
  }

  .mark {
    padding: 6px 12px;
    background: var(--bg-elevated);
    border: 1px solid var(--border-color);
    color: var(--text-secondary);
    font-size: 12px;
    font-weight: 600;
    border-radius: var(--border-radius-sm);
  }

  .mark.active {
    background: var(--accent-primary);
    color: var(--on-accent, #000);
    border-color: var(--accent-primary);
  }

  .actions {
    display: flex;
    justify-content: flex-end;
    gap: 12px;
  }

  .error {
    background: color-mix(in srgb, var(--accent-danger) 12%, transparent);
    color: var(--accent-danger);
    padding: 10px 12px;
    border-radius: var(--border-radius-md);
    font-size: 13px;
    margin-bottom: 16px;
  }

  .schema-info { display: grid; gap: 12px; }
  .schema-row { display: flex; justify-content: space-between; align-items: center; padding: 10px 14px; background: var(--bg-tertiary); border-radius: var(--border-radius-md); border: 1px solid var(--border-color); }
  .schema-row span { color: var(--text-muted); font-size: 13px; }
  .schema-row code { font-family: ui-monospace, monospace; font-size: 14px; color: var(--accent-primary); }
  .schema-warning { display: flex; align-items: center; gap: 10px; padding: 12px; border-radius: 10px; background: color-mix(in srgb, var(--accent-warning) 8%, transparent); border: 1px solid color-mix(in srgb, var(--accent-warning) 25%, transparent); color: var(--accent-warning); font-size: 13px; }
  .schema-ok { color: var(--accent-primary); font-size: 13px; padding: 10px 14px; background: color-mix(in srgb, var(--accent-primary) 6%, transparent); border-radius: 10px; border: 1px solid color-mix(in srgb, var(--accent-primary) 20%, transparent); }
</style>
