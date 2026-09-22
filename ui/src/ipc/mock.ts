// Browser-preview mock of the Rust backend. Used only when the UI runs
// outside the Tauri shell (plain `vite dev`); keeps values in memory and
// resolves themes from the same JSON files the backend embeds.
//
// The schema below intentionally mirrors faerie-core's registry for preview
// purposes only — the Rust registry remains the single source of truth.

import skyblockColors from "@themes/skyblock/colors.json";
import skyblockLayout from "@themes/skyblock/layout.json";
import skyblockSpacing from "@themes/skyblock/spacing.json";
import skyblockTheme from "@themes/skyblock/theme.json";
import skyblockTypography from "@themes/skyblock/typography.json";
import smpColors from "@themes/smp/colors.json";
import smpLayout from "@themes/smp/layout.json";
import smpSpacing from "@themes/smp/spacing.json";
import smpTheme from "@themes/smp/theme.json";
import smpTypography from "@themes/smp/typography.json";
// Vite serves the artwork by URL; the backend embeds the same files.
import smpBackgroundUrl from "@themes/smp/assets/background.jpg";
import skyblockBackgroundUrl from "@themes/skyblock/assets/background.png";
import enUS from "@locales/en-US.json";
import changelogMarkdown from "../../../CHANGELOG.md?raw";
import smpLogoUrl from "../../../themes/smp/assets/logo.png";
import smpMarkUrl from "../../../themes/smp/assets/mark.png";

import type {
  ChangelogEntry,
  ModPreset,
  AccountRecord,
  Backend,
  FaerieEvent,
  Instance,
  RunningGame,
  SettingSchema,
  Theme,
  ThemeLayout,
} from "./types";

const schema: SettingSchema[] = [
  { id: "launcher.language", kind: "text", default: "en-US", restartRequired: false },
  // The preview simulates a fresh install, so setup has not run yet.
  { id: "launcher.setup_complete", kind: "bool", default: false, restartRequired: false },
  { id: "ui.layout_overrides", kind: "text", default: "{}", restartRequired: false },
  { id: "ui.theme", kind: "text", default: "smp", restartRequired: false },
  { id: "ui.font_scale", kind: "uint", min: 50, max: 200, default: 100, restartRequired: false },
  { id: "ui.reduced_motion", kind: "bool", default: false, restartRequired: false },
  { id: "downloads.concurrency", kind: "uint", min: 1, max: 16, default: 4, restartRequired: false },
  { id: "downloads.retries", kind: "uint", min: 0, max: 10, default: 3, restartRequired: false },
  { id: "java.default_max_ram_mb", kind: "uint", min: 0, max: 65536, default: 0, restartRequired: false },
  { id: "java.default_min_ram_mb", kind: "uint", min: 0, max: 65536, default: 512, restartRequired: false },
  { id: "java.default_args", kind: "text", default: "-XX:+UseG1GC -XX:+UnlockExperimentalVMOptions", restartRequired: false },
  { id: "accounts.client_id", kind: "text", default: "", restartRequired: false },
  { id: "shortcuts.palette", kind: "text", default: "Ctrl+K", restartRequired: false },
  { id: "shortcuts.launch", kind: "text", default: "Ctrl+L", restartRequired: false },
  { id: "shortcuts.instances", kind: "text", default: "Ctrl+I", restartRequired: false },
  { id: "shortcuts.mods", kind: "text", default: "Ctrl+M", restartRequired: false },
  { id: "shortcuts.settings", kind: "text", default: "Ctrl+,", restartRequired: false },
  { id: "shortcuts.refresh", kind: "text", default: "Ctrl+Shift+R", restartRequired: false },
  { id: "advanced.log_level", kind: "choice", options: ["error", "warn", "info", "debug", "trace"], default: "info", restartRequired: true },
];

