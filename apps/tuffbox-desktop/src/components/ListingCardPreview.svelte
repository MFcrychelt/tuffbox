<script lang="ts">
  export let style: "modrinth" | "curseforge" = "modrinth";
  export let name = "Untitled pack";
  export let summary = "";
  export let categories: string[] = [];
  export let iconUrl: string | null = null;
  export let minecraftVersion: string | null = null;
  export let loaderKind: string | null = null;
  export let version: string | null = null;
  /** Optional gallery hero for page-style preview */
  export let galleryUrl: string | null = null;
  export let bodyHtml: string | null = null;
  export let variant: "card" | "page" = "card";

  $: catChips = categories.filter(Boolean).slice(0, 8);
  $: loaderLabel = (loaderKind || "").replace(/_/g, " ");

  function prettyCat(c: string) {
    return c
      .split("-")
      .map((p) => (p ? p[0].toUpperCase() + p.slice(1) : p))
      .join(" ");
  }
</script>

{#if style === "modrinth"}
  {#if variant === "page"}
    <article class="mr-page" aria-label="Modrinth-style page preview">
      {#if galleryUrl}
        <div class="mr-hero">
          <img src={galleryUrl} alt="" />
        </div>
      {/if}
      <header class="mr-page-head">
        <div class="mr-page-icon">
          {#if iconUrl}
            <img src={iconUrl} alt="" />
          {:else}
            <div class="icon-ph">?</div>
          {/if}
        </div>
        <div class="mr-page-titles">
          <h2>{name || "Untitled pack"}</h2>
          <p class="mr-summary">{summary || "No summary yet."}</p>
          <div class="mr-page-badges">
            {#if version}<span class="mr-badge">v{version}</span>{/if}
            {#if minecraftVersion}<span class="mr-badge">MC {minecraftVersion}</span>{/if}
            {#if loaderLabel}<span class="mr-badge loader">{loaderLabel}</span>{/if}
          </div>
        </div>
        <div class="mr-page-cta">
          <button type="button" class="mr-dl-btn" tabindex="-1">Download</button>
          <span class="mr-stat">↓ —</span>
          <span class="mr-stat">♡ —</span>
        </div>
      </header>
      {#if catChips.length}
        <div class="chips page-chips">
          {#each catChips as c (c)}
            <span class="chip">{prettyCat(c)}</span>
          {/each}
        </div>
      {/if}
      <div class="mr-page-body prose">
        {#if bodyHtml}
          {@html bodyHtml}
        {:else}
          <p class="muted">Long description preview appears here.</p>
        {/if}
      </div>
    </article>
  {:else}
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
          <div class="mr-stats">
            <span title="Downloads">↓ —</span>
            <span title="Followers">♡ —</span>
          </div>
        </div>
        <p class="mr-summary">{summary || "No summary yet."}</p>
        <div class="mr-meta">
          {#if catChips.length}
            <div class="chips">
              {#each catChips as c (c)}
                <span class="chip">{prettyCat(c)}</span>
              {/each}
            </div>
          {/if}
          <div class="tags">
            {#if version}<span class="tag">v{version}</span>{/if}
            {#if minecraftVersion}<span class="tag">MC {minecraftVersion}</span>{/if}
            {#if loaderLabel}<span class="tag">{loaderLabel}</span>{/if}
          </div>
        </div>
      </div>
    </article>
  {/if}
{:else}
  {#if variant === "page"}
    <article class="cf-page" aria-label="CurseForge-style page preview">
      {#if galleryUrl}
        <div class="cf-hero">
          <img src={galleryUrl} alt="" />
        </div>
      {/if}
      <header class="cf-page-head">
        <div class="cf-page-icon">
          {#if iconUrl}
            <img src={iconUrl} alt="" />
          {:else}
            <div class="icon-ph">?</div>
          {/if}
        </div>
        <div class="cf-page-titles">
          <div class="cf-kicker">Minecraft · Modpacks</div>
          <h2>{name || "Untitled pack"}</h2>
          <p class="cf-summary">{summary || "No summary yet."}</p>
          <div class="cf-page-meta">
            {#if minecraftVersion}<span>{minecraftVersion}</span>{/if}
            {#if loaderLabel}<span>{loaderLabel}</span>{/if}
            {#if version}<span>File {version}</span>{/if}
          </div>
        </div>
        <button type="button" class="cf-install-btn" tabindex="-1">Install</button>
      </header>
      {#if catChips.length}
        <div class="cf-cats">
          {#each catChips as c (c)}
            <span>{prettyCat(c)}</span>
          {/each}
        </div>
      {/if}
      <div class="cf-page-body prose">
        {#if bodyHtml}
          {@html bodyHtml}
        {:else}
          <p class="muted">Long description preview appears here.</p>
        {/if}
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
        <div class="cf-kicker">Modpack</div>
        <h3>{name || "Untitled pack"}</h3>
        <p class="cf-summary">{summary || "No summary yet."}</p>
        <div class="cf-foot">
          <div class="cf-cats inline">
            {#each catChips.slice(0, 3) as c (c)}
              <span>{prettyCat(c)}</span>
            {/each}
          </div>
          {#if minecraftVersion}<span class="cf-mc">{minecraftVersion}</span>{/if}
        </div>
      </div>
    </article>
  {/if}
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
    background: #16181c;
    border-color: #2d323a;
  }

  .cf-card {
    background: #1c1c1f;
    border-color: #3a3a40;
  }

  .mr-icon,
  .cf-icon,
  .mr-page-icon,
  .cf-page-icon {
    width: 96px;
    height: 96px;
    border-radius: 10px;
    overflow: hidden;
    background: #0f1115;
    border: 1px solid #2d323a;
    flex-shrink: 0;
  }

  .mr-page-icon,
  .cf-page-icon {
    width: 72px;
    height: 72px;
    border-radius: 12px;
  }

  .mr-icon img,
  .cf-icon img,
  .mr-page-icon img,
  .cf-page-icon img,
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

  h3,
  h2 {
    margin: 0;
    font-size: 16px;
    color: #ecf0f3;
    line-height: 1.25;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  h2 {
    font-size: 20px;
    white-space: normal;
  }

  .mr-title-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }

  .mr-stats {
    display: inline-flex;
    gap: 8px;
    color: #8b949e;
    font-size: 12px;
    flex: 0 0 auto;
  }

  .mr-summary,
  .cf-summary {
    margin: 0;
    color: #a8b0b9;
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
  .tags,
  .cf-cats {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .chip,
  .tag,
  .cf-mc,
  .mr-badge,
  .cf-cats span {
    font-size: 11px;
    padding: 2px 7px;
    border-radius: 999px;
    border: 1px solid #2d323a;
    color: #c5ccd4;
    background: #22262c;
  }

  .chip {
    border-color: rgba(30, 181, 116, 0.45);
    color: #9ae6c0;
    background: rgba(30, 181, 116, 0.1);
  }

  .mr-badge.loader {
    border-color: rgba(96, 165, 250, 0.4);
    color: #93c5fd;
  }

  .cf-kicker {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: #f16424;
  }

  .cf-foot {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    margin-top: auto;
    font-size: 12px;
    color: #8b949e;
    align-items: center;
  }

  .cf-cats.inline span {
    border-radius: 4px;
    background: #2a2a2e;
    border-color: #3f3f46;
    color: #d4d4d8;
  }

  .cf-mc {
    border-radius: 4px;
    border-color: rgba(241, 100, 36, 0.4);
    color: #fdba8c;
    background: rgba(241, 100, 36, 0.1);
  }

  /* Page layouts */
  .mr-page,
  .cf-page {
    border-radius: 12px;
    border: 1px solid #2d323a;
    background: #121419;
    overflow: hidden;
  }

  .cf-page {
    border-color: #3a3a40;
    background: #17171a;
  }

  .mr-hero,
  .cf-hero {
    height: 120px;
    background: #0b0d10;
    overflow: hidden;
  }

  .mr-hero img,
  .cf-hero img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .mr-page-head,
  .cf-page-head {
    display: flex;
    gap: 14px;
    padding: 16px;
    align-items: flex-start;
  }

  .mr-page-titles,
  .cf-page-titles {
    min-width: 0;
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .mr-page-badges,
  .cf-page-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .cf-page-meta span {
    font-size: 12px;
    color: #a1a1aa;
  }

  .mr-page-cta {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 6px;
    flex: 0 0 auto;
  }

  .mr-dl-btn,
  .cf-install-btn {
    border: none;
    border-radius: 8px;
    padding: 8px 14px;
    font-weight: 700;
    font-size: 13px;
    cursor: default;
    pointer-events: none;
  }

  .mr-dl-btn {
    background: #1bd96a;
    color: #04140a;
  }

  .cf-install-btn {
    background: #f16424;
    color: #fff;
    align-self: flex-start;
  }

  .mr-stat {
    font-size: 11px;
    color: #8b949e;
    text-align: center;
  }

  .page-chips,
  .cf-cats:not(.inline) {
    padding: 0 16px 12px;
  }

  .mr-page-body,
  .cf-page-body {
    padding: 12px 16px 16px;
    border-top: 1px solid #2d323a;
    max-height: 360px;
    overflow: auto;
    color: #c5ccd4;
    font-size: 13px;
  }

  .muted {
    color: #8b949e;
  }

  .prose :global(img) {
    max-width: 100%;
    border-radius: 8px;
  }

  .prose :global(a) {
    color: #1bd96a;
  }

  .cf-page .prose :global(a) {
    color: #fdba8c;
  }

  .prose :global(h1),
  .prose :global(h2),
  .prose :global(h3) {
    margin: 0.6em 0 0.35em;
    color: #ecf0f3;
  }
</style>
