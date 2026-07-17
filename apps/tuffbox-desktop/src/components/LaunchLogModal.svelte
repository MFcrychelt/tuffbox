<script lang="ts">
  import { X, Loader2, RotateCcw, FileText, Folder } from "lucide-svelte";
  import { createEventDispatcher, onMount, onDestroy } from "svelte";
  import { invoke } from "@tauri-apps/api/core";

  const dispatch = createEventDispatcher<{ close: void }>();

  export let projectPath: string;

  let log = "";
  let loading = true;
  let interval: ReturnType<typeof setInterval>;
  let logFiles: { name: string; size: number; modified?: number | null }[] = [];
  let selectedLog = "latest.log";
  let loadingLogList = false;
  let logListOpen = false;

  type SignalKind =
    | "SuspectedMods"
    | "ModFile"
    | "CausedBy"
    | "Mixin"
    | "Exception"
    | "OpenGl"
    | "Performance"
    | "ResourceWarning"
    | "Entrypoint"
    | "LoaderMismatch"
    | "MissingDependency"
    | "ModVersionMismatch"
    | "MinecraftVersionMismatch"
    | "LoaderVersionMismatch"
    | "WrongLoader";

  const KIND_LABEL: Record<SignalKind, string> = {
    SuspectedMods: "Suspected",
    ModFile: "Bad file",
    CausedBy: "Caused by",
    Mixin: "Mixin",
    Exception: "Crash",
    OpenGl: "GPU",
    Performance: "Lag",
    ResourceWarning: "Asset",
    Entrypoint: "Entrypoint",
    LoaderMismatch: "Loader clash",
    MissingDependency: "Missing dependency",
    ModVersionMismatch: "Version conflict",
    MinecraftVersionMismatch: "Wrong MC version",
    LoaderVersionMismatch: "Wrong loader version",
    WrongLoader: "Wrong loader",
  };

  type Highlight = {
    modId: string;
    modName: string;
    confidence: number;
    kind: SignalKind;
    text: string;
  };
  let highlights = new Map<number, Highlight>();
  let suspectCount = 0;

  async function analyzeLog() {
    if (!log) {
      highlights = new Map();
      suspectCount = 0;
      return;
    }
    try {
      const res = (await invoke("analyze_log_text", {
        path: projectPath,
        text: log,
      })) as {
        suspectedMods: { id: string; name: string; confidence: number }[];
        highlights: {
          lineNumber: number;
          modId: string;
          modName: string;
          confidence: number;
          kind: SignalKind;
          text: string;
        }[];
      };
      const map = new Map<number, Highlight>();
      for (const h of res.highlights) {
        map.set(h.lineNumber, {
          modId: h.modId,
          modName: h.modName,
          confidence: h.confidence,
          kind: h.kind,
          text: h.text,
        });
      }
      highlights = map;
      suspectCount = res.suspectedMods?.length ?? 0;
    } catch {
      highlights = new Map();
      suspectCount = 0;
    }
  }

  async function loadLog() {
    try {
      if (selectedLog === "latest.log") {
        const result = await invoke("get_launch_log", { path: projectPath });
        log = result as string;
      } else {
        const result = await invoke("read_instance_log", { path: projectPath, logName: selectedLog });
        log = result as string;
      }
      await analyzeLog();
    } catch (e) {
      log += `\n[error] ${e}`;
    } finally {
      loading = false;
    }
  }

  async function loadLogList() {
    loadingLogList = true;
    try {
      logFiles = await invoke("list_instance_logs", { path: projectPath });
    } catch {
      logFiles = [];
    } finally {
      loadingLogList = false;
    }
  }

  async function switchLog(name: string) {
    selectedLog = name;
    loading = true;
    log = "";
    await loadLog();
  }

  onMount(() => {
    loadLog();
    loadLogList();
    interval = setInterval(loadLog, 1000);
  });

  onDestroy(() => {
    clearInterval(interval);
  });
</script>