// A realistic number of versions (Mojang lists ~900), so the preview
// exercises the same virtualization path as the real manifest.
const mockVersions = [
  { id: "26.2", kind: "release", url: "", sha1: "", releaseTime: "2026-07-20T10:00:00+00:00" },
  { id: "26w35a", kind: "snapshot", url: "", sha1: "", releaseTime: "2026-08-27T10:00:00+00:00" },
  { id: "26.1", kind: "release", url: "", sha1: "", releaseTime: "2026-03-12T10:00:00+00:00" },
  { id: "1.21.4", kind: "release", url: "", sha1: "", releaseTime: "2024-12-03T10:00:00+00:00" },
  { id: "1.8.9", kind: "release", url: "", sha1: "", releaseTime: "2015-12-08T00:00:00+00:00" },
  ...Array.from({ length: 900 }, (_, i) => ({
    id: `1.${20 - Math.floor(i / 60)}.${i % 60}`,
    kind: i % 3 === 0 ? "release" : "snapshot",
    url: "",
    sha1: "",
    releaseTime: `20${String(24 - Math.floor(i / 120)).padStart(2, "0")}-01-01T00:00:00+00:00`,
  })),
];

let nextInstance = 1;
const mockInstances: Instance[] = [];
const mockAccounts: AccountRecord[] = [];
let mockRunning: RunningGame | null = null;

function makeInstance(name: string, minecraftVersion: string): Instance {
  return {
    id: `${name.toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "instance"}-${nextInstance++}`,
    dir: `(browser preview)/instances/${name}`,
    format: 1,
    name,
    minecraftVersion,
    loader: null,
    java: { pathOverride: null, minRamMb: null, maxRamMb: null, extraArgs: [] },
    createdAtSecs: Math.floor(Date.now() / 1000),
    lastPlayedAtSecs: null,
    notes: "",
  };
}

const values: Record<string, unknown> = Object.fromEntries(
  schema.map((s) => [s.id, s.default]),
);

const listeners: Array<(event: FaerieEvent) => void> = [];

const delay = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

