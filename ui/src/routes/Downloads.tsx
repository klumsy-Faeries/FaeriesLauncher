import { For, Show } from "solid-js";

import { t } from "../i18n";
import { backend } from "../ipc/backend";
import { formatBytes, installFraction, installStatus } from "../play";
import { clearFinishedTasks, tasks } from "../tasks";

/**
 * Live view of what the launcher is transferring or doing. Fed by backend
 * events, so it costs nothing while idle (§7).
 */
export function Downloads() {
  const running = () => tasks().filter((task) => task.state === "running");
  const finished = () => tasks().filter((task) => task.state !== "running");

  return (
    <section class="page">
      <h1>{t("nav.downloads")}</h1>

      <Show when={installStatus()}>
        {(status) => (
          <div class="card settings-group">
            <div class="card-header">
              <h2>{t("downloads.installing")}</h2>
              <span class="muted">{status().phase}</span>
            </div>
            <div class="progress-track">
              <div
                class="progress-fill"
                style={{
                  width: `${((installFraction(status()) ?? 0) * 100).toFixed(1)}%`,
                }}
              />
            </div>
            <div class="mini-row">
              <span class="muted">
                {status().filesDone}/{status().filesTotal} ·{" "}
                {formatBytes(status().bytesDone)}
                <Show when={status().bytesTotal}>
                  {(total) => <> / {formatBytes(total())}</>}
                </Show>
              </span>
              <Show when={status().bytesPerSec > 0}>
                <span class="muted">{formatBytes(status().bytesPerSec)}/s</span>
              </Show>
            </div>
            <button type="button" onClick={() => backend.cancelPlay()}>
              {t("home.cancel")}
            </button>
          </div>
        )}
      </Show>

      <Show when={running().length > 0}>
        <div class="card settings-group">
          <div class="card-header">
            <h2>{t("downloads.active")}</h2>
            <span class="badge">{running().length}</span>
          </div>
          <For each={running()}>
            {(task) => (
              <div class="task-row">
                <div class="mini-row">
                  <span class="mini-name">{task.name}</span>
                  <span class="muted">
                    {task.progress === null
                      ? t("downloads.starting")
                      : `${Math.round(task.progress * 100)}%`}
                  </span>
                </div>
                <div class="progress-track">
                  <div
                    class="progress-fill"
                    style={{ width: `${(task.progress ?? 0) * 100}%` }}
                  />
                </div>
                <Show when={task.message}>
                  {(message) => <p class="muted">{message()}</p>}
                </Show>
              </div>
            )}
          </For>
        </div>
      </Show>

      <Show when={finished().length > 0}>
        <div class="card settings-group">
          <div class="card-header">
            <h2>{t("downloads.recent")}</h2>
            <button type="button" onClick={clearFinishedTasks}>
              {t("console.clear")}
            </button>
          </div>
          <For each={finished()}>
            {(task) => (
              <div class="mini-row">
                <span class="mini-name">{task.name}</span>
                <span class={`task-state task-${task.state}`}>
                  {t(`downloads.state.${task.state}`)}
                </span>
              </div>
            )}
          </For>
        </div>
      </Show>

      <Show when={!installStatus() && tasks().length === 0}>
        <p class="muted">{t("downloads.idle")}</p>
      </Show>
    </section>
  );
}
