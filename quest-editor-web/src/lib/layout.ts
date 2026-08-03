/**
 * Auto-layout algorithms for quest arrangements.
 * Implements simple tree and grid layouts.
 */

import type { QuestData } from "./store";

export interface LayoutOptions {
  spacingX: number;
  spacingY: number;
  centerX: number;
  centerY: number;
}

const DEFAULT_OPTIONS: LayoutOptions = {
  spacingX: 3,
  spacingY: 3,
  centerX: 0,
  centerY: 0,
};

/**
 * Topological sort for dependency-based layout.
 * Returns quests in layers (levels) based on dependencies.
 */
function topologicalLayers(quests: QuestData[]): QuestData[][] {
  const questMap = new Map(quests.map((q) => [q.id, q]));
  const inDegree = new Map<string, number>();
  const layers: QuestData[][] = [];

  // Calculate in-degree (number of dependencies)
  for (const q of quests) {
    const deps = q.dependencies.filter((d) => questMap.has(d));
    inDegree.set(q.id, deps.length);
  }

  let currentLayer = quests.filter((q) => (inDegree.get(q.id) ?? 0) === 0);

  while (currentLayer.length > 0) {
    layers.push(currentLayer);

    // Reduce in-degree for dependent quests
    const nextLayer: QuestData[] = [];
    for (const q of currentLayer) {
      for (const other of quests) {
        if (other.dependencies.includes(q.id)) {
          const deg = (inDegree.get(other.id) ?? 1) - 1;
          inDegree.set(other.id, deg);
          if (deg === 0 && !layers.flat().includes(other)) {
            nextLayer.push(other);
          }
        }
      }
    }
    currentLayer = nextLayer;
  }

  return layers;
}

/**
 * Tree layout: arranges quests in a tree structure based on dependencies.
 */
export function layoutTree(
  quests: QuestData[],
  options: Partial<LayoutOptions> = {}
): Map<string, { x: number; y: number }> {
  const opts = { ...DEFAULT_OPTIONS, ...options };
  const result = new Map<string, { x: number; y: number }>();

  if (quests.length === 0) return result;

  const layers = topologicalLayers(quests);
  const maxLayerWidth = Math.max(...layers.map((l) => l.length));

  // Center the layout
  const totalWidth = (maxLayerWidth - 1) * opts.spacingX;
  const totalHeight = (layers.length - 1) * opts.spacingY;
  const startX = opts.centerX - totalWidth / 2;
  const startY = opts.centerY - totalHeight / 2;

  for (let layerIdx = 0; layerIdx < layers.length; layerIdx++) {
    const layer = layers[layerIdx];
    const layerWidth = (layer.length - 1) * opts.spacingX;
    const layerStartX = opts.centerX - layerWidth / 2;

    for (let questIdx = 0; questIdx < layer.length; questIdx++) {
      const q = layer[questIdx];
      result.set(q.id, {
        x: layerStartX + questIdx * opts.spacingX,
        y: startY + layerIdx * opts.spacingY,
      });
    }
  }

  return result;
}

/**
 * Grid layout: arranges quests in a grid pattern.
 */
export function layoutGrid(
  quests: QuestData[],
  options: Partial<LayoutOptions> = {}
): Map<string, { x: number; y: number }> {
  const opts = { ...DEFAULT_OPTIONS, ...options };
  const result = new Map<string, { x: number; y: number }>();

  if (quests.length === 0) return result;

  const cols = Math.ceil(Math.sqrt(quests.length));
  const rows = Math.ceil(quests.length / cols);

  const totalWidth = (cols - 1) * opts.spacingX;
  const totalHeight = (rows - 1) * opts.spacingY;
  const startX = opts.centerX - totalWidth / 2;
  const startY = opts.centerY - totalHeight / 2;

  for (let i = 0; i < quests.length; i++) {
    const col = i % cols;
    const row = Math.floor(i / cols);
    result.set(quests[i].id, {
      x: startX + col * opts.spacingX,
      y: startY + row * opts.spacingY,
    });
  }

  return result;
}

/**
 * Circular layout: arranges quests in a circle.
 */
export function layoutCircle(
  quests: QuestData[],
  options: Partial<LayoutOptions> = {}
): Map<string, { x: number; y: number }> {
  const opts = { ...DEFAULT_OPTIONS, ...options };
  const result = new Map<string, { x: number; y: number }>();

  if (quests.length === 0) return result;

  const radius = Math.max(quests.length * 0.5, 3);

  for (let i = 0; i < quests.length; i++) {
    const angle = (2 * Math.PI * i) / quests.length - Math.PI / 2;
    result.set(quests[i].id, {
      x: opts.centerX + radius * Math.cos(angle),
      y: opts.centerY + radius * Math.sin(angle),
    });
  }

  return result;
}

/**
 * Apply a layout result to quests, updating their positions.
 */
export function applyLayout(
  quests: QuestData[],
  positions: Map<string, { x: number; y: number }>
): QuestData[] {
  return quests.map((q) => {
    const pos = positions.get(q.id);
    if (pos) {
      return { ...q, x: Math.round(pos.x * 2) / 2, y: Math.round(pos.y * 2) / 2 };
    }
    return q;
  });
}