// Mirrors faerie-modding's OPTIMIZED preset (names and reasons are what the
// UI shows, so keep them in step when the Rust list changes).
const OPTIMIZED_PRESET: ModPreset = {
  id: "optimized",
  name: "Optimized",
  description:
    "Fabric with the same mods as Fabulously Optimized: a faster renderer, lighter game logic, lower memory use, shaders and OptiFine-style resource pack features, controller support, zoom, and other quality-of-life mods, plus the Faeries companion mods. No gameplay changes.",
  loader: "fabric",
  mods: [
    { slug: "fabric-api", name: "Fabric API", reason: "Shared library most Fabric mods need.", prereleaseOk: false },
    { slug: "sodium", name: "Sodium", reason: "Rewrites the renderer; the single biggest frame-rate gain.", prereleaseOk: true },
    { slug: "lithium", name: "Lithium", reason: "Optimises game logic — AI, physics, ticking — without changing behaviour.", prereleaseOk: false },
    { slug: "ferrite-core", name: "FerriteCore", reason: "Cuts the memory used by block states and models.", prereleaseOk: false },
    { slug: "immediatelyfast", name: "ImmediatelyFast", reason: "Speeds up immediate-mode rendering: text, GUI, entities.", prereleaseOk: false },
    { slug: "entityculling", name: "Entity Culling", reason: "Skips rendering entities you cannot see.", prereleaseOk: false },
    { slug: "dynamic-fps", name: "Dynamic FPS", reason: "Idles the game while the window is unfocused or hidden.", prereleaseOk: false },
    { slug: "badoptimizations", name: "BadOptimizations", reason: "Removes redundant work in lighting, time, and rendering.", prereleaseOk: false },
    { slug: "reeses-sodium-options", name: "Reese's Sodium Options", reason: "Searchable video settings screen for Sodium.", prereleaseOk: false },
    { slug: "sodium-extra", name: "Sodium Extra", reason: "Extra toggles for Sodium: animations, particles, fog.", prereleaseOk: false },
    { slug: "better-block-entities", name: "Better Block Entities", reason: "Renders chests, signs, and other block entities through Sodium's fast path.", prereleaseOk: true },
    { slug: "moreculling", name: "More Culling", reason: "Culls more of what is hidden: leaves, item frames, block faces.", prereleaseOk: false },
    { slug: "modernfix-mvus", name: "ModernFix-mVUS", reason: "Faster startup and less memory across many small fixes.", prereleaseOk: false },
    { slug: "ixeris", name: "Ixeris", reason: "Polls input on its own thread so the mouse stays smooth when frames dip.", prereleaseOk: false },
    { slug: "language-reload", name: "Language Reload", reason: "Faster resource reloads, and fallback languages for untranslated text.", prereleaseOk: false },
    { slug: "iris", name: "Iris Shaders", reason: "Loads OptiFine-format shader packs; off until you pick one.", prereleaseOk: false },
    { slug: "continuity", name: "Continuity", reason: "Connected textures for glass, bookshelves, and packs that use them.", prereleaseOk: false },
    { slug: "entitytexturefeatures", name: "Entity Texture Features", reason: "Random, emissive, and custom entity textures from resource packs.", prereleaseOk: false },
    { slug: "entity-model-features", name: "Entity Model Features", reason: "Custom entity models from resource packs, OptiFine format.", prereleaseOk: false },
    { slug: "animaticarefabricated", name: "Animatica", reason: "Animated textures in the OptiFine format.", prereleaseOk: false },
    { slug: "bettergrassify", name: "BetterGrassify", reason: "Grass and paths wrap down the side of the block, like OptiFine's Better Grass.", prereleaseOk: false },
    { slug: "skyboxify", name: "Skyboxify", reason: "Custom skies from resource packs, OptiFine format.", prereleaseOk: false },
    { slug: "polytone", name: "Polytone", reason: "Lets resource packs recolour biomes, blocks, maps, and dyes.", prereleaseOk: false },
    { slug: "optigui", name: "OptiGUI", reason: "Custom container GUI textures from resource packs.", prereleaseOk: true },
    { slug: "puzzle", name: "Puzzle", reason: "One settings screen for the resource-pack feature mods.", prereleaseOk: false },
    { slug: "sodium-shadowy-path-blocks", name: "Sodium Shadowy Path Blocks", reason: "Restores vanilla shading on paths and other partial blocks under Sodium.", prereleaseOk: false },
    { slug: "lambdynamiclights", name: "LambDynamicLights", reason: "Held torches, glowing items, and burning mobs light their surroundings.", prereleaseOk: false },
    { slug: "cape-provider", name: "Cape Provider", reason: "Shows capes from OptiFine, MinecraftCapes, and other providers.", prereleaseOk: false },
    { slug: "modmenu", name: "Mod Menu", reason: "Lists installed mods and opens their settings.", prereleaseOk: false },
    { slug: "zoomify", name: "Zoomify", reason: "A zoom key, with scroll-to-zoom and smoothing.", prereleaseOk: false },
    { slug: "controlify", name: "Controlify", reason: "Full controller support with on-screen button prompts.", prereleaseOk: false },
    { slug: "cubes-without-borders", name: "Cubes Without Borders", reason: "Borderless fullscreen, so alt-tab is instant.", prereleaseOk: false },
    { slug: "fastquit", name: "FastQuit", reason: "Back to the title screen while the world saves in the background.", prereleaseOk: false },
    { slug: "rrls", name: "Remove Reloading Screen", reason: "Resource packs load in the background instead of behind a blocking screen.", prereleaseOk: true },
    { slug: "morechathistory", name: "More Chat History", reason: "Keeps far more chat lines than the vanilla limit.", prereleaseOk: false },
    { slug: "paginatedadvancements", name: "Paginated Advancements", reason: "A tidier advancements screen with pages and custom frames.", prereleaseOk: false },
    { slug: "better-mount-hud", name: "Better Mount HUD", reason: "Shows your own hunger and experience while riding.", prereleaseOk: false },
    { slug: "renice-shot", name: "Renice Shot", reason: "Takes screenshots at a higher resolution than the window.", prereleaseOk: false },
    { slug: "debugify", name: "Debugify", reason: "Fixes vanilla bugs from the bug tracker that are still open.", prereleaseOk: false },
    { slug: "e4mc", name: "e4mc", reason: "Opens a LAN world to friends over the internet, no port forwarding.", prereleaseOk: false },
    { slug: "no-chat-reports", name: "No Chat Reports", reason: "Turns off chat signing where the server allows it, as Fabulously Optimized ships.", prereleaseOk: false },
    { slug: "crash-assistant", name: "Crash Assistant", reason: "After a crash, shows the logs and what likely caused it.", prereleaseOk: false },
    { slug: "cloth-config", name: "Cloth Config", reason: "Settings library for FastQuit, More Culling, and others.", prereleaseOk: false },
    { slug: "yacl", name: "YetAnotherConfigLib", reason: "Settings library for Zoomify, Controlify, and Skyboxify.", prereleaseOk: false },
    { slug: "fabric-language-kotlin", name: "Fabric Language Kotlin", reason: "Kotlin runtime for Zoomify and OptiGUI.", prereleaseOk: false },
    { slug: "forge-config-api-port", name: "Forge Config API Port", reason: "Config library Remove Reloading Screen reads its settings through.", prereleaseOk: false },
    { slug: "placeholder-api", name: "Placeholder API", reason: "Text library Mod Menu needs.", prereleaseOk: false },
  ],
  bundled: [
    { id: "faeries-theme", name: "Faeries Theme", reason: "Faeries logo as the window icon, loading screen, title panorama, and menu buttons. Cosmetic only.", fileName: "faeries-theme-0.1.0+mc26.2.jar", gameVersion: "26.2" },
    { id: "faeries-vault", name: "Faeries Pack Vault", reason: "Keeps server resource packs stored locally so joining is instant; verifies by hash and falls back to a normal download.", fileName: "faeries-vault-0.1.0+mc26.2.jar", gameVersion: "26.2" },
  ],
  packs: [],
  retired: [
    { modId: "krypton", name: "Krypton", reason: "Its build for Minecraft 26.2 fails a mixin on the login packet handler, which Controlify loads at startup, so the game crashed before the title screen. Fabulously Optimized does not carry it either." },
  ],
};

