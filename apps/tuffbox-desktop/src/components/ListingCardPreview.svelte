<script lang="ts">
  import {
    Download,
    Heart,
    Bookmark,
    Link2,
    Clock,
    Gamepad2,
    Monitor,
    Tag,
  } from "lucide-svelte";

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
  export let author: string | null = null;

  $: catChips = categories.filter(Boolean).slice(0, 8);
  $: loaderLabel = (loaderKind || "").replace(/_/g, " ");
  $: authorLabel = author || name || "Author";
  $: cfExtraCats = Math.max(0, catChips.length - 1);
  $: cfGameMeta = [minecraftVersion, loaderLabel].filter(Boolean).join(" ");

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
      <div class="mr-center">
        <div class="mr-title-line">
          <h3>{name || "Untitled pack"}</h3>
          <span class="mr-by">by {authorLabel}</span>
        </div>
        <p class="mr-summary card-summary">{summary || "No summary yet."}</p>
        <div class="mr-tags">
          <span class="mr-tag env"><Monitor size={11} /> Client</span>
          {#each catChips as c (c)}
            <span class="mr-tag">{prettyCat(c)}</span>
          {/each}
          {#if loaderLabel}
            <span class="mr-tag loader"><Tag size={11} /> {loaderLabel}</span>
          {/if}
        </div>
      </div>
      <div class="mr-actions">
        <div class="mr-action-row">
          <button type="button" class="mr-dl-btn card-dl" tabindex="-1">
            <Download size={14} />
            Download
          </button>
          <button type="button" class="mr-icon-btn" tabindex="-1" aria-hidden="true">
            <Heart size={14} />
          </button>
          <button type="button" class="mr-icon-btn" tabindex="-1" aria-hidden="true">
            <Bookmark size={14} />
          </button>
          <button type="button" class="mr-icon-btn" tabindex="-1" aria-hidden="true">
            <Link2 size={14} />
          </button>
        </div>
        <div class="mr-stat-row">
          <span class="mr-stat-item"><Download size={12} /> —</span>
          <span class="mr-stat-item"><Heart size={12} /> —</span>
        </div>
        <div class="mr-stat-row time">
          <span class="mr-stat-item"><Clock size={12} /> just now</span>
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
      <div class="cf-thumb-wrap">
        <div class="cf-icon">
          {#if iconUrl}
            <img src={iconUrl} alt="" />
          {:else}
            <div class="icon-ph">?</div>
          {/if}
        </div>
        <span class="cf-badge">Modpacks</span>
      </div>
      <div class="cf-body">
        <h3>{name || "Untitled pack"}</h3>
        <p class="cf-author">By {authorLabel}</p>
        <p class="cf-summary card-summary">{summary || "No summary yet."}</p>
        <div class="cf-meta-row">
          {#if catChips.length}
            <span class="cf-cat-tag">{prettyCat(catChips[0])}</span>
            {#if cfExtraCats > 0}
              <span class="cf-cat-more">+{cfExtraCats}</span>
            {/if}
          {/if}
          <span class="cf-meta-item"><Download size={12} /> —</span>
          <span class="cf-meta-item"><Clock size={12} /> —</span>
          {#if cfGameMeta}
            <span class="cf-meta-item game"><Gamepad2 size={12} /> {cfGameMeta}</span>
          {/if}
        </div>
      </div>
    </article>
  {/if}
{/if}

<style>
  /* ── Modrinth card ── */
  .mr-card {
    display: flex;
    align-items: stretch;
    gap: 16px;
    padding: 16px;
    border-radius: 12px;
    border: 1px solid #2d323a;
    background: #16181c;
  }

  .mr-icon {
    width: 96px;
    height: 96px;
    border-radius: 10px;
    overflow: hidden;
    background: #0f1115;
    border: 1px solid #2d323a;
    flex-shrink: 0;
  }

  .mr-center {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
    justify-content: center;
  }

  .mr-title-line {
    display: flex;
    align-items: baseline;
    gap: 8px;
    flex-wrap: wrap;
    min-width: 0;
  }

  .mr-title-line h3 {
    margin: 0;
    font-size: 16px;
    font-weight: 700;
    color: #ecf0f3;
    line-height: 1.25;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .mr-by {
    font-size: 13px;
    color: #8b949e;
    white-space: nowrap;
    flex-shrink: 0;
  }

  .mr-summary {
    margin: 0;
    color: #a8b0b9;
    font-size: 13px;
    line-height: 1.45;
  }

  .card-summary {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .cf-summary.card-summary {
    -webkit-line-clamp: 1;
  }

  .mr-tags {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
    margin-top: 2px;
  }

  .mr-tag {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    padding: 3px 8px;
    border-radius: 999px;
    border: 1px solid #3a3f47;
    color: #b0b8c1;
    background: #22262c;
    line-height: 1.2;
  }

  .mr-tag.loader {
    border-color: rgba(196, 130, 60, 0.55);
    color: #e8b87a;
    background: rgba(196, 130, 60, 0.15);
  }

  .mr-tag.env {
    border-color: #3a3f47;
    color: #9aa3ad;
    background: #1e2228;
  }

  .mr-actions {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    justify-content: space-between;
    gap: 8px;
    flex-shrink: 0;
    min-width: 0;
  }

  .mr-action-row {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .mr-dl-btn {
    border: none;
    border-radius: 8px;
    padding: 8px 14px;
    font-weight: 700;
    font-size: 13px;
    cursor: default;
    pointer-events: none;
    background: #1bd96a;
    color: #04140a;
  }

  .mr-dl-btn.card-dl {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 7px 12px;
    font-size: 12px;
    border-radius: 6px;
  }

  .mr-icon-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 30px;
    height: 30px;
    padding: 0;
    border-radius: 6px;
    border: 1px solid #3a3f47;
    background: transparent;
    color: #8b949e;
    cursor: default;
    pointer-events: none;
  }

  .mr-stat-row {
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 12px;
    color: #8b949e;
  }

  .mr-stat-row.time {
    gap: 0;
  }

  .mr-stat-item {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }

  /* ── CurseForge card ── */
  .cf-card {
    display: flex;
    align-items: stretch;
    gap: 16px;
    padding: 16px;
    border-radius: 10px;
    border: 1px solid #3a3a40;
    background: #1c1c1f;
  }

  .cf-thumb-wrap {
    position: relative;
    flex-shrink: 0;
  }

  .cf-icon {
    width: 120px;
    height: 120px;
    border-radius: 8px;
    overflow: hidden;
    background: #0f0f11;
    border: 1px solid #3a3a40;
  }

  .cf-badge {
    position: absolute;
    top: 6px;
    left: 6px;
    font-size: 10px;
    font-weight: 700;
    padding: 2px 6px;
    border-radius: 3px;
    background: rgba(0, 0, 0, 0.75);
    color: #fff;
    letter-spacing: 0.02em;
    line-height: 1.3;
  }

  .cf-body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
    justify-content: center;
  }

  .cf-body h3 {
    margin: 0;
    font-size: 18px;
    font-weight: 700;
    color: #f4f4f5;
    line-height: 1.25;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .cf-author {
    margin: 0;
    font-size: 13px;
    color: #a1a1aa;
  }

  .cf-summary {
    margin: 0;
    color: #a8b0b9;
    font-size: 13px;
    line-height: 1.4;
  }

  .cf-meta-row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 10px;
    margin-top: 6px;
    font-size: 12px;
    color: #8b949e;
  }

  .cf-cat-tag {
    padding: 2px 8px;
    border-radius: 4px;
    border: 1px solid #52525b;
    color: #d4d4d8;
    background: transparent;
    font-size: 11px;
  }

  .cf-cat-more {
    color: #71717a;
    font-size: 11px;
  }

  .cf-meta-item {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    white-space: nowrap;
  }

  .cf-meta-item.game {
    color: #a1a1aa;
  }

  /* ── Shared icon placeholder ── */
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

  /* ── Page layouts (unchanged) ── */
  h2 {
    margin: 0;
    font-size: 20px;
    color: #ecf0f3;
    line-height: 1.25;
    white-space: normal;
  }

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

  .mr-page-icon,
  .cf-page-icon {
    width: 72px;
    height: 72px;
    border-radius: 12px;
    overflow: hidden;
    background: #0f1115;
    border: 1px solid #2d323a;
    flex-shrink: 0;
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

  .chip,
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

  .cf-install-btn {
    border: none;
    border-radius: 8px;
    padding: 8px 14px;
    font-weight: 700;
    font-size: 13px;
    cursor: default;
    pointer-events: none;
    background: #f16424;
    color: #fff;
    align-self: flex-start;
  }

  .mr-stat {
    font-size: 11px;
    color: #8b949e;
    text-align: center;
  }

  .chips,
  .page-chips,
  .cf-cats {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
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
