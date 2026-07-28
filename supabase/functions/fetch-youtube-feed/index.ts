// Crawl popular Minecraft YouTube videos → youtube_feed (service role).
// Invoked by Cron / manual invoke. Launcher clients never call YouTube.

import { createClient } from "https://esm.sh/@supabase/supabase-js@2.49.1";

const QUERIES = [
  "Minecraft mods",
  "Minecraft 1.21 survival",
  "Minecraft modpack",
  "TuffBox",
] as const;

const SEARCH_MAX = 8;
const FEED_TOP = 10;
const LOOKBACK_DAYS = 7;

const corsHeaders: Record<string, string> = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Headers":
    "authorization, x-client-info, apikey, content-type",
};

type FeedRow = {
  video_id: string;
  title: string;
  thumbnail_url: string;
  channel_name: string;
  published_at: string | null;
  view_count: number;
  fetched_at: string;
  query_tag: string;
};

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { ...corsHeaders, "Content-Type": "application/json" },
  });
}

function publishedAfterIso(): string {
  const d = new Date();
  d.setUTCDate(d.getUTCDate() - LOOKBACK_DAYS);
  return d.toISOString();
}

function pickThumb(snippet: Record<string, unknown>): string {
  const thumbs = snippet.thumbnails as
    | Record<string, { url?: string } | undefined>
    | undefined;
  return (
    thumbs?.medium?.url ||
    thumbs?.high?.url ||
    thumbs?.default?.url ||
    ""
  );
}

async function searchVideoIds(
  apiKey: string,
  query: string,
): Promise<string[]> {
  const params = new URLSearchParams({
    part: "snippet",
    type: "video",
    order: "viewCount",
    maxResults: String(SEARCH_MAX),
    publishedAfter: publishedAfterIso(),
    q: query,
    key: apiKey,
  });
  const res = await fetch(
    `https://www.googleapis.com/youtube/v3/search?${params}`,
  );
  if (!res.ok) {
    const err = await res.text();
    throw new Error(`YouTube search failed (${res.status}): ${err.slice(0, 200)}`);
  }
  const data = await res.json();
  const items = Array.isArray(data.items) ? data.items : [];
  const ids: string[] = [];
  for (const item of items) {
    const id = item?.id?.videoId;
    if (typeof id === "string" && id.trim()) ids.push(id.trim());
  }
  return ids;
}

async function fetchVideoDetails(
  apiKey: string,
  ids: string[],
): Promise<Map<string, Record<string, unknown>>> {
  const out = new Map<string, Record<string, unknown>>();
  if (ids.length === 0) return out;
  const params = new URLSearchParams({
    part: "snippet,statistics",
    id: ids.join(","),
    key: apiKey,
  });
  const res = await fetch(
    `https://www.googleapis.com/youtube/v3/videos?${params}`,
  );
  if (!res.ok) {
    const err = await res.text();
    throw new Error(`YouTube videos failed (${res.status}): ${err.slice(0, 200)}`);
  }
  const data = await res.json();
  const items = Array.isArray(data.items) ? data.items : [];
  for (const item of items) {
    if (typeof item?.id === "string") out.set(item.id, item);
  }
  return out;
}

Deno.serve(async (req) => {
  if (req.method === "OPTIONS") {
    return new Response("ok", { headers: corsHeaders });
  }
  if (req.method !== "POST" && req.method !== "GET") {
    return jsonResponse(405, { error: "method not allowed" });
  }

  const apiKey = (
    Deno.env.get("YOUTUBE-API-KEY") ??
    Deno.env.get("YOUTUBE_API_KEY") ??
    ""
  ).trim();
  const supabaseUrl = Deno.env.get("SUPABASE_URL") ?? "";
  const serviceKey = Deno.env.get("SUPABASE_SERVICE_ROLE_KEY") ?? "";
  if (!apiKey) {
    return jsonResponse(500, {
      error: "YOUTUBE-API-KEY (or YOUTUBE_API_KEY) not configured",
    });
  }
  if (!supabaseUrl || !serviceKey) {
    return jsonResponse(500, { error: "server misconfigured" });
  }

  const admin = createClient(supabaseUrl, serviceKey, {
    auth: { persistSession: false, autoRefreshToken: false },
  });

  try {
    // video_id → first query that found it
    const idToQuery = new Map<string, string>();
    for (const q of QUERIES) {
      const ids = await searchVideoIds(apiKey, q);
      for (const id of ids) {
        if (!idToQuery.has(id)) idToQuery.set(id, q);
      }
    }

    const allIds = [...idToQuery.keys()];
    const details = await fetchVideoDetails(apiKey, allIds);
    const now = new Date().toISOString();
    const rows: FeedRow[] = [];

    for (const [videoId, item] of details) {
      const snippet = (item.snippet ?? {}) as Record<string, unknown>;
      const stats = (item.statistics ?? {}) as Record<string, unknown>;
      const title = String(snippet.title ?? "").trim();
      const channel = String(snippet.channelTitle ?? "").trim();
      const thumb = pickThumb(snippet).trim();
      if (!title || !channel || !thumb) continue;
      const viewCount = Number.parseInt(String(stats.viewCount ?? "0"), 10) || 0;
      const published =
        typeof snippet.publishedAt === "string" ? snippet.publishedAt : null;
      rows.push({
        video_id: videoId,
        title,
        thumbnail_url: thumb,
        channel_name: channel,
        published_at: published,
        view_count: viewCount,
        fetched_at: now,
        query_tag: idToQuery.get(videoId) ?? "",
      });
    }

    rows.sort((a, b) => b.view_count - a.view_count);
    const top = rows.slice(0, FEED_TOP);

    // Replace entire feed (small table).
    const { error: delErr } = await admin
      .from("youtube_feed")
      .delete()
      .neq("video_id", "");
    if (delErr) {
      return jsonResponse(500, { error: `delete failed: ${delErr.message}` });
    }

    if (top.length > 0) {
      const { error: upsertErr } = await admin.from("youtube_feed").insert(top);
      if (upsertErr) {
        return jsonResponse(500, { error: `insert failed: ${upsertErr.message}` });
      }
    }

    return jsonResponse(200, {
      ok: true,
      candidates: allIds.length,
      stored: top.length,
    });
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    console.error("fetch-youtube-feed", msg);
    return jsonResponse(500, { error: msg });
  }
});