/** Port of apps/launcher/src/changelog.rs: headings, first line of each bullet. */
function parseChangelog(markdown: string, maxEntries: number, maxHighlights: number): ChangelogEntry[] {
  const entries: ChangelogEntry[] = [];
  for (const line of markdown.split(/\r?\n/)) {
    if (line.startsWith("## ")) {
      if (entries.length === maxEntries) break;
      entries.push(parseHeading(line.slice(3).trim()));
      continue;
    }
    const current = entries[entries.length - 1];
    if (!current || current.highlights.length >= maxHighlights) continue;
    if (line.startsWith("- ")) current.highlights.push(cleanMarkdown(line.slice(2).trim()));
  }
  return entries;
}

function parseHeading(heading: string): ChangelogEntry {
  // `0.4.0 — Phase 4 modding (2026-08-30)`
  const dash = heading.indexOf("—");
  const version = dash >= 0 ? heading.slice(0, dash).trim() : heading;
  const rest = dash >= 0 ? heading.slice(dash + 1).trim() : "";
  const paren = rest.lastIndexOf("(");
  const title = paren >= 0 ? rest.slice(0, paren).trim() : rest;
  const date = paren >= 0 ? rest.slice(paren + 1).replace(/\)+$/, "").trim() : "";
  return { version, title, date, highlights: [] };
}

function cleanMarkdown(text: string): string {
  const plain = text.replace(/\*\*|\*|`/g, "").trim();
  const sentence = plain.indexOf(". ");
  return sentence >= 0 ? `${plain.slice(0, sentence)}.` : plain;
}

function emit(event: FaerieEvent) {
  for (const listener of listeners) listener(event);
}

function camelToKebab(key: string): string {
  return key.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`);
}

function flattenInto(
  prefix: string,
  value: unknown,
  out: Record<string, string>,
) {
  if (value !== null && typeof value === "object" && !Array.isArray(value)) {
    for (const [key, child] of Object.entries(value)) {
      if (prefix === "" && key === "meta") continue;
      const kebab = camelToKebab(key);
      flattenInto(prefix === "" ? kebab : `${prefix}-${kebab}`, child, out);
    }
  } else if (
    typeof value === "string" ||
    typeof value === "number" ||
    typeof value === "boolean"
  ) {
    out[prefix] = String(value);
  }
}

function buildTokens(files: [string, unknown][]): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [prefix, value] of files) flattenInto(prefix, value, out);
  return out;
}

const smpFiles: [string, unknown][] = [
  ["", smpTheme],
  ["color", smpColors],
  ["font", smpTypography],
  ["space", smpSpacing],
];
const skyblockFiles: [string, unknown][] = [
  ["", skyblockTheme],
  ["color", skyblockColors],
  ["font", skyblockTypography],
  ["space", skyblockSpacing],
];

/** Mirrors the backend's deep merge of layout overrides onto the theme. */
function mergeLayout(base: unknown, overlay: unknown): unknown {
  if (
    base && typeof base === "object" && !Array.isArray(base) &&
    overlay && typeof overlay === "object" && !Array.isArray(overlay)
  ) {
    const out: Record<string, unknown> = { ...(base as Record<string, unknown>) };
    for (const [key, value] of Object.entries(overlay as Record<string, unknown>)) {
      out[key] = key in out ? mergeLayout(out[key], value) : value;
    }
    return out;
  }
  return overlay;
}

