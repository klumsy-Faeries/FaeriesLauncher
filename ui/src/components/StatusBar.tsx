import { Show } from "solid-js";

import { t } from "../i18n";
import { backend } from "../ipc/backend";
import { installStatus, running } from "../play";
import { appInfo } from "../state";

export function StatusBar() {
  // The left slot states what the launcher is doing right now; it is derived
  // from real state rather than being a decorative "up to date" label.
  const state = () => {
    if (installStatus()) return { kind: "busy", text: t("status.installing") };
    if (running()) return { kind: "busy", text: t("status.running") };
    return { kind: "ok", text: t("status.upToDate") };
  };

  return (
    <footer class="status-bar">
      <span class={`status-pill status-${state().kind}`}>
        <span class="status-dot" aria-hidden="true" />
        {state().text}
      </span>

      <span class="status-center muted">
        <Show when={appInfo()} fallback={t("status.ready")}>
          {(info) => <>{info().dataDir}</>}
        </Show>
      </span>

      <span class="status-actions">
        <button type="button" onClick={() => void backend.openDataFolder()}>
          {t("status.openFolder")}
        </button>
      </span>
    </footer>
  );
}
