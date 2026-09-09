<script lang="ts">
  import { Bold, Italic, Heading, Link, Image as ImageIcon, FilePlus2 } from "@lucide/svelte";

  let {
    onBold = () => {},
    onItalic = () => {},
    onHeading = () => {},
    onLink = () => {},
    onImageUrl = () => {},
    onLocalImage = () => {},
  }: {
    onBold?: () => void;
    onItalic?: () => void;
    onHeading?: () => void;
    onLink?: () => void;
    onImageUrl?: () => void;
    onLocalImage?: () => void;
  } = $props();
</script>

<nav
  class="inline-flex flex-wrap items-center gap-1 px-1.5 py-1 rounded-xl border border-[color:var(--border-color)] bg-[color:color-mix(in_srgb,var(--bg-secondary)_45%,transparent)] backdrop-blur-md shadow-[inset_0_1px_0_var(--border-color)]"
  aria-label="Markdown formatting"
>
  <button type="button" class="md-btn" title="Bold" onclick={onBold}><Bold size={14} /></button>
  <button type="button" class="md-btn" title="Italic" onclick={onItalic}><Italic size={14} /></button>
  <button type="button" class="md-btn" title="Heading" onclick={onHeading}><Heading size={14} /></button>
  <span class="mx-0.5 h-4 w-px bg-[color:var(--border-color)]"></span>
  <button type="button" class="md-btn" title="Link" onclick={onLink}><Link size={14} /></button>
  <button type="button" class="md-btn" title="Image URL" onclick={onImageUrl}><ImageIcon size={14} /></button>
  <button type="button" class="md-btn" title="Insert local image" onclick={onLocalImage}><FilePlus2 size={14} /></button>
</nav>

<style>
  .md-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    border: none;
    background: transparent;
    color: var(--text-muted);
    border-radius: var(--border-radius-sm);
    cursor: pointer;
    transition: background var(--motion-fast, 160ms) ease, color var(--motion-fast, 160ms) ease;
  }
  .md-btn:hover {
    background: var(--bg-hover);
    color: var(--text-primary);
  }
  .md-btn:active {
    background: color-mix(in srgb, var(--accent-primary) 18%, transparent);
    color: var(--accent-primary);
  }
</style>