function resolveTheme(name: string): Theme {
  const warnings: string[] = [];
  // Retired names resolve to SMP silently, as in the backend.
  if (name === "faerie" || name === "dark") name = "smp";
  let applied = "smp";
  let files = smpFiles;
  let layout: ThemeLayout = smpLayout;
  if (name === "skyblock") {
    applied = "skyblock";
    files = [...smpFiles, ...skyblockFiles];
    layout = skyblockLayout;
  } else if (name !== "smp") {
    warnings.push(`theme \`${name}\` was not found; using \`smp\``);
  }
  const tokens = buildTokens(files);
  tokens["background-image"] = `url("${
    applied === "skyblock" ? skyblockBackgroundUrl : smpBackgroundUrl
  }")`;
  // Both worlds share the emblem; the built-ins embed these, Vite serves
  // the same files by URL.
  tokens["asset-logo"] = `url("${smpLogoUrl}")`;
  tokens["asset-mark"] = `url("${smpMarkUrl}")`;

  // The backend merges the user's layout overrides during resolution; do the
  // same here so the preview behaves like the real thing.
  let merged: ThemeLayout = layout;
  const raw = String(values["ui.layout_overrides"] ?? "{}");
  if (raw.trim() && raw.trim() !== "{}") {
    try {
      merged = mergeLayout(layout, JSON.parse(raw)) as ThemeLayout;
    } catch (e) {
      warnings.push(`layout overrides could not be parsed (${e})`);
    }
  }

  return {
    name: applied,
    source: "builtin",
    tokens,
    layout: merged,
    warnings,
  };
}

