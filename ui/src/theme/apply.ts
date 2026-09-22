// Applies a resolved theme to the document. All visual values flow through
// `--fae-*` CSS custom properties — components never hard-code colors,
// sizes, or timings (§31).

import { createSignal } from "solid-js";

import type { Theme, ThemeLayout } from "../ipc/types";

/** How a theme name reads in a picker: the built-ins are the Faeries worlds
 *  and are spelled the way the server spells them; a user's folder name is
 *  shown capitalised. */
export function themeLabel(name: string): string {
  if (name === "smp") return "SMP";
  if (name === "skyblock") return "Skyblock";
  return name.charAt(0).toUpperCase() + name.slice(1);
}

/** Tokens written by the previous apply, so a theme switch can clear the
 *  ones the new theme does not define (a scene-less theme must not keep the
 *  old theme's logo). */
let appliedTokens = new Set<string>();

/** `asset-*` tokens of the current theme. A signal, because custom
 *  properties are not reactive: components that read them at mount would
 *  otherwise miss a theme applied a moment later, or a hot reload. */
const [assetTokens, setAssetTokens] = createSignal<Record<string, string>>({});

/**
 * Writes every token as a `--fae-*` custom property. This includes
 * `--fae-background-image`, the theme's full-window scene, which the
 * stylesheet paints behind the app.
 */
export function applyThemeTokens(theme: Theme) {
  const style = document.documentElement.style;
  const next = new Set<string>();
  const assets: Record<string, string> = {};
  for (const [token, value] of Object.entries(theme.tokens)) {
    style.setProperty(`--fae-${token}`, value);
    next.add(token);
    if (token.startsWith("asset-")) assets[token] = value;
  }
  for (const stale of appliedTokens) {
    if (!next.has(stale)) style.removeProperty(`--fae-${stale}`);
  }
  appliedTokens = next;
  setAssetTokens(assets);
}

/**
 * The current theme's artwork for a slot (`logo`, `mark`, `icon-home`, …) as
 * a CSS image value, or null when the theme supplies none. Reactive.
 */
export function themeAsset(slot: string): string | null {
  const value = assetTokens()[`asset-${slot}`]?.trim();
  return value && value !== "none" ? value : null;
}

export function applyLayoutVars(layout: ThemeLayout) {
  const style = document.documentElement.style;
  style.setProperty("--fae-sidebar-width", layout.sidebar?.width ?? "232px");
}

export function applyFontScale(percent: number) {
  document.documentElement.style.fontSize = `${percent}%`;
}

export function applyReducedMotion(reduced: boolean) {
  document.documentElement.dataset.reducedMotion = String(reduced);
}
