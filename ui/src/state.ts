// UI-side reactive state shared across components.

import { createSignal } from "solid-js";

import type { AppInfo, Theme } from "./ipc/types";

export const [theme, setTheme] = createSignal<Theme | null>(null);
export const [appInfo, setAppInfo] = createSignal<AppInfo | null>(null);
