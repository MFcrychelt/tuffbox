import { api } from "../../lib/api";

/** Shared item-icon data-URL cache for quest UI. */
const cache: Record<string, string | null> = {};
const inflight = new Set<string>();

export function normalizeItemId(id: string | null | undefined): string | null {
  if (!id) return null;
  const t = id.trim();
  if (!t || t.startsWith("#")) return null;
  return t;
}

export function glyphFromItemId(id: string | null | undefined, fallback = "?"): string {
  const n = normalizeItemId(id);
  if (!n) return (fallback[0] || "?").toUpperCase();
  const leaf = n.includes(":") ? n.split(":").pop()! : n;
  return (leaf[0] || fallback[0] || "?").toUpperCase();
}

export function getCachedIcon(id: string | null | undefined): string | null | undefined {
  const n = normalizeItemId(id);
  if (!n) return null;
  if (Object.prototype.hasOwnProperty.call(cache, n)) return cache[n];
  return undefined;
}

export async function preloadItemIcons(
  ids: string[],
  projectPath: string,
): Promise<Record<string, string | null>> {
  const need = [
    ...new Set(
      ids
        .map(normalizeItemId)
        .filter((x): x is string => !!x)
        .filter((id) => cache[id] === undefined && !inflight.has(id)),
    ),
  ];
  for (const id of need) {
    inflight.add(id);
    cache[id] = null;
  }
  try {
    for (let i = 0; i < need.length; i += 48) {
      const chunk = need.slice(i, i + 48);
      const batch = await api.recipes.itemIconsBatch(chunk, projectPath);
      for (const id of chunk) {
        cache[id] = batch[id] ?? null;
        inflight.delete(id);
      }
    }
  } catch {
    for (const id of need) {
      cache[id] = null;
      inflight.delete(id);
    }
  }
  const out: Record<string, string | null> = {};
  for (const raw of ids) {
    const n = normalizeItemId(raw);
    if (n && cache[n] !== undefined) out[n] = cache[n];
  }
  return out;
}
