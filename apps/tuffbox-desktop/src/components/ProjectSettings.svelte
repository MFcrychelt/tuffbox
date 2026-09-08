<script lang="ts">
  import { ArrowLeft, Save, Cpu, Container, Coffee, Terminal, Search, Database, RefreshCw, AlertTriangle, FileCog, User } from "@lucide/svelte";
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

  // ── Shared options.txt sync (docs/17) ─────────────────────────────────
  let optionsManaged = $state(false);
  let optionsGroup = $state("");
  let optionsHasTemplate = $state(false);
  let optionsBusy = $state(false);

  async function loadOptionsStatus() {
    if (!$projectPath) return;
    try {
      const s: any = await invoke("options_sync_status", { path: $projectPath });
      optionsManaged = s.managed ?? false;
      optionsGroup = s.groupId ?? "";
      optionsHasTemplate = s.hasGroupTemplate ?? false;
    } catch {
      /* status stays at defaults */
    }
  }

  async function enableOptionsSync() {
    if (!$projectPath) return;
    optionsBusy = true;
    error = "";
    try {
      const imported: boolean = await invoke("options_sync_enable", { path: $projectPath });
      if (!imported && !optionsHasTemplate) {
        error = "Enabled, but this version group has no template yet — current options.txt will be adopted on first launch.";
      }
      await loadOptionsStatus();
    } catch (e) {
      error = `${e}`;
    } finally {
      optionsBusy = false;
    }
  }

  async function disableOptionsSync() {
    if (!$projectPath) return;
    optionsBusy = true;
    error = "";
    try {
      await invoke("options_sync_disable", { path: $projectPath });
      await loadOptionsStatus();
    } catch (e) {
      error = `${e}`;
    } finally {
      optionsBusy = false;
    }
  }

  async function pushOptionsToGroup() {
    if (!$projectPath) return;
    optionsBusy = true;
    error = "";
    try {
      await invoke("options_sync_push", { path: $projectPath });
      await loadOptionsStatus();
    } catch (e) {
      error = `${e}`;
    } finally {
      optionsBusy = false;
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
    // Independent fetches start in parallel, but the form is usable as soon
    // as the current project's settings are known — the dropdown options
    // (MC versions / loader versions) stream in afterwards without blocking
    // the page behind a dimmed overlay.
    const localFill = (async () => {
      try {
        const info = await invoke("validate_project", { path: $projectPath });
        applyProjectInfo(info as any);
      } catch {
        /* keep the seeded $projectInfo defaults */
      }
    })();
    void detectJavaPreview();
    void loadSchemaStatus();
    void loadOptionsStatus();
    void (async () => {
      try {
        const versions = (await invoke("get_minecraft_versions")) as {
          id: string;
          popular: boolean;
        }[];
        mcVersions = versions;
        // MC list arrived: loader versions can now resolve against the
        // selected Minecraft version.
        await loadLoaderVersions();
      } catch (e) {
        error = `${e}`;
      } finally {
        loading = false;
      }
    })();
    await localFill;
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

  /** Seed the form fields from a validated project manifest. */
  function applyProjectInfo(info: {
    minecraftVersion?: string;
    loaderKind?: string;
    loaderVersion?: string;
    memoryMb?: number;
    jvmArgs?: string[];
    javaPath?: string | null;
    playerName?: string | null;
  }) {
    if (info.minecraftVersion) mcVersion = info.minecraftVersion;
    if (info.loaderKind) loader = info.loaderKind;
    if (info.loaderVersion) loaderVersion = info.loaderVersion;
    if (info.memoryMb) memory = info.memoryMb;
    if (info.jvmArgs?.length) jvmArgs = info.jvmArgs.join(" ");
    if (info.javaPath) javaPath = info.javaPath;
    if (info.playerName) playerName = info.playerName;
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
      applyProjectInfo(info as any);
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

<div class="settings-page w-full">
  <header class="page-header flex items-center justify-between gap-4 mb-6">
    <div class="flex items-center gap-3 min-w-0">
      {#if showBack}
        <button class="ghost back flex items-center gap-2 px-3 h-10 shrink-0" onclick={onBack}>
          <ArrowLeft size={18} />
          Back
        </button>
      {/if}
      <div class="min-w-0">
        <h1 class="text-xl font-bold text-[color:var(--text-primary)] leading-tight truncate">Instance Settings</h1>
                <p class="text-xs text-[color:var(--text-muted)] mt-0.5 truncate">Runtime, memory, Java and project preferences for the active instance.</p>
      </div>
    </div>
  </header>

  {#if $projectPath}
    {#if loading}
      <div class="loading flex items-center gap-2.5 px-4 h-11 mb-4 rounded-2xl bg-[color:var(--bg-secondary)] border border-[color:var(--border-color)] backdrop-blur-xl">
              <RefreshCw size={16} class="spin" />
              <span class="text-sm text-[color:var(--text-secondary)]">Loading instance settings…</span>
            </div>
    {/if}
    <div class="settings-groups flex flex-col gap-7 mb-6" class:dimmed={loading}>
      <!-- ── Runtime ─────────────────────────────────────────────── -->
      <section>
        <h2 class="section-label mb-3">Runtime</h2>
        <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
          <article class="glass-card rounded-2xl p-5 flex flex-col gap-4">
            <h3 class="card-head"><Container size={16} /> Game</h3>
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
            <div class="grid grid-cols-2 gap-3">
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
          </article>

          <article class="glass-card rounded-2xl p-5 flex flex-col gap-4">
            <h3 class="card-head"><Coffee size={16} /> Java</h3>
            <div class="field">
              <label for="java-path">Java executable</label>
              <div class="flex gap-2">
                <input id="java-path" bind:value={javaPath} readonly class="flex-1 min-w-0" />
                <button class="icon-btn" onclick={() => (showJavaPicker = true)} aria-label="Search Java">
                  <Search size={16} />
                </button>
              </div>
              {#if javaVersion}
                <p class="java-preview">{javaVersion}</p>
              {/if}
            </div>
          </article>

          <article class="glass-card rounded-2xl p-5 md:col-span-2">
            <div class="grid grid-cols-1 lg:grid-cols-2 gap-6">
              <div class="memory-block flex flex-col justify-center">
                <div class="mb-4 flex items-center justify-between gap-3">
                  <h3 class="card-head"><Cpu size={16} /> Memory</h3>
                  <span class="memory-value">{formatMemory(memory)}</span>
                </div>
                <input
                  type="range"
                  min={1024}
                  max={16384}
                  step={256}
                  bind:value={memory}
                  class="range-slider w-full accent-[color:var(--accent-primary)]"
                  aria-label="Memory allocation"
                />
                <div class="mt-2.5 flex justify-between text-xs text-[color:var(--text-muted)]">
                  <span>{formatMemory(1024)}</span>
                  <span>{formatMemory(16384)}</span>
                </div>
              </div>

              <div class="jvm-block flex flex-col">
                <h3 class="card-head mb-3"><Terminal size={16} /> JVM Arguments</h3>
                <div class="field flex-1">
                  <textarea
                    bind:value={jvmArgs}
                    rows={5}
                    wrap="off"
                    placeholder="-XX:+UseG1GC"
                    spellcheck="false"
                  ></textarea>
                </div>
              </div>
            </div>
          </article>
        </div>
      </section>

      <!-- ── Project ─────────────────────────────────────────────── -->
      <section>
        <h2 class="section-label mb-3">Project</h2>
        <div class="grid grid-cols-1 md:grid-cols-2 gap-4">
          <article class="glass-card rounded-2xl p-5 flex flex-col gap-4">
            <h3 class="card-head"><User size={16} /> Player</h3>
            <div class="field">
              <label for="player-name">Player name (offline test launches)</label>
              <input id="player-name" bind:value={playerName} placeholder="Player" maxlength="16" />
              <p class="field-hint">
                Used for test runs. TuffBox derives a stable offline UUID from this name
                (same algorithm vanilla uses), so the same name always maps to the same
                in-game identity across launches.
              </p>
            </div>
          </article>

          <article class="glass-card rounded-2xl p-5 flex flex-col gap-3">
            <h3 class="card-head"><Database size={16} /> Project schema</h3>
            <div class="schema-info">
              <div class="schema-row">
                <span>Schema version</span>
                <code>{schemaVersion || "..."}</code>
              </div>
              {#if schemaNeedsMigration}
                <div class="schema-warning">
                  <AlertTriangle size={15} />
                  <span>Schema migration available. This will normalize your manifest to the current format.</span>
                </div>
                <button class="secondary self-start" onclick={migrateSchema} disabled={saving}>
                  <RefreshCw size={15} />
                  {saving ? "Migrating..." : "Migrate schema"}
                </button>
              {:else if schemaVersion}
                <div class="schema-ok">✓ Schema is up to date</div>
              {/if}
            </div>
          </article>

          <article class="glass-card rounded-2xl p-5 md:col-span-2 flex flex-col gap-3">
            <h3 class="card-head"><FileCog size={16} /> Shared options.txt</h3>
            <div class="schema-info">
              <div class="schema-row">
                <span>Status</span>
                <code>{optionsManaged ? `Shared (${optionsGroup})` : "Independent"}</code>
              </div>
              <p class="field-hint">
                Shared projects of the same Minecraft version keep one options.txt:
                your in-game settings sync between them. Edits you make in-game always
                win; backups are created before any automatic change.
              </p>
              <div class="flex flex-wrap gap-3">
                {#if optionsManaged}
                  <button class="secondary" onclick={disableOptionsSync} disabled={optionsBusy}>
                    {optionsBusy ? "Working..." : "Use independent options"}
                  </button>
                  <button class="secondary" onclick={pushOptionsToGroup} disabled={optionsBusy}>
                    Push current to group
                  </button>
                {:else}
                  <button class="secondary" onclick={enableOptionsSync} disabled={optionsBusy}>
                    {optionsBusy ? "Working..." : "Share options across projects"}
                  </button>
                {/if}
              </div>
            </div>
          </article>
        </div>
      </section>
    </div>

    {#if error}
      <div class="error bg-[rgba(239,68,68,0.12)] text-[#ef4444] px-3 py-2.5 rounded-xl text-[13px] mb-4">{error}</div>
    {/if}

    <div class="actions flex justify-end gap-3">
      {#if showBack}
        <button class="secondary" onclick={onBack}>Cancel</button>
      {/if}
      <button class="save-btn" onclick={save} disabled={saving}>
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
  /* .settings-page layout moved to Tailwind utilities */

  /* While dropdown options stream in, the form stays visible and clickable —
     only a light veil hints that lists are still loading. No full lock-out:
     blocking the whole panel behind pointer-events:none was why Setup felt
     "stuck" until every network fetch resolved. */
  .settings-groups.dimmed {
    opacity: 0.85;
    transition: opacity var(--motion-fast) var(--ease-out);
  }

  /* Glass morphism shell for every card on this page. Radius is applied from
     Tailwind (rounded-2xl) so it stays token-aware for the CI border-radius gate. */
  .glass-card {
      background: color-mix(in srgb, var(--bg-secondary) 55%, transparent);
      -webkit-backdrop-filter: blur(22px) saturate(150%);
      backdrop-filter: blur(22px) saturate(150%);
      border: 1px solid var(--border-color);
      box-shadow:
        inset 0 1px 0 color-mix(in srgb, var(--text-muted) 10%, transparent),
        0 14px 44px rgba(3, 6, 10, 0.18);
    }

    .section-label {
      font-size: 13px;
      font-weight: 600;
      color: var(--text-secondary);
      letter-spacing: 0.01em;
    }

    .card-head {
      display: flex;
      align-items: center;
      gap: 8px;
      font-size: 14px;
      font-weight: 600;
      color: var(--text-primary);
    }

    /* Delicate emerald accent for card icons — matches the memory value glow. */
    .card-head :global(svg) {
      color: var(--accent-primary);
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

  label {
      font-size: 13px;
      color: var(--text-secondary);
      font-weight: 500;
      text-transform: none;
      letter-spacing: 0;
    }

  input,
  select {
    padding: 10px 12px;
    border-radius: var(--border-radius-md);
    border: 1px solid var(--border-color);
    background: var(--bg-elevated);
    color: var(--text-primary);
    font-size: 14px;
    transition: border-color var(--motion-fast) ease;
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

  .icon-btn {
    width: 40px;
    height: 40px;
    flex-shrink: 0;
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
      line-height: 1.45;
    }

  textarea {
    font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
    font-size: 12px;
    line-height: 1.6;
    background: var(--bg-elevated);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-md);
    padding: 12px;
    color: var(--text-primary);
    resize: vertical;
    outline: none;
    /* Never wrap long JVM flags onto broken lines — scroll horizontally. */
    white-space: pre;
    overflow-x: auto;
    overflow-wrap: normal;
    word-break: normal;
    min-height: 96px;
    flex: 1;
  }

  textarea:focus {
    border-color: var(--accent-primary);
  }

  .memory-value {
      font-size: 20px;
      font-weight: 700;
      font-variant-numeric: tabular-nums;
      color: var(--accent-primary);
      text-shadow: 0 0 18px color-mix(in srgb, var(--accent-primary) 35%, transparent);
    }

  .range-slider {
    height: 24px;
    cursor: pointer;
  }

  .save-btn {
      box-shadow:
        0 0 14px color-mix(in srgb, var(--accent-primary) 35%, transparent),
        inset 0 1px 0 rgba(255, 255, 255, 0.2);
    }

  .schema-info { display: grid; gap: 12px; }
  .schema-row { display: flex; justify-content: space-between; align-items: center; padding: 10px 14px; background: var(--bg-tertiary); border-radius: var(--border-radius-md); border: 1px solid var(--border-color); }
  .schema-row span { color: var(--text-muted); font-size: 13px; }
  .schema-row code { font-family: ui-monospace, monospace; font-size: 14px; color: var(--accent-primary); }
  .schema-warning { display: flex; align-items: center; gap: 10px; padding: 12px; border-radius: 10px; background: color-mix(in srgb, var(--accent-warning) 8%, transparent); border: 1px solid color-mix(in srgb, var(--accent-warning) 25%, transparent); color: var(--accent-warning); font-size: 13px; }
  .schema-ok { color: var(--accent-primary); font-size: 13px; padding: 10px 14px; background: color-mix(in srgb, var(--accent-primary) 6%, transparent); border-radius: 10px; border: 1px solid color-mix(in srgb, var(--accent-primary) 20%, transparent); }
</style>