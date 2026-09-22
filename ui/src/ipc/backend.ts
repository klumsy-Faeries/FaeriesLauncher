// Backend access. Inside Tauri this calls the Rust command layer; in a plain
// browser (vite dev preview without the shell) it falls back to an in-memory
// mock so the UI stays previewable and testable on its own.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { mockBackend } from "./mock";
import type { Backend, FaerieEvent } from "./types";

export const isTauri =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const tauriBackend: Backend = {
  settingsSchema: () => invoke("settings_schema"),
  settingsValues: () => invoke("settings_values"),
  setSetting: (id, value) => invoke("set_setting", { id, value }),
  recoveryNotices: () => invoke("recovery_notices"),
  appInfo: () => invoke("app_info"),
  listThemes: () => invoke("list_themes"),
  getTheme: (name) => invoke("get_theme", { name }),
  onEvent: async (cb) =>
    listen<FaerieEvent>("faerie://event", (e) => cb(e.payload)),
  listMinecraftVersions: (force) => invoke("list_minecraft_versions", { force }),
  detectJava: () => invoke("detect_java"),
  detectHardware: () => invoke("detect_hardware"),
  listInstances: () => invoke("list_instances"),
  createInstance: (name, minecraftVersion) =>
    invoke("create_instance", { name, minecraftVersion }),
  renameInstance: (id, name) => invoke("rename_instance", { id, name }),
  duplicateInstance: (id, name) => invoke("duplicate_instance", { id, name }),
  deleteInstance: (id) => invoke("delete_instance", { id }),
  updateInstanceJava: (id, java) => invoke("update_instance_java", { id, ...java }),
  listAccounts: () => invoke("list_accounts"),
  beginSignIn: () => invoke("begin_sign_in"),
  completeSignIn: () => invoke("complete_sign_in"),
  cancelSignIn: () => invoke("cancel_sign_in"),
  listMods: (id) => invoke("list_mods", { id }),
  addMods: (id, paths) => invoke("add_mods", { id, paths }),
  setModEnabled: (id, fileName, enabled) =>
    invoke("set_mod_enabled", { id, fileName, enabled }),
  removeMod: (id, fileName) => invoke("remove_mod", { id, fileName }),
  importExistingMods: (id) => invoke("import_existing_mods", { id }),
  createModProfile: (id, name, copyFromActive) =>
    invoke("create_mod_profile", { id, name, copyFromActive }),
  activateModProfile: (id, profile) =>
    invoke("activate_mod_profile", { id, profile }),
  deleteModProfile: (id, profile) => invoke("delete_mod_profile", { id, profile }),
  supportedLoaders: () => invoke("supported_loaders"),
  loaderVersions: (loader, minecraftVersion) =>
    invoke("loader_versions", { loader, minecraftVersion }),
  installLoader: (id, loader, loaderVersion) =>
    invoke("install_loader", { id, loader, loaderVersion }),
  removeLoader: (id) => invoke("remove_loader", { id }),
  listModPresets: () => invoke("list_mod_presets"),
  installModPreset: (id, preset) => invoke("install_mod_preset", { id, preset }),
  changelog: () => invoke("changelog"),
  listLocales: () => invoke("list_locales"),
  getLocale: (name) => invoke("get_locale", { name }),
  onThemesChanged: async (cb) => listen("faerie://themes-changed", () => cb()),
  onConfigReloaded: async (cb) =>
    listen<string[]>("faerie://config-reloaded", (event) => cb(event.payload)),
  performanceSnapshot: () => invoke("performance_snapshot"),
  openFolder: (which) => invoke("open_folder", { which }),
  openDataFolder: () => invoke("open_folder", { which: "data" }),
  setActiveAccount: (id) => invoke("set_active_account", { id }),
  removeAccount: (id) => invoke("remove_account", { id }),
  playInstance: (id) => invoke("play_instance", { id }),
  cancelPlay: () => invoke("cancel_play"),
  runningGame: () => invoke("running_game"),
  stopGame: () => invoke("stop_game"),
};

export const backend: Backend = isTauri ? tauriBackend : mockBackend;
