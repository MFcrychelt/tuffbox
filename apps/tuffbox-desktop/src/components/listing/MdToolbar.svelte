<script lang="ts">
  import { Bold, Italic, Heading, Link, ImagePlus, FolderUp } from "@lucide/svelte";

  type Props = {
    onBold: () => void;
    onItalic: () => void;
    onHeading: () => void;
    onLink: () => void;
    onImageUrl: () => void;
    onLocalImage: () => void;
  };

  let { onBold, onItalic, onHeading, onLink, onImageUrl, onLocalImage }: Props = $props();
</script>

<div class="md-toolbar" role="toolbar" aria-label="Markdown formatting">
  <button type="button" class="tool-btn" onclick={onBold} title="Bold">
    <Bold size={15} />
    <span>Bold</span>
  </button>
  <button type="button" class="tool-btn" onclick={onItalic} title="Italic">
    <Italic size={15} />
    <span>Italic</span>
  </button>
  <button type="button" class="tool-btn" onclick={onHeading} title="Insert heading">
    <Heading size={15} />
    <span>Heading</span>
  </button>
  <span class="tool-sep" aria-hidden="true"></span>
  <button type="button" class="tool-btn" onclick={onLink} title="Insert link">
    <Link size={15} />
    <span>Link</span>
  </button>
  <button type="button" class="tool-btn" onclick={onImageUrl} title="Insert image from URL">
    <ImagePlus size={15} />
    <span>Image URL</span>
  </button>
  <button type="button" class="tool-btn" onclick={onLocalImage} title="Insert local image (added to gallery)">
    <FolderUp size={15} />
    <span>Local image</span>
  </button>
</div>

<style>
  .md-toolbar {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    padding: 8px 10px;
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-md);
    background: color-mix(in srgb, var(--bg-secondary) 30%, transparent);
  }
  .tool-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 30px;
    padding: 0 10px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--border-radius-sm);
    background: rgba(255, 255, 255, 0.04);
    color: var(--text-secondary);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    transition: background var(--motion-fast, 160ms) ease, border-color var(--motion-fast, 160ms) ease;
  }
  .tool-btn:hover {
    background: rgba(255, 255, 255, 0.09);
    border-color: rgba(255, 255, 255, 0.18);
    color: #fff;
  }
  .tool-sep {
    width: 1px;
    height: 20px;
    background: var(--border-color);
    margin: 0 2px;
  }
</style>
