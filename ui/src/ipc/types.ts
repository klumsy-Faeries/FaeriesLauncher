// Shapes shared with the Rust backend. These mirror the serde serialization
// of faerie-core types; if a field changes there, it changes here.

export type SettingKind =
  | { kind: "bool" }
  | { kind: "uint"; min: number; max: number }
  | { kind: "text" }
  | { kind: "choice"; options: string[] };

export type SettingSchema = SettingKind & {
  id: string;
  default: unknown;
  restartRequired: boolean;
};

export interface RecoveryNotice {
  file: string;
  backupPath: string;
  error: string;
}

export interface AppInfo {
  version: string;
  dataDir: string;
  logsDir: string;
}

export interface ThemeLayout {
  sidebar?: {
    enabled?: boolean;
    width?: string;
    items?: string[];
    showSocial?: boolean;
  };
  statusBar?: { enabled?: boolean };
  home?: {
    showPlayButton?: boolean;
    showAccountCard?: boolean;
    showNews?: boolean;
    /** Dashboard cards, in display order. */
    cards?: string[];
  };
}

export interface ChangelogEntry {
  version: string;
  title: string;
  date: string;
  highlights: string[];
}

export interface Theme {
  name: string;
  source: "builtin" | "user";
  tokens: Record<string, string>;
  layout: ThemeLayout;
  warnings: string[];
}

export type TaskOutcome =
  | { kind: "completed" }
  | { kind: "cancelled" }
  | { kind: "failed"; error: string };

export type FaerieEvent =
  | { type: "settingChanged"; id: string; value: unknown; restartRequired: boolean }
  | { type: "taskStarted"; id: number; name: string }
  | { type: "taskProgress"; id: number; progress: number; message: string | null }
  | { type: "taskFinished"; id: number; outcome: TaskOutcome }
  | {
      type: "installProgress";
      instanceId: string;
      phase: string;
      filesDone: number;
      filesTotal: number;
      bytesDone: number;
      bytesTotal: number | null;
      bytesPerSec: number;
    }
  | { type: "gameStarted"; instanceId: string; pid: number }
  | { type: "gameLog"; instanceId: string; stderr: boolean; text: string }
  | {
      type: "gameExited";
      instanceId: string;
      code: number | null;
      class: string;
      detail: string;
    };

export interface AccountRecord {
  id: string;
  name: string;
  xuid: string;
  expiresAtSecs: number;
}

export interface AccountsInfo {
  accounts: AccountRecord[];
  active: string | null;
  signInAvailable: boolean;
}

/**
 * What to show the user during sign-in. The device code itself is
 * deliberately absent — it stays in the backend, which polls with it.
 */
export interface DeviceCodePrompt {
  userCode: string;
  verificationUri: string;
  expiresIn: number;
  interval: number;
  message: string;
}

/** A curated mod set (see faerie-modding presets). */
export interface ModPreset {
  id: string;
  name: string;
  description: string;
  loader: string;
  mods: { slug: string; name: string; reason: string; prereleaseOk: boolean }[];
  /** Jars carried by the launcher itself; each is built for one game version. */
  bundled: { id: string; name: string; reason: string; fileName: string; gameVersion: string }[];
  /** Resource packs copied into the instance and enabled by default. */
  packs: { id: string; name: string; reason: string; fileName: string }[];
  /** Mods the set no longer carries; installing removes them from the profile. */
  retired: { modId: string; name: string; reason: string }[];
}

export interface PresetOutcome {
  installed: string[];
  /** Names of resource packs enabled in options.txt. */
  packs: string[];
  skipped: { name: string; reason: string }[];
  /** File names of retired mods dropped from the profile. */
  removed: string[];
  loaderInstalled: string | null;
}

export interface RunningGame {
  instanceId: string;
  pid: number | null;
}

export interface VersionSummary {
  id: string;
  kind: string;
  url: string;
  sha1: string;
  releaseTime: string;
}

export interface ManifestResult {
  manifest: {
    latest: { release: string; snapshot: string };
    versions: VersionSummary[];
  };
  source: "network" | "cacheFresh" | "cacheStale";
}

export interface JavaInstallation {
  path: string;
  version: string;
  major: number;
  vendor: string;
  arch: string;
  source: string;
}

export interface HardwareInfo {
  cpuModel: string;
  cpuCores: number | null;
  cpuThreads: number;
  totalRamMb: number;
  availableRamMb: number;
  os: string;
  arch: string;
  gpus: string[];
  dataDiskFreeGb: number | null;
  recommendedHeapMb: number;
}

export interface Instance {
  id: string;
  dir: string;
  format: number;
  name: string;
  minecraftVersion: string;
  loader: { kind: string; version: string } | null;
  java: {
    pathOverride: string | null;
    minRamMb: number | null;
    maxRamMb: number | null;
    extraArgs: string[];
  };
  createdAtSecs: number;
  lastPlayedAtSecs: number | null;
  notes: string;
}

export interface InstanceList {
  instances: Instance[];
  problems: string[];
}

export type LoaderKind = "fabric" | "quilt" | "forge" | "neoforge" | "unknown";

