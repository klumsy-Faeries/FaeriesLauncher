// Small toast store for transient notifications (restart-required hints,
// config-recovery notices, theme warnings).

import { createSignal } from "solid-js";

export interface Toast {
  id: number;
  text: string;
  kind: "info" | "warn" | "error";
}

const [toastList, setToastList] = createSignal<Toast[]>([]);
let nextId = 1;

export const toasts = toastList;

export function pushToast(text: string, kind: Toast["kind"] = "info") {
  const id = nextId++;
  setToastList((list) => [...list, { id, text, kind }]);
  setTimeout(() => dismissToast(id), 8000);
}

export function dismissToast(id: number) {
  setToastList((list) => list.filter((toast) => toast.id !== id));
}
