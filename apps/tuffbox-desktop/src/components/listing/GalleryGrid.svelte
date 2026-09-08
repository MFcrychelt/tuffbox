<script lang="ts">
  import { Plus, Link2, Trash2, ArrowLeft, ArrowRight, CornerDownLeft } from "@lucide/svelte";
  import type { ListingGalleryItem } from "../../lib/api";

  type Props = {
    items: ListingGalleryItem[];
    urls: Record<string, string>;
    onAddFile: () => void;
    onAddUrl: () => void;
    onDropFiles: (e: DragEvent) => void;
    onRemove: (index: number) => void;
    onMove: (from: number, to: number) => void;
    onInsert: (item: ListingGalleryItem) => void;
  };

  let { items, urls, onAddFile, onAddUrl, onDropFiles, onRemove, onMove, onInsert }: Props = $props();

  function thumbSrc(item: ListingGalleryItem): string {
    if (item.path && urls[item.path]) return urls[item.path];
    return item.url ?? "";
  }

  function thumbLabel(item: ListingGalleryItem): string {
    return item.caption?.trim() || item.path || item.url || "image";
  }
</script>

<div
  class="gallery-drop"
  ondragover={(e) => e.preventDefault()}
  ondrop={(e) => onDropFiles(e)}
  role="group"
  aria-label="Listing gallery"
>
  <div class="gallery-actions">
    <button type="button" class="sm-btn" onclick={onAddFile}>
      <Plus size={14} /> Add file…
    </button>
    <button type="button" class="sm-btn" onclick={onAddUrl}>
      <Link2 size={14} /> Add URL…
    </button>
    <small class="hint">…or drop images anywhere here</small>
  </div>

  {#if items.length === 0}
    <div class="gallery-empty">No gallery images yet.</div>
  {:else}
    <div class="gallery-grid">
      {#each items as item, i (i)}
        <figure class="gallery-cell">
          {#if thumbSrc(item)}
            <img src={thumbSrc(item)} alt={thumbLabel(item)} loading="lazy" />
          {:else}
            <div class="thumb-missing" title={thumbLabel(item)}>No preview</div>
          {/if}
          <div class="cell-actions">
            <button
              type="button"
              class="icon-btn"
              onclick={() => onInsert(item)}
              title="Insert into description"
              aria-label="Insert into description"
            >
              <CornerDownLeft size={14} />
            </button>
            <button
              type="button"
              class="icon-btn"
              onclick={() => onMove(i, i - 1)}
              disabled={i === 0}
              title="Move left"
              aria-label="Move left"
            >
              <ArrowLeft size={14} />
            </button>
            <button
              type="button"
              class="icon-btn"
              onclick={() => onMove(i, i + 1)}
              disabled={i === items.length - 1}
              title="Move right"
              aria-label="Move right"
            >
              <ArrowRight size={14} />
            </button>
            <button
              type="button"
              class="icon-btn danger"
              onclick={() => onRemove(i)}
              title="Remove from gallery"
              aria-label="Remove from gallery"
            >
              <Trash2 size={14} />
            </button>
          </div>
          <figcaption class="cell-label" title={thumbLabel(item)}>{thumbLabel(item)}</figcaption>
        </figure>
      {/each}
    </div>
  {/if}
</div>

<style>
  .gallery-drop {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .gallery-actions {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px;
  }
  .sm-btn {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 32px;
    padding: 0 11px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--border-radius-sm);
    background: rgba(255, 255, 255, 0.04);
    color: var(--text-secondary);
    font-size: 12.5px;
    font-weight: 600;
    cursor: pointer;
    transition: background var(--motion-fast, 160ms) ease, border-color var(--motion-fast, 160ms) ease;
  }
  .sm-btn:hover {
    background: rgba(255, 255, 255, 0.09);
    border-color: rgba(255, 255, 255, 0.18);
    color: #fff;
  }
  .hint {
    color: var(--text-muted);
    font-size: 12px;
  }
  .gallery-empty {
    padding: 18px;
    text-align: center;
    color: var(--text-muted);
    border: 1px dashed var(--border-color);
    border-radius: var(--border-radius-md);
    font-size: 12.5px;
  }
  .gallery-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
    gap: 10px;
  }
  .gallery-cell {
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px;
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-md);
    background: rgba(0, 0, 0, 0.2);
    min-width: 0;
  }
  .gallery-cell img {
    width: 100%;
    aspect-ratio: 16 / 10;
    object-fit: cover;
    border-radius: var(--border-radius-sm);
    background: rgba(255, 255, 255, 0.04);
    order: 0;
  }
  .thumb-missing {
    width: 100%;
    aspect-ratio: 16 / 10;
    display: grid;
    place-items: center;
    border-radius: var(--border-radius-sm);
    background: rgba(255, 255, 255, 0.04);
    color: var(--text-muted);
    font-size: 11px;
    order: 0;
  }
  .cell-label {
    font-size: 11px;
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .cell-actions {
    display: flex;
    gap: 4px;
  }
  .icon-btn {
    display: inline-grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: var(--border-radius-sm);
    background: rgba(255, 255, 255, 0.04);
    color: var(--text-secondary);
    cursor: pointer;
  }
  .icon-btn:hover:not(:disabled) {
    background: rgba(255, 255, 255, 0.1);
    color: #fff;
  }
  .icon-btn:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }
  .icon-btn.danger:hover:not(:disabled) {
    background: rgba(239, 68, 68, 0.2);
    border-color: rgba(239, 68, 68, 0.4);
    color: #fca5a5;
  }
</style>
