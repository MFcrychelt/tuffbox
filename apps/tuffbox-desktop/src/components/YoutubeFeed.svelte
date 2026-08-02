<script lang="ts">
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-shell";
  import { Youtube, ChevronDown, PictureInPicture2 } from "@lucide/svelte";
  import { supabase } from "../lib/supabaseAuth";
  import { api } from "../lib/api";
  import { openYoutubePlayer } from "../lib/store";

  type FeedVideo = {
    video_id: string;
    title: string;
    thumbnail_url: string | null;
    channel_name: string | null;
    source?: string | null;
    lang?: string | null;
    view_count?: number | null;
    published_at?: string | null;
  };

  /** `row` = horizontal strip; `grid` = main card grid; `rail` = vertical under-skin column. */
  let { variant = "row" }: { variant?: "row" | "grid" | "rail" } = $props();

  const STORAGE_KEY = "tuffbox-youtube-feed-expanded";
  /** Horizontal/grid initial strip size. */
  const FEED_LIMIT_ROW = 24;
  /** Rail: large client pool; reveal in pages (scroll lives on skin+feed column). */
  const FEED_POOL_RAIL = 60;
  const FEED_PAGE_RAIL = 16;
  const FEED_MORE_RAIL = 12;
  const SKEL_COUNT_ROW = 5;
  const SKEL_COUNT_RAIL = 4;
  /** Cap clips from the same channel so mega-creators don't fill the strip. */
  const MAX_PER_CHANNEL = 2;
  const MAX_PER_CHANNEL_RAIL = 3;
  /** Share of tracked-creator videos in the final strip. */
  const CHANNEL_SHARE = 0.4;

  const poolLimit = $derived(variant === "rail" ? FEED_POOL_RAIL : FEED_LIMIT_ROW);
  const skelCount = $derived(variant === "rail" ? SKEL_COUNT_RAIL : SKEL_COUNT_ROW);

  let videoPool = $state<FeedVideo[]>([]);
  let visibleCount = $state(FEED_PAGE_RAIL);
  let loading = $state(true);
  let loadError = $state("");
  let expanded = $state(true);
  let inlinePlayer = $state(true);
  let loadMoreEl = $state<HTMLElement | null>(null);

  const visibleVideos = $derived(
    variant === "rail" ? videoPool.slice(0, visibleCount) : videoPool,
  );
  const canLoadMore = $derived(variant === "rail" && visibleCount < videoPool.length);

  function onCardClick(video: FeedVideo, event: MouseEvent) {
    if (inlinePlayer) {
      const el = event.currentTarget as HTMLElement | null;
      openYoutubePlayer({
        videoId: video.video_id,
        title: video.title,
        originRect: el?.getBoundingClientRect?.() ?? null,
        startMini: false,
      });
    } else {
      void openVideo(video.video_id);
    }
  }

  function onCardMini(video: FeedVideo, event: MouseEvent) {
    event.stopPropagation();
    if (!inlinePlayer) {
      void openVideo(video.video_id);
      return;
    }
    openYoutubePlayer({
      videoId: video.video_id,
      title: video.title,
      originRect: null,
      startMini: true,
    });
  }

  /** User UI language primary tag (ru-RU → ru). Recommendations: native OR en. */
  function userLang(): string {
    try {
      const raw =
        (typeof navigator !== "undefined" &&
          (navigator.languages?.[0] || navigator.language)) ||
        "en";
      return String(raw).trim().toLowerCase().split(/[-_]/)[0] || "en";
    } catch {
      return "en";
    }
  }

  function channelKey(v: FeedVideo): string {
    return (v.channel_name || "").trim().toLowerCase() || "?";
  }

  function hashStr(s: string): number {
    let h = 2166136261;
    for (let i = 0; i < s.length; i++) {
      h ^= s.charCodeAt(i);
      h = Math.imul(h, 16777619);
    }
    return h >>> 0;
  }

  /** Mix view popularity with freshness; soft daily shuffle so the strip rotates. */
  function scoreVideo(v: FeedVideo, daySeed: number): number {
    const views = Math.max(0, Number(v.view_count) || 0);
    const published = v.published_at ? Date.parse(v.published_at) : NaN;
    const ageDays = Number.isFinite(published)
      ? Math.max(0, (Date.now() - published) / 86_400_000)
      : 120;
    const freshness = Math.max(0, 1 - ageDays / 150);
    const base = Math.log10(views + 10) * (0.5 + 0.5 * freshness);
    // Small jitter (±8%) keyed by day + id so order isn't identical every open,
    // but stays stable within a calendar day.
    const jitter = ((hashStr(`${v.video_id}:${daySeed}`) % 1000) / 1000 - 0.5) * 0.16;
    return base * (1 + jitter);
  }

  /**
   * Build a varied strip: channel caps, then native-lang first, foreign after.
   */
  function diversifyFeed(
    rows: FeedVideo[],
    limit: number,
    preferLang: string,
    maxPerChannel = MAX_PER_CHANNEL,
  ): FeedVideo[] {
    if (rows.length === 0) return [];

    const daySeed = Math.floor(Date.now() / 86_400_000);
    const ranked = [...rows].sort(
      (a, b) => scoreVideo(b, daySeed) - scoreVideo(a, daySeed),
    );

    const popularPool = ranked.filter((v) => v.source !== "channel");
    const channelPool = ranked.filter((v) => v.source === "channel");
    const channelTarget = Math.min(
      channelPool.length,
      Math.max(4, Math.round(limit * CHANNEL_SHARE)),
    );
    const popularTarget = limit - channelTarget;

    function isNative(v: FeedVideo): boolean {
      return (v.lang || "en") === preferLang;
    }

    function pick(pool: FeedVideo[], n: number): FeedVideo[] {
      const out: FeedVideo[] = [];
      const counts = new Map<string, number>();
      // Native language first, then any remaining (foreign / unknown).
      const passes: Array<(v: FeedVideo) => boolean> =
        preferLang === "en"
          ? [() => true]
          : [isNative, () => true];

      for (const pass of passes) {
        for (const v of pool) {
          if (out.length >= n) break;
          if (out.some((p) => p.video_id === v.video_id)) continue;
          if (!pass(v)) continue;
          const ch = channelKey(v);
          const used = counts.get(ch) ?? 0;
          if (used >= maxPerChannel) continue;
          counts.set(ch, used + 1);
          out.push(v);
        }
      }
      return out;
    }

    const popular = pick(popularPool, popularTarget);
    const creators = pick(channelPool, channelTarget);

    // Interleave pools, then re-order: all native first, foreign after (stable within each).
    const mixed: FeedVideo[] = [];
    let pi = 0;
    let ci = 0;
    while (mixed.length < limit && (pi < popular.length || ci < creators.length)) {
      for (let k = 0; k < 3 && pi < popular.length && mixed.length < limit; k++) {
        mixed.push(popular[pi++]);
      }
      for (let k = 0; k < 2 && ci < creators.length && mixed.length < limit; k++) {
        mixed.push(creators[ci++]);
      }
    }

    if (mixed.length < limit) {
      const used = new Set(mixed.map((v) => v.video_id));
      const counts = new Map<string, number>();
      for (const v of mixed) counts.set(channelKey(v), (counts.get(channelKey(v)) ?? 0) + 1);
      for (const v of ranked) {
        if (mixed.length >= limit) break;
        if (used.has(v.video_id)) continue;
        const ch = channelKey(v);
        if ((counts.get(ch) ?? 0) >= maxPerChannel) continue;
        counts.set(ch, (counts.get(ch) ?? 0) + 1);
        used.add(v.video_id);
        mixed.push(v);
      }
    }

    const native = mixed.filter(isNative);
    const foreign = mixed.filter((v) => !isNative(v));
    return [...native, ...foreign].slice(0, limit);
  }

  onMount(() => {
    try {
      const stored = localStorage.getItem(STORAGE_KEY);
      if (stored !== null) expanded = stored !== "false";
    } catch {
      // ignore storage errors
    }

    void api.launcher
      .get()
      .then((s) => {
        inlinePlayer = s.youtubeInlinePlayer !== false;
      })
      .catch(() => {
        // keep default true
      });

    loadFeed();
  });

  async function loadFeed() {
    loading = true;
    loadError = "";
    try {
      const lang = userLang();
      // Prefer native + English; also pull a wider pool so "foreign" slots aren't empty.
      const langs =
        lang === "en" ? ["en"] : [lang, "en", "es", "pt", "de", "fr", "pl", "uk"];
      const cols =
        "video_id,title,thumbnail_url,channel_name,source,lang,view_count,published_at";

      // Separate pools so mega popular hits don't crowd out tracked creators.
      const [popularRes, channelRes] = await Promise.all([
        supabase
          .from("youtube_feed")
          .select(cols)
          .in("lang", langs)
          .eq("source", "popular")
          .order("view_count", { ascending: false })
          .limit(120),
        supabase
          .from("youtube_feed")
          .select(cols)
          .in("lang", langs)
          .eq("source", "channel")
          .order("view_count", { ascending: false })
          .limit(80),
      ]);

      if (popularRes.error && channelRes.error) {
        loadError = popularRes.error.message || channelRes.error.message || "Failed to load feed";
        videoPool = [];
        visibleCount = FEED_PAGE_RAIL;
        return;
      }

      const popular =
        !popularRes.error && popularRes.data ? (popularRes.data as FeedVideo[]) : [];
      const channel =
        !channelRes.error && channelRes.data ? (channelRes.data as FeedVideo[]) : [];

      // Dedup by video_id (prefer popular row when both match).
      const byId = new Map<string, FeedVideo>();
      for (const v of channel) byId.set(v.video_id, v);
      for (const v of popular) byId.set(v.video_id, v);

      const maxPer = variant === "rail" ? MAX_PER_CHANNEL_RAIL : MAX_PER_CHANNEL;
      videoPool = diversifyFeed([...byId.values()], poolLimit, lang, maxPer);
      visibleCount = variant === "rail" ? Math.min(FEED_PAGE_RAIL, videoPool.length) : videoPool.length;
      if (videoPool.length === 0) {
        loadError = "";
      }
    } catch (e) {
      videoPool = [];
      visibleCount = FEED_PAGE_RAIL;
      loadError = String(e);
    } finally {
      loading = false;
    }
  }

  function loadMoreFromPool() {
    if (!canLoadMore) return;
    visibleCount = Math.min(visibleCount + FEED_MORE_RAIL, videoPool.length);
  }

  $effect(() => {
    if (variant !== "rail" || !canLoadMore || !loadMoreEl) return;
    const el = loadMoreEl;
    const io = new IntersectionObserver(
      (entries) => {
        if (entries.some((e) => e.isIntersecting)) loadMoreFromPool();
      },
      { root: null, rootMargin: "120px", threshold: 0 },
    );
    io.observe(el);
    return () => io.disconnect();
  });

  function toggleExpanded() {
    expanded = !expanded;
    try {
      localStorage.setItem(STORAGE_KEY, String(expanded));
    } catch {
      // ignore storage errors
    }
  }

  async function openVideo(videoId: string) {
    await open(`https://www.youtube.com/watch?v=${videoId}`);
  }

  /** Map vertical wheel to horizontal scroll so the strip is usable with a mouse. */
  function onFeedWheel(e: WheelEvent) {
    if (variant !== "row") return;
    const el = e.currentTarget as HTMLElement;
    if (el.scrollWidth <= el.clientWidth) return;
    // Prefer horizontal delta; otherwise tilt vertical into horizontal.
    const dx = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY;
    if (dx === 0) return;
    el.scrollLeft += dx;
    e.preventDefault();
  }
