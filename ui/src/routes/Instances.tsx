import { A } from "@solidjs/router";
import { createResource, createSignal, For, Show } from "solid-js";

import { backend } from "../ipc/backend";
import type { Instance } from "../ipc/types";
import { t } from "../i18n";
import { running } from "../play";
import { reportPreset } from "../presets";
import { pushToast } from "../toasts";

export function Instances() {
  const [list, { refetch }] = createResource(() => backend.listInstances());
  // Kept as a promise as well as a resource: creating an instance must wait
  // for the list rather than fail because the user clicked Create before the
  // manifest arrived from Mojang.
  const releases = backend
    .listMinecraftVersions(false)
    .then((result) => result.manifest.versions.filter((v) => v.kind === "release"));
  const [versions] = createResource(() => releases);

  const [newName, setNewName] = createSignal("");
  const [newVersion, setNewVersion] = createSignal("");
  // On by default: a new instance starts as Fabric + the performance set.
  const [optimized, setOptimized] = createSignal(true);
  const [busy, setBusy] = createSignal(false);

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

  const create = () =>
    run(async () => {
      const available = versions() ?? (await releases);
      const version = newVersion() || available[0]?.id;
      if (!version) throw new Error(t("instances.noVersion"));
      // A blank name is the common case for a first instance; default it
      // rather than bouncing the user with a validation error.
      const name = newName().trim() || t("instances.defaultName");
      const created = await backend.createInstance(name, version);
      setNewName("");
      if (optimized()) {
        pushToast(t("presets.installing"), "info");
        reportPreset(await backend.installModPreset(created.id, "optimized"));
      }
    });

  return (
    <section class="page">
      <h1>{t("nav.instances")}</h1>

      <div class="card create-row">
        <input
          type="text"
          placeholder={t("instances.namePlaceholder")}
          value={newName()}
          disabled={busy()}
          onInput={(e) => setNewName(e.currentTarget.value)}
          onKeyDown={(e) => e.key === "Enter" && create()}
        />
        <select
          value={newVersion() || versions()?.[0]?.id || ""}
          disabled={busy()}
          onChange={(e) => setNewVersion(e.currentTarget.value)}
        >
          <Show when={versions()} fallback={<option value="">{t("instances.loadingVersions")}</option>}>
            <For each={versions() ?? []}>
              {(version) => <option value={version.id}>{version.id}</option>}
            </For>
          </Show>
        </select>
        <label class="checkbox-row" title={t("presets.optimized.hint")}>
          <input
            type="checkbox"
            checked={optimized()}
            disabled={busy()}
            onChange={(e) => setOptimized(e.currentTarget.checked)}
          />
          {t("presets.startOptimized")}
        </label>
        <button type="button" class="button-primary" disabled={busy()} onClick={create}>
          {t("instances.create")}
        </button>
      </div>

      <Show when={(list()?.problems.length ?? 0) > 0}>
        <div class="card problem-card">
          <For each={list()?.problems}>{(problem) => <p>{problem}</p>}</For>
        </div>
      </Show>

      <Show
        when={(list()?.instances.length ?? 0) > 0}
        fallback={<p class="muted">{t("instances.empty")}</p>}
      >
        <For each={list()?.instances}>
          {(instance) => (
            <InstanceRow instance={instance} busy={busy()} run={run} />
          )}
        </For>
      </Show>
    </section>
  );
}

function InstanceRow(props: {
  instance: Instance;
  busy: boolean;
  run: (action: () => Promise<unknown>) => Promise<void>;
}) {
  const [renaming, setRenaming] = createSignal(false);
  const [confirming, setConfirming] = createSignal(false);
  const [name, setName] = createSignal(props.instance.name);

  const commitRename = () =>
    props.run(async () => {
      await backend.renameInstance(props.instance.id, name());
      setRenaming(false);
    });

  return (
    <div class="card instance-row">
      <div class="instance-info">
        <Show
          when={renaming()}
          fallback={<span class="instance-name">{props.instance.name}</span>}
        >
          <input
            type="text"
            value={name()}
            disabled={props.busy}
            onInput={(e) => setName(e.currentTarget.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") commitRename();
              if (e.key === "Escape") setRenaming(false);
            }}
          />
        </Show>
        <span class="muted">
          {props.instance.minecraftVersion}
          {props.instance.loader ? ` · ${props.instance.loader.kind}` : ""}
        </span>
      </div>
      <div class="instance-actions">
        <Show
          when={!renaming()}
          fallback={
            <>
              <button type="button" disabled={props.busy} onClick={commitRename}>
                {t("instances.save")}
              </button>
              <button type="button" disabled={props.busy} onClick={() => setRenaming(false)}>
                {t("instances.cancel")}
              </button>
            </>
          }
        >
          <button
            type="button"
            class="button-primary"
            disabled={props.busy || running() !== null}
            onClick={() =>
              props.run(async () => {
                try {
                  await backend.playInstance(props.instance.id);
                } catch (error) {
                  pushToast(String(error), "error");
                }
              })
            }
          >
            {t("home.play")}
          </button>
          <A href={`/mods?instance=${props.instance.id}`} class="button-link">
            {t("instances.mods")}
          </A>
          <button type="button" disabled={props.busy} onClick={() => { setName(props.instance.name); setRenaming(true); }}>
            {t("instances.rename")}
          </button>
          <button
            type="button"
            disabled={props.busy}
            onClick={() =>
              props.run(() =>
                backend.duplicateInstance(
                  props.instance.id,
                  t("instances.copyName", { name: props.instance.name }),
                ),
              )
            }
          >
            {t("instances.duplicate")}
          </button>
          <Show
            when={confirming()}
            fallback={
              <button type="button" disabled={props.busy} onClick={() => setConfirming(true)}>
                {t("instances.delete")}
              </button>
            }
          >
            <button
              type="button"
              class="button-danger"
              disabled={props.busy}
              onClick={() =>
                props.run(async () => {
                  const trash = await backend.deleteInstance(props.instance.id);
                  pushToast(t("instances.deleted", { path: trash }), "info");
                })
              }
            >
              {t("instances.confirmDelete")}
            </button>
            <button type="button" disabled={props.busy} onClick={() => setConfirming(false)}>
              {t("instances.cancel")}
            </button>
          </Show>
        </Show>
      </div>
    </div>
  );
}
