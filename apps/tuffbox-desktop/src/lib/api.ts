import { invoke } from "@tauri-apps/api/core";
import { projectPath, type AuthState, type McProfile, type DeviceCodeInfo, type SkinSource, type AccountEntry, type McCapeEntry, type CapeProvider, type CapeCatalog, type YggdrasilPreset, type PresenceSettings } from "./store";
import { get } from "svelte/store";

// ─── Types ──────────────────────────────────────────────────────────

export interface ProjectSummary {
  id: string;
  name: string;
  version: string;
  minecraftVersion: string;
  loaderKind: string;
  loaderVersion: string;
  javaPath: string | null;
  memoryMb: number;
  jvmArgs: string[];
  playerName: string;
  /** Canonical `.tuffbox.json` path (may differ from the path passed to validate). */
  manifestPath: string;
}

export interface SchemaStatus {
  current: string;
  detected: string;
  needsMigration: boolean;
  supported: string[];
}

export interface ConfigFileSummary {
  path: string;
  name: string;
  extension: string;
  size: number;
  modified: number | null;
}

export interface QuestTask {
  id: string;
  type: string;
  title?: string | null;
  value?: unknown;
  properties?: Record<string, unknown>;
}

export interface QuestReward {
  id: string;
  type: string;
  title?: string | null;
  properties?: Record<string, unknown>;
}

export interface QuestData {
  id: string;
  title: string;
  subtitle?: string | null;
  description: string[];
  x: number;
  y: number;
  icon?: string | null;
  dependencies: string[];
  tasks: QuestTask[];
  rewards: QuestReward[];
  optional: boolean;
  shape?: string | null;
  size?: number | null;
}

export interface QuestChapter {
  id: string;
  title: string;
  icon?: string | null;
  quests: QuestData[];
  group?: string | null;
  sourceFile?: string | null;
}

export interface QuestBook {
  chapters: QuestChapter[];
  title?: string | null;
  subtitle?: string | null;
}

export interface QuestValidationIssue {
  questId: string;
  message: string;
}

export interface IngredientDisplay {
  id: string;
  kind?: string;
  name?: string;
  count?: number;
  tooltip?: string[];
  iconUrl?: string | null;
  alts?: IngredientDisplay[];
}

export interface RuntimeRecipeSlot {
  role: string;
  name?: string | null;
  x: number;
  y: number;
  width: number;
  height: number;
  ingredients: IngredientDisplay[];
}

export interface RecipeLayout {
  category: string;
  shapeless: boolean;
  grid: (IngredientDisplay | null)[];
  output: IngredientDisplay;
  outputCount: number;
  cookTime?: number;
  experience?: number;
  slots?: RuntimeRecipeSlot[];
}

export interface ScannedRecipe {
  id: string;
  recipeType: string;
  category: string;
  modSource: string;
  sourceFile: string;
  layout: RecipeLayout;
  inputIds: string[];
  outputId: string;
  isConditional: boolean;
}

export interface RecipeScanResult {
  recipes: ScannedRecipe[];
  jarCount: number;
  datapackFiles: number;
  truncated: boolean;
  totalScanned: number;
}

export interface RecipeRuntimeStatus {
  connected: boolean;
  supported: boolean;
  message: string;
  minecraftVersion?: string | null;
  pid?: number | null;
}

export interface RuntimeRecipeCategory {
  id: string;
  title: string;
  width: number;
  height: number;
  stations: IngredientDisplay[];
}

export interface RecipeRuntimeSnapshot extends RecipeScanResult {
  source: "runtime";
  generatedAt: string;
  protocolVersion: number;
  categories: RuntimeRecipeCategory[];
}

export interface KubeJsScript {
  kind: string;
  filename: string;
  content: string;
}

export interface CraftDraft {
  /** shaped | shapeless | smelting | blasting | smoking | campfire | smithing | stonecutting */
  kind?: string | null;
  shaped: boolean;
  /** Row-major 3×3; null = empty. Tags use `#ns:path`. */
  grid: (string | null)[];
  output: string;
  outputCount: number;
  replaceId?: string | null;
  input?: string | null;
  xp?: number | null;
  cookTime?: number | null;
  template?: string | null;
  base?: string | null;
  addition?: string | null;
}

export interface TagDraft {
  tagId: string;
  add: string[];
  remove: string[];
  removeAll?: boolean;
}

export interface ProfileSummary {
  id: string;
  name: string;
  side: string;
  memoryMb: number | null;
  jvmArgs: string[];
}

export interface PackBrief {
  goal: string;
  targetAudience: string;
  gameplayPillars: string[];
  constraints: string[];
  releaseTargets: string[];
  notes: string;
}

export interface ModDependencySpec {
  type: "requires" | "optional" | "recommended" | "incompatible" | "embedded";
  target: string;
  versionConstraint?: string | null;
  reason?: string | null;
}

export interface ModInstallPreview {
  projectId: string;
  slug: string;
  name: string;
  version: string;
  fileName: string | null;
  side: string;
  dependencies: ModDependencySpec[];
}

export interface GraphNode {
  id: string;
  kind: string;
  label: string;
  version: string | null;
  side: string;
  metadata: Record<string, string>;
}

export interface GraphEdge {
  from: string;
  to: string;
  kind: string;
  constraint: string | null;
  reason: string | null;
}

export interface DependencyGraph {
  nodes: GraphNode[];
  edges: GraphEdge[];
  source?: "local" | "cache" | "network" | string;
  generatedAt?: string | null;
}

export interface Diagnostic {
  severity: "Info" | "Warning" | "Error" | string;
  code: string;
  message: string;
  relatedNodes: string[];
}

export interface ChangePlan {
  summary: string;
  risk: string;
  actions: ChangeAction[];
  requiresSnapshot: boolean;
}

export interface ChangeAction {
  InstallMod?: { projectId: string; version?: string | null };
  RemoveMod?: { nodeId: string };
  DisableMod?: { nodeId: string };
  UpdateMod?: { nodeId: string; targetVersion: string };
  EditConfig?: { path: string; patch: string };
}

export interface HistorySettings {
  tracked: Record<string, boolean>;
}

export interface ProjectChangeEntry {
  id: string;
  snapshotId: string;
  operation: string;
  reason: string;
  createdAt: string;
  path: string;
  category: string;
  kind: string;
  preview: string;
  diff: string;
  canOpen: boolean;
}

export interface HistoryFileContent {
  path: string;
  content: string;
}

export interface Snapshot {
  id: string;
  name: string;
  createdAt: string;
  reason: string;
  manifestPath: string;
  lockfilePath: string | null;
  changedFiles: string[];
}

