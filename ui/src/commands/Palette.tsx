// The global search palette (§21).
//
// One box searches everything the launcher can do or show: actions,
// settings, instances, mods, and Minecraft versions. Results come from the
// command registry, so anything registered as a command is searchable for
// free — including things added later.

import { createEffect, createSignal, For, onCleanup, Show } from "solid-js";

import { t } from "../i18n";
import { allCommands, search, type Command } from "./registry";

const [open, setOpen] = createSignal(false);

export function openPalette() {
  setOpen(true);
}

export function closePalette() {
  setOpen(false);
}

export function paletteOpen() {
  return open();
}

/** A command's display label, whether it came from i18n or live data. */
function label(command: Command): string {
  return command.titleText ?? (command.titleKey ? t(command.titleKey) : command.id);
}

export function Palette() {
  const [query, setQuery] = createSignal("");
  const [entries, setEntries] = createSignal<Command[]>([]);
  const [active, setActive] = createSignal(0);
  let inputRef: HTMLInputElement | undefined;
  let listRef: HTMLDivElement | undefined;

  // Refresh the command set every time the palette opens, so live entries
  // (instances, settings values) reflect the current state.
  createEffect(() => {
    if (!open()) return;
    setQuery("");
    setActive(0);
    void allCommands().then(setEntries);
    queueMicrotask(() => inputRef?.focus());
  });

  const results = () => search(query(), entries(), label).slice(0, 40);

  const grouped = () => {
    const groups: Array<{ key: string; items: Command[] }> = [];
    for (const command of results()) {
      const existing = groups.find((g) => g.key === command.categoryKey);
      if (existing) existing.items.push(command);
      else groups.push({ key: command.categoryKey, items: [command] });
    }
    return groups;
  };

  /** Results in display order, so arrow keys walk the grouped list. */
  const flat = () => grouped().flatMap((group) => group.items);

  const runActive = () => {
    const command = flat()[active()];
    if (!command) return;
    closePalette();
    void command.run();
  };

  const move = (delta: number) => {
    const total = flat().length;
    if (total === 0) return;
    setActive((current) => (current + delta + total) % total);
    // Keep the highlighted row in view without scrolling the page.
    queueMicrotask(() => {
      listRef
        ?.querySelector<HTMLElement>(".palette-item.active")
        ?.scrollIntoView({ block: "nearest" });
    });
  };

  const onKeyDown = (event: KeyboardEvent) => {
    switch (event.key) {
      case "Escape":
        event.preventDefault();
        closePalette();
        break;
      case "ArrowDown":
        event.preventDefault();
        move(1);
        break;
      case "ArrowUp":
        event.preventDefault();
        move(-1);
        break;
      case "Enter":
        event.preventDefault();
        runActive();
        break;
    }
  };

  onCleanup(() => setOpen(false));

  return (
    <Show when={open()}>
      <div
        class="palette-backdrop"
        onClick={closePalette}
        role="presentation"
      >
        <div
          class="palette"
          role="dialog"
          aria-modal="true"
          aria-label={t("palette.title")}
          onClick={(e) => e.stopPropagation()}
        >
          <input
            ref={inputRef}
            class="palette-input"
            type="text"
            placeholder={t("palette.placeholder")}
            value={query()}
            onInput={(e) => {
              setQuery(e.currentTarget.value);
              setActive(0);
            }}
            onKeyDown={onKeyDown}
            aria-label={t("palette.placeholder")}
          />

          <div class="palette-results" ref={listRef}>
            <Show
              when={flat().length > 0}
              fallback={<p class="palette-empty muted">{t("palette.noResults")}</p>}
            >
              <For each={grouped()}>
                {(group) => (
                  <div class="palette-group">
                    <div class="palette-group-title">{t(group.key)}</div>
                    <For each={group.items}>
                      {(command) => {
                        const index = () => flat().indexOf(command);
                        return (
                          <button
                            type="button"
                            class={`palette-item${index() === active() ? " active" : ""}`}
                            onMouseEnter={() => setActive(index())}
                            onClick={() => {
                              closePalette();
                              void command.run();
                            }}
                          >
                            <span class="palette-label">{label(command)}</span>
                            <Show when={command.hint}>
                              <span class="palette-hint muted">{command.hint}</span>
                            </Show>
                          </button>
                        );
                      }}
                    </For>
                  </div>
                )}
              </For>
            </Show>
          </div>

          <div class="palette-footer muted">{t("palette.footer")}</div>
        </div>
      </div>
    </Show>
  );
}
