import { Route, Router, useNavigate } from "@solidjs/router";
import { createSignal, onCleanup, onMount, Show, type ParentProps } from "solid-js";

import { Sidebar } from "./components/Sidebar";
import { StatusBar } from "./components/StatusBar";
import { WindowControls } from "./components/TitleBar";
import { Toasts } from "./components/Toasts";
import { openPalette, Palette } from "./commands/Palette";
import { registerCommand, registerProvider, type Command } from "./commands/registry";
import {
  formatAccelerator,
  installShortcuts,
  parseAccelerator,
  type Binding,
} from "./commands/shortcuts";
import { applyMessages, t } from "./i18n";
import { backend } from "./ipc/backend";
import { NAV_ITEMS } from "./nav";
import { handlePlayEvent } from "./play";
import { Accounts } from "./routes/Accounts";
import { Downloads } from "./routes/Downloads";
import { Home } from "./routes/Home";
import { Instances } from "./routes/Instances";
import { Mods } from "./routes/Mods";
import { Performance } from "./routes/Performance";
import { Settings } from "./routes/Settings";
import { Setup } from "./routes/Setup";
import { Versions } from "./routes/Versions";
import { appInfo, setAppInfo, setTheme, theme } from "./state";
import { handleTaskEvent } from "./tasks";
import {
  applyFontScale,
  applyLayoutVars,
  applyReducedMotion,
  applyThemeTokens,
} from "./theme/apply";
import { pushToast } from "./toasts";

/** Live settings values, so shortcuts and providers see current config. */
const [settingsValues, setSettingsValues] = createSignal<Record<string, unknown>>({});
/** null until settings load, then true when the wizard still needs to run. */
const [needsSetup, setNeedsSetup] = createSignal<boolean | null>(null);

async function loadAndApplyTheme(name: string) {
  const resolved = await backend.getTheme(name);
  setTheme(resolved);
  applyThemeTokens(resolved);
  applyLayoutVars(resolved.layout);
  for (const warning of resolved.warnings) {
    pushToast(t("theme.warning", { warning }), "warn");
  }
}

function Layout(props: ParentProps) {
  const navigate = useNavigate();

  // ---- Commands (§23) -------------------------------------------------
  // Navigation and launcher actions are static; everything that depends on
  // live state comes from providers, consulted when the palette opens.

  for (const item of NAV_ITEMS) {
    registerCommand({
      id: `navigate.${item.id}`,
      titleKey: `nav.${item.id}`,
      categoryKey: "palette.category.navigate",
      keywords: [item.id],
      run: () => navigate(item.path),
    });
  }

  registerCommand({
    id: "launcher.refreshVersions",
    titleKey: "command.refreshVersions",
    categoryKey: "palette.category.actions",
    keywords: ["refresh", "manifest", "update"],
    run: async () => {
      await backend.listMinecraftVersions(true);
      pushToast(t("command.refreshedVersions"), "info");
    },
  });

  registerCommand({
    id: "launcher.playSelected",
    titleKey: "command.play",
    categoryKey: "palette.category.actions",
    keywords: ["launch", "start", "run"],
    run: async () => {
      const list = await backend.listInstances();
      const first = list.instances[0];
      if (!first) {
        pushToast(t("home.noInstances"), "warn");
        return;
      }
      await backend.playInstance(first.id);
    },
  });

  // Instances: launch any of them straight from the palette.
  const disposeInstances = registerProvider(async () => {
    const list = await backend.listInstances();
    return list.instances.map(
      (instance): Command => ({
        id: `instance.play.${instance.id}`,
        titleText: instance.name,
        categoryKey: "palette.category.instances",
        keywords: [instance.minecraftVersion, instance.loader?.kind ?? "vanilla"],
        hint: instance.minecraftVersion,
        run: async () => {
          try {
            await backend.playInstance(instance.id);
          } catch (error) {
            pushToast(String(error), "error");
          }
        },
      }),
    );
  });

  // Settings: typing "RAM" finds RAM-related settings (§21).
  const disposeSettings = registerProvider(async () => {
    const [schema, values] = await Promise.all([
      backend.settingsSchema(),
      backend.settingsValues(),
    ]);
    return schema.map(
      (def): Command => ({
        id: `setting.${def.id}`,
        titleText: t(`setting.${def.id}.name`),
        categoryKey: "palette.category.settings",
        keywords: [def.id, t(`setting.${def.id}.description`)],
        hint: String(values[def.id] ?? ""),
        run: () => {
          navigate("/settings");
          // Scroll the setting into view once the page has rendered.
          setTimeout(() => {
            const field = document.getElementById(def.id);
            field?.scrollIntoView({ block: "center" });
            field?.focus();
          }, 60);
        },
      }),
    );
  });

  // Minecraft versions, so "1.8.9" jumps to the version list.
  const disposeVersions = registerProvider(async () => {
    const manifest = await backend.listMinecraftVersions(false);
    return manifest.manifest.versions.slice(0, 400).map(
      (version): Command => ({
        id: `version.${version.id}`,
        titleText: version.id,
        categoryKey: "palette.category.versions",
        keywords: [version.kind],
        hint: version.kind,
        run: () => navigate("/versions"),
      }),
    );
  });

  onCleanup(() => {
    disposeInstances();
    disposeSettings();
    disposeVersions();
  });

  // ---- Shortcuts (§22) ------------------------------------------------

  const binding = (settingId: string, run: () => void): Binding | null => {
    const raw = String(settingsValues()[settingId] ?? "");
    const accel = parseAccelerator(raw);
    return accel ? { settingId, accel, run } : null;
  };

  const bindings = (): Binding[] =>
    [
      binding("shortcuts.palette", openPalette),
      binding("shortcuts.launch", () => void backend
        .listInstances()
        .then((l) => l.instances[0] && backend.playInstance(l.instances[0].id))
        .catch((e) => pushToast(String(e), "error"))),
      binding("shortcuts.instances", () => navigate("/instances")),
      binding("shortcuts.mods", () => navigate("/mods")),
      binding("shortcuts.settings", () => navigate("/settings")),
      binding("shortcuts.refresh", () => {
        void backend.listMinecraftVersions(true);
        pushToast(t("command.refreshedVersions"), "info");
      }),
    ].filter((b): b is Binding => b !== null);

  onMount(() => {
    const dispose = installShortcuts(bindings);
    onCleanup(dispose);
  });

  const sidebarEnabled = () => theme()?.layout.sidebar?.enabled ?? true;
  const sidebarItems = () =>
    theme()?.layout.sidebar?.items ?? NAV_ITEMS.map((item) => item.id);
  const statusBarEnabled = () => theme()?.layout.statusBar?.enabled ?? true;
  const socialEnabled = () => theme()?.layout.sidebar?.showSocial ?? true;
  const paletteHint = () => {
    const accel = parseAccelerator(String(settingsValues()["shortcuts.palette"] ?? ""));
    return accel ? formatAccelerator(accel) : "";
  };

  return (
    <div class="app">
      <Show when={sidebarEnabled()}>
        <Sidebar items={sidebarItems()} showSocial={socialEnabled()} />
      </Show>
      <div class="main-column">
        <div class="topbar" data-tauri-drag-region>
          <button type="button" class="search-trigger" onClick={openPalette}>
            <span>{t("palette.open")}</span>
            <Show when={paletteHint()}>
              {(hint) => <kbd>{hint()}</kbd>}
            </Show>
          </button>
          <WindowControls />
        </div>
        <main class="content">{props.children}</main>
        <Show when={statusBarEnabled()}>
          <StatusBar />
        </Show>
      </div>
      <Palette />
      <Toasts />
    </div>
  );
}