export interface SnapshotDiff {
  addedFiles: string[];
  removedFiles: string[];
  modifiedFiles: string[];
}

export interface SnapshotFileDiff {
  path: string;
  fromExists: boolean;
  toExists: boolean;
  text: string;
}

export interface TestRunRecord {
  id: string;
  profile: string;
  startedAt: string;
  status: string;
  logPath: string;
  durationSeconds: number | null;
}

export interface LaunchResult {
  exitCode: number | null;
  logPath: string;
}

/// Structured launch error returned by the launch Tauri commands
/// (mirrors `tuffbox_core::launch_error::LaunchErrorInfo`).
export interface LaunchErrorInfo {
  kind: string;
  message: string;
  logPath?: string;
}

export interface ExportResult {
  path: string;
  fileCount: number;
  overrideCount: number;
}

export interface ExportIssue {
  severity: "error" | "warning";
  code: string;
  message: string;
  target: string | null;
}

export interface ReleaseSnapshotResult {
  snapshot: Snapshot;
  changelogPath: string;
}

export interface ReleaseArtifactRecord {
  id: string;
  kind: string;
  path: string;
  createdAt: string;
  fileCount: number;
  overrideCount: number;
}

export interface ReleaseDraftResult {
  draftPath: string;
  metadataPath: string;
  artifactCount: number;
}

export interface ModInfo {
  id: string;
  name: string;
  version: string;
  side: string;
  source: string;
  projectId?: string | null;
  fileName?: string | null;
  iconUrl?: string | null;
  clientSide?: string | null;
  serverSide?: string | null;
  contentType?: string;
}

export interface SearchResult {
  id: string;
  slug: string;
  name: string;
  description: string;
  projectType: string;
  iconUrl?: string | null;
  clientSide?: string | null;
  serverSide?: string | null;
  author?: string | null;
  downloads?: number | null;
  follows?: number | null;
  dateModified?: string | null;
  categories?: string[];
}

export interface CrashAnalysisFinding {
  severity: string;
  code: string;
  title: string;
  description: string;
  autoFix: string | null;
  references: string[];
}

export interface CrashAnalysisReport {
  findings: CrashAnalysisFinding[];
  supportMessageDiscord: string;
  supportMessageGithub: string;
  modsAdded: string[];
  modsRemoved: string[];
  suspectedMods: string[];
  mcreatorMods: string[];
  classFinderResults: ClassMatch[];
}

export interface ClassMatch {
  className: string;
  modId: string;
  modName: string;
}

export interface ModSyncFailure {
  modId: string;
  error: string;
}

export interface ModSyncReport {
  downloaded: string[];
  alreadyPresent: string[];
  skipped: string[];
  failed: ModSyncFailure[];
}

export interface TuffboxLockfile {
  schemaVersion: string;
  projectId: string;
  projectVersion: string;
  minecraftVersion: string;
  loader: { kind: string; version: string };
  javaMajor: number | null;
  mods: LockedMod[];
  graph: LockedGraphData;
  generatedAt: string;
}

export interface LockedMod {
  id: string;
  name: string;
  version: string;
  source: LockedSource;
  fileName: string | null;
  hashes: LockedHashes;
  side: string;
}

export interface LockedSource {
  kind: string;
  projectId: string | null;
  fileId: string | null;
  url: string | null;
  path: string | null;
}

export interface LockedHashes {
  sha1: string | null;
  sha512: string | null;
}

export interface LockedGraphData {
  nodeCount: number;
  edgeCount: number;
  edges: LockedEdgeData[];
}

export interface LockedEdgeData {
  from: string;
  to: string;
  kind: string;
  constraint: string | null;
  reason: string | null;
}

export interface TemplateInfo {
  id: string;
  name: string;
  minecraftVersion: string;
  loader: string;
  description: string;
}

export interface WorldInfo {
  name: string;
  size: string;
  lastPlayed: string | null;
  gameType: string;
  difficulty: string;
  seed: number;
  spawnX: number;
  spawnY: number;
  spawnZ: number;
  time: number;
  raining: boolean;
  thundering: boolean;
}

export interface WorldListItem {
  name: string;
  size: number;
  sizeFormatted: string;
  hasLevelDat: boolean;
}

export interface ContentPackEntry {
  name: string;
  fileName: string;
  enabled: boolean;
  kind: string;
  size: number;
  sizeFormatted: string;
}

export interface McServerEntry {
  name: string;
  address: string;
  icon: string | null;
  acceptTextures: number | null;
}

export interface McServerPing {
  address: string;
  online: boolean;
  latencyMs: number | null;
  error: string | null;
}

export interface WorldDetail {
  name: string;
  seed: number;
  gameType: string | number;
  difficulty: string | number;
  lastPlayed: string | null;
  time: number;
  spawnX: number;
  spawnY: number;
  spawnZ: number;
  hardcore: boolean;
  cheatsEnabled: boolean;
  sizeBytes: number;
  sizeFormatted: string;
}

export interface ChunkCell {
  present: number;
  lastModified: number;
  status: number;
  inhabitedTime?: number;
  dataVersion?: number;
  biomeId?: number;
  surfaceY?: number;
  entityCount?: number;
  structureCount?: number;
}

export interface RegionInfo {
  regionX: number;
  regionZ: number;
  present: number;
  minModified: number;
  maxModified: number;
  chunks: ChunkCell[];
}

export type WorldDimension = "overworld" | "nether" | "end";

export interface WorldMap {
  regions: RegionInfo[];
  minRegionX: number;
  minRegionZ: number;
  maxRegionX: number;
  maxRegionZ: number;
  totalPresent: number;
  regionCount: number;
  dimension?: WorldDimension | string;
}

export interface ChunkData {
  regionX: number;
  regionZ: number;
  index: number;
  data: number[];
  lastModified: number;
}

export interface ChunkClipboard {
  sourceWorld: string;
  chunks: ChunkData[];
  bounds: [number, number, number, number];
  entities?: ChunkData[];
  poi?: ChunkData[];
}

export interface NbtNode {
  tagType: number;
  name: string;
  value?: unknown;
  children?: NbtNode[];
  listType?: number;
}

export interface ChunkEditorData {
  regionX: number;
  regionZ: number;
  index: number;
  chunkX: number;
  chunkZ: number;
  layer: string;
  root: NbtNode;
}

export interface NbtChangeRequest {
  inhabitedTime?: number | null;
  status?: string | null;
  dataVersion?: number | null;
  lightPopulated?: number | null;
  lastUpdate?: number | null;
  biome?: string | null;
  deleteSections?: string | null;
  replaceBlocks?: string | null;
  deleteStructureRefs?: string | null;
  preventRetrogen?: boolean;
  forceBlend?: boolean;
  deleteEntities?: boolean;
  fixStatus?: boolean;
  force?: boolean;
}

