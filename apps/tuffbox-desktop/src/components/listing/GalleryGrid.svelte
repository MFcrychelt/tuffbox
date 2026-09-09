<script lang="ts">
  import { ArrowDown, ArrowUp, Image, Link, Trash2 } from "@lucide/svelte";
  import type { ListingGalleryItem } from "../../lib/api";

  let { items = [], urls = {}, onAddFile, onAddUrl, onDropFiles, onRemove, onMove, onInsert }: {
    items: ListingGalleryItem[];
    urls: Record<string, string>;
    onAddFile: () => void;
    onAddUrl: () => void;
    onDropFiles: (event: DragEvent) => void;
    onRemove: (index: number) => void;
    onMove: (from: number, to: number) => void;
    onInsert: (item: ListingGalleryItem) => void;
  } = $props();

  function preview(item: ListingGalleryItem): string | null {
    return item.path ? urls[item.path] ?? null : item.url ?? null;
  }
</script>

<div class="gallery" role="region" aria-label="Gallery drop zone" tabindex="0" ondragover={(event) => event.preventDefault()} ondrop={onDropFiles}>
  <div class="actions">
    <button type="button" onclick={onAddFile}><Image size={14} /> Add image</button>
    <button type="button" onclick={onAddUrl}><Link size={14} /> Add URL</button>
    <span>Drop images here</span>
  </div>
  {#if items.length === 0}
    <div class="empty">No gallery images yet.</div>
  {:else}
    <div class="grid">
      {#each items as item, index (item.path ?? item.url ?? index)}
        <article>
          <div class="preview">
            {#if preview(item)}<img src={preview(item) ?? ""} alt={item.caption ?? ""} />{:else}<Image size={22} />{/if}
          </div>
          <p title={item.caption ?? item.path ?? item.url ?? ""}>{item.caption || item.path || item.url || "Image"}</p>
          <div class="row">
            <button type="button" title="Insert into Markdown" onclick={() => onInsert(item)}>Insert</button>
            <button type="button" title="Move up" disabled={index === 0} onclick={() => onMove(index, index - 1)}><ArrowUp size={13} /></button>
            <button type="button" title="Move down" disabled={index === items.length - 1} onclick={() => onMove(index, index + 1)}><ArrowDown size={13} /></button>
            <button type="button" title="Remove" onclick={() => onRemove(index)}><Trash2 size={13} /></button>
          </div>
        </article>
      {/each}
    </div>
  {/if}
</div>

<style>
  .gallery { display: grid; gap: 10px; }
  .actions, .row { display: flex; flex-wrap: wrap; align-items: center; gap: 6px; }
  .actions span { margin-left: auto; color: var(--text-muted); font-size: 11px; }
  button { display: inline-flex; align-items: center; gap: 5px; padding: 5px 7px; color: var(--text-secondary); background: transparent; border: 1px solid var(--border-color); border-radius: var(--border-radius-sm); cursor: pointer; font-size: 11px; }
  button:hover:not(:disabled) { color: var(--text-primary); background: var(--bg-hover); }
  button:disabled { opacity: .4; cursor: default; }
  .grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: 10px; }
  article { min-width: 0; overflow: hidden; padding: 8px; border: 1px solid var(--border-color); border-radius: var(--border-radius-md); background: var(--bg-primary); }
  .preview { display: grid; place-items: center; height: 96px; overflow: hidden; color: var(--text-muted); background: var(--bg-tertiary); border-radius: var(--border-radius-sm); }
  img { width: 100%; height: 100%; object-fit: cover; }
  p { margin: 7px 0; overflow: hidden; color: var(--text-muted); font-size: 10px; text-overflow: ellipsis; white-space: nowrap; }
  .empty { padding: 22px; color: var(--text-muted); text-align: center; border: 1px dashed var(--border-color); border-radius: var(--border-radius-md); font-size: 12px; }
</style>