export const mockBackend: Backend = {
  settingsSchema: async () => schema,
  settingsValues: async () => ({ ...values }),
  setSetting: async (id, value) => {
    const def = schema.find((s) => s.id === id);
    if (!def) throw new Error(`unknown setting \`${id}\``);
    values[id] = value;
    emit({ type: "settingChanged", id, value, restartRequired: def.restartRequired });
    return { restartRequired: def.restartRequired };
  },
  recoveryNotices: async () => [],
  appInfo: async () => ({
    version: "dev-preview",
    dataDir: "(browser preview — no data directory)",
    logsDir: "(browser preview — no logs directory)",
  }),
  listThemes: async () => ["smp", "skyblock"],
  getTheme: async (name) => resolveTheme(name),
  onEvent: async (cb) => {
    listeners.push(cb);
    return () => {
      const i = listeners.indexOf(cb);
      if (i >= 0) listeners.splice(i, 1);
    };
  },
  listMinecraftVersions: async () => ({
    manifest: { latest: { release: "26.2", snapshot: "26w35a" }, versions: mockVersions },
    source: "cacheFresh",
  }),
  detectJava: async () => [
    { path: "C:\\Program Files\\Eclipse Adoptium\\jdk-21\\bin\\java.exe", version: "21.0.3", major: 21, vendor: "Eclipse Adoptium", arch: "amd64", source: "vendor-dir" },
    { path: "C:\\Program Files\\Java\\jre-1.8\\bin\\java.exe", version: "1.8.0_401", major: 8, vendor: "Oracle Corporation", arch: "amd64", source: "registry" },
  ],
  detectHardware: async () => ({
    cpuModel: "Preview CPU", cpuCores: 8, cpuThreads: 16,
    totalRamMb: 16384, availableRamMb: 9216,
    os: "Browser Preview OS", arch: "x86_64", gpus: ["Preview GPU"],
    dataDiskFreeGb: 128, recommendedHeapMb: 6144,
  }),
  listInstances: async () => ({ instances: [...mockInstances], problems: [] }),
  createInstance: async (name, minecraftVersion) => {
    const instance = makeInstance(name.trim(), minecraftVersion);
    if (!name.trim()) throw new Error("an instance name cannot be empty");
    mockInstances.unshift(instance);
    return instance;
  },
  renameInstance: async (id, name) => {
    const instance = mockInstances.find((i) => i.id === id);
    if (!instance) throw new Error(`no instance with id \`${id}\` exists`);
    instance.name = name;
    return instance;
  },
  duplicateInstance: async (id, name) => {
    const source = mockInstances.find((i) => i.id === id);
    if (!source) throw new Error(`no instance with id \`${id}\` exists`);
    const copy = { ...makeInstance(name, source.minecraftVersion), loader: source.loader };
    mockInstances.unshift(copy);
    return copy;
  },
  deleteInstance: async (id) => {
    const index = mockInstances.findIndex((i) => i.id === id);
    if (index < 0) throw new Error(`no instance with id \`${id}\` exists`);
    mockInstances.splice(index, 1);
    return `(browser preview)/instances/.trash/${id}`;
  },
  updateInstanceJava: async (id, java) => {
    const instance = mockInstances.find((i) => i.id === id);
    if (!instance) throw new Error(`no instance with id \`${id}\` exists`);
    instance.java = {
      maxRamMb: java.maxRamMb,
      minRamMb: java.minRamMb,
      pathOverride: java.javaPath,
      extraArgs: java.extraArgs ? java.extraArgs.split(/\s+/).filter(Boolean) : [],
    };
    return instance;
  },
  listAccounts: async () => ({
    accounts: mockAccounts,
    active: mockAccounts[0]?.id ?? null,
    // Mirrors the backend: sign-in is offered as soon as a client id is set,
    // so the preview can exercise the device-code prompt (paste anything
    // into Settings → Accounts → Microsoft client ID).
    signInAvailable: String(values["accounts.client_id"] ?? "").trim() !== "",
  }),
  beginSignIn: async () => {
    if (String(values["accounts.client_id"] ?? "").trim() === "") {
      throw new Error(
        "no Microsoft client ID is configured. Register an Azure application and have Mojang approve it for the Minecraft API, then set it in Settings → Accounts.",
      );
    }
    // Same camelCase shape the backend serializes (see DeviceCodePrompt).
    return {
      userCode: "H4KL2M9",
      verificationUri: "https://microsoft.com/link",
      expiresIn: 900,
      interval: 5,
      message: "To sign in, use a web browser to open the page https://microsoft.com/link and enter the code H4KL2M9 to authenticate.",
    };
  },
  completeSignIn: async () => {
    // Leave the prompt on screen long enough to look at before failing the
    // way the real flow would without a browser to finish in.
    await delay(4000);
    throw new Error("sign-in is not available in the browser preview");
  },
  cancelSignIn: async () => {},
  listMods: async (id) => ({
    instanceId: id,
    activeProfile: "default",
    profiles: [
      { key: "default", name: "Default", modCount: 3, enabledCount: 2 },
      { key: "shaders", name: "Shaders", modCount: 5, enabledCount: 5 },
    ],
    mods: [
      { fileName: "sodium.jar", modId: "sodium", name: "Sodium", version: "0.5.8",
        loader: "fabric", description: "A rendering engine replacement.",
        authors: ["JellySquid"], enabled: true, size: 1_200_000,
        sha1: "aaa", warnings: [] },
      { fileName: "indium.jar", modId: "indium", name: "Indium", version: "1.0.30",
        loader: "fabric", description: "Sodium addon.", authors: [],
        enabled: true, size: 240_000, sha1: "bbb", warnings: [] },
      { fileName: "jei.jar", modId: "jei", name: "Just Enough Items",
        version: "15.2.0", loader: "forge", description: "Item viewer.",
        authors: [], enabled: false, size: 900_000, sha1: "ccc", warnings: [] },
    ],
    report: {
      loader: "fabric", minecraftVersion: "26.2", loaderVersion: "0.19.5",
      modsChecked: 2,
      issues: [
        { severity: "warning", kind: "wrongDependencyVersion", subject: "indium.jar",
          summary: "Indium works best with sodium >=0.6, but 0.5.8 is installed.",
          detail: "This dependency is optional, so the game should still start.",
          fix: "Update sodium to >=0.6 when convenient." },
      ],
    },
    problems: [],
  }),
  addMods: async (_id, paths) => paths.length,
  setModEnabled: async () => {},
  removeMod: async () => {},
  importExistingMods: async () => 0,
  createModProfile: async (_id, name) => name.toLowerCase().replace(/\s+/g, "-"),
  activateModProfile: async () => {},
  deleteModProfile: async () => {},
  supportedLoaders: async () => [
    { kind: "fabric", display: "Fabric", installable: true },
    { kind: "quilt", display: "Quilt", installable: true },
    { kind: "neoforge", display: "NeoForge", installable: false },
    { kind: "forge", display: "Forge", installable: false },
  ],
  loaderVersions: async () => [
    { version: "0.19.5", stable: false, versionId: "fabric-loader-0.19.5-26.2" },
    { version: "0.19.4", stable: true, versionId: "fabric-loader-0.19.4-26.2" },
  ],
  installLoader: async (_id, loader, v) => `${loader}-loader-${v}`,
  listModPresets: async () => [OPTIMIZED_PRESET],
  installModPreset: async (_id, preset) => {
    if (preset !== OPTIMIZED_PRESET.id) throw new Error(`unknown mod preset \`${preset}\``);
    // Long enough to see the busy state; the real one downloads ~45 MB.
    await delay(2500);
    return {
      installed: [
        ...OPTIMIZED_PRESET.mods.map((m) => `${m.slug}-26.2.jar`),
        ...OPTIMIZED_PRESET.bundled.map((b) => b.fileName),
      ],
      packs: OPTIMIZED_PRESET.packs.map((p) => p.name),
      skipped: [],
      removed: [],
      loaderInstalled: "0.18.4",
    };
  },
  removeLoader: async () => {},
  listLocales: async () => ["en-US"],
  getLocale: async (name) => ({
    name: name === "en-US" ? "en-US" : "en-US",
    messages: enUS as Record<string, string>,
    translatedKeys: 0,
    totalKeys: Object.keys(enUS).length,
    warnings: [],
  }),
  // No filesystem in the browser preview, so nothing ever fires.
  onThemesChanged: async () => () => {},
  onConfigReloaded: async () => () => {},
  openFolder: async () => {
    throw new Error("opening folders needs the desktop app");
  },
  openDataFolder: async () => {
    throw new Error("opening folders needs the desktop app");
  },
  performanceSnapshot: async () => {
    // Drift a little so the sparkline shows movement in the preview.
    const jitter = (base: number, spread: number) =>
      base + Math.round((Math.sin(Date.now() / 3000) + 1) * spread);
    return {
      launcher: { pid: 1234, memoryMb: jitter(120, 20), cpuPercent: jitter(0, 3) },
      game: null,
      startupMs: [
        ["paths", 1.4],
        ["config", 3.2],
        ["logging", 2.1],
        ["services", 6.8],
      ] as [string, number][],
      startupTotalMs: 13.5,
      uptimeSecs: Math.floor(performance.now() / 1000),
    };
  },
  // The real CHANGELOG.md, parsed the way changelog.rs does it, so the news
  // panel previews with the same text lengths the launcher shows.
  changelog: async () => parseChangelog(changelogMarkdown, 4, 3),
  setActiveAccount: async () => {},
  removeAccount: async (id) => {
    const index = mockAccounts.findIndex((a) => a.id === id);
    if (index >= 0) mockAccounts.splice(index, 1);
  },
  playInstance: async (id) => {
    // Simulate an install + launch so the Home page flow is previewable.
    const steps: Array<[string, number, number]> = [
      ["resolving version metadata", 0, 0],
      ["downloading", 1200, 5147],
      ["downloading", 3600, 5147],
      ["extracting natives", 5147, 5147],
    ];
    for (const [phase, done, total] of steps) {
      emit({
        type: "installProgress",
        instanceId: id,
        phase,
        filesDone: done,
        filesTotal: total,
        bytesDone: done * 115_000,
        bytesTotal: total * 115_000,
        bytesPerSec: 34_000_000,
      });
      await new Promise((r) => setTimeout(r, 400));
    }
    emit({ type: "gameStarted", instanceId: id, pid: 4242 });
    for (const text of [
      "[Render thread/INFO]: Setting user: PreviewPlayer",
      "[Render thread/INFO]: LWJGL Version: 3.4.1",
      "[Render thread/INFO]: Backend library: OpenGL",
    ]) {
      emit({ type: "gameLog", instanceId: id, stderr: false, text });
    }
    mockRunning = { instanceId: id, pid: 4242 };
    return 4242;
  },
  cancelPlay: async () => {},
  runningGame: async () => mockRunning,
  stopGame: async () => {
    if (mockRunning) {
      emit({
        type: "gameExited",
        instanceId: mockRunning.instanceId,
        code: 0,
        class: "killed",
        detail: "The game was stopped from the launcher.",
      });
      mockRunning = null;
    }
  },
};
