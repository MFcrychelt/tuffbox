<script lang="ts">
  import { Sparkles, RefreshCw, Eye, Pencil, ChevronDown } from "@lucide/svelte";
  import { t } from "../lib/i18n";
  let { value = $bindable(""), onRegenerate, onAi, disabled = false }: { value?: string; onRegenerate: () => void; onAi: () => void; disabled?: boolean } = $props();
  let mode = $state<"edit" | "preview">("edit");
  let showUnchanged = $state(false);
  const lines = $derived(value.split("\n").map((line) => line.trim()).filter(Boolean));
  const added = $derived(lines.filter((line) => /^\+|^[-*]\s*(new|added)/i.test(line)));
  const removed = $derived(lines.filter((line) => /^-|removed|deleted/i.test(line) && !/^\+/.test(line)));
  const updated = $derived(lines.filter((line) => !added.includes(line) && !removed.includes(line) && line.length > 0));
  const unchanged = $derived(Math.max(0, updated.length - 8));
</script>

<!-- Themed surface: this panel sits inside ReleaseRoom's own panel, so it must
     follow the active theme instead of the dark-only palette it used to hardcode. -->
<section class="flex min-h-[520px] flex-col overflow-hidden rounded-2xl border border-[var(--border-color)] bg-[var(--bg-tertiary)] shadow-[var(--shadow-md)]">
  <header class="flex flex-wrap items-center justify-between gap-3 border-b border-[var(--border-color)] p-4">
    <div><h2 class="m-0 text-lg font-semibold text-[var(--text-primary)]">Changelog</h2><p class="mt-1 text-xs text-[var(--text-secondary)]">Keep a concise release story instead of a raw dependency dump.</p></div>
    <div class="flex flex-wrap items-center gap-2">
      <div class="flex rounded-lg border border-[var(--border-color)] bg-[var(--bg-elevated)] p-1"><button type="button" class="rounded px-2.5 py-1.5 text-xs {mode === 'edit' ? 'bg-[var(--bg-hover)] text-[var(--text-primary)]' : 'text-[var(--text-secondary)]'}" onclick={() => mode = 'edit'}><Pencil size={13}/> Edit</button><button type="button" class="rounded px-2.5 py-1.5 text-xs {mode === 'preview' ? 'bg-[var(--bg-hover)] text-[var(--text-primary)]' : 'text-[var(--text-secondary)]'}" onclick={() => mode = 'preview'}><Eye size={13}/> Preview</button></div>
      <button type="button" class="inline-flex items-center gap-2 rounded-lg border border-[color-mix(in_srgb,var(--accent-secondary)_25%,transparent)] bg-[color-mix(in_srgb,var(--accent-secondary)_10%,transparent)] px-3 py-2 text-xs text-[var(--accent-secondary)] hover:bg-[color-mix(in_srgb,var(--accent-secondary)_20%,transparent)]" onclick={onAi} disabled={disabled}><Sparkles size={14}/> Generate with AI / Ollama</button>
      <button type="button" class="rounded-lg border border-[var(--border-color)] p-2 text-[var(--text-secondary)] hover:bg-[var(--bg-hover)]" onclick={onRegenerate} disabled={disabled} title="Regenerate"><RefreshCw size={15}/></button>
    </div>
  </header>
  {#if mode === 'edit'}
    <textarea class="min-h-[430px] flex-1 resize-none border-0 bg-transparent p-5 font-mono text-xs leading-6 text-[var(--text-primary)] outline-none focus:ring-0" bind:value spellcheck="false" aria-label="Release changelog"></textarea>
  {:else}
    <div class="flex-1 overflow-y-auto p-5 font-mono text-xs leading-6 text-[var(--text-secondary)]">
      {#if added.length}<h2 class="mb-2 text-base font-bold text-[var(--accent-primary)]">{$t("release.changelogAdded")}</h2>{#each added as item}<div class="rounded px-2 py-1 text-[var(--accent-primary)]">+ {item.replace(/^[-+*]\s*/, '')}</div>{/each}{/if}
      {#if updated.length}<h2 class="mb-2 mt-5 text-base font-bold text-[var(--accent-warning)]">{$t("release.changelogUpdated")}</h2>{#each updated.slice(0, showUnchanged ? updated.length : 8) as item}<div class="rounded px-2 py-1 text-[var(--text-secondary)]">~ {item.replace(/^[-+*]\s*/, '')}</div>{/each}{/if}
      {#if removed.length}<h2 class="mb-2 mt-5 text-base font-bold text-[var(--accent-danger)]">{$t("release.changelogRemoved")}</h2>{#each removed as item}<div class="rounded px-2 py-1 text-[var(--accent-danger)]">- {item.replace(/^[-+*]\s*/, '')}</div>{/each}{/if}
      {#if unchanged > 0}<button type="button" class="mt-4 flex items-center gap-2 text-[var(--text-secondary)] hover:text-[var(--text-primary)]" onclick={() => showUnchanged = !showUnchanged}><ChevronDown size={14}/> {showUnchanged ? $t('release.changelogCollapse') : $t('release.changelogMore', { n: unchanged })}</button>{/if}
    </div>
  {/if}
</section>
