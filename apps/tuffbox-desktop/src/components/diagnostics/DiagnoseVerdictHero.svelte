<script lang="ts">
  import { createEventDispatcher } from "svelte";
  import { CheckCircle, AlertTriangle, CircleHelp, Wrench, ArrowDownToLine, Lightbulb } from "lucide-svelte";
  import { ideStageRequest } from "../../lib/store";

  type MergedRec = {
    id: string;
    source: "rules" | "ai";
    label: string;
    detail: string;
    risk: string;
    modId: string | null;
    apply: () => void;
  };
  type WorldCoords = { x: number; y: number; z: number; label: string };

  /** Verdict banner + "class card" hints + secondary recommendations list.
   *  The single biggest, most state-coupled seam in Diagnostics: it reads
   *  from nearly every derived signal the parent computes, but owns none of
   *  it — every button here either calls a self-contained callback already
   *  bound by the parent (`rec.apply`) or dispatches an event for the
   *  parent's invoke-backed fix functions. */
  export let sessionOk = false;
  export let topSuspect: any = null;
  export let topFinding: any = null;
  export let heroCulpritLabel = "";
  export let strongestEvidence: any = null;
  export let analysisBusy = false;
  export let primaryRec: MergedRec | null = null;
  export let mergedRecommendations: MergedRec[] = [];
  export let aiApplyBusy = false;
  export let applyingHintId: string | null = null;
  export let disablingModId: string | null = null;
  export let fixingIdx: number | null = null;
  export let aiAnalysis: any = null;
  export let logDisplayText = "";
  export let isHsErr = false;
  export let hsErrKind: string | null = null;
  export let memoryHint: string | null = null;
  export let worldCoords: WorldCoords | null | undefined = null;
  export let cascadingFinding: any = null;
  export let mixinFinding: any = null;
  export let sideMismatchFinding: any = null;
  export let suspected: any[] = [];

  const dispatch = createEventDispatcher<{
    fixDisableMod: string;
    applyTopSuspectUpdate: void;
    applyAiPlan: void;
    jumpToFirstError: void;
    applyBisectDisableHalf: void;
  }>();

  function severityChip(sev: string): string {
    if (sev === "critical") return "Fix this first";
    if (sev === "error") return "Needs a fix";
    if (sev === "warning") return "Worth checking";
    return "FYI";
  }

  function aiPlanActions(analysis: any): any[] {
    return analysis?.actions ?? analysis?.recommended_actions ?? analysis?.recommendedActions ?? [];
  }
</script>

