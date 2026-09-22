// Localization (§49).
//
// Components never hard-code user-facing text; they call `t("key")`. The
// message set is loaded from the backend at startup, which merges the
// selected language over English, so a partial translation shows translated
// text where it exists and English everywhere else.
//
// The embedded English copy is kept as an immediate fallback so the very
// first render (before the backend responds) still shows real words rather
// than raw keys.

import { createSignal } from "solid-js";

import enUS from "@locales/en-US.json";

const embedded: Record<string, string> = enUS;

const [messages, setMessages] = createSignal<Record<string, string>>(embedded);
const [locale, setLocale] = createSignal("en-US");

/** Replace the active message set (called after loading from the backend). */
export function applyMessages(name: string, next: Record<string, string>) {
  setLocale(name);
  setMessages(next);
}

export function currentLocale() {
  return locale();
}

export function t(
  key: string,
  params?: Record<string, string | number>,
): string {
  let message = messages()[key] ?? embedded[key] ?? key;
  if (params) {
    for (const [name, value] of Object.entries(params)) {
      message = message.replace(`{${name}}`, String(value));
    }
  }
  return message;
}
