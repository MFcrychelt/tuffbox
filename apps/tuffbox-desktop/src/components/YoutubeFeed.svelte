<script lang="ts">
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-shell";
  import { Youtube, ChevronDown } from "lucide-svelte";
  import { supabase } from "../lib/supabaseAuth";
  import { api } from "../lib/api";
  import YoutubePlayer from "./YoutubePlayer.svelte";

  type FeedVideo = {
    video_id: string;
    title: string;
    thumbnail_url: string | null;
    channel_name: string | null;
    source?: string | null;
    lang?: string | null;
  };

  const STORAGE_KEY = "tuffbox-youtube-feed-expanded";
  const FEED_LIMIT = 20;
  const SKEL_COUNT = 5;

  let videos: FeedVideo[] = [];
  let loading = true;
  let expanded = true;
  let inlinePlayer = true;
  let activeVideo: FeedVideo | null = null;
  let activeOrigin: DOMRect | null = null;

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
    try {
      const lang = userLang();
      const langs = lang === "en" ? ["en"] : [lang, "en"];
      const { data, error } = await supabase
        .from("youtube_feed")
        .select("video_id,title,thumbnail_url,channel_name,source,lang")
        .in("lang", langs)
        .order("view_count", { ascending: false })
        .limit(60);
      if (error || !data) {
        videos = [];
        return;
      }
      const rows = data as FeedVideo[];
      // Popular keyword hits first, then tracked creators (each by views).
      // Reserve room so creators are not crowded out by a long popular list.
      const popularAll = rows.filter((v) => v.source !== "channel");
      const channelAll = rows.filter((v) => v.source === "channel");
      const popularCap = Math.min(popularAll.length, Math.max(12, FEED_LIMIT - Math.min(8, channelAll.length)));
      const popular = popularAll.slice(0, popularCap);
      const channel = channelAll.slice(0, FEED_LIMIT - popular.length);
      videos = [...popular, ...channel];
    } catch {
      videos = [];
    } finally {
      loading = false;
    }
  }

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

  function onCardClick(video: FeedVideo, event: MouseEvent) {
    if (inlinePlayer) {
      const el = event.currentTarget as HTMLElement | null;
      activeOrigin = el?.getBoundingClientRect?.() ?? null;
      activeVideo = video;
    } else {
      void openVideo(video.video_id);
    }
  }
</script>

{#if loading || videos.length > 0}
  <section class="youtube-feed" aria-busy={loading}>
    <button type="button" class="section-header" on:click={toggleExpanded} disabled={loading}>
      <Youtube size={18} />
      <h2>Minecraft on YouTube</h2>
      <span class="chevron" class:rotated={expanded} aria-hidden="true">
        <ChevronDown size={18} />
      </span>
    </button>
    {#if expanded}
      {#if loading}
        <div class="feed-row home-skel-stagger" aria-hidden="true">
          {#each Array(SKEL_COUNT) as _, i (i)}
            <div class="video-card skel-card" style={`--i: ${i}`}>
              <div class="thumb skeleton skeleton-block skeleton-card"></div>
              <span class="skeleton skeleton-block skeleton-line medium"></span>
              <span class="skeleton skeleton-block skeleton-line short"></span>
            </div>
          {/each}
        </div>
      {:else}
        <div class="feed-row tb-anim-fade-in">
          {#each videos as video (video.video_id)}
            <button
              type="button"
              class="video-card"
              on:click={(e) => onCardClick(video, e)}
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
          {/each}
        </div>
      {/if}
    {/if}
  </section>
{/if}

{#if activeVideo}
  <YoutubePlayer
    videoId={activeVideo.video_id}
    title={activeVideo.title}
    originRect={activeOrigin}
    on:close={() => {
      activeVideo = null;
      activeOrigin = null;
    }}
  />
{/if}

<style>
  .youtube-feed {
    margin-bottom: 32px;
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
    gap: 12px;
    overflow-x: auto;
    padding-bottom: 4px;
    scrollbar-width: thin;
    scrollbar-color: var(--bg-elevated) transparent;
  }

  .feed-row::-webkit-scrollbar {
    height: 6px;
  }

  .feed-row::-webkit-scrollbar-thumb {
    background: var(--bg-elevated);
    border-radius: 3px;
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
</style>
