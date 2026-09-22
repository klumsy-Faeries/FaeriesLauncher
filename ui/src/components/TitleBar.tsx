// Window controls for a custom titlebar.
//
// These only render when the window is genuinely undecorated. On this
// Tauri/Windows build, `decorations: false` prevents the window from being
// created at all, so the OS frame is currently left on and these stay
// hidden rather than duplicating the system buttons. Asking the window
// itself keeps the two in sync if that config ever changes.

import { createSignal, onMount, Show } from "solid-js";

import { isTauri } from "../ipc/backend";
import { t } from "../i18n";

async function currentWindow() {
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  return getCurrentWindow();
}

export function WindowControls() {
  const [undecorated, setUndecorated] = createSignal(false);

  onMount(async () => {
    if (!isTauri) return;
    try {
      const win = await currentWindow();
      setUndecorated(!(await win.isDecorated()));
    } catch {
      // If the window cannot be queried, showing nothing is the safe
      // outcome: the OS frame is still there to close the app.
      setUndecorated(false);
    }
  });

  const act = async (action: "minimize" | "toggleMaximize" | "close") => {
    const win = await currentWindow();
    if (action === "minimize") await win.minimize();
    else if (action === "toggleMaximize") await win.toggleMaximize();
    else await win.close();
  };

  return (
    <Show when={undecorated()}>
      <div class="window-controls">
        <button
          type="button"
          class="window-button"
          aria-label={t("window.minimize")}
          onClick={() => void act("minimize")}
        >
          <svg viewBox="0 0 12 12" aria-hidden="true">
            <path d="M2 6h8" />
          </svg>
        </button>
        <button
          type="button"
          class="window-button"
          aria-label={t("window.maximize")}
          onClick={() => void act("toggleMaximize")}
        >
          <svg viewBox="0 0 12 12" aria-hidden="true">
            <rect x="2.5" y="2.5" width="7" height="7" rx="1" />
          </svg>
        </button>
        <button
          type="button"
          class="window-button window-close"
          aria-label={t("window.close")}
          onClick={() => void act("close")}
        >
          <svg viewBox="0 0 12 12" aria-hidden="true">
            <path d="M3 3l6 6M9 3l-6 6" />
          </svg>
        </button>
      </div>
    </Show>
  );
}
