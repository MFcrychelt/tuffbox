<script lang="ts">
  import { X, Loader2, RotateCcw, FileText, Folder, Radio } from "lucide-svelte";
  import { createEventDispatcher, onMount, onDestroy, tick } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import { trapFocus } from "../lib/focusTrap";
  import CopyButton from "./CopyButton.svelte";

  const dispatch = createEventDispatcher<{ close: void }>();

  export let projectPath: string;

  let log = "";
  let loading = true;
  let interval: ReturnType<typeof setInterval>;
  let logFiles: { name: string; size: number; modified?: number | null }[] = [];
  /** `__live__` = auto-pick console/latest for the running session */
  let selectedLog = "__live__";
  let loadingLogList = false;
  let logListOpen = false;
  let followTail = true;
  let logPreEl: HTMLPreElement | null = null;
  let userScrolledUp = false;

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

  async function scrollToBottomIfFollowing() {
    if (!followTail || userScrolledUp || !logPreEl) return;
    await tick();
    logPreEl.scrollTop = logPreEl.scrollHeight;
  }

  function onLogScroll() {
    if (!logPreEl) return;
    const dist = logPreEl.scrollHeight - logPreEl.scrollTop - logPreEl.clientHeight;
    userScrolledUp = dist > 48;
    if (!userScrolledUp) followTail = true;
  }

  async function loadLog() {
    try {
      if (selectedLog === "__live__" || selectedLog === "latest.log") {
        const result = await invoke("get_launch_log", { path: projectPath });
        log = result as string;
      } else {
        const result = await invoke("read_instance_log", {
          path: projectPath,
          logName: selectedLog,
        });
        log = result as string;
      }
      // Analyze less often on live poll — only when idle selection or every ~4s via length change.
      if (selectedLog !== "__live__" || log.length < 80_000) {
        await analyzeLog();
      }
      await scrollToBottomIfFollowing();
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
    userScrolledUp = false;
    followTail = true;
    await loadLog();
  }

  onMount(() => {
    loadLog();
    loadLogList();
    interval = setInterval(loadLog, 750);
  });

  onDestroy(() => {
    clearInterval(interval);
  });
</script>

<div
  class="modal-backdrop"
  on:click={(e) => e.target === e.currentTarget && dispatch("close")}
  role="button"
  tabindex="-1"
  aria-label="Close"
  on:keydown={() => {}}
>
  <div
    class="modal"
    role="dialog"
    aria-modal="true"
    use:trapFocus={{ onEscape: () => dispatch("close") }}
  >
    <div class="modal-header">
      <div class="modal-header-left">
        <h2>
          <Radio size={16} class="live-icon" />
          Live logs
        </h2>
        <div class="log-selector">
          <button
            class="log-select-btn"
            class:active={selectedLog === "__live__"}
            on:click={() => switchLog("__live__")}
            title="JVM console + latest.log (auto)"
          >
            <Radio size={13} /> Live
          </button>
          <button
            class="log-select-btn"
            class:active={selectedLog === "latest.log"}
            on:click={() => switchLog("latest.log")}
          >
            <FileText size={13} /> latest.log
          </button>
          <button
            class="log-select-btn"
            class:active={selectedLog === "tuffbox-console.log"}
            on:click={() => switchLog("tuffbox-console.log")}
          >
            <FileText size={13} /> console
          </button>
          {#if logFiles.length > 0}
            <button
              class="log-select-btn toggle"
              on:click={() => {
                logListOpen = !logListOpen;
                if (logListOpen) loadLogList();
              }}
            >
              <Folder size={13} /> {logFiles.length} logs
            </button>
          {/if}
        </div>
        {#if logListOpen}
          <div class="log-dropdown">
            {#each logFiles as f}
              <button
                class="log-file-row"
                class:selected={selectedLog === f.name}
                on:click={() => {
                  switchLog(f.name);
                  logListOpen = false;
                }}
              >
                <span>{f.name}</span>
                <small
                  >{f.size < 1024
                    ? f.size + " B"
                    : f.size < 1048576
                      ? (f.size / 1024).toFixed(1) + " KB"
                      : (f.size / 1048576).toFixed(1) + " MB"}</small
                >
              </button>
            {/each}
          </div>
        {/if}
      </div>
      <div class="modal-header-right">
        <label class="follow-toggle" title="Keep scrolling to the newest lines">
          <input
            type="checkbox"
            bind:checked={followTail}
            on:change={() => {
              if (followTail) {
                userScrolledUp = false;
                void scrollToBottomIfFollowing();
              }
            }}
          />
          Follow
        </label>
        {#if log}
          <CopyButton text={log} label="Copy log" />
        {/if}
        <button class="icon-btn" on:click={() => dispatch("close")} aria-label="Close">
          <X size={18} />
        </button>
      </div>
    </div>

    <div class="modal-body">
      {#if loading && !log}
        <div class="loader">
          <Loader2 size={20} class="spin" />
          Waiting for process output…
        </div>
      {/if}
      {#if suspectCount > 0 && selectedLog !== "__live__"}
        <div class="suspect-banner">
          <strong>{suspectCount}</strong> mod{suspectCount === 1 ? "" : "s"} referenced in this log —
          highlighted below.
        </div>
      {/if}
      {#if log}
        <pre class="log" bind:this={logPreEl} on:scroll={onLogScroll}
          >{#each log.split("\n") as line, i}{@const ln = i + 1}{@const h = highlights.get(ln)}{#if h}<span
              class="log-hl"
              title={h.modName +
                " — " +
                (KIND_LABEL[h.kind] ?? h.kind) +
                " (" +
                h.confidence +
                "%)"}
              ><span class="log-line-no">{ln}</span><span class="log-badge">{h.modName}</span
              ><span class="log-tag">{KIND_LABEL[h.kind] ?? h.kind}</span>{line}
</span
            >{:else}<span class="log-line-no">{ln}</span>{line}
{/if}{/each}</pre
        >
      {:else}
        <pre class="log">Waiting for process output…</pre>
      {/if}
    </div>

    <div class="modal-footer">
      <span class="live-hint"
        >{selectedLog === "__live__"
          ? "Streaming tuffbox-console.log / latest.log · refreshes ~0.75s"
          : `Showing ${selectedLog}`}</span
      >
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
    width: min(960px, 100%);
    max-height: min(85vh, 820px);
    background: var(--bg-elevated, #1a1f28);
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 12px;
    display: flex;
    flex-direction: column;
    box-shadow: 0 24px 64px rgba(0, 0, 0, 0.45);
  }

  .modal-header {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    padding: 14px 16px;
    border-bottom: 1px solid rgba(255, 255, 255, 0.06);
    position: relative;
  }

  .modal-header-left {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }

  .modal-header-left h2 {
    margin: 0;
    font-size: 16px;
    display: flex;
    align-items: center;
    gap: 8px;
  }

  :global(.live-icon) {
    color: #1bd96a;
  }

  .modal-header-right {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-shrink: 0;
  }

  .follow-toggle {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    color: var(--text-muted, #9aa4b2);
    cursor: pointer;
    user-select: none;
  }

  .log-selector {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .log-select-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 4px 10px;
    border-radius: 6px;
    border: 1px solid rgba(255, 255, 255, 0.08);
    background: transparent;
    color: var(--text-muted, #9aa4b2);
    font-size: 12px;
    cursor: pointer;
  }

  .log-select-btn.active {
    background: rgba(27, 217, 106, 0.12);
    border-color: rgba(27, 217, 106, 0.35);
    color: #fff;
  }

  .log-dropdown {
    position: absolute;
    top: 100%;
    left: 16px;
    z-index: 5;
    max-height: 220px;
    overflow: auto;
    background: var(--bg-elevated, #1a1f28);
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    min-width: 260px;
    box-shadow: 0 12px 28px rgba(0, 0, 0, 0.4);
  }

  .log-file-row {
    display: flex;
    justify-content: space-between;
    width: 100%;
    padding: 8px 12px;
    border: none;
    background: transparent;
    color: inherit;
    cursor: pointer;
    font-size: 12px;
  }

  .log-file-row:hover,
  .log-file-row.selected {
    background: rgba(255, 255, 255, 0.06);
  }

  .icon-btn {
    border: none;
    background: transparent;
    color: var(--text-muted, #9aa4b2);
    cursor: pointer;
    padding: 4px;
    border-radius: 6px;
  }

  .icon-btn:hover {
    background: rgba(255, 255, 255, 0.06);
    color: #fff;
  }

  .modal-body {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    padding: 0;
  }

  .loader {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 16px;
    color: var(--text-muted, #9aa4b2);
    font-size: 13px;
  }

  :global(.spin) {
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .suspect-banner {
    padding: 8px 14px;
    font-size: 12px;
    background: rgba(255, 180, 60, 0.1);
    border-bottom: 1px solid rgba(255, 180, 60, 0.2);
  }

  .log {
    flex: 1;
    margin: 0;
    padding: 12px 14px;
    overflow: auto;
    font-family: var(--font-mono, ui-monospace, SFMono-Regular, Menlo, Consolas, monospace);
    font-size: 11.5px;
    line-height: 1.45;
    white-space: pre-wrap;
    word-break: break-word;
    color: #d5dbe6;
    background: #0e1218;
    min-height: 360px;
  }

  .log-line-no {
    display: inline-block;
    min-width: 36px;
    margin-right: 8px;
    color: #5c6778;
    user-select: none;
  }

  .log-hl {
    display: block;
    background: rgba(255, 120, 80, 0.12);
  }

  .log-badge,
  .log-tag {
    display: inline-block;
    margin-right: 6px;
    padding: 0 5px;
    border-radius: 4px;
    font-size: 10px;
    background: rgba(255, 255, 255, 0.08);
  }

  .modal-footer {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 14px;
    border-top: 1px solid rgba(255, 255, 255, 0.06);
  }

  .live-hint {
    flex: 1;
    font-size: 11px;
    color: var(--text-muted, #9aa4b2);
  }

  .ghost {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border-radius: 6px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    background: transparent;
    color: inherit;
    cursor: pointer;
    font-size: 13px;
  }

  .ghost:hover {
    background: rgba(255, 255, 255, 0.05);
  }
</style>
