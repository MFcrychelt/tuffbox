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
    UploadCloud,
    Rocket,
    CheckCircle2,
    Mountain,
    PackageOpen,
    ScrollText,
    Circle,
  } from "lucide-svelte";
  import { projectPath } from "../lib/store";
  import ProjectSettings from "./ProjectSettings.svelte";
  import Mods from "./Mods.svelte";
  import Graph from "./Graph.svelte";
  import ConfigEditor from "./ConfigEditor.svelte";
  import Diagnostics from "./Diagnostics.svelte";
  import Snapshots from "./Snapshots.svelte";
  import TestRuns from "./TestRuns.svelte";
  import ChangeHistory from "./ChangeHistory.svelte";
  import OreGenVisualizer from "./OreGenVisualizer.svelte";
  import RecipeBrowser from "./RecipeBrowser.svelte";
  import QuestEditor from "./QuestEditor.svelte";
  import ExportBuilder from "./ExportBuilder.svelte";
  import ReleaseRoom from "./ReleaseRoom.svelte";

  type StageId =
    | "brief"
    | "setup"
    | "content"
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
      goal: "Define the pack: audience, Minecraft version, loader, gameplay pillars and constraints.",
      outputs: ["pack brief", "target player", "risk notes"],
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
      goal: "Design FTB Quests chapters and quests visually without launching Minecraft.",
      outputs: ["quest tree", "SNBT files", "validation report"],
    },
    {
      id: "recipes",
      label: "Recipes",
      short: "Craft",
      icon: PackageOpen,
      goal: "Browse all recipes from installed mods, generate KubeJS disable scripts.",
      outputs: ["recipe list", "disable scripts", "ingredient search"],
    },
    {
      id: "ore-gen",
      label: "World",
      short: "Ores",
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
      goal: "Review tracked changes across mods, configs, shaders, resource packs and project files.",
      outputs: ["file tree", "change preview", "editor"],
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
      short: "History",
      icon: History,
      goal: "Checkpoint risky edits, compare states and rollback broken experiments.",
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
  let briefGoal = "";
  let briefAudience = "";
  let briefPillars = "";
  let briefConstraints = "";
  let briefReleaseTargets = "";
  let briefNotes = "";
  let briefMessage = "";
  let briefError = "";
  let lastBriefPath: string | null = null;

  async function loadBrief() {
    if (!$projectPath || lastBriefPath === $projectPath) return;
    briefError = "";
    try {
      const brief: any = await invoke("get_project_brief", { path: $projectPath });
      briefGoal = brief.goal ?? "";
      briefAudience = brief.targetAudience ?? "";
      briefPillars = (brief.gameplayPillars ?? []).join("\n");
      briefConstraints = (brief.constraints ?? []).join("\n");
      briefReleaseTargets = (brief.releaseTargets ?? []).join("\n");
      briefNotes = brief.notes ?? "";
      lastBriefPath = $projectPath;
    } catch (e) {
      briefError = String(e);
    }
  }

  async function saveBrief() {
    if (!$projectPath) return;
    briefError = "";
    briefMessage = "";
    try {
      await invoke("update_project_brief", {
        path: $projectPath,
        brief: {
          goal: briefGoal,
          targetAudience: briefAudience,
          gameplayPillars: lines(briefPillars),
          constraints: lines(briefConstraints),
          releaseTargets: lines(briefReleaseTargets),
          notes: briefNotes,
        },
      });
      briefMessage = "Brief saved. Auto snapshot created.";
    } catch (e) {
      briefError = String(e);
    }
  }

  function lines(value: string) {
    return value
      .split("\n")
      .map((line) => line.trim())
      .filter(Boolean);
  }

  $: if ($projectPath) loadBrief();
  $: activeIndex = stages.findIndex((stage) => stage.id === activeStage);
  $: completed = new Set(stages.slice(0, Math.max(activeIndex, 0)).map((stage) => stage.id));

</script>

<div class="ide-workspace">
  <section class="stage-shell">
    <div class="stage-content">
      {#if activeStage === "brief"}
        <div class="skeleton-page">
          <div class="page-header">
            <div>
              <h2>Pack brief</h2>
              <p>Pre-production document saved into the project manifest. Use it to keep the pack direction clear before dependency work.</p>
            </div>
            <button on:click={saveBrief} disabled={!$projectPath}>Save brief</button>
          </div>
          {#if briefError}<div class="inline-error">{briefError}</div>{/if}
          {#if briefMessage}<div class="inline-success">{briefMessage}</div>{/if}
          <div class="brief-grid">
            <label>Pack goal<textarea bind:value={briefGoal} placeholder="Example: low-end-friendly tech + exploration Fabric pack for 1.21.x" /></label>
            <label>Target player<textarea bind:value={briefAudience} placeholder="Developers, server owners, casual players, low-end PCs..." /></label>
            <label>Gameplay pillars<textarea bind:value={briefPillars} placeholder="One pillar per line: Performance, progression, QoL..." /></label>
            <label>Hard constraints<textarea bind:value={briefConstraints} placeholder="One constraint per line: No client-only mods in server profile..." /></label>
            <label>Release targets<textarea bind:value={briefReleaseTargets} placeholder="Modrinth, private server, Prism zip, GitHub Releases..." /></label>
            <label>Notes<textarea bind:value={briefNotes} placeholder="Open questions, references, balancing notes..." /></label>
          </div>
        </div>
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

  <nav class="workflow-rail" aria-label="Modpack production workflow">
    {#each stages as stage, index}
      <button
        class="stage-tab"
        class:active={activeStage === stage.id}
        class:done={completed.has(stage.id)}
        on:click={() => (activeStage = stage.id)}
        title={stage.goal}
      >
        <span class="stage-status">
          {#if completed.has(stage.id)}
            <CheckCircle2 size={15} />
          {:else}
            <Circle size={15} />
          {/if}
        </span>
        <svelte:component this={stage.icon} size={20} />
        <span class="stage-text">
          <strong>{stage.label}</strong>
          <small>{stage.short}</small>
        </span>
      </button>
      {#if index < stages.length - 1}<span class="rail-line" />{/if}
    {/each}
  </nav>
</div>

<style>
  .ide-workspace {
    width: min(1840px, 100%);
    min-height: calc(100vh - 120px);
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .workflow-rail,
  .stage-shell,
  .skeleton-page {
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
    background: var(--bg-secondary);
  }

  .stage-shell {
    flex: 1;
    min-height: 76vh;
    padding: 18px;
    background:
      radial-gradient(circle at top right, rgba(27, 217, 106, 0.06), transparent 32%),
      rgba(255, 255, 255, 0.015);
    overflow: hidden;
  }

  .stage-content {
    min-width: 0;
    width: 100%;
    height: 100%;
  }

  .workflow-rail {
    position: sticky;
    bottom: 0;
    z-index: 20;
    display: flex;
    align-items: stretch;
    gap: 0;
    padding: 14px 16px;
    overflow-x: auto;
    box-shadow: 0 -18px 40px rgba(0, 0, 0, 0.25);
    background:
      linear-gradient(180deg, rgba(24, 24, 27, 0.92), rgba(12, 12, 14, 0.98)),
      var(--bg-secondary);
    backdrop-filter: blur(18px);
  }

  .stage-tab {
    min-width: 148px;
    min-height: 64px;
    flex: 1 0 148px;
    justify-content: flex-start;
    gap: 10px;
    padding: 14px 12px;
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

  .stage-tab.done .stage-status {
    color: var(--accent-primary);
  }

  .stage-text {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    line-height: 1.1;
  }

  .stage-text small {
    color: var(--text-muted);
    font-size: 11px;
  }

  .rail-line {
    width: 10px;
    align-self: center;
    border-top: 1px solid var(--border-color);
  }

  .skeleton-page {
    padding: 24px;
    min-height: 72vh;
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
    .stage-shell {
      min-height: 72vh;
    }

    .brief-grid {
      grid-template-columns: 1fr;
    }
  }
</style>
