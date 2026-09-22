import { A } from "@solidjs/router";
import { For, Show } from "solid-js";

import { t } from "../i18n";
import { NAV_ITEMS } from "../nav";
import { appInfo } from "../state";
import { themeAsset } from "../theme/apply";
import { Icon } from "./Icon";

export function Sidebar(props: { items: string[]; showSocial: boolean }) {
  const entries = () =>
    props.items
      .map((id) => NAV_ITEMS.find((item) => item.id === id))
      .filter((item): item is (typeof NAV_ITEMS)[number] => item !== undefined);

  // A theme can supply `assets/mark.*`; otherwise the built-in glyph shows.
  const markStyle = () => {
    const url = themeAsset("mark");
    return url ? { "background-image": url } : undefined;
  };

  return (
    <nav class="sidebar" aria-label={t("app.name")}>
      <div class="brand-card">
        <div
          class="brand-mark"
          classList={{ "has-mark": markStyle() !== undefined }}
          style={markStyle()}
          aria-hidden="true"
        >
          <Show when={!markStyle()}>&#10022;</Show>
        </div>
        <span class="brand-name">{t("app.name")}</span>
        <Show when={appInfo()}>
          {(info) => <span class="brand-version">v{info().version}</span>}
        </Show>
      </div>

      <div class="nav-list">
        <For each={entries()}>
          {(item) => (
            <A
              href={item.path}
              class="nav-item"
              activeClass="active"
              end={item.path === "/"}
            >
              <Icon name={item.id} />
              <span>{t(`nav.${item.id}`)}</span>
            </A>
          )}
        </For>
      </div>

      <Show when={props.showSocial}>
        <div class="sidebar-social">
          <a href="https://discord.com" target="_blank" rel="noreferrer" title="Discord">
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path d="M9 5.5S6 6 4.6 8.6 3.2 17 4.7 18.4 8 20 8 20l1-1.6m6-12.9S18 6 19.4 8.6 20.8 17 19.3 18.4 16 20 16 20l-1-1.6" />
              <circle cx="9.2" cy="12.4" r="1.2" />
              <circle cx="14.8" cy="12.4" r="1.2" />
            </svg>
          </a>
          <a href="https://x.com" target="_blank" rel="noreferrer" title="X">
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <path d="M5 5l14 14M19 5L5 19" />
            </svg>
          </a>
          <a href="https://minecraft.net" target="_blank" rel="noreferrer" title="Web">
            <svg viewBox="0 0 24 24" aria-hidden="true">
              <circle cx="12" cy="12" r="8" />
              <path d="M4 12h16" />
              <path d="M12 4a13 13 0 010 16 13 13 0 010-16z" />
            </svg>
          </a>
        </div>
      </Show>
    </nav>
  );
}
