<script lang="ts">
  import {
    RefreshCw,
    Shield,
    Camera,
    XCircle,
    FolderOpen,
  } from "@lucide/svelte";

  type Profile = { id: string; name: string; side: string; memoryMb?: number | null; jvmArgs: string[] };
  type ValidationBadge = { ok: boolean; label: string } | null;

  let {
    projectPath = "",
    profiles = $bindable<Profile[]>([]),
    selectedProfile = $bindable("client"),
    loading = false,
    running = false,
    matrixRunning = false,
    validationLoading = false,
    validationError = $bindable<string | null>(null),
    validationReport = $bindable<any>(null),
    validationBadge = $bindable<ValidationBadge>(null),
    forceRun = $bindable(false),
    autoSnapshot = $bindable(false),
    timeoutSeconds = $bindable(180),
    worlds = $bindable<{ name: string }[]>([]),
    quickPlayWorld = $bindable(""),
    serverDir = $bindable(""),
    levelSeed = $bindable(""),
    onlineModeOff = $bindable(true),
    startupSeconds = $bindable<number | null>(null),
    livePhase = $bindable("idle"),
    formatRam = (mb?: number | null): string => {
      const v = mb ?? 4096;
      return v >= 1024 ? `${(v / 1024).toFixed(v % 1024 ? 1 : 0)} GB` : `${v} MB`;
    },
    onrefresh,
    onvalidate,
    onquickplay,
    onbrowseserver,
    ondefaultserver,
    onwriteserverprops,
  }: {
    projectPath?: string;
    profiles?: Profile[];
    selectedProfile?: string;
    loading?: boolean;
    running?: boolean;
    matrixRunning?: boolean;
    validationLoading?: boolean;
    validationError?: string | null;
    validationReport?: any;
    validationBadge?: ValidationBadge;
    forceRun?: boolean;
    autoSnapshot?: boolean;
    timeoutSeconds?: number;
    worlds?: { name: string }[];
    quickPlayWorld?: string;
    serverDir?: string;
    levelSeed?: string;
    onlineModeOff?: boolean;
    startupSeconds?: number | null;
    livePhase?: string;
    formatRam?: (mb?: number | null) => string;
    onrefresh?: () => void;
    onvalidate?: () => void;
    onquickplay?: () => void;
    onbrowseserver?: () => void;
    ondefaultserver?: () => void;
    onwriteserverprops?: () => void;
  } = $props();

  const selected = $derived(profiles.find((p) => p.id === selectedProfile) ?? null);
</script>

