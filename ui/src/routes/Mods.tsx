import { useSearchParams } from "@solidjs/router";
import { createResource, createSignal, For, Show } from "solid-js";

import { backend } from "../ipc/backend";
import type { CompatIssue, LoaderVersion, ModEntry } from "../ipc/types";
import { t } from "../i18n";
import { reportPreset } from "../presets";
import { pushToast } from "../toasts";

/// Mods are always shown for one instance; the picker chooses which.
export function Mods() {
  const [instances] = createResource(() => backend.listInstances());
  // `/mods?instance=<id>` opens a specific instance (links from Home and
  // the Instances page); the picker overrides it from then on.
  const [params] = useSearchParams<{ instance?: string }>();
  const [selected, setSelected] = createSignal<string>("");
  const [busy, setBusy] = createSignal(false);

  const instanceId = () => {
    const explicit = selected();
    if (explicit) return explicit;
    const wanted = params.instance;
    if (wanted && instances()?.instances.some((i) => i.id === wanted)) return wanted;
    return instances()?.instances[0]?.id ?? "";
  };

  const [view, { refetch }] = createResource(instanceId, (id) =>
    id ? backend.listMods(id) : Promise.resolve(null),
  );

  const run = async (action: () => Promise<unknown>) => {
    setBusy(true);
    try {
      await action();
      await refetch();
    } catch (error) {
      pushToast(String(error), "error");
    } finally {
      setBusy(false);
    }
  };

  return (
    <section class="page">
      <h1>{t("nav.mods")}</h1>

      <Show
        when={(instances()?.instances.length ?? 0) > 0}
        fallback={<p class="muted">{t("mods.noInstances")}</p>}
      >
        <div class="card create-row">
          <select
            value={instanceId()}
            disabled={busy()}
            onChange={(e) => setSelected(e.currentTarget.value)}
          >
            <For each={instances()?.instances}>
              {(instance) => (
                <option value={instance.id} selected={instance.id === instanceId()}>
                  {instance.name} · {instance.minecraftVersion}
                  {instance.loader ? ` · ${instance.loader.kind}` : ""}
                </option>
              )}
            </For>
          </select>
          <button
            type="button"
            disabled={busy()}
            onClick={() =>
              run(async () => {
                const n = await backend.importExistingMods(instanceId());
                pushToast(t("mods.imported", { count: n }), "info");
              })
            }
          >
            {t("mods.import")}
          </button>
          <button
            type="button"
            class="button-primary"
            disabled={busy()}
            title={t("presets.optimized.hint")}
            onClick={() =>
              run(async () => {
                pushToast(t("presets.installing"), "info");
                reportPreset(await backend.installModPreset(instanceId(), "optimized"));
              })
            }
          >
            {t("presets.optimized.action")}
          </button>
        </div>

        <Show when={view()}>
          {(v) => (
            <>
              <LoaderCard
                instanceId={instanceId()}
                minecraftVersion={
                  instances()?.instances.find((i) => i.id === instanceId())
                    ?.minecraftVersion ?? ""
                }
                currentLoader={
                  instances()?.instances.find((i) => i.id === instanceId())?.loader ??
                  null
                }
                busy={busy()}
                run={run}
              />

              <ProfileCard
                instanceId={instanceId()}
                active={v().activeProfile}
                profiles={v().profiles}
                busy={busy()}
                run={run}
              />

              <CompatCard report={v().report} />

              <Show
                when={v().mods.length > 0}
                fallback={<p class="muted">{t("mods.empty")}</p>}
              >
                <div class="card mod-list">
                  <For each={v().mods}>
                    {(mod) => (
                      <ModRow
                        mod={mod}
                        instanceId={instanceId()}
                        busy={busy()}
                        run={run}
                      />
                    )}
                  </For>
                </div>
              </Show>

              <Show when={v().problems.length > 0}>
                <div class="card problem-card">
                  <For each={v().problems}>{(p) => <p>{p}</p>}</For>
                </div>
              </Show>
            </>
          )}
        </Show>
      </Show>
    </section>
  );
}

