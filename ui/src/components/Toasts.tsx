import { For } from "solid-js";

import { dismissToast, toasts } from "../toasts";

export function Toasts() {
  return (
    <div class="toasts" role="status" aria-live="polite">
      <For each={toasts()}>
        {(toast) => (
          <button
            type="button"
            class={`toast toast-${toast.kind}`}
            onClick={() => dismissToast(toast.id)}
          >
            {toast.text}
          </button>
        )}
      </For>
    </div>
  );
}
