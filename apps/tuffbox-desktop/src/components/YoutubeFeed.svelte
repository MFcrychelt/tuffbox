<script lang="ts">
  import { onMount } from "svelte";
  import { open } from "@tauri-apps/plugin-shell";
  import { Youtube } from "lucide-svelte";
  import { supabase } from "../lib/supabaseAuth";

  type FeedVideo = {
    video_id: string;
    title: string;
    thumbnail_url: string | null;
    channel_name: string | null;
  };

  let videos: FeedVideo[] = [];

  onMount(async () => {
    try {
      const { data, error } = await supabase
        .from("youtube_feed")
        .select("video_id,title,thumbnail_url,channel_name")
        .order("view_count", { ascending: false })
        .limit(10);
      if (error || !data) return;
      videos = data as FeedVideo[];
    } catch {
      // silent — empty feed on failure
    }
  });

  async function openVideo(videoId: string) {
    await open(`https://www.youtube.com/watch?v=${videoId}`);
  }
</script>

{#if videos.length > 0}
  <section class="youtube-feed">
    <div class="section-header">
      <Youtube size={18} />
      <h2>Minecraft on YouTube</h2>
    </div>
    <div class="feed-row">
      {#each videos as video (video.video_id)}
        <button
          type="button"
          class="video-card"
          on:click={() => openVideo(video.video_id)}
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
  </section>
{/if}

<style>
  .youtube-feed {
    margin-bottom: 32px;
  }

  .section-header {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-bottom: 16px;
    color: var(--text-primary);
  }

  .section-header :global(svg) {
    color: var(--accent-primary);
    flex-shrink: 0;
  }

  .section-header h2 {
    margin: 0;
    font-size: 18px;
    font-weight: 700;
    color: var(--text-primary);
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