<div class="modal-backdrop" on:click={(e) => e.target === e.currentTarget && dispatch("close")} role="button" tabindex="-1" aria-label="Close" on:keydown={(e) => e.key === "Escape" && dispatch("close")}>
  <div class="modal" role="dialog" aria-modal="true">
    <div class="modal-header">
      <div class="modal-header-left">
        <h2>Launch Log</h2>
        <div class="log-selector">
          <button class="log-select-btn" class:active={selectedLog === "latest.log"} on:click={() => switchLog("latest.log")}>
            <FileText size={13} /> latest.log
          </button>
          {#if logFiles.length > 0}
            <button class="log-select-btn toggle" on:click={() => { logListOpen = !logListOpen; if (logListOpen) loadLogList(); }}>
              <Folder size={13} /> {logFiles.length} logs
            </button>
          {/if}
        </div>
        {#if logListOpen}
          <div class="log-dropdown">
            {#each logFiles as f}
              <button class="log-file-row" class:selected={selectedLog === f.name} on:click={() => { switchLog(f.name); logListOpen = false; }}>
                <span>{f.name}</span>
                <small>{f.size < 1024 ? f.size + ' B' : f.size < 1048576 ? (f.size/1024).toFixed(1)+' KB' : (f.size/1048576).toFixed(1)+' MB'}</small>
              </button>
            {/each}
          </div>
        {/if}
      </div>
      <button class="icon-btn" on:click={() => dispatch("close")} aria-label="Close">
        <X size={18} />
      </button>
    </div>

    <div class="modal-body">
      {#if loading && !log}
        <div class="loader">
          <Loader2 size={20} class="spin" />
          Waiting for log...
        </div>
      {/if}
      {#if suspectCount > 0}
        <div class="suspect-banner">
          <strong>{suspectCount}</strong> mod{suspectCount === 1 ? "" : "s"} referenced in this log —
          highlighted below.
        </div>
      {/if}
      {#if log}
        <pre class="log">{#each log.split("\n") as line, i}{@const ln = i + 1}{@const h = highlights.get(ln)}{#if h}<span class="log-hl" title={h.modName + " — " + (KIND_LABEL[h.kind] ?? h.kind) + " (" + h.confidence + "%)"}><span class="log-line-no">{ln}</span><span class="log-badge">{h.modName}</span><span class="log-tag">{KIND_LABEL[h.kind] ?? h.kind}</span>{line}
</span>{:else}<span class="log-line-no">{ln}</span>{line}
{/if}{/each}</pre>
      {:else}
        <pre class="log">Waiting for process output...</pre>
      {/if}
    </div>

    <div class="modal-footer">
      <button class="ghost" on:click={loadLog}>
        <RotateCcw size={16} />
        Refresh
      </button>
      <button class="ghost" on:click={() => dispatch("close")}>Close</button>
    </div>
  </div>
</div>

<style>
  .modal-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
    padding: 24px;
  }

  .modal {
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-xl);
    width: 100%;
    max-width: 800px;
    height: 70vh;
    box-shadow: 0 24px 64px rgba(0, 0, 0, 0.5);
    display: flex;
    flex-direction: column;
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 16px 20px;
    border-bottom: 1px solid var(--border-color);
  }

  .modal-header h2 {
    font-size: 16px;
    font-weight: 800;
  }

  .icon-btn {
    width: 32px;
    height: 32px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--border-radius-md);
    background: transparent;
    border: none;
    color: var(--text-muted);
    cursor: pointer;
  }

  .icon-btn:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }

  .modal-body {
    flex: 1;
    overflow: auto;
    padding: 16px 20px;
    display: flex;
    flex-direction: column;
  }

  .loader {
    display: flex;
    align-items: center;
    gap: 10px;
    color: var(--text-muted);
    margin-bottom: 12px;
  }

  :global(.spin) {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .log {
    flex: 1;
    margin: 0;
    padding: 12px;
    background: #0b0b0d;
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-md);
    color: var(--text-secondary);
    font-family: ui-monospace, monospace;
    font-size: 12px;
    white-space: pre-wrap;
    overflow: auto;
  }

  .log-hl {
    display: block;
    background: rgba(255, 71, 87, 0.14);
    border-left: 3px solid #ff4757;
    color: #ffd9dd;
    padding: 0 6px;
  }

  .log-line-no {
    display: inline-block;
    width: 42px;
    color: var(--text-muted);
    user-select: none;
    text-align: right;
    margin-right: 8px;
  }

  .log-badge {
    display: inline-block;
    margin-right: 8px;
    padding: 0 6px;
    border-radius: 4px;
    background: rgba(255, 71, 87, 0.25);
    color: #ff8a93;
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }

  .log-tag {
    display: inline-block;
    margin-right: 8px;
    padding: 0 6px;
    border-radius: 4px;
    background: rgba(255, 255, 255, 0.08);
    color: #ffc2c8;
    font-size: 9px;
    font-weight: 600;
    letter-spacing: 0.02em;
  }

  .suspect-banner {
    margin-bottom: 10px;
    padding: 8px 12px;
    border-radius: var(--border-radius-md);
    background: rgba(255, 71, 87, 0.12);
    border: 1px solid rgba(255, 71, 87, 0.35);
    color: #ff8a93;
    font-size: 12px;
  }

  .modal-header-left { display: grid; gap: 8px; position: relative; }
  .log-selector { display: flex; gap: 6px; }
  .log-select-btn { display: flex; align-items: center; gap: 6px; padding: 4px 10px; border-radius: 6px; background: var(--bg-tertiary); color: var(--text-muted); border: 1px solid var(--border-color); font-size: 11px; cursor: pointer; }
  .log-select-btn.active { color: var(--accent-primary); border-color: rgba(27,217,106,.35); background: rgba(27,217,106,.08); }
  .log-select-btn:hover { color: var(--text-primary); }
  .log-dropdown { position: absolute; top: 100%; left: 0; z-index: 10; margin-top: 4px; min-width: 220px; max-height: 240px; overflow: auto; background: var(--bg-elevated); border: 1px solid var(--border-color); border-radius: 10px; box-shadow: 0 8px 24px rgba(0,0,0,.4); }
  .log-file-row { width: 100%; display: flex; justify-content: space-between; gap: 8px; padding: 7px 10px; background: transparent; color: var(--text-secondary); border: none; font-size: 12px; cursor: pointer; text-align: left; }
  .log-file-row:hover, .log-file-row.selected { background: var(--bg-hover); color: var(--text-primary); }
  .log-file-row small { color: var(--text-muted); font-size: 11px; }

  .modal-footer {
    display: flex;
    justify-content: flex-end;
    gap: 10px;
    padding: 12px 20px;
    border-top: 1px solid var(--border-color);
  }

  .modal-footer button {
    display: flex;
    align-items: center;
    gap: 8px;
  }
</style>
