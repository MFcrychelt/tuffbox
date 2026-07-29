<script lang="ts">
  export let style: "modrinth" | "curseforge" = "modrinth";
  export let name = "Untitled pack";
  export let summary = "";
  export let authors: string[] = [];
  export let categories: string[] = [];
  export let iconUrl: string | null = null;
  export let minecraftVersion: string | null = null;
  export let loaderKind: string | null = null;
  export let version: string | null = null;

  $: authorLabel = authors.filter(Boolean).join(", ") || "Unknown author";
  $: catChips = categories.filter(Boolean).slice(0, 6);
</script>

{#if style === "modrinth"}
  <article class="mr-card" aria-label="Modrinth-style listing preview">
    <div class="mr-icon">
      {#if iconUrl}
        <img src={iconUrl} alt="" />
      {:else}
        <div class="icon-ph">?</div>
      {/if}
    </div>
    <div class="mr-body">
      <div class="mr-title-row">
        <h3>{name || "Untitled pack"}</h3>
        <span class="mr-dl" title="Placeholder">↓ —</span>
      </div>
      <p class="mr-summary">{summary || "No summary yet."}</p>
      <div class="mr-meta">
        {#if catChips.length}
          <div class="chips">
            {#each catChips as c (c)}
              <span class="chip">{c}</span>
            {/each}
          </div>
        {/if}
        <div class="tags">
          {#if version}<span class="tag">{version}</span>{/if}
          {#if minecraftVersion}<span class="tag">MC {minecraftVersion}</span>{/if}
          {#if loaderKind}<span class="tag">{loaderKind}</span>{/if}
        </div>
      </div>
    </div>
  </article>
{:else}
  <article class="cf-card" aria-label="CurseForge-style listing preview">
    <div class="cf-icon">
      {#if iconUrl}
        <img src={iconUrl} alt="" />
      {:else}
        <div class="icon-ph">?</div>
      {/if}
    </div>
    <div class="cf-body">
      <h3>{name || "Untitled pack"}</h3>
      <p class="cf-summary">{summary || "No summary yet."}</p>
      <div class="cf-foot">
        <span class="cf-author">By {authorLabel}</span>
        {#if minecraftVersion}<span class="cf-mc">{minecraftVersion}</span>{/if}
      </div>
    </div>
  </article>
{/if}

<style>
  .mr-card,
  .cf-card {
    display: grid;
    grid-template-columns: 96px 1fr;
    gap: 14px;
    padding: 14px;
    border-radius: 12px;
    border: 1px solid var(--border-color);
    background: var(--bg-elevated);
  }

  .mr-card {
    background: linear-gradient(145deg, rgba(30, 181, 116, 0.08), var(--bg-elevated) 55%);
    border-color: rgba(30, 181, 116, 0.28);
  }

  .cf-card {
    background: linear-gradient(145deg, rgba(241, 100, 36, 0.1), var(--bg-elevated) 55%);
    border-color: rgba(241, 100, 36, 0.3);
  }

  .mr-icon,
  .cf-icon {
    width: 96px;
    height: 96px;
    border-radius: 10px;
    overflow: hidden;
    background: var(--bg-tertiary);
    border: 1px solid var(--border-color);
  }

  .mr-icon img,
  .cf-icon img,
  .icon-ph {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: grid;
    place-items: center;
    color: var(--text-muted);
    font-size: 28px;
    font-weight: 700;
  }

  .mr-body,
  .cf-body {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  h3 {
    margin: 0;
    font-size: 16px;
    color: var(--text-primary);
    line-height: 1.25;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .mr-title-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .mr-dl {
    color: var(--text-muted);
    font-size: 12px;
    flex: 0 0 auto;
  }

  .mr-summary,
  .cf-summary {
    margin: 0;
    color: var(--text-secondary);
    font-size: 13px;
    line-height: 1.4;
    display: -webkit-box;
    -webkit-line-clamp: 3;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .mr-meta {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: auto;
  }

  .chips,
  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .chip,
  .tag,
  .cf-mc {
    font-size: 11px;
    padding: 2px 7px;
    border-radius: 999px;
    border: 1px solid var(--border-color);
    color: var(--text-secondary);
    background: var(--bg-tertiary);
  }

  .chip {
    border-color: rgba(30, 181, 116, 0.35);
    color: #9ae6c0;
  }

  .cf-foot {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    margin-top: auto;
    font-size: 12px;
    color: var(--text-muted);
  }

  .cf-author {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .cf-mc {
    flex: 0 0 auto;
    border-color: rgba(241, 100, 36, 0.4);
    color: #fdba8c;
  }
</style>
