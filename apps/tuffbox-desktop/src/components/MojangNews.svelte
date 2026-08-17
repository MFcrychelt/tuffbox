<script lang="ts">
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-shell";

  /**
   * Minecraft Java Edition update feed — snapshots & releases only.
   * Source: official Minecraft Feedback "Release Changelogs" section (Zendesk
   * Help Center API) — https://feedback.minecraft.net/hc/en-us/sections/360001186971
   * The API sends `access-control-allow-origin: *`, and the host is whitelisted
   * in the app CSP.
   */
  const FEED_URL =
    "https://feedback.minecraft.net/api/v2/help_center/en-us/sections/360001186971/articles.json?per_page=100&sort_by=created_at&sort_order=desc";
  /** Help Center articles publish with public-facing URLs on this domain. */
  const HTML_ORIGIN = "https://feedback.minecraft.net";

  type ZendeskArticle = {
    id: number;
    name?: string | null;
    title?: string | null;
    html_url?: string | null;
    created_at?: string | null;
  };

  type Card = {
    key: string;
    title: string;
    url: string;
    version: string;
    kind: "Snapshot" | "Release";
    date: string;
    hue: number;
  };

  let { limit = 10 }: { limit?: number } = $props();

  let entries = $state<Card[]>([]);
  let loading = $state(true);

  const cards = $derived(entries.slice(0, limit));

  function hashStr(s: string): number {
    let h = 2166136261;
    for (let i = 0; i < s.length; i++) {
      h ^= s.charCodeAt(i);
      h = Math.imul(h, 16777619);
    }
    return h >>> 0;
  }

  /** Keep only Java Edition releases and Java snapshots (Bedrock uses "Preview"). */
  function isJavaChangelog(t: string): boolean {
    return /Java Edition/i.test(t) || /^Minecraft Snapshot\s/i.test(t);
  }

  /** Last version token in the title: "1.21.5" or "25w14a"-style snapshot ids. */
  function parseVersion(t: string): string {
    const m = t.match(
      /(\d+(?:\.\d+)+[a-z0-9]*|(?:2[3-9]|[3-9][0-9])w\d{1,2}[a-z]*(?:craftmine)?)/gi,
    );
    return m ? m[m.length - 1] : "";
  }

  function absoluteUrl(u: string): string {
    if (/^https?:\/\//i.test(u)) return u;
    return HTML_ORIGIN + (u.startsWith("/") ? "" : "/") + u;
  }

  function mapCard(a: ZendeskArticle): Card | null {
    const t = (a.title || a.name || "").trim();
    if (!t || !isJavaChangelog(t)) return null;
    const version = parseVersion(t);
    if (a.html_url && version) {
      return {
        key: `${a.id}:${t}`,
        title: t,
        url: absoluteUrl(a.html_url),
        version,
        kind: /Snapshot/i.test(t) ? "Snapshot" : "Release",
        date: a.created_at ?? "",
        hue: hashStr(version) % 360,
      };
    }
    return null;
  }

  onMount(() => {
    let cancelled = false;
    void (async () => {
      try {
        const res = await fetch(FEED_URL, { cache: "no-store" });
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        const data = (await res.json()) as { articles?: ZendeskArticle[] };
        const list: Card[] = [];
        for (const a of data.articles ?? []) {
          const c = mapCard(a);
          if (c) list.push(c);
        }
        list.sort((a, b) => (a.date < b.date ? 1 : a.date > b.date ? -1 : 0));
        if (!cancelled) entries = list;
      } catch {
        // Offline / blocked — render nothing; the hero shouldn't show an error strip.
        if (!cancelled) entries = [];
      } finally {
        if (!cancelled) loading = false;
      }
    })();
    return () => {
      cancelled = true;
    };
  });

  function onOpen(url: string) {
    void open(url);
  }

  // ─── horizontal navigation ────────────────────────────────
  let stripEl = $state<HTMLDivElement | null>(null);
  let canScrollLeft = $state(false);
  let canScrollRight = $state(false);

  function refreshScrollState() {
    const el = stripEl;
    if (!el) {
      canScrollLeft = false;
      canScrollRight = false;
      return;
    }
    canScrollLeft = el.scrollLeft > 4;
    canScrollRight = el.scrollLeft < el.scrollWidth - el.clientWidth - 4;
  }

  let scrollTickQueued = false;
  function queueScrollRefresh() {
    if (scrollTickQueued) return;
    scrollTickQueued = true;
    queueMicrotask(() => {
      scrollTickQueued = false;
      refreshScrollState();
    });
  }

  function scrollDir(dir: 1 | -1) {
    const el = stripEl;
    if (!el) return;
    const card = el.querySelector<HTMLElement>(".news-card");
    const gap = card ? parseFloat(getComputedStyle(el).columnGap || "10") : 10;
    const cardW = card ? card.getBoundingClientRect().width : 0;
    const step = cardW + gap;
    const max = Math.max(0, el.scrollWidth - el.clientWidth);
    const target = Math.max(0, Math.min(el.scrollLeft + dir * step, max));
    el.scrollTo({ left: target, behavior: "smooth" });
  }

  $effect(() => {
    const el = stripEl;
    if (!el) return;
    const update = () => refreshScrollState();
    update();
    el.addEventListener("scroll", update, { passive: true });
    const ro = new ResizeObserver(update);
    ro.observe(el);
    return () => {
      el.removeEventListener("scroll", update);
      ro.disconnect();
    };
  });

  $effect(() => {
    void cards.length;
    void loading;
    queueScrollRefresh();
  });

  /** Vertical wheel → horizontal scroll on the strip. */
  function onShelfWheel(e: WheelEvent) {
    const el = e.currentTarget as HTMLElement;
    if (el.scrollWidth <= el.clientWidth + 1) return;
    const dx = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY;
    if (dx === 0) return;
    el.scrollLeft += dx;
    e.preventDefault();
  }

  /** Pixel-map (7x5) of a right-pointing chevron, drawn with unit squares. */
  const ARROW_CELLS = [
    [3, 0],
    [2, 1],
    [3, 1],
    [4, 1],
    [1, 2],
    [2, 2],
    [3, 2],
    [4, 2],
    [5, 2],
    [2, 3],
    [3, 3],
    [4, 3],
    [3, 4],
  ] as const;

  function formatDate(iso: string): string {
    const d = iso ? new Date(iso) : null;
    if (!d || Number.isNaN(d.getTime())) return iso || "";
    const months = [
      "Jan",
      "Feb",
      "Mar",
      "Apr",
      "May",
      "Jun",
      "Jul",
      "Aug",
      "Sep",
      "Oct",
      "Nov",
      "Dec",
    ];
    return `${d.getDate()} ${months[d.getMonth()]} ${d.getFullYear()}`;
  }
</script>

{#snippet pixelArrow(dir: "left" | "right")}
  <svg
    class="pixel-arrow"
    viewBox="0 0 7 5"
    aria-hidden="true"
    shape-rendering="crispEdges"
  >
    {#each ARROW_CELLS as [x, y], i (i)}
      {@const cx = dir === "left" ? 6 - x : x}
      <rect x={cx} y={y} width="1" height="1" />
    {/each}
  </svg>
{/snippet}

<div class="mojang-news" aria-label="Minecraft Java updates">
  <div class="news-head">
    <span class="news-dot" aria-hidden="true"></span>
    <span class="news-title">Java Updates</span>
  </div>

  <div class="news-nav">
    <button
      type="button"
      class="news-arrow is-left"
      disabled={!canScrollLeft}
      aria-label="Scroll left"
      title="Scroll left"
      onclick={() => scrollDir(-1)}
    >
      {@render pixelArrow("left")}
    </button>

    {#if loading}
      <div class="news-rows" aria-hidden="true">
        {#each Array(Math.min(limit, 8)) as _, i (i)}
          <div class="news-card is-skel">
            <div class="card-shot skeleton skeleton-block"></div>
            <div class="card-cap">
              <span class="skeleton skeleton-block skeleton-line medium"></span>
              <span class="skeleton skeleton-block skeleton-line short"></span>
            </div>
          </div>
        {/each}
      </div>
    {:else if cards.length}
      <div class="news-rows" bind:this={stripEl} onwheel={onShelfWheel}>
        {#each cards as entry (entry.key)}
          <button
            type="button"
            class="news-card"
            title="Open changelog"
            aria-label={entry.title}
            onclick={() => onOpen(entry.url)}
          >
            <span
              class="card-shot shot-version"
              style={`--h: ${entry.hue}`}
            >
              <span class="shot-kind">{entry.kind}</span>
              <span class="shot-ver">{entry.version}</span>
            </span>
            <span class="card-cap">
              <span class="card-title">{entry.title}</span>
              <span class="card-version">{entry.version} · {formatDate(entry.date)}</span>
            </span>
          </button>
        {/each}
      </div>
    {/if}

    <button
      type="button"
      class="news-arrow is-right"
      disabled={!canScrollRight}
      aria-label="Scroll right"
      title="Scroll right"
      onclick={() => scrollDir(1)}
    >
      {@render pixelArrow("right")}
    </button>
  </div>
</div>

<style>
  .mojang-news {
    --card-w: clamp(132px, 13cqi, 156px);
    --gap: 10px;
    width: 100%;
    min-width: 0;
    max-width: 100%;
    color: var(--hero-fg, #fff);
  }

  .news-head {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 10px;
  }

  .news-dot {
    width: 10px;
    height: 10px;
    flex-shrink: 0;
    background: var(--accent-primary, #22c55e);
    border-radius: 0;
    box-shadow: 0 0 10px color-mix(in srgb, var(--accent-primary, #22c55e) 55%, transparent);
  }

  .news-title {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--hero-fg-muted, rgba(255, 255, 255, 0.72));
    text-shadow: 0 1px 8px rgba(0, 0, 0, 0.45);
  }

  /** Card-sized pixel arrows flanking the strip. */
  .news-nav {
    display: grid;
    grid-template-columns: var(--card-w) minmax(0, 1fr) var(--card-w);
    gap: var(--gap);
    width: 100%;
    min-width: 0;
    align-items: stretch;
  }

  .news-arrow {
    display: flex;
    align-items: center;
    justify-content: center;
    min-width: 0;
    margin: 0;
    padding: 0;
    border: 2px solid var(--glass-border, rgba(255, 255, 255, 0.2));
    border-radius: 0;
    background: color-mix(in srgb, #000 55%, transparent);
    box-shadow: none;
    color: var(--hero-fg, #fff);
    cursor: pointer;
    transition:
      background var(--motion-fast, 160ms) var(--ease-out, ease),
      border-color var(--motion-fast, 160ms) var(--ease-out, ease),
      color var(--motion-fast, 160ms) var(--ease-out, ease),
      opacity var(--motion-fast, 160ms) var(--ease-out, ease);
  }

  .news-arrow:hover:not(:disabled) {
    background: color-mix(in srgb, var(--accent-primary, #22c55e) 45%, #000);
    border-color: color-mix(in srgb, var(--accent-primary, #22c55e) 70%, transparent);
    color: #fff;
    transform: none;
  }

  .news-arrow:active:not(:disabled) .pixel-arrow {
    transform: translateY(2px);
  }

  .news-arrow:disabled {
    opacity: 0.3;
    cursor: default;
  }

  .pixel-arrow {
    width: 46%;
    height: auto;
    display: block;
    fill: currentColor;
    transition: transform var(--motion-fast, 160ms) var(--ease-out, ease);
  }

  /* ── cards ─────────────────────────────────────────────── */
  .news-rows {
    display: grid;
    grid-auto-flow: column;
    grid-auto-columns: var(--card-w);
    gap: var(--gap);
    width: 100%;
    min-width: 0;
    overflow-x: auto;
    overflow-y: hidden;
    overscroll-behavior-x: contain;
    scroll-snap-type: x proximity;
    -webkit-overflow-scrolling: touch;
    touch-action: pan-x;
    padding-bottom: 4px;
    scrollbar-width: none;
  }

  .news-rows::-webkit-scrollbar {
    display: none;
  }

  .news-card {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    width: 100%;
    min-width: 0;
    margin: 0;
    padding: 0;
    background: none;
    border: none;
    border-radius: 0;
    box-shadow: none;
    color: inherit;
    cursor: pointer;
    text-align: left;
    scroll-snap-align: start;
    transition:
      transform var(--motion-med, 240ms) var(--ease-hover-in, ease),
      filter var(--motion-med, 240ms) var(--ease-hover-in, ease);
  }

  .news-card:hover:not(.is-skel) {
    transform: translateY(-3px);
  }

  .news-card.is-skel {
    cursor: default;
    pointer-events: none;
  }

  /** Pixel-style version tile standing in for the screenshot. */
  .card-shot {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 6px;
    width: 100%;
    aspect-ratio: 1 / 1;
    overflow: hidden;
    border-radius: 0;
    background:
      radial-gradient(
        ellipse at 30% 22%,
        hsl(calc(var(--h) * 1deg) 60% 42% / 0.55),
        transparent 58%
      ),
      linear-gradient(
        165deg,
        hsl(calc(var(--h) * 1deg) 42% 20%),
        hsl(calc(var(--h) * 1deg) 48% 9%) 62%
      );
  }

  .card-shot::before {
    content: "";
    position: absolute;
    inset: 0;
    background-image:
      radial-gradient(circle at 22% 30%, rgba(255, 255, 255, 0.14) 0 5%, transparent 6%),
      radial-gradient(circle at 68% 12%, rgba(0, 0, 0, 0.35) 0 6%, transparent 7%),
      radial-gradient(circle at 82% 64%, rgba(255, 255, 255, 0.1) 0 4%, transparent 5%),
      radial-gradient(circle at 38% 78%, rgba(0, 0, 0, 0.3) 0 7%, transparent 8%),
      repeating-conic-gradient(
        rgba(255, 255, 255, 0.05) 0% 25%,
        transparent 0% 50%
      );
    image-rendering: pixelated;
    pointer-events: none;
  }

  .shot-kind {
    position: absolute;
    top: 8px;
    left: 8px;
    z-index: 1;
    padding: 2px 5px;
    font-size: 9px;
    font-weight: 800;
    letter-spacing: 0.07em;
    text-transform: uppercase;
    color: #000;
    background: color-mix(in srgb, var(--accent-primary, #22c55e) 62%, #fff);
  }

  .shot-ver {
    position: relative;
    z-index: 1;
    max-width: calc(100% - 16px);
    font-size: clamp(20px, 4.5cqi, 30px);
    font-weight: 800;
    line-height: 1;
    color: #fff;
    text-shadow: 0 3px 0 rgba(0, 0, 0, 0.45);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /** Pitch-black caption, glued straight under the tile. */
  .card-cap {
    display: flex;
    flex-direction: column;
    gap: 3px;
    align-items: flex-start;
    min-height: 56px;
    padding: 8px 10px;
    background: #000;
    border-radius: 0;
  }

  .card-title {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    width: 100%;
    font-size: 12px;
    font-weight: 700;
    line-height: 1.3;
    color: #fff;
  }

  .card-version {
    width: 100%;
    font-size: 11px;
    font-weight: 400;
    color: rgba(255, 255, 255, 0.62);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .card-cap .skeleton {
    width: 100%;
  }

  :global(html.potato-pc) .news-card:hover {
    transform: none;
  }

  @media (prefers-reduced-motion: reduce) {
    .news-card,
    .news-arrow,
    .pixel-arrow {
      transition: none;
    }
  }
</style>