<!-- Verdict first (answer before the scary log) -->
<section class="dx-verdict" class:ok={sessionOk} class:warn={!sessionOk && !!(topSuspect || topFinding)} class:neutral={!sessionOk && !topSuspect && !topFinding}>
  <div class="dx-verdict-icon">
    {#if sessionOk}
      <CheckCircle size={22} />
    {:else if topSuspect || topFinding}
      <AlertTriangle size={22} />
    {:else}
      <CircleHelp size={22} />
    {/if}
  </div>
  <div class="dx-verdict-body">
    {#if sessionOk}
      <span class="eyebrow">You're good</span>
      <h1>Last launch looked fine</h1>
      <p class="dx-verdict-copy">No crash to chase right now. Pack graph warnings below still matter if any show up.</p>
    {:else if topFinding && (!topSuspect || (topFinding.severity === "critical" || topFinding.severity === "error"))}
      <span class="eyebrow">{severityChip(topFinding.severity)}</span>
      <h1>{topFinding.title}</h1>
      <p class="dx-verdict-copy">{topFinding.description}</p>
      {#if topFinding.autoFix}
        <p class="dx-next-step"><strong>Try this:</strong> {topFinding.autoFix}</p>
      {/if}
      {#if topSuspect}
        <p class="dx-verdict-copy muted-inline">
          Suspect mod: <code>{topSuspect.id}</code>
          · {topSuspect.confidence}%
        </p>
      {/if}
    {:else if topSuspect}
      <span class="eyebrow">Looks like this broke it</span>
      <h1>{heroCulpritLabel || topSuspect.name}</h1>
      <p class="dx-verdict-copy">
        <code>{topSuspect.id}</code>
        · {topSuspect.confidence}% confidence
        {#if topSuspect.blameRole}· {topSuspect.blameRole}{/if}
      </p>
      {#if strongestEvidence}
        <p class="dx-evidence"><code>{strongestEvidence}</code></p>
      {/if}
    {:else}
      <span class="eyebrow">Still figuring it out</span>
      <h1>No clear culprit yet</h1>
      <p class="dx-verdict-copy">
        {analysisBusy
          ? "Scanning the log…"
          : "Hit Re-analyze, or jump to the first error in the log below."}
      </p>
    {/if}

    <div class="dx-cta">
      {#if !sessionOk && primaryRec}
        <button
          class="primary"
          type="button"
          on:click={primaryRec.apply}
          disabled={aiApplyBusy || applyingHintId !== null}
        >
          <Wrench size={15} />
          {primaryRec.label}
        </button>
        {#if mergedRecommendations.length > 1}
          <span class="dx-cta-more">{mergedRecommendations.length - 1} more below</span>
        {/if}
      {:else if !sessionOk && topSuspect?.knownInManifest}
        <button class="primary" on:click={() => dispatch("fixDisableMod", topSuspect.id)} disabled={disablingModId === topSuspect.id}>
          {disablingModId === topSuspect.id ? "Disabling…" : `Disable ${topSuspect.name}`}
        </button>
        <button class="ghost" on:click={() => dispatch("applyTopSuspectUpdate")} disabled={fixingIdx === -1}>Update</button>
      {/if}
      {#if !sessionOk && aiAnalysis && aiPlanActions(aiAnalysis).length > 1}
        <button
          class="secondary"
          on:click={() => dispatch("applyAiPlan")}
          disabled={aiApplyBusy || (aiAnalysis.validation && aiAnalysis.validation.ok === false)}
        >
          {aiApplyBusy ? "Applying…" : "Review & apply AI plan"}
        </button>
      {/if}
      {#if !sessionOk}
        <button class="ghost" type="button" on:click={() => dispatch("jumpToFirstError")} disabled={!logDisplayText}>
          <ArrowDownToLine size={15} /> Jump to error
        </button>
      {/if}
    </div>
  </div>
</section>

{#if !sessionOk && (isHsErr || memoryHint || cascadingFinding || mixinFinding || sideMismatchFinding || worldCoords)}
  <div class="dx-class-cards">
    {#if isHsErr}
      <div class="dx-class-card">
        <strong>Java native crash (hs_err)</strong>
        <p>
          {hsErrKind === "oom"
            ? "JVM ran out of memory (native/heap). Raise -Xmx carefully and check for leaks."
            : "JVM fatal error — check Problematic frame and GPU/Java version."}
        </p>
        <button type="button" class="ghost mini" on:click={() => ideStageRequest.set("setup")}>Open Setup</button>
      </div>
    {/if}
    {#if memoryHint && !isHsErr}
      <div class="dx-class-card">
        <strong>Out of memory</strong>
        <p>{memoryHint}</p>
        <button type="button" class="ghost mini" on:click={() => ideStageRequest.set("setup")}>JVM / Setup</button>
      </div>
    {/if}
    {#if cascadingFinding}
      <div class="dx-class-card warn">
        <strong>Cascading error</strong>
        <p>{cascadingFinding.description}</p>
        <button type="button" class="ghost mini" on:click={() => dispatch("jumpToFirstError")}>Jump to early error</button>
      </div>
    {/if}
    {#if mixinFinding}
      <div class="dx-class-card">
        <strong>Mixin conflict</strong>
        <p>{mixinFinding.description}</p>
        <button type="button" class="ghost mini" on:click={() => dispatch("jumpToFirstError")}>Open log at mixin</button>
        {#if suspected.length >= 2}
          <button type="button" class="ghost mini" on:click={() => dispatch("applyBisectDisableHalf")}>Try disable half (bisect)</button>
        {/if}
      </div>
    {/if}
    {#if sideMismatchFinding}
      <div class="dx-class-card">
        <strong>Client-only / wrong side</strong>
        <p>{sideMismatchFinding.description}</p>
        <button type="button" class="ghost mini" on:click={() => ideStageRequest.set("export")}>Server pack checklist</button>
      </div>
    {/if}
    {#if worldCoords}
      <div class="dx-class-card">
        <strong>{worldCoords.label} @ {worldCoords.x}, {worldCoords.y}, {worldCoords.z}</strong>
        <p>Hint: restore nearby chunk or teleport away if a ticking entity is stuck.</p>
      </div>
    {/if}
  </div>
{/if}

<!-- Recommended plan (before evidence & log) -->
{#if !sessionOk && mergedRecommendations.length > 1}
  <section class="dx-more-actions panel">
    <h2><Lightbulb size={16} /> Other ways to fix it</h2>
    <ul class="merged-list compact">
      {#each mergedRecommendations.slice(1) as rec (rec.id)}
        <li class="merged-item {rec.source}">
          <span class="src-tag">{rec.source === "ai" ? "AI" : "Rules"}</span>
          <div class="merged-body">
            <strong>{rec.label}</strong>
            {#if rec.detail}<span>{rec.detail}</span>{/if}
          </div>
          <button class="secondary small" type="button" on:click={rec.apply} disabled={aiApplyBusy || applyingHintId !== null}>
            Do it
          </button>
        </li>
      {/each}
    </ul>
  </section>
{/if}

<style>
  .panel { padding: 16px; min-width: 0; background: var(--bg-secondary); border: 1px solid var(--border-color); border-radius: var(--border-radius-lg); }
  .muted-inline { margin: 0; color: var(--text-muted); font-size: 12px; }
  .eyebrow { display: block; margin-bottom: 4px; color: var(--text-muted); font-size: 11px; font-weight: 800; letter-spacing: .08em; text-transform: uppercase; }
  .dx-verdict {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    gap: 14px;
    padding: 18px;
    margin-bottom: 14px;
    border-radius: var(--border-radius-lg);
    border: 1px solid var(--border-color);
    background: var(--bg-secondary);
  }
  .dx-verdict.warn {
    border-color: rgba(245, 158, 11, 0.42);
    background: linear-gradient(135deg, rgba(245, 158, 11, 0.11), var(--bg-secondary) 65%);
  }
  .dx-verdict.ok {
    border-color: rgba(27, 217, 106, 0.35);
    background: linear-gradient(135deg, rgba(27, 217, 106, 0.08), var(--bg-secondary) 65%);
  }
  .dx-verdict-icon {
    display: grid;
    place-items: center;
    width: 42px;
    height: 42px;
    border-radius: var(--border-radius-md);
    color: var(--text-muted);
    background: var(--bg-tertiary);
  }
  .dx-verdict.warn .dx-verdict-icon { color: var(--accent-warning); background: rgba(245, 158, 11, 0.13); }
  .dx-verdict.ok .dx-verdict-icon { color: var(--accent-primary); background: rgba(27, 217, 106, 0.13); }
  .dx-verdict-body { min-width: 0; }
  .dx-verdict-body h1 { margin: 0; color: var(--text-primary); font-size: 20px; line-height: 1.3; }
  .dx-verdict-copy { margin: 6px 0 0; color: var(--text-secondary); font-size: 13px; line-height: 1.45; }
  .dx-verdict-copy code { font-size: 12px; color: var(--text-muted); }
  .dx-next-step {
    margin: 10px 0 0;
    padding: 10px 12px;
    border-radius: var(--border-radius-sm);
    background: rgba(27, 217, 106, 0.08);
    border: 1px solid rgba(27, 217, 106, 0.22);
    color: var(--text-primary);
    font-size: 13px;
    line-height: 1.4;
  }
  .dx-evidence {
    margin: 10px 0 0;
    padding: 10px 12px;
    border-left: 3px solid var(--accent-warning);
    border-radius: 0 10px 10px 0;
    background: var(--bg-tertiary);
    font-size: 12px;
    color: var(--text-secondary);
    word-break: break-word;
  }
  .dx-cta { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; margin-top: 14px; }
  .dx-cta-more { color: var(--text-muted); font-size: 12px; }
  .dx-more-actions { margin-bottom: 14px; }
  .merged-list.compact { margin: 0; padding: 0; list-style: none; display: flex; flex-direction: column; gap: 8px; }
  .merged-item {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    gap: 10px;
    align-items: center;
    padding: 10px 12px;
    border-radius: 10px;
    border: 1px solid var(--border-color);
    background: var(--bg-tertiary);
  }
  .merged-body { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .merged-body strong { color: var(--text-primary); font-size: 13px; }
  .merged-body span { color: var(--text-muted); font-size: 12px; }
  .src-tag {
    padding: 2px 7px;
    border-radius: 999px;
    background: var(--bg-secondary);
    color: var(--text-muted);
    font-size: 10px;
    font-weight: 800;
  }
  .merged-item.ai .src-tag { color: var(--accent-primary); background: rgba(27, 217, 106, 0.12); }
  .dx-class-cards {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: 8px;
    margin-bottom: 12px;
  }
  .dx-class-card {
    padding: 10px 12px;
    border-radius: var(--border-radius-sm);
    border: 1px solid var(--border-color);
    background: var(--bg-secondary);
    font-size: 12px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .dx-class-card.warn {
    border-color: rgba(251, 191, 36, 0.45);
    background: rgba(251, 191, 36, 0.08);
  }
  .dx-class-card p { margin: 0; color: var(--text-secondary); }
  @media (max-width: 720px) {
    .dx-verdict { grid-template-columns: 1fr; }
    .merged-item { grid-template-columns: 1fr; }
  }
</style>