export interface ModEntry {
  fileName: string;
  modId: string;
  name: string;
  version: string;
  loader: LoaderKind;
  description: string;
  authors: string[];
  enabled: boolean;
  size: number;
  sha1: string | null;
  warnings: string[];
}

export interface CompatIssue {
  severity: "error" | "warning" | "info";
  kind: string;
  subject: string;
  /** What happened. */
  summary: string;
  /** Why it happened. */
  detail: string;
  /** How to fix it. */
  fix: string;
}

export interface CompatReport {
  loader: LoaderKind;
  minecraftVersion: string;
  loaderVersion: string;
  modsChecked: number;
  issues: CompatIssue[];
}

export interface ProfileSummary {
  key: string;
  name: string;
  modCount: number;
  enabledCount: number;
}

export interface ModsView {
  instanceId: string;
  activeProfile: string;
  profiles: ProfileSummary[];
  mods: ModEntry[];
  report: CompatReport;
  problems: string[];
}

export interface LoaderOption {
  kind: LoaderKind;
  display: string;
  /** False when versions can be listed but installation is not supported yet. */
  installable: boolean;
}

export interface LoaderVersion {
  version: string;
  stable: boolean;
  versionId: string;
}

export interface LocaleBundle {
  name: string;
  messages: Record<string, string>;
  translatedKeys: number;
  totalKeys: number;
  warnings: string[];
}

export interface ProcessStats {
  pid: number;
  memoryMb: number;
  cpuPercent: number;
}

export interface PerformanceSnapshot {
  launcher: ProcessStats | null;
  game: ProcessStats | null;
  /** [stage name, milliseconds] pairs from this session's boot. */
  startupMs: [string, number][];
  startupTotalMs: number;
  uptimeSecs: number;
}

export interface Backend {
  settingsSchema(): Promise<SettingSchema[]>;
  settingsValues(): Promise<Record<string, unknown>>;
  setSetting(id: string, value: unknown): Promise<{ restartRequired: boolean }>;
  recoveryNotices(): Promise<RecoveryNotice[]>;
  appInfo(): Promise<AppInfo>;
  listThemes(): Promise<string[]>;
  getTheme(name: string): Promise<Theme>;
  onEvent(cb: (event: FaerieEvent) => void): Promise<() => void>;
  listMinecraftVersions(force: boolean): Promise<ManifestResult>;
  detectJava(): Promise<JavaInstallation[]>;
  detectHardware(): Promise<HardwareInfo>;
  listInstances(): Promise<InstanceList>;
  createInstance(name: string, minecraftVersion: string): Promise<Instance>;
  renameInstance(id: string, name: string): Promise<Instance>;
  duplicateInstance(id: string, name: string): Promise<Instance>;
  deleteInstance(id: string): Promise<string>;
  updateInstanceJava(
    id: string,
    java: {
      maxRamMb: number | null;
      minRamMb: number | null;
      javaPath: string | null;
      extraArgs: string | null;
    },
  ): Promise<Instance>;
  listAccounts(): Promise<AccountsInfo>;
  beginSignIn(): Promise<DeviceCodePrompt>;
  /** Waits for the user to finish in their browser. Takes no device code:
   *  the backend holds it (see DeviceCodePrompt). */
  completeSignIn(): Promise<AccountRecord>;
  cancelSignIn(): Promise<void>;
  listMods(id: string): Promise<ModsView>;
  addMods(id: string, paths: string[]): Promise<number>;
  setModEnabled(id: string, fileName: string, enabled: boolean): Promise<void>;
  removeMod(id: string, fileName: string): Promise<void>;
  importExistingMods(id: string): Promise<number>;
  createModProfile(id: string, name: string, copyFromActive: boolean): Promise<string>;
  activateModProfile(id: string, profile: string): Promise<void>;
  deleteModProfile(id: string, profile: string): Promise<void>;
  supportedLoaders(): Promise<LoaderOption[]>;
  loaderVersions(loader: string, minecraftVersion: string): Promise<LoaderVersion[]>;
  installLoader(id: string, loader: string, loaderVersion: string): Promise<string>;
  removeLoader(id: string): Promise<void>;
  listModPresets(): Promise<ModPreset[]>;
  /** Installs the preset's loader if needed, then its mods. Slow: network. */
  installModPreset(id: string, preset: string): Promise<PresetOutcome>;
  changelog(): Promise<ChangelogEntry[]>;
  listLocales(): Promise<string[]>;
  getLocale(name: string): Promise<LocaleBundle>;
  /** Fires when the themes folder changes on disk (hot reload). */
  onThemesChanged(cb: () => void): Promise<() => void>;
  /** Fires when settings files that were unreadable at startup became
   *  readable and were reloaded; the page should refresh its state. */
  onConfigReloaded(cb: (files: string[]) => void): Promise<() => void>;
  performanceSnapshot(): Promise<PerformanceSnapshot>;
  openFolder(which: string): Promise<void>;
  openDataFolder(): Promise<void>;
  setActiveAccount(id: string): Promise<void>;
  removeAccount(id: string): Promise<void>;
  playInstance(id: string): Promise<number>;
  cancelPlay(): Promise<void>;
  runningGame(): Promise<RunningGame | null>;
  stopGame(): Promise<void>;
}
