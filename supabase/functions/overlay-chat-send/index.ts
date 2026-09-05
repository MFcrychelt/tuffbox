// TuffBox overlay chat: send a direct message to an accepted friend.
// Auth: writeSecret against cosmetics_profiles (see overlay-friends).

import { createClient } from "https://esm.sh/@supabase/supabase-js@2.49.1";

const corsHeaders: Record<string, string> = {
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Allow-Headers":
    "authorization, x-client-info, apikey, content-type",
};

function jsonResponse(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { ...corsHeaders, "Content-Type": "application/json" },
  });
}

function asString(v: unknown): string {
  return typeof v === "string" ? v : "";
}

async function sha256Hex(text: string): Promise<string> {
  const buf = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(text),
  );
  return Array.from(new Uint8Array(buf))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

function conversationId(a: string, b: string): string {
  return a < b ? `${a}:${b}` : `${b}:${a}`;
}

type ServiceClient = ReturnType<typeof createClient>;

async function authenticate(
  sb: ServiceClient,
  playerKey: string,
  username: string,
  writeSecret: string,
): Promise<Response | null> {
  if (!playerKey || !username || !writeSecret || writeSecret.length < 16) {
    return jsonResponse(400, {
      error: "playerKey, username, writeSecret (>=16) required",
    });
  }
  const secretHash = await sha256Hex(writeSecret);
  const { data: existing } = await sb
    .from("cosmetics_profiles")
    .select("player_key,write_secret_hash")
    .eq("player_key", playerKey)
    .maybeSingle();

  if (existing) {
    if (existing.write_secret_hash !== secretHash) {
      return jsonResponse(403, { error: "writeSecret mismatch" });
    }
    return null;
  }

  const { error } = await sb.from("cosmetics_profiles").insert({
    player_key: playerKey,
    username,
    write_secret_hash: secretHash,
    share_public: false,
  });
  if (error && error.code !== "23505") {
    return jsonResponse(500, { error: error.message });
  }
  return null;
}

Deno.serve(async (req) => {
  if (req.method === "OPTIONS") {
    return new Response("ok", { headers: corsHeaders });
  }
  if (req.method !== "POST") {
    return jsonResponse(405, { error: "method not allowed" });
  }

  const supabaseUrl = Deno.env.get("SUPABASE_URL") ?? "";
  const serviceKey = Deno.env.get("SUPABASE_SERVICE_ROLE_KEY") ?? "";
  if (!supabaseUrl || !serviceKey) {
    return jsonResponse(500, { error: "server misconfigured" });
  }

  let body: Record<string, unknown>;
  try {
    body = await req.json();
  } catch {
    return jsonResponse(400, { error: "invalid json" });
  }

  const playerKey = asString(body.playerKey).trim();
  const username = asString(body.username).trim();
  const writeSecret = asString(body.writeSecret).trim();
  const toKey = asString(body.toKey).trim();
  const text = asString(body.body).trim();

  if (!toKey) return jsonResponse(400, { error: "toKey required" });
  if (!text || text.length > 500) {
    return jsonResponse(400, { error: "body must be 1..500 chars" });
  }
  if (toKey === playerKey) return jsonResponse(400, { error: "cannot message yourself" });

  const sb = createClient(supabaseUrl, serviceKey);
  const authError = await authenticate(sb, playerKey, username, writeSecret);
  if (authError) return authError;

  // Anti-spam: only accepted friends can be messaged.
  const { data: friendship } = await sb
    .from("player_friendships")
    .select("id")
    .eq("status", "accepted")
    .or(
      `and(requester_key.eq.${playerKey},addressee_key.eq.${toKey}),` +
        `and(requester_key.eq.${toKey},addressee_key.eq.${playerKey})`,
    )
    .maybeSingle();
  if (!friendship) {
    return jsonResponse(403, { error: "not friends" });
  }

  const { data, error } = await sb
    .from("chat_messages")
    .insert({
      conversation_id: conversationId(playerKey, toKey),
      from_key: playerKey,
      from_name: username,
      to_key: toKey,
      body: text,
    })
    .select("id,created_at")
    .single();
  if (error) return jsonResponse(500, { error: error.message });

  return jsonResponse(200, { ok: true, id: data.id, createdAt: data.created_at });
});
