// Performance dashboard (§27).
//
// Sampling is the one place the launcher deliberately polls, because
// process CPU and memory can only be observed by asking. It is confined to
// this page: the interval starts on mount and is cleared on cleanup, so
// closing the page stops it completely and the launcher returns to being
// entirely event-driven (§7).

import { createSignal, For, onCleanup, onMount, Show } from "solid-js";

import { t } from "../i18n";
import { backend } from "../ipc/backend";
import type { PerformanceSnapshot } from "../ipc/types";
import { formatBytes, installStatus } from "../play";
import { tasks } from "../tasks";

/** Slow enough to be invisible in CPU terms, fast enough to feel live. */
const SAMPLE_INTERVAL_MS = 1000;
/** Roughly a minute of history at the sample rate. */
const HISTORY = 60;

export function Performance() {
  const [snapshot, setSnapshot] = createSignal<PerformanceSnapshot | null>(null);
  const [memoryHistory, setMemoryHistory] = createSignal<number[]>([]);
  const [error, setError] = createSignal<string | null>(null);

  const sample = async () => {
    try {
      const next = await backend.performanceSnapshot();
      setSnapshot(next);
      setError(null);
      if (next.launcher) {
        setMemoryHistory((history) =>
          [...history, next.launcher!.memoryMb].slice(-HISTORY),
        );
      }
    } catch (e) {
      setError(String(e));
    }
  };

  onMount(() => {
    void sample();
    const timer = setInterval(() => void sample(), SAMPLE_INTERVAL_MS);
    // Stopping on cleanup is what keeps this page from costing anything
    // once the user navigates away.
    onCleanup(() => clearInterval(timer));
  });

  const activeTasks = () => tasks().filter((task) => task.state === "running");

  return (
    <section class="page">
      <h1>{t("perf.title")}</h1>
      <p class="muted">{t("perf.body")}</p>

      <Show when={error()}>
        {(message) => <div class="card problem-card">{message()}</div>}
      </Show>

      <div class="perf-grid">
        <div class="card home-card">
          <div class="card-header">
            <h2>{t("perf.launcher")}</h2>
          </div>
          <Show
            when={snapshot()?.launcher}
            fallback={<p class="muted">{t("home.card.detecting")}</p>}
          >
            {(stats) => (
              <div class="mini-list">
                <div class="mini-row">
                  <span class="muted">{t("perf.memory")}</span>
                  <span class="mini-name">{stats().memoryMb} MB</span>
                </div>
                <div class="mini-row">
                  <span class="muted">{t("perf.cpu")}</span>
                  <span>{stats().cpuPercent.toFixed(1)}%</span>
                </div>
                <div class="mini-row">
                  <span class="muted">{t("perf.pid")}</span>
                  <span>{stats().pid}</span>
                </div>
                <div class="mini-row">
                  <span class="muted">{t("perf.uptime")}</span>
                  <span>{formatDuration(snapshot()?.uptimeSecs ?? 0)}</span>
                </div>
              </div>
            )}
          </Show>
          <Sparkline values={memoryHistory()} />
        </div>

        <div class="card home-card">
          <div class="card-header">
            <h2>{t("perf.game")}</h2>
          </div>
          <Show
            when={snapshot()?.game}
            fallback={<p class="muted">{t("perf.gameIdle")}</p>}
          >
            {(stats) => (
              <div class="mini-list">
                <div class="mini-row">
                  <span class="muted">{t("perf.memory")}</span>
                  <span class="mini-name">{stats().memoryMb} MB</span>
                </div>
                <div class="mini-row">
                  <span class="muted">{t("perf.cpu")}</span>
                  <span>{stats().cpuPercent.toFixed(1)}%</span>
                </div>
                <div class="mini-row">
                  <span class="muted">{t("perf.pid")}</span>
                  <span>{stats().pid}</span>
                </div>
              </div>
            )}
          </Show>
        </div>

        <div class="card home-card">
          <div class="card-header">
            <h2>{t("perf.startup")}</h2>
            <span class="badge">
              {(snapshot()?.startupTotalMs ?? 0).toFixed(0)} ms
            </span>
          </div>
          <Show
            when={(snapshot()?.startupMs.length ?? 0) > 0}
            fallback={<p class="muted">{t("home.card.detecting")}</p>}
          >
            <div class="mini-list">
              <For each={snapshot()?.startupMs}>
                {([name, ms]) => (
                  <div class="mini-row">
                    <span class="muted">{name}</span>
                    <span>{ms.toFixed(1)} ms</span>
                  </div>
                )}
              </For>
            </div>
          </Show>
        </div>

        <div class="card home-card">
          <div class="card-header">
            <h2>{t("perf.activity")}</h2>
            <span class="badge">{activeTasks().length}</span>
          </div>
          <div class="mini-list">
            <Show
              when={installStatus()}
              fallback={<p class="muted">{t("perf.noTransfers")}</p>}
            >
              {(status) => (
                <>
                  <div class="mini-row">
                    <span class="muted">{t("perf.phase")}</span>
                    <span class="mini-name">{status().phase}</span>
                  </div>
                  <div class="mini-row">
                    <span class="muted">{t("perf.speed")}</span>
                    <span>{formatBytes(status().bytesPerSec)}/s</span>
                  </div>
                  <div class="mini-row">
                    <span class="muted">{t("perf.files")}</span>
                    <span>
                      {status().filesDone}/{status().filesTotal}
                    </span>
                  </div>
                </>
              )}
            </Show>
            <For each={activeTasks()}>
              {(task) => (
                <div class="mini-row">
                  <span class="muted">{task.name}</span>
                  <span>
                    {task.progress === null
                      ? "…"
                      : `${Math.round(task.progress * 100)}%`}
                  </span>
                </div>
              )}
            </For>
          </div>
        </div>
      </div>
    </section>
  );
}

/** Memory over the last minute, drawn as a simple filled area. */
function Sparkline(props: { values: number[] }) {
  const points = () => {
    const values = props.values;
    if (values.length < 2) return null;
    const max = Math.max(...values);
    const min = Math.min(...values);
    const span = Math.max(max - min, 1);
    const step = 100 / (values.length - 1);
    const path = values
      .map((v, i) => `${(i * step).toFixed(2)},${(28 - ((v - min) / span) * 26).toFixed(2)}`)
      .join(" ");
    return { path, max, min };
  };

  return (
    <Show when={points()}>
      {(data) => (
        <div class="sparkline">
          <svg viewBox="0 0 100 30" preserveAspectRatio="none" aria-hidden="true">
            <polyline
              points={data().path}
              fill="none"
              stroke="var(--fae-color-primary)"
              stroke-width="1.5"
              vector-effect="non-scaling-stroke"
            />
          </svg>
          <span class="muted sparkline-caption">
            {t("perf.range", { min: data().min, max: data().max })}
          </span>
        </div>
      )}
    </Show>
  );
}

function formatDuration(seconds: number): string {
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m ${seconds % 60}s`;
  return `${Math.floor(minutes / 60)}h ${minutes % 60}m`;
}