</script>

{#if loading || videoPool.length > 0 || loadError !== "" || (!loading && videoPool.length === 0)}
  <section
    class="youtube-feed"
    class:rail={variant === "rail"}
    class:grid={variant === "grid"}
    aria-busy={loading}
  >
    <button type="button" class="section-header" onclick={toggleExpanded} disabled={loading}>
      <Youtube size={18} />
      <h2>Minecraft on YouTube</h2>
      <span class="chevron" class:rotated={expanded} aria-hidden="true">
        <ChevronDown size={18} />
      </span>
    </button>
    {#if expanded}
      {#if loading}
        <div class="feed-row home-skel-stagger" aria-hidden="true" onwheel={onFeedWheel}>
          {#each Array(skelCount) as _, i (i)}
            <div class="video-card skel-card" style={`--i: ${i}`}>
              <div class="thumb skeleton skeleton-block skeleton-card"></div>
              <span class="skeleton skeleton-block skeleton-line medium"></span>
              <span class="skeleton skeleton-block skeleton-line short"></span>
            </div>
          {/each}
        </div>
      {:else if loadError}
        <div class="feed-status">
          <p>Couldn’t load YouTube feed.</p>
          <span class="feed-status-detail">{loadError}</span>
          <button type="button" class="retry-btn" onclick={() => loadFeed()}>Retry</button>
        </div>
      {:else if videoPool.length === 0}
        <div class="feed-status">
          <p>No videos yet. The feed fills every few hours.</p>
          <button type="button" class="retry-btn" onclick={() => loadFeed()}>Refresh</button>
        </div>
      {:else}
        <div class="feed-row tb-anim-fade-in" onwheel={onFeedWheel}>
          {#each visibleVideos as video (video.video_id)}
            <div class="video-card-wrap">
              <button
                type="button"
                class="video-card"
                onclick={(e) => onCardClick(video, e)}
              >
                <div class="thumb">
                  {#if video.thumbnail_url}
                    <img src={video.thumbnail_url} alt="" loading="lazy" />
                  {/if}
                </div>
                <span class="title">{video.title}</span>
                {#if video.channel_name}
                  <span class="channel">{video.channel_name}</span>
                {/if}
              </button>
              {#if inlinePlayer}
                <button
                  type="button"
                  class="pip-btn"
                  title="Play in mini window"
                  aria-label="Play in mini window"
                  onclick={(e) => onCardMini(video, e)}
                >
                  <PictureInPicture2 size={14} />
                </button>
              {/if}
            </div>
          {/each}
        </div>
        {#if canLoadMore}
          <div class="load-more-wrap" bind:this={loadMoreEl}>
            <button type="button" class="load-more-btn" onclick={loadMoreFromPool}>
              Load more ({videoPool.length - visibleCount} left)
            </button>
          </div>
        {/if}
      {/if}
    {/if}
  </section>
{/if}

<style>
  .youtube-feed {
    margin-bottom: 0;
  }

  .section-header {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    margin: 0 0 16px;
    padding: 0;
    background: transparent;
    border: none;
    cursor: pointer;
    color: var(--text-primary);
    text-align: left;
  }

  .section-header :global(svg) {
    color: var(--accent-primary);
    flex-shrink: 0;
  }

  .section-header h2 {
    margin: 0;
    flex: 1;
    font-size: 18px;
    font-weight: 700;
    color: var(--text-primary);
  }

  .rail .section-header h2 {
    font-size: 14px;
  }

  .chevron {
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-muted);
    transition: transform 0.2s ease;
  }

  .chevron :global(svg) {
    color: var(--text-muted);
  }

  .chevron.rotated {
    transform: rotate(180deg);
  }

  .feed-row {
    display: flex;
    flex-wrap: nowrap;
    gap: 12px;
    width: 100%;
    max-width: 100%;
    min-width: 0;
    overflow-x: auto;
    overflow-y: hidden;
    padding-bottom: 6px;
    scrollbar-width: thin;
    scrollbar-color: var(--bg-elevated) transparent;
    overscroll-behavior-x: contain;
    -webkit-overflow-scrolling: touch;
    touch-action: pan-x;
  }

  /* Rail: natural height — parent `.home-side` scrolls skin + feed together. */
  .rail .feed-row {
    flex-direction: column;
    gap: 10px;
    overflow: visible;
    padding-bottom: 4px;
    touch-action: pan-y;
  }

  /* Grid: card mosaic for YouTube-main home layout. */
  .grid .feed-row {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
    gap: 14px 12px;
    overflow: visible;
    padding-bottom: 0;
    touch-action: auto;
  }

  .grid .video-card-wrap {
    flex: unset;
    width: auto;
    min-width: 0;
  }

  .grid .video-card {
    flex: unset;
    width: 100%;
  }

  .feed-row::-webkit-scrollbar {
    height: 6px;
  }

  .feed-row::-webkit-scrollbar-thumb {
    background: var(--bg-elevated);
    border-radius: 3px;
  }

  .youtube-feed.rail {
    min-width: 0;
  }

  .rail .section-header {
    margin-bottom: 10px;
  }

  .load-more-wrap {
    display: flex;
    justify-content: center;
    padding: 8px 0 4px;
  }

  .load-more-btn {
    width: 100%;
    padding: 8px 10px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-secondary);
    background: var(--bg-tertiary);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius-sm);
    cursor: pointer;
  }

  .load-more-btn:hover {
    color: var(--text-primary);
    border-color: color-mix(in srgb, var(--accent-primary) 40%, var(--border-color));
  }

  .video-card {
    flex: 0 0 190px;
    width: 190px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 0;
    margin: 0;
    background: transparent;
    border: none;
    text-align: left;
    cursor: pointer;
    color: inherit;
    transition: transform 0.15s ease;
  }

  .video-card-wrap {
    position: relative;
    flex: 0 0 190px;
    width: 190px;
  }

  .rail .video-card-wrap {
    flex: 0 0 auto;
    width: 100%;
  }

  .video-card-wrap .video-card {
    flex: 1 1 auto;
    width: 100%;
  }

  .pip-btn {
    position: absolute;
    top: 8px;
    right: 8px;
    z-index: 2;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    padding: 0;
    border: 1px solid rgba(255, 255, 255, 0.18);
    border-radius: var(--border-radius-sm);
    background: rgba(0, 0, 0, 0.55);
    color: #fff;
    cursor: pointer;
    opacity: 0;
    transition: opacity 0.15s ease, background 0.15s ease;
  }

  .video-card-wrap:hover .pip-btn,
  .video-card-wrap:focus-within .pip-btn {
    opacity: 1;
  }

  .pip-btn:hover {
    background: color-mix(in srgb, var(--accent-primary) 70%, #000);
    border-color: transparent;
  }

  .rail .video-card {
    flex: 0 0 auto;
    width: 100%;
    gap: 6px;
  }

  .rail .thumb {
    border-radius: var(--border-radius-sm);
  }

  .rail .title {
    font-size: 12px;
    line-height: 1.35;
    -webkit-line-clamp: 2;
  }

  .rail .channel {
    font-size: 11px;
  }

  .rail .video-card:hover {
    transform: none;
  }

  .skel-card {
    cursor: default;
    pointer-events: none;
  }

  .skel-card .thumb {
    border-color: transparent;
  }

  .section-header:disabled {
    cursor: default;
    opacity: 0.85;
  }

  .video-card:hover {
    transform: translateY(-3px);
  }

  .video-card:hover .thumb {
    border-color: var(--accent-primary);
  }

  .thumb {
    width: 100%;
    aspect-ratio: 16 / 9;
    border-radius: var(--border-radius-md, 8px);
    overflow: hidden;
    background: var(--bg-secondary);
    border: 1px solid var(--border-color);
    transition: border-color 0.15s ease;
  }

  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }

  .title {
    font-size: 12px;
    font-weight: 700;
    color: var(--text-primary);
    line-height: 1.35;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .channel {
    font-size: 11px;
    color: var(--text-muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .feed-status {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
    padding: 12px 0;
    color: var(--text-muted);
    font-size: 13px;
  }

  .feed-status p {
    margin: 0;
    color: var(--text-secondary);
  }

  .feed-status-detail {
    font-size: 11px;
    opacity: 0.85;
    word-break: break-word;
  }

  .retry-btn {
    padding: 6px 12px;
    border-radius: var(--border-radius-sm);
    border: 1px solid var(--border-color);
    background: var(--bg-secondary);
    color: var(--accent-primary);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }

  .retry-btn:hover {
    border-color: var(--accent-primary);
  }
</style>
