<script lang="ts">
  import { RefreshCw, ShieldAlert } from "@lucide/svelte";
  import { untrack } from "svelte";
  import { api } from "../lib/api";
  import { projectPath } from "../lib/store";

  type Change = Record<string, unknown>;
  type Preview = {
    repo: string;
    installedVersion: string;
    incomingVersion: string;
    requiresFullReinstall: boolean;
    customFiles: boolean;
    signerState?: "ok" | "changed" | "unsigned";
    changes: Change[];
  };

  let checking = $state(false);
  let applying = $state(false);
  let error = $state("");
  let updateAvailable = $state(false);
  let preview = $state<Preview | null>(null);
  let snapshotId = $state("");

  async function check() {
    if (!$projectPath) return;
    checking = true;
    error = "";
    try {
      const status = await api.transport.github.checkUpdate($projectPath);
      updateAvailable = !!status.updateAvailable;
      if (updateAvailable) {
        preview = await api.transport.github.previewUpdate($projectPath);
      } else {
        preview = null;
      }
    } catch (e) {
      const text = String(e);
      if (text.includes("not a GitHub pack")) {
        updateAvailable = false;
        preview = null;
      } else {
        error = text;
      }
    } finally {
      checking = false;
    }
  }

  $effect(() => {
    void $projectPath;
    untrack(() => {
      void check();
    });
  });

  function asObj(value: unknown): Record<string, unknown> | null {
    return value && typeof value === "object" ? (value as Record<string, unknown>) : null;
  }

  function originNote(origin: unknown): string {
    return origin === "custom" ? " · custom file" : "";
  }

  function changeLabel(change: Change): string {
    const added = asObj(change.modAdded);
    if (added) return `Added ${added.id} ${added.version}${originNote(added.origin)}`;
    const removed = asObj(change.modRemoved);
    if (removed) return `Removed ${removed.id} ${removed.version}`;
    const bumped = asObj(change.modBumped);
    if (bumped) return `Updated ${bumped.id} ${bumped.from} → ${bumped.to}${originNote(bumped.origin)}`;
    const mc = asObj(change.minecraftChanged);
    if (mc) return `Minecraft ${mc.from} → ${mc.to}`;
    const loader = asObj(change.loaderChanged);
    if (loader) return `Loader ${loader.from} → ${loader.to}`;
    if (change.overridesChanged) return "Override files changed";
    return Object.keys(change)[0] ?? "change";
  }

  async function apply() {
    if (!$projectPath || !preview) return;
    applying = true;
    error = "";
    try {
      const result = await api.transport.github.applyUpdate($projectPath);
      snapshotId = result.snapshotId;
      updateAvailable = false;
      preview = null;
    } catch (e) {
      error = String(e);
    } finally {
      applying = false;
    }
  }
</script>

{#if updateAvailable && preview}
  <section class="update-card">
    <header>
      <RefreshCw size={16} />
      <div>
        <strong>GitHub pack update</strong>
        <span>{preview.installedVersion} → {preview.incomingVersion} · {preview.repo}</span>
      </div>
    </header>
    {#if preview.customFiles || preview.requiresFullReinstall || preview.signerState === "changed" || preview.signerState === "unsigned"}
      <p class="warn">
        <ShieldAlert size={14} />
        {#if preview.signerState === "changed"}
          Author key changed — refusing silent apply. Confirm this is the same author before updating.
        {:else if preview.requiresFullReinstall}
          Minecraft or loader changed — full rematerialize of provider jars.
        {:else if preview.signerState === "unsigned"}
          Incoming pack is unsigned. Review the diff before applying.
        {:else}
          Includes custom files from GitHub. Review before applying.
        {/if}
      </p>
    {/if}
    <ul>
      {#each preview.changes as change, i (i)}
        <li>{changeLabel(change)}</li>
      {/each}
    </ul>
    <div class="actions">
      <button class="mini" onclick={apply} disabled={applying || preview.signerState === "changed"}>{applying ? "Applying…" : preview.requiresFullReinstall ? "Reinstall pack" : "Apply update"}</button>
      <button class="secondary mini" onclick={check} disabled={checking}>Refresh</button>
    </div>
    {#if error}<p class="err">{error}</p>{/if}
  </section>
{:else if snapshotId}
  <p class="ok">Update applied. Rollback snapshot: {snapshotId}</p>
{/if}

<style>
  .update-card {
    display: grid;
    gap: 8px;
    margin: 0 0 12px;
    padding: 12px 14px;
    border: 1px solid color-mix(in srgb, var(--accent-primary) 30%, var(--border-color));
    border-radius: var(--border-radius-md);
    background: color-mix(in srgb, var(--accent-primary) 8%, var(--bg-secondary));
  }
  header { display: flex; gap: 10px; align-items: center; }
  header span, li, .warn, .ok, .err { color: var(--text-muted); font-size: 12px; }
  ul { margin: 0; padding-left: 18px; }
  .actions { display: flex; gap: 8px; }
  .warn { display: flex; gap: 6px; align-items: center; margin: 0; color: #fde68a; }
  .ok { margin: 0 0 12px; color: var(--accent-primary); font-size: 12px; }
  .err { margin: 0; color: #fecaca; }
</style>
