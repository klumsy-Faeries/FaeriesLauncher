// Layout customization controls (§6).
//
// Writes to the `ui.layout_overrides` setting — a JSON object merged over
// the active theme's `layout.json`. Overrides live in config rather than in
// the theme folder, so the user's arrangement survives a theme switch and a
// built-in theme is never written to.

import { createMemo, For, Show } from "solid-js";

import { t } from "../i18n";
import { backend } from "../ipc/backend";
import { NAV_ITEMS } from "../nav";
import { theme } from "../state";
import { pushToast } from "../toasts";

const HOME_CARDS = ["mods", "instance", "system", "versions"];

export function LayoutPanel(props: {
  overrides: string;
  onChange: (next: string) => void;
}) {
  const parsed = createMemo<Record<string, any>>(() => {
    try {
      const value = JSON.parse(props.overrides || "{}");
      return value && typeof value === "object" ? value : {};
    } catch {
      // A hand-edited override that will not parse is reported by the
      // backend; treat it as empty here rather than crashing the panel.
      return {};
    }
  });

  // What is actually in effect: the theme's layout after merging overrides.
  const effective = () => theme()?.layout ?? {};

  const write = async (mutate: (draft: Record<string, any>) => void) => {
    const draft = structuredClone(parsed());
    mutate(draft);
    const text = JSON.stringify(draft);
    try {
      await backend.setSetting("ui.layout_overrides", text);
      props.onChange(text);
    } catch (error) {
      pushToast(String(error), "error");
    }
  };

  const setPath = (section: string, key: string, value: unknown) =>
    write((draft) => {
      draft[section] = { ...(draft[section] ?? {}), [key]: value };
    });

  const sidebarItems = (): string[] =>
    effective().sidebar?.items ?? NAV_ITEMS.map((i) => i.id);

  const homeCards = (): string[] => effective().home?.cards ?? HOME_CARDS;

  const moveItem = (list: string[], id: string, delta: number): string[] => {
    const index = list.indexOf(id);
    const target = index + delta;
    if (index < 0 || target < 0 || target >= list.length) return list;
    const next = [...list];
    next.splice(index, 1);
    next.splice(target, 0, id);
    return next;
  };

  const toggleItem = (list: string[], id: string, all: string[]): string[] =>
    list.includes(id)
      ? list.filter((entry) => entry !== id)
      // Re-adding restores the item to its canonical position rather than
      // dumping it at the end.
      : all.filter((entry) => list.includes(entry) || entry === id);

  return (
    <div class="card settings-group">
      <div class="card-header">
        <h2>{t("layout.title")}</h2>
        <Show when={Object.keys(parsed()).length > 0}>
          <button
            type="button"
            onClick={async () => {
              await backend.setSetting("ui.layout_overrides", "{}");
              props.onChange("{}");
            }}
          >
            {t("layout.reset")}
          </button>
        </Show>
      </div>
      <p class="muted">{t("layout.body")}</p>

      <div class="setting-row">
        <div class="setting-info">
          <label for="layout-sidebar">{t("layout.sidebarEnabled")}</label>
        </div>
        <div class="setting-control">
          <input
            id="layout-sidebar"
            type="checkbox"
            checked={effective().sidebar?.enabled ?? true}
            onChange={(e) => setPath("sidebar", "enabled", e.currentTarget.checked)}
          />
        </div>
      </div>

      <div class="setting-row">
        <div class="setting-info">
          <label for="layout-sidebar-width">{t("layout.sidebarWidth")}</label>
        </div>
        <div class="setting-control">
          <input
            id="layout-sidebar-width"
            type="text"
            value={effective().sidebar?.width ?? "232px"}
            onChange={(e) => setPath("sidebar", "width", e.currentTarget.value)}
          />
        </div>
      </div>

      <div class="setting-row">
        <div class="setting-info">
          <label for="layout-statusbar">{t("layout.statusBar")}</label>
        </div>
        <div class="setting-control">
          <input
            id="layout-statusbar"
            type="checkbox"
            checked={effective().statusBar?.enabled ?? true}
            onChange={(e) => setPath("statusBar", "enabled", e.currentTarget.checked)}
          />
        </div>
      </div>

      <h3 class="layout-subtitle">{t("layout.navItems")}</h3>
      <OrderedToggleList
        all={NAV_ITEMS.map((i) => i.id)}
        current={sidebarItems()}
        labelKey={(id) => `nav.${id}`}
        onChange={(next) => setPath("sidebar", "items", next)}
        move={moveItem}
        toggle={toggleItem}
      />

      <h3 class="layout-subtitle">{t("layout.homeCards")}</h3>
      <OrderedToggleList
        all={HOME_CARDS}
        current={homeCards()}
        labelKey={(id) => `home.card.${id === "versions" ? "versions" : id}`}
        onChange={(next) => setPath("home", "cards", next)}
        move={moveItem}
        toggle={toggleItem}
      />
    </div>
  );
}

function OrderedToggleList(props: {
  all: string[];
  current: string[];
  labelKey: (id: string) => string;
  onChange: (next: string[]) => void;
  move: (list: string[], id: string, delta: number) => string[];
  toggle: (list: string[], id: string, all: string[]) => string[];
}) {
  // Enabled entries first in their configured order, then the hidden ones.
  const rows = () => [
    ...props.current,
    ...props.all.filter((id) => !props.current.includes(id)),
  ];

  return (
    <div class="layout-list">
      <For each={rows()}>
        {(id) => {
          const enabled = () => props.current.includes(id);
          return (
            <div class={`layout-row${enabled() ? "" : " layout-off"}`}>
              <input
                type="checkbox"
                checked={enabled()}
                aria-label={t(props.labelKey(id))}
                onChange={() =>
                  props.onChange(props.toggle(props.current, id, props.all))
                }
              />
              <span class="layout-name">{t(props.labelKey(id))}</span>
              <div class="layout-move">
                <button
                  type="button"
                  disabled={!enabled() || props.current.indexOf(id) === 0}
                  onClick={() => props.onChange(props.move(props.current, id, -1))}
                  aria-label={t("layout.moveUp")}
                >
                  ↑
                </button>
                <button
                  type="button"
                  disabled={
                    !enabled() ||
                    props.current.indexOf(id) === props.current.length - 1
                  }
                  onClick={() => props.onChange(props.move(props.current, id, 1))}
                  aria-label={t("layout.moveDown")}
                >
                  ↓
                </button>
              </div>
            </div>
          );
        }}
      </For>
    </div>
  );
}
