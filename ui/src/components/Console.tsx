import { createEffect, createSignal, For, Show } from "solid-js";

import { t } from "../i18n";
import { clearConsole, consoleLines } from "../play";

/// Live Minecraft output (§26): searchable, copyable, clearable, with
/// auto-scroll the user can pause. The buffer itself is bounded in play.ts.
export function Console() {
  const [filter, setFilter] = createSignal("");
  const [autoScroll, setAutoScroll] = createSignal(true);
  let viewport: HTMLDivElement | undefined;

  const visible = () => {
    const needle = filter().toLowerCase();
    const lines = consoleLines();
    return needle ? lines.filter((l) => l.text.toLowerCase().includes(needle)) : lines;
  };

  createEffect(() => {
    // Re-run whenever lines change; scroll only if the user wants it.
    visible();
    if (autoScroll() && viewport) {
      viewport.scrollTop = viewport.scrollHeight;
    }
  });

  const copyAll = async () => {
    try {
      await navigator.clipboard.writeText(visible().map((l) => l.text).join("\n"));
    } catch {
      // Clipboard access can be denied; not worth interrupting the user.
    }
  };

  return (
    <div class="card console">
      <div class="console-toolbar">
        <input
          type="text"
          placeholder={t("console.search")}
          value={filter()}
          onInput={(e) => setFilter(e.currentTarget.value)}
        />
        <label class="toggle-label">
          <input
            type="checkbox"
            checked={autoScroll()}
            onChange={(e) => setAutoScroll(e.currentTarget.checked)}
          />
          {t("console.autoScroll")}
        </label>
        <button type="button" onClick={copyAll}>
          {t("console.copy")}
        </button>
        <button type="button" onClick={clearConsole}>
          {t("console.clear")}
        </button>
      </div>
      <div class="console-output" ref={viewport}>
        <Show
          when={visible().length > 0}
          fallback={<p class="muted">{t("console.empty")}</p>}
        >
          <For each={visible()}>
            {(line) => (
              <div class={line.stderr ? "console-line console-stderr" : "console-line"}>
                {line.text}
              </div>
            )}
          </For>
        </Show>
      </div>
    </div>
  );
}
