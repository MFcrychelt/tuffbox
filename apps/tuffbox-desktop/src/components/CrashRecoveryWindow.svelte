<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { AlertTriangle, CheckCircle2, Loader2, PackageOpen, Play, X } from "@lucide/svelte";
  import { retryLastLaunch } from "../lib/launch";

  type FixAction = { kind: string; label: string; modId?: string | null };
  type SuspectedMod = {
    id: string;
    name: string;
    version?: string | null;
    confidence?: number;
    blameRole?: string;
    evidence?: { text?: string }[];
  };
  type DiagnosisHint = {
    relatedMods?: string[];
    fix?: FixAction | null;
    fixes?: FixAction[];
  };

  let { path, message, onclose }: { path: string; message: string; onclose: () => void } = $props();
  let mods = $state<SuspectedMod[]>([]);
  let suggestedActions = $state<Record<string, FixAction>>({});
  let loading = $state(true);
  let fixing = $state<string | null>(null);
  let fixed = $state(new Set<string>());
  let error = $state<string | null>(null);
  let running = $state(false);

  async function loadSuspects() {
    loading = true;
    error = null;
    try {
      const diagnosis: any = await invoke("get_crash_diagnosis", { path, reportId: "__latest_log__" });
      mods = (diagnosis?.suspectedMods ?? []).slice(0, 8);
      const nextActions: Record<string, FixAction> = {};
      for (const hint of (diagnosis?.hints ?? []) as DiagnosisHint[]) {
        const actions = [...(hint.fixes ?? []), ...(hint.fix ? [hint.fix] : [])];
        for (const action of actions) {
          const id = action.modId;
          if (!id) continue;
          for (const related of hint.relatedMods ?? [id]) {
            if (!nextActions[related]) nextActions[related] = action;
          }
        }
      }
      suggestedActions = nextActions;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  function actionFor(mod: SuspectedMod): FixAction {
    return suggestedActions[mod.id] ?? {
      kind: "disableMod",
      label: `Disable ${mod.name}`,
      modId: mod.id,
    };
  }

  async function fixMod(mod: SuspectedMod) {
    if (!mod.id || fixing || fixed.has(mod.id)) return;
    fixing = mod.id;
    error = null;
    try {
      const action = actionFor(mod);
      const result: any = await invoke("apply_fix_actions", {
        path,
        actions: [{ ...action, modId: mod.id }],
      });
      if (result?.stopped) throw new Error(result?.error ?? "Fix was not applied");
      fixed = new Set([...fixed, mod.id]);
    } catch (e) {
      error = String(e);
    } finally {
      fixing = null;
    }
  }

  function runPack() {
    if (running) return;
    running = true;
    retryLastLaunch();
    onclose();
    setTimeout(() => (running = false), 1200);
  }

  $effect(() => {
    void path;
    void loadSuspects();
  });
</script>

<div class="overlay" role="presentation" onclick={(event) => event.target === event.currentTarget && onclose()}>
  <section class="recovery" role="dialog" aria-modal="true" aria-label="Crash recovery">
    <header>
      <div class="heading-icon"><AlertTriangle size={18} /></div>
      <div class="heading-copy">
        <strong>Build crashed</strong>
        <span>{message}</span>
      </div>
      <button class="close" type="button" aria-label="Close" onclick={onclose}><X size={17} /></button>
    </header>

    {#if loading}
      <div class="state"><Loader2 size={18} class="spin" /> Reading likely affected mods…</div>
    {:else if mods.length > 0}
      <div class="mod-list">
        <p class="hint">Disable a suspected mod, then run the pack again to verify the fix.</p>
        {#each mods as mod (mod.id)}
          <article class="mod-row">
            <div class="mod-icon"><PackageOpen size={17} /></div>
            <div class="mod-copy">
              <strong>{mod.name || mod.id}</strong>
              <span>{mod.id}{#if mod.version} · v{mod.version}{/if}</span>
            </div>
            {#if fixed.has(mod.id)}
              <span class="fixed"><CheckCircle2 size={15} /> Fixed</span>
            {:else}
              <button class="fix" type="button" title={actionFor(mod).label} disabled={!!fixing || running} onclick={() => fixMod(mod)}>
                {#if fixing === mod.id}<Loader2 size={14} class="spin" /> Fix{:else}Fix{/if}
              </button>
            {/if}
          </article>
        {/each}
      </div>
    {:else}
      <div class="state">No specific mod was identified. Open Diagnose for the full report.</div>
    {/if}

    {#if error}<p class="error">{error}</p>{/if}
    <footer>
      <button class="secondary" type="button" onclick={onclose}>Open later</button>
      <button class="run" type="button" disabled={running || !!fixing} onclick={runPack}>
        <Play size={15} /> {running ? "Starting…" : "Run"}
      </button>
    </footer>
  </section>
</div>

<style>
  .overlay { position: fixed; inset: 0; z-index: 220; display: flex; align-items: flex-start; justify-content: center; padding: 76px 16px 16px; background: rgba(0,0,0,.42); backdrop-filter: blur(5px); }
  .recovery { width: min(480px, 100%); max-height: min(620px, calc(100vh - 92px)); overflow: auto; padding: 16px; color: var(--text-primary); background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); box-shadow: var(--shadow-lg); }
  header { display: flex; align-items: flex-start; gap: 10px; padding-bottom: 14px; border-bottom: 1px solid var(--border-color); }
  .heading-icon { display: grid; place-items: center; width: 34px; height: 34px; flex: 0 0 34px; color: #fbbf24; background: rgba(245,158,11,.13); border: 1px solid rgba(245,158,11,.3); border-radius: 10px; }
  .heading-copy { display: grid; gap: 4px; min-width: 0; flex: 1; }
  .heading-copy strong { font-size: 14px; } .heading-copy span { color: var(--text-muted); font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .close { padding: 5px; color: var(--text-muted); background: transparent; border: 0; border-radius: 6px; cursor: pointer; } .close:hover { color: var(--text-primary); background: var(--bg-hover); }
  .hint { margin: 0 0 10px; color: var(--text-muted); font-size: 12px; line-height: 1.4; }
  .mod-list { display: grid; gap: 7px; padding-top: 14px; }
  .mod-row { display: flex; align-items: center; gap: 9px; min-width: 0; padding: 9px; border: 1px solid var(--border-color); border-radius: var(--border-radius-md); background: color-mix(in srgb, var(--bg-tertiary) 65%, transparent); }
  .mod-icon { display: grid; place-items: center; width: 32px; height: 32px; flex: 0 0 32px; color: var(--accent-primary); background: color-mix(in srgb, var(--accent-primary) 12%, transparent); border-radius: 9px; }
  .mod-copy { display: grid; gap: 2px; min-width: 0; flex: 1; } .mod-copy strong, .mod-copy span { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; } .mod-copy strong { font-size: 12px; } .mod-copy span { color: var(--text-muted); font: 10px ui-monospace, monospace; }
  button.fix, button.run, button.secondary { display: inline-flex; align-items: center; justify-content: center; gap: 6px; border-radius: var(--border-radius-sm); cursor: pointer; font-size: 12px; }
  .fix { padding: 6px 11px; color: var(--on-accent); background: var(--accent-primary); border: 0; font-weight: 700; } .fix:hover:not(:disabled), .run:hover:not(:disabled) { background: var(--accent-hover); }
  .fixed { display: inline-flex; align-items: center; gap: 4px; color: var(--accent-primary); font-size: 11px; }
  .state { display: flex; align-items: center; justify-content: center; gap: 8px; min-height: 100px; color: var(--text-muted); font-size: 12px; text-align: center; }
  .error { margin: 12px 0 0; padding: 8px; color: #fecaca; background: rgba(239,68,68,.1); border: 1px solid rgba(239,68,68,.3); border-radius: var(--border-radius-sm); font-size: 11px; word-break: break-word; }
  footer { display: flex; justify-content: flex-end; gap: 8px; padding-top: 14px; margin-top: 14px; border-top: 1px solid var(--border-color); }
  .secondary { padding: 7px 11px; color: var(--text-secondary); background: transparent; border: 1px solid var(--border-color); } .secondary:hover { background: var(--bg-hover); }
  .run { padding: 7px 16px; color: var(--on-accent); background: var(--accent-primary); border: 0; font-weight: 700; }
  button:disabled { opacity: .55; cursor: default; }
  :global(.spin) { animation: spin .9s linear infinite; } @keyframes spin { to { transform: rotate(360deg); } }
</style>
