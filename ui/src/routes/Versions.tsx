import { createResource, createSignal, Show } from "solid-js";

import { VirtualList } from "../components/VirtualList";

import { backend } from "../ipc/backend";
import { t } from "../i18n";
import { pushToast } from "../toasts";

export function Versions() {
  const [showAll, setShowAll] = createSignal(false);
  const [force, setForce] = createSignal(false);
  const [result, { refetch }] = createResource(async () => {
    const manifest = await backend.listMinecraftVersions(force());
    if (manifest.source === "cacheStale") {
      pushToast(t("versions.stale"), "warn");
    }
    return manifest;
  });

  const versions = () => {
    const all = result()?.manifest.versions ?? [];
    return showAll() ? all : all.filter((v) => v.kind === "release");
  };

  const refresh = async () => {
    setForce(true);
    await refetch();
    setForce(false);
  };

  return (
    <section class="page">
      <h1>{t("nav.versions")}</h1>

      <div class="card create-row">
        <label class="toggle-label">
          <input
            type="checkbox"
            checked={showAll()}
            onChange={(e) => setShowAll(e.currentTarget.checked)}
          />
          {t("versions.showAll")}
        </label>
        <span class="muted">
          <Show when={result()}>
            {(r) => (
              <>
                {t("versions.latest", {
                  release: r().manifest.latest.release,
                  snapshot: r().manifest.latest.snapshot,
                })}
              </>
            )}
          </Show>
        </span>
        <button type="button" disabled={result.loading} onClick={refresh}>
          {t("versions.refresh")}
        </button>
      </div>

      {/* Windowed: the full list is ~900 entries, but only the visible
          rows are ever in the DOM (§7, §43). */}
      <div class="card version-list">
        <VirtualList items={versions()} rowHeight={34} height={460}>
          {(version) => (
            <div class="version-row">
              <span class="version-id">{version.id}</span>
              <span class={`version-kind version-kind-${version.kind}`}>
                {version.kind}
              </span>
              <span class="muted">{version.releaseTime.slice(0, 10)}</span>
            </div>
          )}
        </VirtualList>
      </div>
    </section>
  );
}