export interface AdvancedChunkFilter {
  entityNames?: string | null;
  structureNames?: string | null;
  paletteNames?: string | null;
  minEntities?: number | null;
  maxEntities?: number | null;
}

export interface ChunkRef {
  regionX: number;
  regionZ: number;
  index: number;
}

export interface JavaRuntime {
  path: string;
  version: string;
  major: number;
}

export interface MinecraftVersion {
  id: string;
  type: "release" | "snapshot" | "old_beta" | "old_alpha";
}

export interface LoaderVersion {
  version: string;
  stable: boolean;
}

export interface RunningInstance {
  id: string;
  pid: number;
  profile: string;
  startedAt: string;
}

export interface KeyboardShortcut {
  key: string;
  description: string;
  category: string;
}

export interface BackupEntry {
  id: string;
  name: string;
  createdAt: string;
  sizeBytes: number;
  fileCount: number;
}

export interface LocalizationEntry {
  key: string;
  ru: string;
}

export interface ProjectStats {
  totalLaunches: number;
  totalCrashes: number;
  totalPlaytimeSeconds: number;
  lastLaunch: string | null;
  byProfile: Array<{
    id: string;
    launches: number;
    crashes: number;
    playtimeSeconds: number;
    lastLaunch: string | null;
  }>;
}

export interface ConfigSearchMatch {
  path: string;
  line: number;
  column: number;
  lineContent: string;
}

export interface LintResult {
  severity: "error" | "warning";
  line: number;
  column: number;
  message: string;
  code: string;
}

export interface ProjectValidationResult {
  valid: boolean;
  errors: string[];
  warnings: string[];
}

export interface HealthReport {
  diagnostics: Diagnostic[];
  crashStatus: { hasCrashed: boolean; reportCount: number };
  performanceIssues: string[];
}

// ─── API wrapper ────────────────────────────────────────────────────

function pathArg(p?: string): { path: string } {
  return { path: p ?? get(projectPath) ?? "" };
}

async function cmd<T>(name: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(name, args);
  } catch (e) {
    console.error(`[api] ${name} failed:`, e);
    throw e;
  }
}

// ─── Domain API ─────────────────────────────────────────────────────

