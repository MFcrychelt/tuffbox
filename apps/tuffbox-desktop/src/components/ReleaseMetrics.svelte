<script lang="ts">
  import { Tag, GitBranch, Boxes } from "@lucide/svelte";
  let { version = $bindable("1.0.0"), changedMods = 0, components = 0, onSave, onIncrement }: {
    version?: string;
    changedMods?: number;
    components?: number;
    onSave: () => void;
    onIncrement: (part: "patch" | "minor" | "major") => void;
  } = $props();
</script>

<!-- Theme tokens, not the hardcoded dark palette: text-neutral-100 numbers
     measured 1.07:1 on the light themes and the emerald Save button 3.65:1
     (docs/problems.md §16/§20). -->
<section class="grid gap-4">
  <div class="grid grid-cols-1 gap-3 sm:grid-cols-3">
    <div class="rounded-xl border border-[var(--border-color)] bg-[var(--bg-secondary)] p-3"><div class="flex items-center gap-2 text-[var(--accent-primary)]"><Tag size={16}/><span class="text-[12px] uppercase tracking-wide text-[var(--text-secondary)]">Version</span></div><strong class="mt-2 block text-lg text-[var(--text-primary)]">{version || "—"}</strong><span class="text-xs text-[var(--text-secondary)]">SemVer tag</span></div>
    <div class="rounded-xl border border-[var(--border-color)] bg-[var(--bg-secondary)] p-3"><div class="flex items-center gap-2 text-[var(--accent-secondary)]"><GitBranch size={16}/><span class="text-[12px] uppercase tracking-wide text-[var(--text-secondary)]">Changed mods</span></div><strong class="mt-2 block text-lg text-[var(--text-primary)]">{changedMods}</strong><span class="text-xs text-[var(--text-secondary)]">since last release</span></div>
    <div class="rounded-xl border border-[var(--border-color)] bg-[var(--bg-secondary)] p-3"><div class="flex items-center gap-2 text-[var(--accent-warning)]"><Boxes size={16}/><span class="text-[12px] uppercase tracking-wide text-[var(--text-secondary)]">Components</span></div><strong class="mt-2 block text-lg text-[var(--text-primary)]">{components}</strong><span class="text-xs text-[var(--text-secondary)]">in changelog</span></div>
  </div>
  <div class="flex flex-wrap gap-2 rounded-xl border border-[var(--border-color)] bg-[var(--bg-tertiary)] p-2">
    <input class="min-w-[150px] flex-1 bg-[var(--bg-primary)] font-mono text-sm text-[var(--text-primary)]" bind:value={version} placeholder="Major.Minor.Patch" aria-label="Release version" />
    <button type="button" class="rounded-lg border border-[var(--border-color)] px-2.5 py-2 text-xs text-[var(--text-secondary)] hover:bg-[var(--bg-hover)]" onclick={() => onIncrement("patch")}>+Patch</button>
    <button type="button" class="rounded-lg border border-[var(--border-color)] px-2.5 py-2 text-xs text-[var(--text-secondary)] hover:bg-[var(--bg-hover)]" onclick={() => onIncrement("minor")}>+Minor</button>
    <button type="button" class="rounded-lg border border-[var(--border-color)] px-2.5 py-2 text-xs text-[var(--text-secondary)] hover:bg-[var(--bg-hover)]" onclick={() => onIncrement("major")}>+Major</button>
    <button type="button" class="rounded-lg bg-[var(--accent-primary)] px-3 py-2 text-xs font-semibold text-[var(--on-accent)]" onclick={onSave}>Save version</button>
  </div>
</section>