<div class="grid gap-4">
  <div class="opt-card">
    <span class="panel-section-title">Diagnostics & validation</span>
    <div class="flex flex-wrap items-center gap-3 text-[color:var(--text-muted)] text-[12px]">
      <button class="ghost" onclick={onrefresh} disabled={!projectPath || loading}>
        <RefreshCw size={15} class={loading ? "spin" : ""} />
        Refresh
      </button>
      <button class="ghost" onclick={onvalidate} disabled={!projectPath || validationLoading}>
        <Shield size={15} />
        {validationLoading ? "Checking…" : "Validate pack"}
      </button>
      {#if validationBadge}
        <span class="text-[11px] font-bold px-2 py-1 rounded-full border {
          validationBadge.ok
            ? "text-emerald-400 border-emerald-500/35 bg-emerald-500/10"
            : "text-[#fca5a5] border-[rgba(239,68,68,0.35)] bg-[rgba(239,68,68,0.08)]"
        }">
          {validationBadge.label}
        </span>
      {/if}
    </div>
    <div class="flex flex-wrap items-center gap-4 mt-3 pt-3 border-t border-[color:var(--border-color)]">
      <label class="inline-flex items-center gap-1.5 cursor-pointer text-[color:var(--text-secondary)] text-[12px]" title="Allow launch even when validation has errors">
        <input type="checkbox" class="w-auto accent-emerald-500" bind:checked={forceRun} /> Force run
      </label>
      <label class="inline-flex items-center gap-1.5 cursor-pointer text-[color:var(--text-secondary)] text-[12px]" title="Create a snapshot before smoke / dry run">
        <input type="checkbox" class="w-auto accent-emerald-500" bind:checked={autoSnapshot} />
        <Camera size={13} /> Auto-snapshot
      </label>
      <label class="inline-flex items-center gap-1.5 cursor-pointer text-[color:var(--text-secondary)] text-[12px]">
        Timeout
        <input type="number" min="30" max="900" class="w-[72px]" bind:value={timeoutSeconds} />
        s
      </label>
      {#if selected}
        <span class="text-[color:var(--text-muted)]">
          {selected.name} · {formatRam(selected.memoryMb)}
        </span>
      {/if}
      {#if startupSeconds != null && livePhase === "pass"}
        <span class="text-emerald-400 font-bold">Startup {startupSeconds}s</span>
      {/if}
    </div>
  </div>

  <div class="opt-card">
    <span class="panel-section-title">Quick play</span>
    <div class="flex flex-wrap items-end gap-3">
      <label class="field flex-1 min-w-[200px]">
        <span class="field-label">World</span>
        <select class="min-w-[200px]" bind:value={quickPlayWorld}>
          {#if worlds.length === 0}
            <option value="">No worlds in saves/</option>
          {:else}
            {#each worlds as w (w.name)}
              <option value={w.name}>{w.name}</option>
            {/each}
          {/if}
        </select>
      </label>
      <button class="ghost" onclick={onquickplay} disabled={running || matrixRunning || !quickPlayWorld}>
        Launch Quick Play
      </button>
    </div>
  </div>

  <div class="opt-card">
    <span class="panel-section-title">Server staging</span>
    <div class="flex flex-wrap items-end gap-3">
      <label class="field flex-1 min-w-[220px]">
        <span class="field-label">Server folder</span>
        <div class="relative">
          <input type="text" class="path-input" placeholder="Where the server instance will be staged" bind:value={serverDir} />
          <FolderOpen size={13} class="path-ico" />
        </div>
      </label>
      <button class="ghost" onclick={onbrowseserver} disabled={!projectPath}>
        <FolderOpen size={14} /> Browse…
      </button>
      <button class="ghost" onclick={ondefaultserver} disabled={!projectPath}>
        Default
      </button>
    </div>
    <div class="flex flex-wrap items-end gap-3 mt-2">
      <label class="field">
        <span class="field-label">level-seed</span>
        <input type="text" class="min-w-[140px] font-mono text-xs" placeholder="optional" bind:value={levelSeed} />
      </label>
      <label class="switch" title="Disable online-mode in the generated server.properties">
        <input type="checkbox" bind:checked={onlineModeOff} />
        <span class="switch-track"></span>
        <span class="switch-body">
          <span class="switch-label">online-mode=false</span>
          <span class="switch-desc">Lets players join without Mojang auth</span>
        </span>
      </label>
      <button class="ghost" onclick={onwriteserverprops}>Write server.properties</button>
    </div>
  </div>

  {#if validationReport && !validationReport.passed}
    <div class="bg-[var(--bg-tertiary)] border border-[var(--border-color)] rounded-[var(--border-radius-lg)] px-3 py-3">
      <div class="flex items-center justify-between mb-2.5 gap-2">
        <h3 class="flex items-center gap-2 text-[14px] text-[var(--text-primary)] m-0"><Shield size={16} /> Validation</h3>
        <span class="flex items-center gap-1.5 text-[#fca5a5] font-bold text-[12px]"><XCircle size={14} /> Issues — use Force run to launch</span>
      </div>
      <div class="grid grid-cols-3 gap-2 mb-2">
        <div class="rounded-[var(--border-radius-md)] border px-2 py-2 grid gap-0.5 text-center {
          validationReport.graphErrors > 0
            ? "border-[rgba(239,68,68,0.35)] bg-[rgba(239,68,68,0.06)]"
            : "border-[var(--border-color)] bg-[var(--bg-tertiary)]"
        }">
          <strong class="text-[18px] { validationReport.graphErrors > 0 ? "text-[#fca5a5]" : "text-[var(--text-primary)]" }">{ validationReport.graphErrors }</strong>
          <span class="text-[11px] text-[var(--text-muted)]">graph</span>
        </div>
        <div class="rounded-[var(--border-radius-md)] border px-2 py-2 grid gap-0.5 text-center {
          (validationReport.jsonErrors?.length ?? 0) > 0
            ? "border-[rgba(239,68,68,0.35)] bg-[rgba(239,68,68,0.06)]"
            : "border-[var(--border-color)] bg-[var(--bg-tertiary)]"
        }">
          <strong class="text-[18px] { (validationReport.jsonErrors?.length ?? 0) > 0 ? "text-[#fca5a5]" : "text-[var(--text-primary)]" }">{ validationReport.jsonErrors?.length ?? 0 }</strong>
          <span class="text-[11px] text-[var(--text-muted)]">JSON</span>
        </div>
        <div class="rounded-[var(--border-radius-md)] border px-2 py-2 grid gap-0.5 text-center {
          (validationReport.circularDeps?.length ?? 0) > 0
            ? "border-[rgba(239,68,68,0.35)] bg-[rgba(239,68,68,0.06)]"
            : "border-[var(--border-color)] bg-[var(--bg-tertiary)]"
        }">
          <strong class="text-[18px] { (validationReport.circularDeps?.length ?? 0) > 0 ? "text-[#fca5a5]" : "text-[var(--text-primary)]" }">{ validationReport.circularDeps?.length ?? 0 }</strong>
          <span class="text-[11px] text-[var(--text-muted)]">cycles</span>
        </div>
      </div>
      <button class="ghost" onclick={() => (validationReport = null)}>Hide</button>
    </div>
  {/if}
</div>

<style>
  .opt-card {
    display: grid;
    gap: 10px;
    padding: 12px 14px;
    border-radius: var(--border-radius-lg);
    background: color-mix(in srgb, var(--bg-secondary) 40%, transparent);
    border: 1px solid var(--border-color);
  }
  .panel-section-title {
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--text-primary);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 5px;
    min-width: 0;
  }
  .field-label {
    font-size: 12px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--text-secondary);
  }
  .field select,
  .field input[type="text"] {
    width: 100%;
    min-height: 36px;
    padding: 7px 30px 7px 12px;
    font-size: 13px;
    line-height: 1.2;
    appearance: none;
    background-image: linear-gradient(45deg, transparent 50%, var(--text-muted) 50%),
      linear-gradient(135deg, var(--text-muted) 50%, transparent 50%);
    background-position: calc(100% - 16px) 55%, calc(100% - 10px) 55%;
    background-size: 5px 5px;
    background-repeat: no-repeat;
  }
  .field select:focus,
  .field input:focus {
    border-color: var(--accent-primary);
  }
  :global(.spin) { animation: spin 900ms linear infinite; }
  @keyframes spin { to { transform: rotate(360deg); } }

  .path-input {
    width: 100%;
    min-height: 30px;
    padding: 5px 10px 5px 30px;
    font-family: ui-monospace, monospace;
    font-size: 12px;
    color: var(--text-secondary);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    border-radius: var(--border-radius-sm);
    border: 1px solid var(--border-color);
    background: var(--bg-elevated);
  }
  :global(.path-ico) {
    position: absolute;
    left: 9px;
    top: 50%;
    transform: translateY(-50%);
    color: var(--text-muted);
    pointer-events: none;
  }

  .switch {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    cursor: pointer;
    padding: 6px 0;
  }
  .switch input {
    position: absolute;
    opacity: 0;
    width: 0;
    height: 0;
  }
  .switch-track {
    width: 38px;
    height: 22px;
    border-radius: 999px;
    background: var(--bg-tertiary);
    border: 1px solid var(--border-color);
    position: relative;
    flex-shrink: 0;
    transition: background var(--motion-fast, 160ms) ease, border-color var(--motion-fast, 160ms) ease;
  }
  .switch-track::after {
    content: "";
    position: absolute;
    top: 2px;
    left: 2px;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: #9ca3af;
    transition: transform var(--motion-fast, 160ms) ease, background var(--motion-fast, 160ms) ease;
  }
  .switch input:checked + .switch-track {
    background: rgba(16, 185, 129, 0.35);
    border-color: rgba(16, 185, 129, 0.5);
  }
  .switch input:checked + .switch-track::after {
    transform: translateX(16px);
    background: #a7f3d0;
  }
  .switch-body {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .switch-label {
    font-size: 12px;
    font-weight: 600;
    color: #d1d5db;
    font-family: ui-monospace, monospace;
  }
  .switch-desc {
    font-size: 11px;
    color: var(--text-muted);
  }
</style>