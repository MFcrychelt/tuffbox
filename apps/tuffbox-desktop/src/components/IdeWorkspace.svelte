<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import {
    ClipboardList,
    SlidersHorizontal,
    Package,
    GitGraph,
    FileCode2,
    PlayCircle,
    Stethoscope,
    History,
    Camera,
    UploadCloud,
    Rocket,
    Mountain,
    PackageOpen,
    ScrollText,
    Circle,
    Map as MapIcon,
  } from "lucide-svelte";
  import { projectPath, ideStageRequest, autoHideWorkflowRail, tuneDirty, briefDirty, questDirty } from "../lib/store";
  import { onDestroy, onMount } from "svelte";
  import ProjectSettings from "./ProjectSettings.svelte";
  import Mods from "./Mods.svelte";
  import Graph from "./Graph.svelte";
  import ConfigEditor from "./ConfigEditor.svelte";
  import Diagnostics from "./Diagnostics.svelte";
  import Snapshots from "./Snapshots.svelte";
  import TestRuns from "./TestRuns.svelte";
  import ChangeHistory from "./ChangeHistory.svelte";
  import OreGenVisualizer from "./OreGenVisualizer.svelte";
  import World from "./World.svelte";
  import RecipeBrowser from "./RecipeBrowser.svelte";
  import QuestEditor from "./QuestEditor.svelte";
  import ExportBuilder from "./ExportBuilder.svelte";
  import ReleaseRoom from "./ReleaseRoom.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import BriefEditor from "./BriefEditor.svelte";

  type StageId =
    | "brief"
    | "setup"
    | "content"
    | "world-map"
    | "ore-gen"
    | "recipes"
    | "quests"
    | "resolve"
    | "configs"
    | "history"
    | "test"
    | "diagnose"
    | "snapshots"
    | "export"
    | "release";

  type Stage = {
    id: StageId;
    label: string;
    short: string;
    icon: any;
    goal: string;
    outputs: string[];
  };

  const stages: Stage[] = [
    {
      id: "brief",
      label: "Brief",
      short: "Idea",
      icon: ClipboardList,
      goal: "Shape the storefront listing: icon, summary, markdown description, and live Modrinth/CurseForge card preview.",
      outputs: ["listing card", "summary + icon", "author notes"],
    },
    {
      id: "setup",
      label: "Setup",
      short: "Project",
      icon: SlidersHorizontal,
      goal: "Choose Minecraft/loader/Java, memory budget and base project settings.",
      outputs: ["manifest", "profiles", "runtime settings"],
    },
    {
      id: "content",
      label: "Content",
      short: "Mods",
      icon: Package,
      goal: "Add, update and remove mods as managed dependencies, not loose files.",
      outputs: ["mod list", "source metadata", "auto snapshots"],
    },
    {
      id: "quests",
      label: "Quests",
      short: "Lore",
      icon: ScrollText,
      goal: "Author FTB Quests lines with AI sidebar (20+ quests, lore, tasks/rewards) and edit SNBT visually.",
      outputs: ["quest tree", "AI QuestPlan", "SNBT files", "validation report"],
    },
    {
      id: "recipes",
      label: "Recipes",
      short: "Craft",
      icon: PackageOpen,
      goal: "JEI-style recipe browser: search, Recipes/Uses, KubeJS remove scripts.",
      outputs: ["recipe list", "disable scripts", "ingredient search"],
    },
    {
      id: "world-map",
      label: "World",
      short: "Map",
      icon: MapIcon,
      goal: "MCA Selector-style chunk map: select, delete, export/import, NBT edit.",
      outputs: ["chunk map", "selection", "export / backup"],
    },
    {
      id: "ore-gen",
      label: "Ores",
      short: "Heights",
      icon: Mountain,
      goal: "Visualize ore generation heights, vein sizes and toggle worldgen from configs.",
      outputs: ["ore layers", "generation config", "spawn rates"],
    },
    {
      id: "resolve",
      label: "Resolve",
      short: "Graph",
      icon: GitGraph,
      goal: "Inspect dependency graph, missing dependencies, conflicts and side mismatches.",
      outputs: ["diagnostics", "change plan", "lockfile graph"],
    },
    {
      id: "configs",
      label: "Tune",
      short: "Configs",
      icon: FileCode2,
      goal: "Edit configs, scripts and overrides with rollback-safe saves.",
      outputs: ["configs", "KubeJS/scripts", "tracked changes"],
    },
    {
      id: "history",
      label: "History",
      short: "Changes",
      icon: History,
      goal: "Chronological pack activity: launcher ops, external disk edits, AI fixes.",
      outputs: ["timeline", "delta scan", "AI context"],
    },
    {
      id: "test",
      label: "Test",
      short: "Runs",
      icon: PlayCircle,
      goal: "Launch client/server profiles, collect logs and measure startup stability.",
      outputs: ["latest.log", "run history", "performance notes"],
    },
    {
      id: "diagnose",
      label: "Diagnose",
      short: "Health",
      icon: Stethoscope,
      goal: "Turn errors, crashes and graph diagnostics into clear next actions.",
      outputs: ["suspected mods", "fix hypotheses", "safe plan"],
    },
    {
      id: "snapshots",
      label: "Snapshots",
      short: "Checkpoints",
      icon: Camera,
      goal: "Checkpoint risky edits, compare states and rollback — not the activity feed (see History).",
      outputs: ["snapshots", "diff", "rollback point"],
    },
    {
      id: "export",
      label: "Export",
      short: "Build",
      icon: UploadCloud,
      goal: "Package the pack into .mrpack, Prism zip, server pack and changelog.",
      outputs: ["artifacts", "server pack", "changelog"],
    },
    {
      id: "release",
      label: "Release",
      short: "Ship",
      icon: Rocket,
      goal: "Prepare release notes, publish draft and track post-release hotfixes.",
      outputs: ["release snapshot", "publish draft", "support checklist"],
    },
  ];

  let activeStage: StageId = "brief";
  let leaveConfirmOpen = false;
  let pendingStage: StageId | null = null;
  let leaveKind: "tune" | "brief" | "quests" = "tune";

  function goToStage(id: StageId) {
    if (id === activeStage) return;
    if (activeStage === "configs" && $tuneDirty) {
      leaveKind = "tune";
      pendingStage = id;
      leaveConfirmOpen = true;
      return;
    }
    if (activeStage === "brief" && $briefDirty) {
      leaveKind = "brief";
      pendingStage = id;
      leaveConfirmOpen = true;
      return;
    }
    if (activeStage === "quests" && $questDirty) {
      leaveKind = "quests";
      pendingStage = id;
      leaveConfirmOpen = true;
      return;
    }
    activeStage = id;
  }

  function confirmLeaveStage() {
    leaveConfirmOpen = false;
    if (leaveKind === "tune") tuneDirty.set(false);
    else if (leaveKind === "brief") briefDirty.set(false);
    else questDirty.set(false);
    if (pendingStage) {
      activeStage = pendingStage;
      pendingStage = null;
    }
  }

  function cancelLeaveStage() {
    leaveConfirmOpen = false;
    pendingStage = null;
  }

  $: if ($ideStageRequest) {
    const req = $ideStageRequest;
    ideStageRequest.set(null);
    if (stages.some((s) => s.id === req)) {
      goToStage(req as StageId);
    }
  }

  let focusedScanTimer: ReturnType<typeof setInterval> | null = null;

  async function refreshFocusedScanLoop() {
    if (focusedScanTimer) {
      clearInterval(focusedScanTimer);
      focusedScanTimer = null;
    }
    if (!$projectPath) return;
    try {
      const settings: { focusedScan?: boolean } = await invoke("get_history_settings", {
        path: $projectPath,
      });
      if (!settings?.focusedScan) return;
      focusedScanTimer = setInterval(() => {
        if (!$projectPath) return;
        void invoke("scan_project_changes", { path: $projectPath }).catch(() => {});
      }, 60_000);
    } catch {
      // ignore
    }
  }

  $: if ($projectPath) void refreshFocusedScanLoop();

  onMount(() => {
    const onVis = () => {
      if (document.visibilityState === "visible") void refreshFocusedScanLoop();
      else if (focusedScanTimer) {
        clearInterval(focusedScanTimer);
        focusedScanTimer = null;
      }
    };
    const onSettings = () => void refreshFocusedScanLoop();
    document.addEventListener("visibilitychange", onVis);
    window.addEventListener("tuffbox:history-settings-changed", onSettings);
    return () => {
      document.removeEventListener("visibilitychange", onVis);
      window.removeEventListener("tuffbox:history-settings-changed", onSettings);
    };
  });

  let railRevealed = false;
  let railHideTimer: ReturnType<typeof setTimeout> | null = null;
  /** Grace before hide — enough to move from hotzone onto the rail. */
  const RAIL_HIDE_MS = 280;

  function clearRailHideTimer() {
    if (railHideTimer) {
      clearTimeout(railHideTimer);
      railHideTimer = null;
    }
  }

  function revealRail() {
    if (!$autoHideWorkflowRail) return;
    clearRailHideTimer();
    railRevealed = true;
  }

  function scheduleHideRail(delay = RAIL_HIDE_MS) {
    if (!$autoHideWorkflowRail) return;
    clearRailHideTimer();
    railHideTimer = setTimeout(() => {
      railRevealed = false;
      railHideTimer = null;
    }, delay);
  }

  function onRailFocusOut(e: FocusEvent) {
    if (!$autoHideWorkflowRail) return;
    const next = e.relatedTarget;
    if (next instanceof Node && e.currentTarget instanceof Node && e.currentTarget.contains(next)) {
      return;
    }
    scheduleHideRail();
  }

  $: if (!$autoHideWorkflowRail) {
    railRevealed = false;
    clearRailHideTimer();
  }

  onDestroy(() => {
    if (focusedScanTimer) clearInterval(focusedScanTimer);
    clearRailHideTimer();
  });