function LoaderCard(props: {
  instanceId: string;
  minecraftVersion: string;
  currentLoader: { kind: string; version: string } | null;
  busy: boolean;
  run: (action: () => Promise<unknown>) => Promise<void>;
}) {
  const [options] = createResource(() => backend.supportedLoaders());
  const [kind, setKind] = createSignal("fabric");
  const [versions, setVersions] = createSignal<LoaderVersion[]>([]);
  const [chosen, setChosen] = createSignal("");
  const [loading, setLoading] = createSignal(false);

  const loadVersions = async (loaderKind: string) => {
    setLoading(true);
    setVersions([]);
    try {
      const list = await backend.loaderVersions(loaderKind, props.minecraftVersion);
      setVersions(list);
      setChosen(list.find((v) => v.stable)?.version ?? list[0]?.version ?? "");
    } catch (error) {
      pushToast(String(error), "error");
    } finally {
      setLoading(false);
    }
  };

  const selectedOption = () => options()?.find((o) => o.kind === kind());

  return (
    <div class="card settings-group">
      <h2>{t("mods.loaderTitle")}</h2>
      <p class="muted">
        <Show
          when={props.currentLoader}
          fallback={t("mods.loaderNone")}
        >
          {(l) => t("mods.loaderCurrent", { kind: l().kind, version: l().version })}
        </Show>
      </p>
      <div class="create-row">
        <select
          value={kind()}
          disabled={props.busy}
          onChange={(e) => {
            setKind(e.currentTarget.value);
            void loadVersions(e.currentTarget.value);
          }}
        >
          <For each={options()}>
            {(option) => <option value={option.kind}>{option.display}</option>}
          </For>
        </select>
        <select
          value={chosen()}
          disabled={props.busy || versions().length === 0}
          onChange={(e) => setChosen(e.currentTarget.value)}
        >
          <Show
            when={versions().length > 0}
            fallback={
              <option value="">
                {loading() ? t("mods.loadingVersions") : t("mods.pickLoader")}
              </option>
            }
          >
            <For each={versions()}>
              {(v) => (
                <option value={v.version}>
                  {v.version}
                  {v.stable ? "" : " (beta)"}
                </option>
              )}
            </For>
          </Show>
        </select>
        <button
          type="button"
          disabled={props.busy || versions().length === 0}
          onClick={() => void loadVersions(kind())}
        >
          {t("versions.refresh")}
        </button>
        <button
          type="button"
          class="button-primary"
          disabled={props.busy || !chosen()}
          onClick={() =>
            props.run(async () => {
              await backend.installLoader(props.instanceId, kind(), chosen());
              pushToast(t("mods.loaderInstalled", { kind: kind() }), "info");
            })
          }
        >
          {t("mods.installLoader")}
        </button>
      </div>
      <Show when={selectedOption() && !selectedOption()!.installable}>
        <p class="muted">
          {t("mods.loaderNotInstallable", {
            loader: selectedOption()!.display,
          })}
        </p>
      </Show>
    </div>
  );
}

function ProfileCard(props: {
  instanceId: string;
  active: string;
  profiles: { key: string; name: string; modCount: number; enabledCount: number }[];
  busy: boolean;
  run: (action: () => Promise<unknown>) => Promise<void>;
}) {
  const [newName, setNewName] = createSignal("");

  return (
    <div class="card settings-group">
      <h2>{t("mods.profilesTitle")}</h2>
      <div class="create-row">
        <select
          value={props.active}
          disabled={props.busy}
          onChange={(e) =>
            props.run(() =>
              backend.activateModProfile(props.instanceId, e.currentTarget.value),
            )
          }
        >
          <For each={props.profiles}>
            {(p) => (
              <option value={p.key}>
                {p.name} ({p.enabledCount}/{p.modCount})
              </option>
            )}
          </For>
        </select>
        <input
          type="text"
          placeholder={t("mods.newProfile")}
          value={newName()}
          disabled={props.busy}
          onInput={(e) => setNewName(e.currentTarget.value)}
        />
        <button
          type="button"
          disabled={props.busy || !newName().trim()}
          onClick={() =>
            props.run(async () => {
              await backend.createModProfile(props.instanceId, newName(), true);
              setNewName("");
            })
          }
        >
          {t("mods.duplicateProfile")}
        </button>
        <button
          type="button"
          disabled={props.busy || props.profiles.length <= 1}
          onClick={() =>
            props.run(() =>
              backend.deleteModProfile(props.instanceId, props.active),
            )
          }
        >
          {t("instances.delete")}
        </button>
      </div>
    </div>
  );
}

function CompatCard(props: { report: { issues: CompatIssue[]; modsChecked: number } }) {
  const errors = () => props.report.issues.filter((i) => i.severity === "error");
  const others = () => props.report.issues.filter((i) => i.severity !== "error");

  return (
    <Show
      when={props.report.issues.length > 0}
      fallback={
        <div class="card compat-ok">
          {t("mods.allGood", { count: props.report.modsChecked })}
        </div>
      }
    >
      <div class="card settings-group">
        <h2>{t("mods.issuesTitle")}</h2>
        <For each={[...errors(), ...others()]}>
          {(issue) => (
            <div class={`issue issue-${issue.severity}`}>
              <div class="issue-summary">{issue.summary}</div>
              <div class="issue-detail">{issue.detail}</div>
              <div class="issue-fix">{issue.fix}</div>
            </div>
          )}
        </For>
      </div>
    </Show>
  );
}

function ModRow(props: {
  mod: ModEntry;
  instanceId: string;
  busy: boolean;
  run: (action: () => Promise<unknown>) => Promise<void>;
}) {
  const displayName = () => props.mod.name || props.mod.fileName;
  const meta = () => {
    const bits = [];
    if (props.mod.version) bits.push(props.mod.version);
    if (props.mod.loader !== "unknown") bits.push(props.mod.loader);
    if (props.mod.authors.length > 0) bits.push(props.mod.authors.join(", "));
    return bits.join(" · ");
  };

  return (
    <div class={`mod-row${props.mod.enabled ? "" : " mod-disabled"}`}>
      <input
        type="checkbox"
        checked={props.mod.enabled}
        disabled={props.busy || !props.mod.sha1}
        title={props.mod.sha1 ? "" : t("mods.untracked")}
        onChange={(e) =>
          props.run(() =>
            backend.setModEnabled(
              props.instanceId,
              props.mod.fileName,
              e.currentTarget.checked,
            ),
          )
        }
      />
      <div class="mod-info">
        <span class="mod-name">{displayName()}</span>
        <span class="muted">{meta()}</span>
      </div>
      <button
        type="button"
        disabled={props.busy || !props.mod.sha1}
        onClick={() =>
          props.run(() => backend.removeMod(props.instanceId, props.mod.fileName))
        }
      >
        {t("mods.remove")}
      </button>
    </div>
  );
}
