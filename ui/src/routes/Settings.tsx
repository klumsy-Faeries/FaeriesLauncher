import { createResource, createSignal, For, Match, Show, Switch } from "solid-js";

import { LayoutPanel } from "../components/LayoutPanel";
import { backend } from "../ipc/backend";
import type { SettingSchema } from "../ipc/types";
import { t } from "../i18n";
import { appInfo } from "../state";
import { themeLabel } from "../theme/apply";
import { pushToast } from "../toasts";

// The settings page renders entirely from the backend's schema (§19, §38):
// adding a setting to faerie-core's registry makes it appear here with
// validation, description, and persistence — no UI changes needed.
export function Settings() {
  const [schema] = createResource(() => backend.settingsSchema());
  const [values, { mutate: mutateValues }] = createResource(() =>
    backend.settingsValues(),
  );
  const [themeNames] = createResource(() => backend.listThemes());
  const [localeNames] = createResource(() => backend.listLocales());
  const [saving, setSaving] = createSignal<string | null>(null);

  const groups = () => {
    const defs = schema() ?? [];
    const order: string[] = [];
    const byGroup = new Map<string, SettingSchema[]>();
    for (const def of defs) {
      const group = def.id.split(".")[0] ?? "";
      if (!byGroup.has(group)) {
        byGroup.set(group, []);
        order.push(group);
      }
      byGroup.get(group)?.push(def);
    }
    return order.map((name) => ({ name, defs: byGroup.get(name) ?? [] }));
  };

  const valueOf = (def: SettingSchema) => values()?.[def.id] ?? def.default;

  async function save(def: SettingSchema, value: unknown) {
    setSaving(def.id);
    try {
      const outcome = await backend.setSetting(def.id, value);
      mutateValues((current) => ({ ...(current ?? {}), [def.id]: value }));
      if (outcome.restartRequired) {
        pushToast(t("settings.restartRequired"), "warn");
      }
    } catch (error) {
      pushToast(t("settings.saveError", { error: String(error) }), "error");
    } finally {
      setSaving(null);
    }
  }

  return (
    <section class="page">
      <h1>{t("settings.title")}</h1>
      <For each={groups()}>
        {(group) => (
          <div class="card settings-group">
            <h2>{t(`settings.group.${group.name}`)}</h2>
            <For each={group.defs.filter((d) => d.id !== "ui.layout_overrides")}>
              {(def) => (
                <div class="setting-row">
                  <div class="setting-info">
                    <label for={def.id}>{t(`setting.${def.id}.name`)}</label>
                    <p class="muted">{t(`setting.${def.id}.description`)}</p>
                  </div>
                  <div class="setting-control">
                    <SettingControl
                      def={def}
                      value={valueOf(def)}
                      disabled={saving() === def.id}
                      themeNames={themeNames() ?? []}
                      localeNames={localeNames() ?? []}
                      onChange={(value) => save(def, value)}
                    />
                  </div>
                </div>
              )}
            </For>
          </div>
        )}
      </For>

      <LayoutPanel
        overrides={String(values()?.["ui.layout_overrides"] ?? "{}")}
        onChange={(next) =>
          mutateValues((current) => ({ ...(current ?? {}), "ui.layout_overrides": next }))
        }
      />

      <div class="card settings-group">
        <h2>{t("settings.about")}</h2>
        <Show when={appInfo()}>
          {(info) => (
            <>
              <div class="about-row">
                <span>{t("settings.aboutVersion")}</span>
                <code>{info().version}</code>
              </div>
              <div class="about-row">
                <span>{t("settings.aboutDataDir")}</span>
                <code>{info().dataDir}</code>
              </div>
              <div class="about-row">
                <span>{t("settings.aboutLogsDir")}</span>
                <code>{info().logsDir}</code>
              </div>
            </>
          )}
        </Show>
      </div>
    </section>
  );
}

function SettingControl(props: {
  def: SettingSchema;
  value: unknown;
  disabled: boolean;
  themeNames: string[];
  localeNames: string[];
  onChange: (value: unknown) => void;
}) {
  return (
    <Switch>
      <Match when={props.def.kind === "bool"}>
        <input
          id={props.def.id}
          type="checkbox"
          checked={Boolean(props.value)}
          disabled={props.disabled}
          onChange={(e) => props.onChange(e.currentTarget.checked)}
        />
      </Match>
      <Match when={props.def.kind === "uint" && props.def}>
        {(def) => {
          const uint = def() as Extract<SettingSchema, { kind: "uint" }>;
          return (
            <input
              id={props.def.id}
              type="number"
              min={uint.min}
              max={uint.max}
              value={Number(props.value)}
              disabled={props.disabled}
              onChange={(e) => {
                const parsed = Number(e.currentTarget.value);
                if (Number.isInteger(parsed)) props.onChange(parsed);
              }}
            />
          );
        }}
      </Match>
      <Match when={props.def.kind === "choice" && props.def}>
        {(def) => {
          const choice = def() as Extract<SettingSchema, { kind: "choice" }>;
          return (
            <select
              id={props.def.id}
              value={String(props.value)}
              disabled={props.disabled}
              onChange={(e) => props.onChange(e.currentTarget.value)}
            >
              <For each={choice.options}>
                {(option) => (
                  <option value={option} selected={option === String(props.value)}>
                    {option}
                  </option>
                )}
              </For>
            </select>
          );
        }}
      </Match>
      <Match when={props.def.id === "launcher.language"}>
        <select
          id={props.def.id}
          value={String(props.value)}
          disabled={props.disabled}
          onChange={(e) => props.onChange(e.currentTarget.value)}
        >
          <For each={props.localeNames}>
            {(name) => (
              <option value={name} selected={name === String(props.value)}>
                {name}
              </option>
            )}
          </For>
        </select>
      </Match>
      <Match when={props.def.kind === "text" && props.def.id === "ui.theme"}>
        <select
          id={props.def.id}
          value={String(props.value)}
          disabled={props.disabled}
          onChange={(e) => props.onChange(e.currentTarget.value)}
        >
          <For each={props.themeNames}>
            {(name) => (
              <option value={name} selected={name === String(props.value)}>
                {themeLabel(name)}
              </option>
            )}
          </For>
        </select>
      </Match>
      <Match when={props.def.kind === "text"}>
        <input
          id={props.def.id}
          type="text"
          value={String(props.value)}
          disabled={props.disabled}
          onChange={(e) => props.onChange(e.currentTarget.value)}
        />
      </Match>
    </Switch>
  );
}