export default function App() {
  onMount(async () => {
    try {
      const values = await backend.settingsValues();
      setSettingsValues(values);

      // Language first, so every later message is already localized.
      const bundle = await backend.getLocale(String(values["launcher.language"] ?? "en-US"));
      applyMessages(bundle.name, bundle.messages);
      for (const warning of bundle.warnings) pushToast(warning, "warn");
      setNeedsSetup(!values["launcher.setup_complete"]);
      applyFontScale(Number(values["ui.font_scale"] ?? 100));
      applyReducedMotion(Boolean(values["ui.reduced_motion"]));
      await loadAndApplyTheme(String(values["ui.theme"] ?? "smp"));

      if (!appInfo()) setAppInfo(await backend.appInfo());

      for (const notice of await backend.recoveryNotices()) {
        pushToast(
          t("notice.configRecovered", {
            file: notice.file,
            backup: notice.backupPath,
          }),
          "warn",
        );
      }

      // Theme hot reload: the backend watches the themes folder and pings
      // us when anything under it changes (§31).
      await backend.onThemesChanged(() => {
        void loadAndApplyTheme(String(settingsValues()["ui.theme"] ?? "smp"));
      });

      // The backend could not see some of its files at startup and has now
      // recovered them; every page holds state derived from those files, so
      // the simplest correct reaction is to start the page over.
      await backend.onConfigReloaded(() => {
        window.location.reload();
      });

      await backend.onEvent((event) => {
        if (handlePlayEvent(event)) return;
        if (handleTaskEvent(event)) return;
        if (event.type !== "settingChanged") return;
        // Keep the local copy current so shortcuts rebind immediately.
        setSettingsValues((current) => ({ ...current, [event.id]: event.value }));
        if (event.id === "launcher.language") {
          void backend.getLocale(String(event.value)).then((next) => {
            applyMessages(next.name, next.messages);
          });
        } else if (event.id === "ui.layout_overrides") {
          // Layout lives in the theme payload, so re-resolve it.
          void loadAndApplyTheme(String(settingsValues()["ui.theme"] ?? "smp"));
        } else if (event.id === "ui.theme") {
          void loadAndApplyTheme(String(event.value));
        } else if (event.id === "ui.font_scale") {
          applyFontScale(Number(event.value));
        } else if (event.id === "ui.reduced_motion") {
          applyReducedMotion(Boolean(event.value));
        }
      });
    } catch (error) {
      pushToast(String(error), "error");
    }
  });

  // Hold the shell back until we know whether setup is needed, so the main
  // window never flashes before the wizard.
  return (
    <Show when={needsSetup() === false} fallback={
      <Show when={needsSetup() === true}>
        <Setup onFinish={() => setNeedsSetup(false)} />
      </Show>
    }>
    <Router root={Layout}>
      <Route path="/" component={Home} />
      <Route path="/instances" component={Instances} />
      <Route path="/mods" component={Mods} />
      <Route path="/versions" component={Versions} />
      <Route path="/downloads" component={Downloads} />
      <Route path="/accounts" component={Accounts} />
      <Route path="/performance" component={Performance} />
      <Route path="/settings" component={Settings} />
    </Router>
    </Show>
  );
}