</script>

<div class="ide-workspace" class:auto-hide-rail={$autoHideWorkflowRail}>
  <section class="stage-shell">
    <div class="stage-content" class:fill-stage={activeStage === "configs" || activeStage === "world-map" || activeStage === "brief" || activeStage === "quests"}>
      {#if activeStage === "brief"}
        <BriefEditor />
      {:else if activeStage === "setup"}
        {#if $projectPath}
          <ProjectSettings showBack={false} stayAfterSave={true} />
        {:else}
          <div class="skeleton-page">
            <h2>No project opened</h2>
            <p>Go to Home, create or open an instance, then return to the IDE workflow.</p>
          </div>
        {/if}
      {:else if activeStage === "quests"}
        <QuestEditor />
      {:else if activeStage === "recipes"}
        <RecipeBrowser />
      {:else if activeStage === "world-map"}
        <World />
      {:else if activeStage === "ore-gen"}
        <OreGenVisualizer />
      {:else if activeStage === "content"}
        <Mods />
      {:else if activeStage === "resolve"}
        <Graph />
      {:else if activeStage === "configs"}
        <ConfigEditor />
      {:else if activeStage === "history"}
        <ChangeHistory />
      {:else if activeStage === "test"}
        <TestRuns />
      {:else if activeStage === "diagnose"}
        <Diagnostics />
      {:else if activeStage === "snapshots"}
        <Snapshots />
      {:else if activeStage === "export"}
        <ExportBuilder />
      {:else if activeStage === "release"}
        <ReleaseRoom />
      {/if}
    </div>
  </section>

  {#if $autoHideWorkflowRail}
    <div
      class="rail-hotzone"
      aria-hidden="true"
      on:mouseenter={revealRail}
      on:mouseleave={() => scheduleHideRail()}
    ></div>
  {/if}
  <nav
    class="workflow-rail"
    class:revealed={railRevealed || !$autoHideWorkflowRail}
    aria-label="Modpack production workflow"
    on:mouseenter={revealRail}
    on:mouseleave={() => scheduleHideRail()}
    on:focusin={revealRail}
    on:focusout={onRailFocusOut}
  >
    {#each stages as stage (stage.id)}
      <button
        class="stage-tab"
        class:active={activeStage === stage.id}
        on:click={(e) => {
          goToStage(stage.id);
          if (e.currentTarget instanceof HTMLElement) e.currentTarget.blur();
          scheduleHideRail(320);
        }}
        title={stage.goal}
        aria-current={activeStage === stage.id ? "step" : undefined}
      >
        <span class="stage-status" aria-hidden="true">
          <Circle size={12} fill={activeStage === stage.id ? "currentColor" : "none"} />
        </span>
        <svelte:component this={stage.icon} size={20} />
        <span class="stage-text">
          <strong>{stage.label}</strong>
          <small>{stage.short}</small>
        </span>
      </button>
    {/each}
  </nav>
</div>

{#if leaveConfirmOpen}
  <ConfirmDialog
    title={leaveKind === "tune"
      ? "Discard Tune changes?"
      : leaveKind === "brief"
        ? "Discard Brief changes?"
        : "Discard Quests changes?"}
    message={leaveKind === "tune"
      ? "You have unsaved config edits. Leave Tune and discard them?"
      : leaveKind === "brief"
        ? "You have unsaved listing edits. Leave Brief and discard them?"
        : "You have unsaved quest edits. Leave Quests and discard them?"}
    danger={false}
    confirmLabel="Discard & leave"
    on:confirm={confirmLeaveStage}
    on:cancel={cancelLeaveStage}
  />
{/if}

<style>
  .ide-workspace {
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
    display: grid;
    grid-template-rows: minmax(0, 1fr) auto;
    position: relative;
  }

  .ide-workspace.auto-hide-rail {
    grid-template-rows: minmax(0, 1fr);
  }

  .skeleton-page {
    width: min(1120px, 100%);
    margin: 0 auto;
  }

  .stage-shell {
    min-width: 0;
    min-height: 0;
    height: 100%;
    display: flex;
    flex-direction: column;
    background:
      radial-gradient(circle at top right, rgba(27, 217, 106, 0.06), transparent 32%),
      rgba(255, 255, 255, 0.015);
    overflow: hidden;
  }

  .stage-content {
    flex: 1;
    min-width: 0;
    min-height: 0;
    width: 100%;
    overflow: auto;
    padding: 20px 24px;
    scrollbar-gutter: stable;
  }

  .stage-content.fill-stage {
    overflow: hidden;
    display: flex;
    flex-direction: column;
    padding: 0;
  }

  .stage-content.fill-stage > :global(.config-editor) {
    flex: 1;
    min-height: 0;
    height: 100%;
    padding: 16px 20px;
    box-sizing: border-box;
  }

  .stage-content.fill-stage > :global(.worlds-view) {
    flex: 1;
    min-height: 0;
  }

  .stage-content.fill-stage > :global(.qe.ftbq) {
    flex: 1;
    min-height: 0;
    height: 100%;
  }

  .stage-content.fill-stage > :global(.brief-editor) {
    flex: 1;
    min-height: 0;
    height: 100%;
  }

  .rail-hotzone {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 22px;
    z-index: 6;
  }

  .workflow-rail {
    flex: 0 0 auto;
    display: flex;
    flex-wrap: wrap;
    align-items: stretch;
    gap: 4px;
    min-width: 0;
    padding: 8px 12px;
    overflow: visible;
    border-top: 1px solid var(--border-color);
    background: var(--bg-secondary);
    z-index: 5;
  }

  .ide-workspace.auto-hide-rail .workflow-rail {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    transform: translateY(calc(100% + 2px));
    opacity: 0;
    visibility: hidden;
    transition:
      transform 0.2s cubic-bezier(0.22, 1, 0.36, 1),
      opacity 0.16s ease,
      visibility 0s linear 0.2s;
    box-shadow: 0 -12px 32px rgba(0, 0, 0, 0.32);
    pointer-events: none;
    will-change: transform, opacity;
  }

  .ide-workspace.auto-hide-rail .workflow-rail.revealed {
    transform: translateY(0);
    opacity: 1;
    visibility: visible;
    pointer-events: auto;
    transition:
      transform 0.2s cubic-bezier(0.22, 1, 0.36, 1),
      opacity 0.14s ease,
      visibility 0s linear 0s;
  }

  .ide-workspace.auto-hide-rail:has(.workflow-rail.revealed) .rail-hotzone {
    pointer-events: none;
  }

  .stage-tab {
    min-width: 0;
    min-height: 52px;
    flex: 1 1 auto;
    justify-content: flex-start;
    gap: 8px;
    padding: 9px 10px;
    background: transparent;
    color: var(--text-secondary);
    border: 1px solid transparent;
  }

  .stage-tab:hover,
  .stage-tab.active {
    transform: none;
    background: var(--bg-tertiary);
    border-color: rgba(27, 217, 106, 0.35);
    color: var(--text-primary);
  }

  .stage-tab.active .stage-status {
    color: var(--accent-primary);
  }

  .stage-tab:focus-visible,
  textarea:focus-visible {
    outline: 2px solid var(--accent-primary);
    outline-offset: 2px;
  }

  .stage-status {
    display: grid;
    flex: 0 0 auto;
    place-items: center;
    color: var(--text-muted);
  }

  .stage-text {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    min-width: 0;
    line-height: 1.1;
  }

  .stage-text strong,
  .stage-text small {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .stage-text small {
    color: var(--text-muted);
    font-size: 11px;
  }

  .skeleton-page {
    min-height: 100%;
  }

  .skeleton-page h2 {
    margin-bottom: 8px;
  }

  .skeleton-page p {
    color: var(--text-muted);
  }

  .page-header {
    display: flex;
    justify-content: space-between;
    gap: 16px;
    align-items: flex-start;
  }

  .inline-error,
  .inline-success {
    margin-top: 12px;
    padding: 10px 12px;
    border-radius: var(--border-radius-md);
    border: 1px solid var(--border-color);
  }

  .inline-error {
    color: #fecaca;
    background: rgba(239, 68, 68, 0.08);
    border-color: rgba(239, 68, 68, 0.28);
  }

  .inline-success {
    color: var(--accent-primary);
    background: rgba(27, 217, 106, 0.08);
    border-color: rgba(27, 217, 106, 0.25);
  }

  .brief-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 14px;
    margin-top: 18px;
  }

  label {
    display: flex;
    flex-direction: column;
    gap: 8px;
    color: var(--text-secondary);
    font-weight: 700;
  }

  textarea {
    min-height: 120px;
    resize: vertical;
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-md);
    background: var(--bg-elevated);
    color: var(--text-primary);
    padding: 12px;
    font-family: inherit;
  }

  @media (max-width: 1100px) {
    .brief-grid {
      grid-template-columns: 1fr;
    }

    .stage-content {
      padding: 16px;
    }

    /* Keep every stage tab visible (no horizontal scrollbar): show the short
       label + icon and drop the long descriptive name on narrower windows. */
    .stage-tab {
      justify-content: center;
      flex: 1 1 auto;
    }

    .stage-text strong {
      display: none;
    }
  }

  @media (max-width: 720px) {
    .workflow-rail {
      padding-inline: 8px;
    }

    .stage-tab {
      flex-direction: column;
      gap: 4px;
      padding-inline: 6px;
    }

    .stage-text small {
      font-size: 10px;
    }
  }
</style>
