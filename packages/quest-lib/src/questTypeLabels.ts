/** Human-readable FTB Quests task / reward type labels. */

export type QuestTypeOption = { id: string; label: string };

export const TASK_TYPE_OPTIONS: readonly QuestTypeOption[] = [
  { id: "item", label: "Item" },
  { id: "checkmark", label: "Checkmark" },
  { id: "kill", label: "Kill entity" },
  { id: "dimension", label: "Visit dimension" },
  { id: "biome", label: "Visit biome" },
  { id: "xp", label: "Experience" },
  { id: "advancement", label: "Advancement" },
  { id: "stat", label: "Statistic" },
  { id: "stage", label: "Game stage" },
  { id: "fluid", label: "Fluid" },
  { id: "location", label: "Location" },
  { id: "observation", label: "Observation" },
  { id: "structure", label: "Structure" },
  { id: "custom", label: "Custom" },
] as const;

export const REWARD_TYPE_OPTIONS: readonly QuestTypeOption[] = [
  { id: "item", label: "Item" },
  { id: "xp", label: "Experience points" },
  { id: "xp_levels", label: "Experience levels" },
  { id: "command", label: "Command" },
  { id: "random", label: "Random table" },
  { id: "choice", label: "Choice table" },
  { id: "loot", label: "Loot crate" },
  { id: "all_tables", label: "All reward tables" },
  { id: "stage", label: "Game stage" },
  { id: "toast", label: "Toast" },
  { id: "custom", label: "Custom" },
] as const;

export const TASK_TYPES: readonly string[] = TASK_TYPE_OPTIONS.map((o) => o.id);
export const REWARD_TYPES: readonly string[] = REWARD_TYPE_OPTIONS.map((o) => o.id);

const TASK_LABELS = Object.fromEntries(TASK_TYPE_OPTIONS.map((o) => [o.id, o.label]));
const REWARD_LABELS = Object.fromEntries(REWARD_TYPE_OPTIONS.map((o) => [o.id, o.label]));

export function taskTypeLabel(id: string): string {
  return TASK_LABELS[id] ?? id;
}

export function rewardTypeLabel(id: string): string {
  return REWARD_LABELS[id] ?? id;
}

export const SHAPE_OPTIONS: readonly QuestTypeOption[] = [
  { id: "", label: "(chapter default)" },
  { id: "circle", label: "Circle" },
  { id: "square", label: "Square" },
  { id: "rsquare", label: "Rounded square" },
  { id: "diamond", label: "Diamond" },
  { id: "hexagon", label: "Hexagon" },
  { id: "pentagon", label: "Pentagon" },
  { id: "gear", label: "Gear" },
  { id: "none", label: "None" },
] as const;

export const DEP_REQUIREMENT_OPTIONS: readonly QuestTypeOption[] = [
  { id: "", label: "(default)" },
  { id: "all_completed", label: "All completed" },
  { id: "one_completed", label: "One completed" },
  { id: "all_started", label: "All started" },
  { id: "one_started", label: "One started" },
] as const;
