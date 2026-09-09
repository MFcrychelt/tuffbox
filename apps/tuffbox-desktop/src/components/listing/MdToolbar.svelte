<script lang="ts">
  import { Bold, Heading, Image, Italic, Link } from "@lucide/svelte";

  let { onBold, onItalic, onHeading, onLink, onImageUrl, onLocalImage }: {
    onBold: () => void;
    onItalic: () => void;
    onHeading: () => void;
    onLink: () => void;
    onImageUrl: () => void;
    onLocalImage: () => void;
  } = $props();
</script>

<div class="toolbar" aria-label="Markdown toolbar">
  <button type="button" onclick={onBold} title="Bold"><Bold size={14} /></button>
  <button type="button" onclick={onItalic} title="Italic"><Italic size={14} /></button>
  <button type="button" onclick={onHeading} title="Heading"><Heading size={14} /></button>
  <button type="button" onclick={onLink} title="Link"><Link size={14} /></button>
  <button type="button" onclick={onImageUrl} title="Image URL"><Image size={14} /></button>
  <button type="button" onclick={onLocalImage} title="Add local image">Local image</button>
</div>

<style>
  .toolbar { display: flex; flex-wrap: wrap; gap: 5px; padding: 7px; border-bottom: 1px solid var(--border-color); background: color-mix(in srgb, var(--bg-tertiary) 65%, transparent); }
  button { display: inline-flex; align-items: center; gap: 5px; padding: 5px 7px; color: var(--text-secondary); background: transparent; border: 1px solid transparent; border-radius: var(--border-radius-sm); cursor: pointer; font-size: 11px; }
  button:hover, button:focus-visible { color: var(--text-primary); background: var(--bg-hover); border-color: var(--border-color); outline: none; }
</style>
