<script lang="ts">
  import { Plus, X, MoveUp, MoveDown, ImagePlus } from "@lucide/svelte";
  import type { ListingGalleryItem } from "../../lib/api";

  let {
    items = [],
    urls = {},
    onAddFile = () => {},
    onAddUrl = () => {},
    onDropFiles = () => {},
    onRemove = () => {},
    onMove = () => {},
    onInsert = () => {},
  }: {
    items: ListingGalleryItem[];
    urls: Record<string, string>;
    onAddFile?: () => void;
    onAddUrl?: () => void;
    onDropFiles?: (e: DragEvent) => void;
    onRemove?: (index: number) => void;
    onMove?: (from: number, to: number) => void;
    onInsert?: (item: ListingGalleryItem) => void;
  } = $props();

  function src(item: ListingGalleryItem): string | null {
    if (item.url) return item.url;
    if (item.path && urls[item.path]) return urls[item.path];
    return null;
  }

  function onDrop(e: DragEvent) {
    e.preventDefault();
    if (e.dataTransfer?.files?.length) onDropFiles(e);
  }

  function onDragOver(e: DragEvent) {
    e.preventDefault();
  }
</script>

<div class="gallery-root">
  <!-- Dropzone → pick file(s) or drop them -->
  <button
    type="button"
    class="dropzone"
    onclick={onAddFile}
    ondragenter={(e) => e.preventDefault()}
    ondragover={onDragOver}
    ondrop={onDrop}
  >
    <ImagePlus size={22} />
    <span class="dz-title">Drop screenshots here or click to choose</span>
    <span class="dz-sub">PNG / WebP / JPG , dragged or pasted images land here too</span>
  </button>

  {#if items.length > 0}
    <div class="gallery-grid">
      {#each items as item, i (item.path || item.url || `g-${i}`)}
        <div class="gal-tile">
          <div class="gal-frame">
            {#if src(item)}
              <img src={src(item)} alt={item.caption || "Gallery screenshot"} />
            {:else}
              <div class="gal-ph">?</div>
            {/if}
            <div class="gal-overlay">
              <button
                type="button"
                class="ov-btn"
                title="Insert into description"
                onclick={() => onInsert(item)}
              >
                Insert
              </button>
              <div class="ov-divider" aria-hidden="true"></div>
              <button
                type="button"
                class="ov-btn icon"
                title="Move earlier"
                disabled={i === 0}
                onclick={() => onMove(i, i - 1)}
              >
                <MoveUp size={14} />
              </button>
              <button
                type="button"
                class="ov-btn icon"
                title="Move later"
                disabled={i === items.length - 1}
                onclick={() => onMove(i, i + 1)}
              >
                <MoveDown size={14} />
              </button>
              <div class="ov-divider" aria-hidden="true"></div>
              <button
                type="button"
                class="ov-btn icon danger"
                title="Remove"
                onclick={() => onRemove(i)}
              >
                <X size={14} />
              </button>
            </div>
          </div>
          {#if item.caption}
            <span class="gal-caption">{item.caption}</span>
          {/if}
        </div>
      {/each}
      <button type="button" class="add-tile" onclick={onAddUrl} title="Add from URL">
        <Plus size={18} />
        <span class="add-tile-label">From URL</span>
      </button>
    </div>
  {:else}
    <p class="gallery-empty-hint">No gallery images yet. Add files, URLs, or paste from clipboard.</p>
  {/if}
</div>

<style>
  .gallery-root {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .dropzone {
    width: 100%;
    min-height: 108px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 18px;
    border: 1.5px dashed var(--border-color);
    border-radius: var(--border-radius-md);
    background: color-mix(in srgb, var(--bg-secondary) 30%, transparent);
    color: var(--text-muted);
    cursor: pointer;
    transition: border-color var(--motion-fast, 160ms) ease, background var(--motion-fast, 160ms) ease, color var(--motion-fast, 160ms) ease;
  }
  .dropzone:hover {
    border-color: color-mix(in srgb, var(--accent-primary) 50%, transparent);
    background: color-mix(in srgb, var(--accent-primary) 5%, transparent);
    color: var(--text-primary);
  }
  .dropzone :global(svg) {
    color: var(--text-muted);
    transition: color var(--motion-fast, 160ms) ease;
  }
  .dropzone:hover :global(svg) {
    color: var(--accent-primary);
  }
  .dz-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-primary);
  }
  .dz-sub {
    font-size: 11.5px;
    color: var(--text-muted);
  }

  .gallery-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(168px, 1fr));
    gap: 12px;
  }

  .gal-tile {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }

  .gal-frame {
    position: relative;
    aspect-ratio: 16 / 10;
    border-radius: var(--border-radius-md);
    overflow: hidden;
    border: 1px solid var(--border-color);
    background: color-mix(in srgb, var(--bg-secondary) 30%, transparent);
  }
  .gal-frame img,
  .gal-ph {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .gal-ph {
    display: grid;
    place-items: center;
    color: var(--text-muted);
    font-size: 20px;
  }

  .gal-overlay {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 4px;
    background: linear-gradient(0deg, rgba(5, 8, 12, 0.72), rgba(5, 8, 12, 0.12) 55%, transparent);
    opacity: 0;
    transition: opacity var(--motion-fast, 160ms) ease;
  }
  .gal-tile:hover .gal-overlay {
    opacity: 1;
  }
  .ov-divider {
    width: 1px;
    height: 20px;
    background: rgba(255, 255, 255, 0.16);
  }
  .ov-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    height: 28px;
    padding: 0 10px;
    border: 1px solid rgba(255, 255, 255, 0.14);
    border-radius: var(--border-radius-sm);
    background: rgba(15, 18, 24, 0.72);
    color: #e7ebf2;
    font-size: 11.5px;
    font-weight: 600;
    cursor: pointer;
    backdrop-filter: blur(6px);
  }
  .ov-btn.icon {
    width: 28px;
    padding: 0;
  }
  .ov-btn:disabled {
    opacity: 0.35;
    cursor: not-allowed;
  }
  .ov-btn.danger {
    color: #f87171;
  }
  .ov-btn.danger:hover:not(:disabled) {
    background: rgba(239, 68, 68, 0.18);
  }

  .gal-caption {
    font-size: 11.5px;
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .add-tile {
      min-height: 100%;
      aspect-ratio: 16 / 10;
      display: flex;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      gap: 4px;
      border: 1px dashed var(--border-color);
      border-radius: var(--border-radius-md);
      background: color-mix(in srgb, var(--bg-secondary) 20%, transparent);
      color: var(--text-muted);
      cursor: pointer;
      transition: border-color var(--motion-fast, 160ms) ease, color var(--motion-fast, 160ms) ease;
    }
    .add-tile:hover {
      border-color: color-mix(in srgb, var(--accent-primary) 50%, transparent);
      color: var(--accent-primary);
    }
  .add-tile-label {
    font-size: 11.5px;
    font-weight: 600;
  }

  .gallery-empty-hint {
    font-size: 12.5px;
    color: var(--text-muted);
    margin: 0;
  }
</style>