export const api = {

  // ── Project ───────────────────────────────────────────────────────
  project: {
    validate(p?: string) { return cmd<ProjectSummary>("validate_project", pathArg(p)); },
    resolvePath(p?: string) { return cmd<string>("resolve_project_path", pathArg(p)); },
    getSchemaStatus(p?: string) { return cmd<SchemaStatus>("get_project_schema_status", pathArg(p)); },
    migrateSchema(p?: string) { return cmd<SchemaStatus>("migrate_project_schema", pathArg(p)); },
    getBrief(p?: string) { return cmd<PackBrief>("get_project_brief", pathArg(p)); },
    updateBrief(brief: PackBrief, p?: string) { return cmd<void>("update_project_brief", { ...pathArg(p), brief }); },
    getDir(p?: string) { return cmd<string>("get_project_dir", pathArg(p)); },
    getManifestSchema(p?: string) { return cmd<Record<string, unknown>>("get_manifest_schema", pathArg(p)); },
    runValidation(p?: string) { return cmd<Record<string, unknown>>("run_project_validation", pathArg(p)); },
    getDiagnostics(p?: string) { return cmd<Diagnostic[]>("get_diagnostics", pathArg(p)); },
    repair(p?: string) { return cmd<ModSyncReport>("repair_project", pathArg(p)); },
    cleanup(p?: string) { return cmd<Record<string, unknown>>("cleanup_project", pathArg(p)); },
    listProfiles(p?: string) { return cmd<ProfileSummary[]>("list_profiles", pathArg(p)); },
  },

  // ── Mods ──────────────────────────────────────────────────────────
  mods: {
    list(p?: string) { return cmd<ModInfo[]>("list_mods", pathArg(p)); },
    syncFolder(p?: string) { return cmd<Record<string, unknown>[]>("sync_mods_folder", pathArg(p)); },
    add(modId: string, side: string, p?: string) { return cmd<void>("add_modrinth_mod", { ...pathArg(p), modId, side }); },
    addWithDeps(modId: string, side: string, p?: string) { return cmd<string[]>("add_modrinth_mod_with_dependencies", { ...pathArg(p), modId, side }); },
    addManyWithDeps(modIds: string[], side: string, p?: string) { return cmd<string[]>("add_modrinth_mods_with_dependencies", { ...pathArg(p), modIds, side }); },
    remove(modId: string, p?: string) { return cmd<void>("remove_project_mod", { ...pathArg(p), modId }); },
    update(modId: string, p?: string, versionId?: string | null) {
      return cmd<Record<string, unknown>>("update_project_mod", {
        ...pathArg(p),
        modId,
        versionId: versionId ?? null,
      });
    },
    changeVersion(modId: string, newVersionId: string, p?: string) { return cmd<Record<string, unknown>>("change_mod_version", { ...pathArg(p), modId, newVersionId }); },
    getVersions(modId: string, minecraftVersion: string, loader?: string | null) { return cmd<Record<string, unknown>[]>("get_mod_versions", { modId, minecraftVersion, loader }); },
    checkUpdates(p?: string) { return cmd<Record<string, unknown>[]>("check_mod_updates", pathArg(p)); },
    updateAll(p?: string) {
      return cmd<{ updated: string[]; errors?: string[]; download?: Record<string, unknown> }>("update_all_mods", pathArg(p));
    },
    retryFailedDownloads(modIds: string[], p?: string) {
      return cmd<Record<string, unknown>>("retry_failed_mod_downloads", { ...pathArg(p), modIds });
    },
    recommend(p?: string) { return cmd<Record<string, unknown>[]>("recommend_mods", pathArg(p)); },
    disable(modId: string, p?: string) {
      return cmd<{ id: string; disabled: boolean; fileName?: string }>("disable_project_mod", {
        ...pathArg(p),
        modId,
      });
    },
    enable(modId: string, p?: string) {
      return cmd<{ id: string; disabled: boolean; fileName?: string }>("enable_project_mod", {
        ...pathArg(p),
        modId,
      });
    },
    detectWrongLoader(p?: string) { return cmd<Record<string, unknown>[]>("detect_wrong_loader_mods", pathArg(p)); },
    disableJar(fileName: string, p?: string) { return cmd<string>("disable_wrong_loader_jar", { ...pathArg(p), fileName }); },
    removeLooseJar(fileName: string, p?: string) { return cmd<string>("remove_loose_jar", { ...pathArg(p), fileName }); },
    checkCompatibility(p?: string) { return cmd<Record<string, unknown>[]>("check_mod_compatibility", pathArg(p)); },
    getInfo(slug: string) { return cmd<Record<string, unknown> | null>("get_mod_info", { slug }); },
    compareModpacks(pathA: string, pathB: string) { return cmd<Record<string, unknown>>("compare_modpacks", { pathA, pathB }); },

    // Modrinth search
    search(query: string, opts?: {
      gameVersion?: string; loader?: string; category?: string;
      environment?: string; license?: string; sort?: string; contentType?: string;
      p?: string;
    }) {
      const { p, ...rest } = opts ?? {};
      return cmd<SearchResult[]>("search_modrinth_mods", { ...pathArg(p), query, ...rest });
    },
    previewInstall(modId: string, p?: string) { return cmd<ModInstallPreview>("preview_modrinth_install", { ...pathArg(p), modId }); },
    getIcon(projectId: string) { return cmd<string | null>("get_modrinth_project_icon", { projectId }); },
    getProject(projectId: string) { return cmd<SearchResult>("get_modrinth_project", { projectId }); },
    getUserState(p?: string) {
      return cmd<{ favorites: Record<string, boolean>; lists: Record<string, string[]>; ratings: Record<string, number> }>("get_mod_user_state", pathArg(p));
    },
    setUserState(modId: string, patch: { favorite?: boolean; saved?: boolean; rating?: number }, p?: string) {
      return cmd<{ favorites: Record<string, boolean>; lists: Record<string, string[]>; ratings: Record<string, number> }>("set_mod_user_state", {
        ...pathArg(p), modId,
        favorite: patch.favorite ?? null,
        saved: patch.saved ?? null,
        rating: patch.rating ?? null,
      });
    },
    createList(name: string, p?: string) {
      return cmd<{ favorites: Record<string, boolean>; lists: Record<string, string[]>; ratings: Record<string, number> }>("create_mod_list", { ...pathArg(p), name });
    },
    deleteList(name: string, p?: string) {
      return cmd<{ favorites: Record<string, boolean>; lists: Record<string, string[]>; ratings: Record<string, number> }>("delete_mod_list", { ...pathArg(p), name });
    },
    renameList(oldName: string, newName: string, p?: string) {
      return cmd<{ favorites: Record<string, boolean>; lists: Record<string, string[]>; ratings: Record<string, number> }>("rename_mod_list", { ...pathArg(p), oldName, newName });
    },
    addToList(name: string, modId: string, p?: string) {
      return cmd<{ favorites: Record<string, boolean>; lists: Record<string, string[]>; ratings: Record<string, number> }>("add_to_mod_list", { ...pathArg(p), name, modId });
    },
    removeFromList(name: string, modId: string, p?: string) {
      return cmd<{ favorites: Record<string, boolean>; lists: Record<string, string[]>; ratings: Record<string, number> }>("remove_from_mod_list", { ...pathArg(p), name, modId });
    },
  },

  // ── Config ────────────────────────────────────────────────────────
  config: {
    list(p?: string) { return cmd<ConfigFileSummary[]>("list_config_files", pathArg(p)); },
    read(relativePath: string, p?: string) { return cmd<string>("read_config_file", { ...pathArg(p), relativePath }); },
    write(relativePath: string, content: string, p?: string) { return cmd<void>("write_config_file", { ...pathArg(p), relativePath, content }); },
    search(query: string, p?: string) { return cmd<ConfigSearchMatch[]>("search_in_configs", { ...pathArg(p), query }); },
    lint(relativePath: string, p?: string) { return cmd<LintResult[]>("lint_config", { ...pathArg(p), relativePath }); },
  },

  // ── Graph & Resolve ───────────────────────────────────────────────
  graph: {
    get(p?: string) { return cmd<DependencyGraph>("get_graph", pathArg(p)); },
    refresh(p?: string) { return cmd<DependencyGraph>("refresh_graph", pathArg(p)); },
    getResolvePlan(p?: string) { return cmd<ChangePlan | null>("get_resolve_change_plan", pathArg(p)); },
    applyAction(actionIndex: number, p?: string) { return cmd<string[]>("apply_resolve_action", { ...pathArg(p), actionIndex }); },
    applyPlan(p?: string) { return cmd<string[]>("apply_resolve_change_plan", pathArg(p)); },
    resolveMissing(p?: string) { return cmd<string[]>("resolve_missing_dependencies", pathArg(p)); },
    exportDot(p?: string) { return cmd<string>("export_graph_dot", pathArg(p)); },
  },

  // ── Launch ────────────────────────────────────────────────────────
  launch: {
    profile(profile: string, p?: string) { return cmd<LaunchResult>("launch_profile", { ...pathArg(p), profile }); },
    server(p?: string) { return cmd<LaunchResult>("launch_server", pathArg(p)); },
    quickPlay(profile: string, quickPlayType?: string | null, quickPlayValue?: string | null, p?: string) {
      return cmd<LaunchResult>("launch_with_quick_play", { ...pathArg(p), profile, quickPlayType, quickPlayValue });
    },
    listRunning() { return cmd<RunningInstance[]>("list_running_instances"); },
    kill(instanceId: string) { return cmd<string>("kill_running_instance", { instanceId }); },
    generateServerProperties(p?: string) { return cmd<string>("generate_server_properties", pathArg(p)); },
  },

  // ── Stats & History ───────────────────────────────────────────────
  stats: {
    recordLaunch(p?: string) { return cmd<void>("record_launch", pathArg(p)); },
    recordCrash(p?: string) { return cmd<void>("record_crash", pathArg(p)); },
    get(p?: string) { return cmd<ProjectStats>("get_launch_stats", pathArg(p)); },
  },
  history: {
    getSettings(p?: string) { return cmd<HistorySettings>("get_history_settings", pathArg(p)); },
    updateSettings(settings: HistorySettings, p?: string) { return cmd<HistorySettings>("update_history_settings", { ...pathArg(p), settings }); },
    list(p?: string) { return cmd<ProjectChangeEntry[]>("list_project_change_history", pathArg(p)); },
    readFile(relativePath: string, p?: string) { return cmd<HistoryFileContent>("read_project_history_file", { ...pathArg(p), relativePath }); },
    createSnapshot(roots: string[], p?: string) { return cmd<Snapshot>("create_tracked_history_snapshot", { ...pathArg(p), roots }); },
    rollbackFile(snapshotId: string, relativePath: string, p?: string) { return cmd<void>("rollback_history_file", { ...pathArg(p), snapshotId, relativePath }); },
  },

  // ── Snapshots ─────────────────────────────────────────────────────
  snapshots: {
    list(projectDir?: string) { return cmd<Snapshot[]>("list_snapshots", { projectDir: projectDir ?? get(projectPath) ?? "" }); },
    create(name: string, reason: string, projectDir?: string) { return cmd<Snapshot>("create_snapshot", { projectDir: projectDir ?? get(projectPath) ?? "", name, reason }); },
    diff(from: string, to: string, projectDir?: string) { return cmd<SnapshotDiff>("diff_snapshots", { projectDir: projectDir ?? get(projectPath) ?? "", from, to }); },
    rollback(id: string, projectDir?: string) { return cmd<Snapshot>("rollback_snapshot", { projectDir: projectDir ?? get(projectPath) ?? "", id }); },
    diffManifest(fromId: string, toId: string, projectDir?: string) { return cmd<Record<string, unknown>>("diff_manifest_snapshots", { projectDir: projectDir ?? get(projectPath) ?? "", fromId, toId }); },
    fileDiff(from: string, to: string, relativePath: string, projectDir?: string) { return cmd<SnapshotFileDiff>("get_snapshot_file_diff", { projectDir: projectDir ?? get(projectPath) ?? "", from, to, relativePath }); },
  },

  // ── Backups ───────────────────────────────────────────────────────
  backups: {
    create(name?: string | null, p?: string) { return cmd<BackupEntry>("create_project_backup", { ...pathArg(p), name }); },
    list(p?: string) { return cmd<BackupEntry[]>("list_backups", pathArg(p)); },
    delete(backupId: string, p?: string) { return cmd<void>("delete_backup", { ...pathArg(p), backupId }); },
    restore(backupId: string, p?: string) { return cmd<void>("restore_backup", { ...pathArg(p), backupId }); },
  },

  // ── Worlds ────────────────────────────────────────────────────────
  worlds: {
    list(p?: string) { return cmd<WorldListItem[]>("list_worlds", pathArg(p)); },
    readInfo(worldName: string, p?: string) { return cmd<WorldDetail>("read_world_info", { ...pathArg(p), worldName }); },
    backup(worldName: string, p?: string) { return cmd<string>("backup_world", { ...pathArg(p), worldName }); },
    dimensions(worldName: string, p?: string) {
      return cmd<string[]>("list_world_dimensions", { ...pathArg(p), worldName });
    },
    map(worldName: string, dimension?: string, p?: string) {
      return cmd<WorldMap>("read_world_map", { ...pathArg(p), worldName, dimension: dimension ?? "overworld" });
    },
    deleteChunks(
      worldName: string,
      selections: { regionX: number; regionZ: number; indices: number[] }[],
      dimension?: string,
      p?: string,
    ) {
      return cmd<number>("delete_world_chunks", {
        ...pathArg(p),
        worldName,
        selections,
        dimension: dimension ?? "overworld",
      });
    },
    copyChunks(
      worldName: string,
      selections: { regionX: number; regionZ: number; indices: number[] }[],
      dimension?: string,
      p?: string,
    ) {
      return cmd<ChunkClipboard>("copy_world_chunks", {
        ...pathArg(p),
        worldName,
        selections,
        dimension: dimension ?? "overworld",
      });
    },
    pasteChunks(
      worldName: string,
      clipboard: ChunkClipboard,
      offsetX?: number,
      offsetZ?: number,
      dimension?: string,
      p?: string,
    ) {
      return cmd<number>("paste_world_chunks", {
        ...pathArg(p),
        worldName,
        clipboard,
        offsetX: offsetX ?? 0,
        offsetZ: offsetZ ?? 0,
        dimension: dimension ?? "overworld",
      });
    },
    purge(worldName: string, dimension?: string, p?: string) {
      return cmd<number>("purge_world_regions", {
        ...pathArg(p),
        worldName,
        dimension: dimension ?? "overworld",
      });
    },
    exportChunks(
      worldName: string,
      selections: { regionX: number; regionZ: number; indices: number[] }[],
      destDir: string,
      dimension?: string,
      p?: string,
    ) {
      return cmd<number>("export_world_chunks", {
        ...pathArg(p),
        worldName,
        selections,
        destDir,
        dimension: dimension ?? "overworld",
      });
    },
    importChunks(
      worldName: string,
      sourceDir: string,
      opts?: {
        offsetX?: number;
        offsetZ?: number;
        overwrite?: boolean;
        yOffset?: number;
        sections?: string;
        sourceSelections?: { regionX: number; regionZ: number; indices: number[] }[];
        targetSelections?: { regionX: number; regionZ: number; indices: number[] }[];
        sourceDimension?: string;
        dimension?: string;
      },
      p?: string,
    ) {
      return cmd<number>("import_world_chunks", {
        ...pathArg(p),
        worldName,
        sourceDir,
        offsetX: opts?.offsetX ?? 0,
        offsetZ: opts?.offsetZ ?? 0,
        overwrite: opts?.overwrite ?? true,
        yOffset: opts?.yOffset ?? 0,
        sections: opts?.sections,
        sourceSelections: opts?.sourceSelections ?? [],
        targetSelections: opts?.targetSelections,
        sourceDimension: opts?.sourceDimension ?? opts?.dimension ?? "overworld",
        dimension: opts?.dimension ?? "overworld",
      });
    },
    selectByQuery(worldName: string, query: string, dimension?: string, p?: string) {
      return cmd<{ regionX: number; regionZ: number; index: number }[]>("select_world_by_query", {
        ...pathArg(p),
        worldName,
        query,
        dimension: dimension ?? "overworld",
      });
    },
    renderMapPng(
      worldName: string,
      destPath: string,
      opts?: {
        colorMode?: string;
        scale?: number;
        selections?: { regionX: number; regionZ: number; indices: number[] }[];
        dimension?: string;
      },
      p?: string,
    ) {
      return cmd<[number, number]>("render_world_map_png", {
        ...pathArg(p),
        worldName,
        destPath,
        colorMode: opts?.colorMode ?? "status",
        scale: opts?.scale ?? 4,
        selections: opts?.selections ?? [],
        dimension: opts?.dimension ?? "overworld",
      });
    },
    warmCache(worldName: string, dimension?: string, p?: string) {
      return cmd<number>("warm_world_map_cache", {
        ...pathArg(p),
        worldName,
        dimension: dimension ?? "overworld",
      });
    },
    clearCache(worldName: string, dimension?: string | null, p?: string) {
      return cmd<number>("clear_world_map_cache", {
        ...pathArg(p),
        worldName,
        dimension: dimension === undefined ? "overworld" : dimension,
      });
    },
    swapChunks(
      worldName: string,
      a: { regionX: number; regionZ: number; indices: number[] },
      b: { regionX: number; regionZ: number; indices: number[] },
      dimension?: string,
      p?: string,
    ) {
      return cmd<void>("swap_world_chunks", {
        ...pathArg(p),
        worldName,
        a,
        b,
        dimension: dimension ?? "overworld",
      });
    },
    changeChunks(
      worldName: string,
      selections: { regionX: number; regionZ: number; indices: number[] }[],
      change: NbtChangeRequest,
      dimension?: string,
      p?: string,
    ) {
      return cmd<number>("change_world_chunks", {
        ...pathArg(p),
        worldName,
        selections,
        change,
        dimension: dimension ?? "overworld",
      });
    },
    readChunkEditor(
      worldName: string,
      regionX: number,
      regionZ: number,
      index: number,
      dimension?: string,
      layer?: string,
      p?: string,
    ) {
      return cmd<ChunkEditorData>("read_chunk_editor", {
        ...pathArg(p),
        worldName,
        regionX,
        regionZ,
        index,
        dimension: dimension ?? "overworld",
        layer: layer ?? "region",
      });
    },
    writeChunkEditor(
      worldName: string,
      data: ChunkEditorData,
      dimension?: string,
      p?: string,
    ) {
      return cmd<void>("write_chunk_editor", {
        ...pathArg(p),
        worldName,
        data,
        dimension: dimension ?? "overworld",
      });
    },
    filterAdvanced(
      worldName: string,
      filter: AdvancedChunkFilter,
      selections?: { regionX: number; regionZ: number; indices: number[] }[],
      dimension?: string,
      p?: string,
    ) {
      return cmd<ChunkRef[]>("filter_world_chunks_advanced", {
        ...pathArg(p),
        worldName,
        filter,
        selections: selections ?? null,
        dimension: dimension ?? "overworld",
      });
    },
  },

  // ── Recipes (JEI-style browser) ─────────────────────────────────
  recipes: {
    scan(p?: string) { return cmd<RecipeScanResult>("scan_mod_recipes", pathArg(p)); },
    itemIcon(itemId: string, p?: string) {
      return cmd<string | null>("get_item_icon", { ...pathArg(p), itemId });
    },
    itemIconsBatch(itemIds: string[], p?: string) {
      return cmd<Record<string, string | null>>("get_item_icons_batch", { ...pathArg(p), itemIds });
    },
    runtimeStatus(p?: string) { return cmd<RecipeRuntimeStatus>("get_recipe_runtime_status", pathArg(p)); },
    runtimeSnapshot(p?: string) { return cmd<RecipeRuntimeSnapshot>("get_recipe_runtime_snapshot", pathArg(p)); },
    writeRemoves(recipeIds: string[], p?: string) {
      return cmd<string>("write_kubejs_recipe_removes", { ...pathArg(p), recipeIds });
    },
    writeCraft(draft: CraftDraft, p?: string) {
      return cmd<string>("write_kubejs_craft_recipe", { ...pathArg(p), draft });
    },
    writeTags(draft: TagDraft, p?: string) {
      return cmd<string>("write_kubejs_tag_edits", { ...pathArg(p), draft });
    },
    listItemTags(p?: string) {
      return cmd<string[]>("list_item_tags", pathArg(p));
    },
    getTagEntries(tagId: string, p?: string) {
      return cmd<string[]>("get_item_tag_entries", { ...pathArg(p), tagId });
    },
    generateScript(kind: string, recipeIds: string[], newItem?: string | null, count?: number | null) {
      return cmd<KubeJsScript>("generate_kubejs_recipe_script", {
        kind,
        recipeIds,
        newItem: newItem ?? null,
        count: count ?? null,
      });
    },
  },

  // ── Diagnostics & Crash ───────────────────────────────────────────
  diagnostics: {
    scanOre(p?: string) { return cmd<Record<string, unknown>[]>("scan_ore_generation", pathArg(p)); },
    auditPerformance(p?: string) { return cmd<Record<string, unknown>[]>("audit_performance", pathArg(p)); },
    detectDuplicateItems(p?: string) { return cmd<Record<string, unknown>[]>("detect_duplicate_items", pathArg(p)); },
    generateUnifyConfig(save?: boolean | null, p?: string) { return cmd<Record<string, unknown>>("generate_unify_config", { ...pathArg(p), save }); },
    findClass(className: string, p?: string) { return cmd<ClassMatch[]>("find_class_in_mods", { ...pathArg(p), className }); },
    findDependents(className: string, p?: string) { return cmd<ClassMatch[]>("find_dependents_on_class", { ...pathArg(p), className }); },
    hasCrashed(p?: string) { return cmd<boolean>("has_crashed", pathArg(p)); },
    getCrashDiagnosis(reportId?: string | null, p?: string) { return cmd<CrashAnalysisReport>("get_crash_diagnosis", { ...pathArg(p), reportId }); },
    createCrashFixPlan(reportId?: string | null, p?: string) { return cmd<ChangePlan>("create_crash_fix_plan", { ...pathArg(p), reportId }); },
    applyCrashFixPlan(reportId?: string | null, p?: string) { return cmd<string[]>("apply_crash_fix_plan", { ...pathArg(p), reportId }); },
    runCrashAssistantFull(p?: string) { return cmd<CrashAnalysisReport>("run_crash_assistant_full", pathArg(p)); },
    buildAiContext(reportId?: string | null, p?: string) {
      return cmd<Record<string, unknown>>("build_ai_crash_context", {
        ...pathArg(p),
        reportId: reportId ?? null,
      });
    },
    analyzeWithAi(reportId?: string | null, p?: string) {
      return cmd<Record<string, unknown>>("analyze_crash_with_ai", {
        ...pathArg(p),
        reportId: reportId ?? null,
      });
    },
    applyActionPlan(plan: Record<string, unknown>, p?: string) {
      return cmd<Record<string, unknown>>("apply_action_plan", { ...pathArg(p), plan });
    },
    recordAiFeedback(
      feedback: {
        helped: boolean;
        fingerprintKey?: string | null;
        humanExplanation?: string | null;
        suspectedMods?: string[] | null;
        recommendedActions?: Record<string, unknown>[] | null;
        reportId?: string | null;
      },
      p?: string,
    ) {
      return cmd<string>("record_crash_ai_feedback", { ...pathArg(p), feedback });
    },
    draftAuthoredCase(reportId?: string | null, p?: string) {
      return cmd<Record<string, unknown>>("draft_authored_crash_case", {
        ...pathArg(p),
        reportId: reportId ?? null,
      });
    },
    saveAuthoredCase(input: Record<string, unknown>, p?: string) {
      return cmd<Record<string, unknown>>("save_authored_crash_case", { ...pathArg(p), input });
    },
    listAuthoredCases(p?: string) {
      return cmd<Record<string, unknown>[]>("list_authored_crash_cases", pathArg(p));
    },
    getAuthoredCaseExport(caseId: string, p?: string) {
      return cmd<string>("get_authored_case_export", { ...pathArg(p), caseId });
    },
    openAuthoredKbFolder(p?: string) {
      return cmd<void>("open_authored_kb_folder", pathArg(p));
    },
    saveProblematicModsConfig(entries: Record<string, unknown>[], p?: string) { return cmd<void>("save_problematic_mods_config", { ...pathArg(p), entries }); },
    getProblematicModsConfig(p?: string) { return cmd<Record<string, unknown>[]>("get_problematic_mods_config", pathArg(p)); },
  },

  // ── Quests (FTB Quests SNBT) ─────────────────────────────────────
  quests: {
    load(p?: string) { return cmd<QuestBook>("load_quest_book", pathArg(p)); },
    saveChapter(chapter: QuestChapter, relativePath?: string | null, p?: string) {
      return cmd<{ relativePath: string; questCount: number }>("save_quest_chapter", {
        ...pathArg(p),
        chapter,
        relativePath: relativePath ?? null,
      });
    },
    validate(p?: string) { return cmd<QuestValidationIssue[]>("validate_quest_book", pathArg(p)); },
  },

  // ── Export ────────────────────────────────────────────────────────
  export: {
    modrinthPack(targetPath?: string | null, p?: string) { return cmd<ExportResult>("export_modrinth_pack", { ...pathArg(p), targetPath }); },
    serverPack(targetPath?: string | null, p?: string) { return cmd<ExportResult>("export_server_pack", { ...pathArg(p), targetPath }); },
    prismInstance(targetPath?: string | null, p?: string) { return cmd<ExportResult>("export_prism_instance", { ...pathArg(p), targetPath }); },
    curseforgePack(targetPath?: string | null, p?: string) { return cmd<ExportResult>("export_curseforge_pack", { ...pathArg(p), targetPath }); },
    batchAll(p?: string) { return cmd<Record<string, unknown>[]>("batch_export_all", pathArg(p)); },
    projectReport(p?: string) { return cmd<Record<string, unknown>>("export_project_report", pathArg(p)); },
    validateModrinth(p?: string) { return cmd<ExportIssue[]>("validate_modrinth_export", pathArg(p)); },
  },

  // ── Modpack library (remote browse + import) ─────────────────────
  modpacks: {
    getModpackUrl(projectId: string) { return cmd<string>("get_modrinth_pack_download", { projectId }); },
    install(url: string, targetDir: string, instanceName: string) {
      return cmd<{ path: string; download?: Record<string, unknown> }>("install_modpack", {
        source: url,
        targetDir,
        instanceName,
      });
    },
  },

  // ── Release ───────────────────────────────────────────────────────
  release: {
    generateChangelog(p?: string) { return cmd<string>("generate_release_changelog", pathArg(p)); },
    generateGitHubRelease(tag?: string | null, target?: string | null, p?: string) { return cmd<Record<string, unknown>>("generate_github_release", { ...pathArg(p), tag, target }); },
    updateVersion(version: string, p?: string) { return cmd<ProjectSummary>("update_project_version", { ...pathArg(p), version }); },
    createSnapshot(changelog: string, p?: string) { return cmd<ReleaseSnapshotResult>("create_release_snapshot", { ...pathArg(p), changelog }); },
    listArtifacts(p?: string) { return cmd<ReleaseArtifactRecord[]>("list_release_artifacts", pathArg(p)); },
    createDraft(changelog: string, p?: string) { return cmd<ReleaseDraftResult>("create_release_draft", { ...pathArg(p), changelog }); },
    generateLockfile(p?: string) { return cmd<TuffboxLockfile>("generate_lockfile", pathArg(p)); },
  },

  // ── Import ────────────────────────────────────────────────────────
  import: {
    project(source: string, targetDir: string) { return cmd<string>("import_project", { source, targetDir }); },
    curseforge(source: string, targetDir: string) { return cmd<string>("import_curseforge_project", { source, targetDir }); },
    installModpack(source: string, targetDir: string, instanceName?: string | null) {
      return cmd<Record<string, unknown>>("install_modpack", { source, targetDir, instanceName: instanceName ?? null });
    },
  },

  curseforge: {
    searchModpacks(query: string, gameVersion?: string | null, offset?: number) {
      return cmd<Record<string, unknown>[]>("search_curseforge_modpacks", {
        query,
        gameVersion: gameVersion ?? null,
        offset: offset ?? 0,
      });
    },
    getModpackFiles(modId: number, gameVersion?: string | null) {
      return cmd<Record<string, unknown>[]>("get_curseforge_modpack_files", {
        modId,
        gameVersion: gameVersion ?? null,
      });
    },
  },

  // ── Content packs (resourcepacks / shaderpacks on disk) ───────────
  content: {
    listPacks(folder: "resourcepacks" | "shaderpacks", p?: string) {
      return cmd<ContentPackEntry[]>("list_content_packs", { ...pathArg(p), folder });
    },
    setEnabled(folder: "resourcepacks" | "shaderpacks", fileName: string, enabled: boolean, p?: string) {
      return cmd<ContentPackEntry>("set_content_pack_enabled", {
        ...pathArg(p),
        folder,
        fileName,
        enabled,
      });
    },
  },

  // ── Minecraft servers.dat ─────────────────────────────────────────
  servers: {
    list(p?: string) {
      return cmd<McServerEntry[]>("list_mc_servers", pathArg(p));
    },
    add(name: string, address: string, p?: string) {
      return cmd<McServerEntry[]>("add_mc_server", { ...pathArg(p), name, address });
    },
    remove(address: string, p?: string) {
      return cmd<McServerEntry[]>("remove_mc_server", { ...pathArg(p), address });
    },
    ping(address: string) {
      return cmd<McServerPing>("ping_mc_server", { address });
    },
  },

  // ── Instance ──────────────────────────────────────────────────────
  instance: {
    create(name: string, minecraftVersion: string, loader: string, loaderVersion: string, location: string) {
      return cmd<string>("create_instance", { name, minecraftVersion, loader, loaderVersion, location });
    },
    updateSettings(opts: {
      minecraftVersion: string; loader: string; loaderVersion: string;
      javaPath?: string | null; memoryMb: number; jvmArgs: string[]; playerName?: string | null;
      p?: string;
    }) { return cmd<void>("update_project_settings", { ...pathArg(opts.p), ...opts }); },
    getSize(p?: string) { return cmd<string>("get_instance_size", pathArg(p)); },
  },

  // ── Logs ──────────────────────────────────────────────────────────
  logs: {
    getLaunch(p?: string) { return cmd<string>("get_launch_log", pathArg(p)); },
    listInstance(p?: string) { return cmd<Record<string, unknown>[]>("list_instance_logs", pathArg(p)); },
    readInstance(logName: string, p?: string) { return cmd<string>("read_instance_log", { ...pathArg(p), logName }); },
    createZip(p?: string) { return cmd<string>("create_logs_zip", pathArg(p)); },
    captureTestRun(runId: string, p?: string) { return cmd<string>("capture_test_run_logs", { ...pathArg(p), runId }); },
  },

  // ── Test Runs ─────────────────────────────────────────────────────
  testRuns: {
    list(p?: string) { return cmd<TestRunRecord[]>("list_test_runs", pathArg(p)); },
  },

  // ── Templates ─────────────────────────────────────────────────────
  templates: {
    save(templateName: string, p?: string) { return cmd<void>("save_as_template", { ...pathArg(p), templateName }); },
    list(p?: string) { return cmd<TemplateInfo[]>("list_templates", pathArg(p)); },
  },

  // ── System ────────────────────────────────────────────────────────
  system: {
    getAppVersion() { return cmd<string>("get_app_version"); },
    checkForUpdate() { return cmd<Record<string, unknown>>("check_for_app_update"); },
    getHomeDir() { return cmd<string>("get_home_dir"); },
    getMinecraftVersions() { return cmd<MinecraftVersion[]>("get_minecraft_versions"); },
    getLoaderVersions(loader: string, minecraftVersion: string) { return cmd<LoaderVersion[]>("get_loader_versions", { loader, minecraftVersion }); },
    findJavaRuntimes() { return cmd<JavaRuntime[]>("find_java_runtimes"); },
    getJavaVersion(path: string) { return cmd<string>("get_java_version", { path }); },
    getDefaultJavaVersion() { return cmd<string>("get_default_java_version"); },
    getKeyboardShortcuts() { return cmd<KeyboardShortcut[]>("get_keyboard_shortcuts"); },
    getDownloadProgress() { return cmd<Record<string, unknown>[]>("get_download_progress"); },
  },

  // ── Pinning & Session ─────────────────────────────────────────────
  session: {
    pin(pin: boolean, p?: string) { return cmd<void>("pin_project", { ...pathArg(p), pin }); },
    isPinned(p?: string) { return cmd<boolean>("is_project_pinned", pathArg(p)); },
    setLastOpened(p?: string) { return cmd<void>("set_last_opened_project", pathArg(p)); },
    getLastOpened() { return cmd<string | null>("get_last_opened_project"); },
  },

  // ── File Operations ───────────────────────────────────────────────
  files: {
    openFolder(p?: string) { return cmd<void>("open_project_folder", pathArg(p)); },
    deleteProject(p?: string) { return cmd<void>("delete_project", pathArg(p)); },
    cloneProject(newName: string, p?: string) { return cmd<string>("clone_project", { ...pathArg(p), newName }); },
  },

  // ── Localization ──────────────────────────────────────────────────
  l10n: {
    get(key: string) { return cmd<string>("localize", { key }); },
    list() { return cmd<LocalizationEntry[]>("list_localizations"); },
  },

  // ── Minecraft Auth ───────────────────────────────────────────────
  mcAuth: {
    startDeviceCode() { return cmd<DeviceCodeInfo>("mc_start_device_code"); },
    pollDeviceCode() { return cmd<{ profile: McProfile; mcAccessToken: string }>("mc_poll_device_code"); },
    offlineLogin(username: string, skinSource: SkinSource) {
      return cmd<{ profile: McProfile; mcAccessToken: string }>("mc_offline_login", { username, skinSource });
    },
    getAuthStatus() { return cmd<AuthState>("mc_get_auth_status"); },
    logout() { return cmd<void>("mc_logout"); },
    refreshProfile() { return cmd<McProfile>("mc_refresh_profile"); },
    getSkinPath(uuid: string) { return cmd<string>("mc_get_skin_path", { uuid }); },
    fetchSkinUrl(uuid: string) { return cmd<string | null>("mc_fetch_skin_url", { uuid }); },
    fetchSkinForUsername(username: string, source: SkinSource) {
      return cmd<string | null>("mc_fetch_skin_for_username", { username, source });
    },
    setSkinSource(source: SkinSource) { return cmd<void>("mc_set_skin_source", { source }); },
    listAccounts() { return cmd<AccountEntry[]>("mc_list_accounts"); },
    switchAccount(uuid: string) { return cmd<AuthState>("mc_switch_account", { uuid }); },
    removeAccount(uuid: string) { return cmd<void>("mc_remove_account", { uuid }); },
    applySkin(skinUrl: string, variant: string) { return cmd<void>("mc_apply_skin", { skinUrl, variant }); },
    applyCape(capeId: string) { return cmd<AuthState>("mc_apply_cape", { capeId }); },
    listCapes() { return cmd<CapeCatalog>("mc_list_capes"); },
    setCapeProvider(provider: CapeProvider) { return cmd<AuthState>("mc_set_cape_provider", { provider }); },
    checkEntitlement() { return cmd<boolean>("mc_check_entitlement"); },
    getSkinBase64(url: string) { return cmd<string>("mc_get_skin_base64", { url }); },
    listYggdrasilPresets() { return cmd<YggdrasilPreset[]>("mc_list_yggdrasil_presets"); },
    yggdrasilLogin(username: string, password: string, authority: string) {
      return cmd<{ profile: McProfile; mcAccessToken: string }>("mc_yggdrasil_login", {
        username,
        password,
        authority,
      });
    },
  },

  // ── Discord Rich Presence ─────────────────────────────────────────
  presence: {
    get() { return cmd<PresenceSettings>("get_presence_settings"); },
    save(settings: PresenceSettings) { return cmd<void>("save_presence_settings", { settings }); },
    setPlaying(details: string, state: string) {
      return cmd<void>("set_discord_presence", { details, state });
    },
    clear() { return cmd<void>("clear_discord_presence"); },
  },
};
