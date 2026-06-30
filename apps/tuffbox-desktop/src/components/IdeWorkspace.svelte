<script lang="ts">
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
    Circle,
    ArrowRight,
  } from "lucide-svelte";
  import { projectPath, projectInfo } from "../lib/store";
  import ProjectSettings from "./ProjectSettings.svelte";
  import Mods from "./Mods.svelte";
  import Graph from "./Graph.svelte";
  import ConfigEditor from "./ConfigEditor.svelte";
  import Diagnostics from "./Diagnostics.svelte";
  import Snapshots from "./Snapshots.svelte";

  type StageId =
    | "brief"
    | "setup"
    | "content"
    | "resolve"
    | "configs"
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

  $: activeIndex = stages.findIndex((stage) => stage.id === activeStage);
  $: active = stages[activeIndex] ?? stages[0];
  $: completed = new Set(stages.slice(0, Math.max(activeIndex, 0)).map((stage) => stage.id));

  function goNext() {
    const next = stages[Math.min(activeIndex + 1, stages.length - 1)];
    activeStage = next.id;
  }
</script>

<div class="ide-workspace">
  <header class="ide-hero">
    <div>
      <span class="eyebrow">TuffBox IDE workflow</span>
      <h1>{active.label}</h1>
      <p>{active.goal}</p>
    </div>
    <div class="project-context">
      {#if $projectInfo}
        <strong>{$projectInfo.name}</strong>
        <span>{$projectInfo.minecraftVersion} · {$projectInfo.loaderKind} {$projectInfo.loaderVersion}</span>
      {:else}
        <strong>No project opened</strong>
        <span>Open or create an instance from the launcher Home page.</span>
      {/if}
    </div>
  </header>

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

  <section class="stage-shell">
    <aside class="stage-brief">
      <h2>{active.label} outputs</h2>
      <ul>
        {#each active.outputs as output}
          <li>{output}</li>
        {/each}
      </ul>
      <button class="secondary next" on:click={goNext} disabled={activeIndex === stages.length - 1}>
        Next stage
        <ArrowRight size={16} />
      </button>
    </aside>

    <div class="stage-content">
      {#if activeStage === "brief"}
        <div class="skeleton-page">
          <h2>Pack brief</h2>
          <p>This is the pre-production page. It will store the design intent before any heavy dependency work starts.</p>
          <div class="brief-grid">
            <label>Pack goal<textarea placeholder="Example: low-end-friendly tech + exploration Fabric pack for 1.21.x" /></label>
            <label>Target player<textarea placeholder="Developers, server owners, casual players, low-end PCs..." /></label>
            <label>Gameplay pillars<textarea placeholder="Performance, progression, QoL, server safety..." /></label>
            <label>Hard constraints<textarea placeholder="No client-only mods in server profile, Java version, memory budget..." /></label>
          </div>
        </div>
      {:else if activeStage === "setup"}
        {#if $projectPath}
          <ProjectSettings onBack={() => (activeStage = "brief")} />
        {:else}
          <div class="skeleton-page">
            <h2>No project opened</h2>
            <p>Go to Home, create or open an instance, then return to the IDE workflow.</p>
          </div>
        {/if}
      {:else if activeStage === "content"}
        <Mods />
      {:else if activeStage === "resolve"}
        <Graph />
      {:else if activeStage === "configs"}
        <ConfigEditor />
      {:else if activeStage === "test"}
        <div class="skeleton-page">
          <h2>Test matrix</h2>
          <p>Planned page for client/server/dev/release runs, startup timing, latest.log tail and result history.</p>
          <div class="cards">
            <div><strong>Client smoke test</strong><span>Launch the selected client profile and watch latest.log.</span></div>
            <div><strong>Server dry run</strong><span>Build a temporary server profile and detect client-only mods.</span></div>
            <div><strong>Low-end profile</strong><span>Validate memory budget and optimization settings.</span></div>
          </div>
        </div>
      {:else if activeStage === "diagnose"}
        <Diagnostics />
      {:else if activeStage === "snapshots"}
        <Snapshots />
      {:else if activeStage === "export"}
        <div class="skeleton-page">
          <h2>Export builder</h2>
          <p>Planned page for `.mrpack`, Prism instance zip, CurseForge zip, server pack and generated changelog.</p>
          <div class="cards">
            <div><strong>.mrpack</strong><span>Modrinth-compatible manifest + overrides.</span></div>
            <div><strong>Server pack</strong><span>Server-only file set with install instructions.</span></div>
            <div><strong>Prism zip</strong><span>Importable developer/testing instance.</span></div>
          </div>
        </div>
      {:else if activeStage === "release"}
        <div class="skeleton-page">
          <h2>Release room</h2>
          <p>Planned page for release snapshot, changelog review, Modrinth draft publishing and hotfix tracking.</p>
          <div class="cards">
            <div><strong>Release snapshot</strong><span>Freeze exact manifest/lockfile state.</span></div>
            <div><strong>Changelog</strong><span>Summarize dependency/config changes since previous release.</span></div>
            <div><strong>Post-release support</strong><span>Collect crash reports and plan hotfixes.</span></div>
          </div>
        </div>
      {/if}
    </div>
  </section>
</div>

<style>
  .ide-workspace {
    max-width: 1600px;
  }

  .ide-hero,
  .workflow-rail,
  .stage-shell,
  .skeleton-page,
  .stage-brief,
  .project-context {
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-lg);
    background: var(--bg-secondary);
  }

  .ide-hero {
    display: flex;
    justify-content: space-between;
    gap: 20px;
    padding: 22px 24px;
    margin-bottom: 16px;
    background:
      radial-gradient(circle at top left, rgba(27, 217, 106, 0.13), transparent 35%),
      var(--bg-secondary);
  }

  .eyebrow {
    color: var(--accent-primary);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    font-weight: 800;
  }

  h1 {
    margin: 4px 0;
    font-size: 32px;
  }

  .ide-hero p,
  .project-context span,
  .skeleton-page p,
  .cards span {
    color: var(--text-muted);
  }

  .project-context {
    min-width: 260px;
    padding: 14px;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 4px;
    background: rgba(255, 255, 255, 0.02);
  }

  .workflow-rail {
    display: flex;
    align-items: stretch;
    gap: 0;
    padding: 10px;
    margin-bottom: 16px;
    overflow-x: auto;
  }

  .stage-tab {
    min-width: 132px;
    flex: 1 0 132px;
    justify-content: flex-start;
    gap: 8px;
    padding: 10px;
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
    width: 12px;
    align-self: center;
    border-top: 1px solid var(--border-color);
  }

  .stage-shell {
    display: grid;
    grid-template-columns: 240px minmax(0, 1fr);
    gap: 16px;
    padding: 16px;
    background: rgba(255, 255, 255, 0.015);
  }

  .stage-brief {
    padding: 16px;
    align-self: start;
    position: sticky;
    top: 0;
  }

  .stage-brief h2 {
    font-size: 14px;
    margin-bottom: 12px;
  }

  .stage-brief ul {
    list-style: none;
    display: grid;
    gap: 8px;
    margin-bottom: 16px;
  }

  .stage-brief li {
    color: var(--text-secondary);
    padding: 8px 10px;
    border-radius: 10px;
    background: var(--bg-tertiary);
  }

  .next {
    width: 100%;
  }

  .stage-content {
    min-width: 0;
  }

  .skeleton-page {
    padding: 22px;
    min-height: 520px;
  }

  .skeleton-page h2 {
    margin-bottom: 8px;
  }

  .brief-grid,
  .cards {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 14px;
    margin-top: 18px;
  }

  label,
  .cards div {
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

  .cards div {
    padding: 16px;
    border-radius: var(--border-radius-lg);
    border: 1px solid var(--border-color);
    background: var(--bg-tertiary);
  }

  @media (max-width: 1100px) {
    .ide-hero,
    .stage-shell {
      grid-template-columns: 1fr;
      flex-direction: column;
    }

    .stage-shell {
      display: block;
    }

    .stage-brief {
      position: static;
      margin-bottom: 16px;
    }
  }
</style>
