// Windowed list for large collections (§7, §43).
//
// Only the rows visible in the viewport (plus a small overscan) are in the
// DOM, so a 900-version list costs the same as a 20-row one. Rows are a
// fixed height, which is what the launcher's lists are anyway and keeps the
// maths exact — no measurement pass, no layout thrash.

import { createSignal, For, onCleanup, onMount, type JSX } from "solid-js";

export function VirtualList<T>(props: {
  items: T[];
  /** Row height in pixels; every row must match this. */
  rowHeight: number;
  /** Viewport height in pixels. */
  height: number;
  /** Extra rows rendered above and below the viewport. */
  overscan?: number;
  children: (item: T, index: number) => JSX.Element;
}) {
  const [scrollTop, setScrollTop] = createSignal(0);
  let viewport: HTMLDivElement | undefined;

  const overscan = () => props.overscan ?? 6;
  const total = () => props.items.length;

  const firstVisible = () =>
    Math.max(0, Math.floor(scrollTop() / props.rowHeight) - overscan());

  const visibleCount = () =>
    Math.ceil(props.height / props.rowHeight) + overscan() * 2;

  const slice = () =>
    props.items.slice(firstVisible(), firstVisible() + visibleCount());

  const onScroll = () => setScrollTop(viewport?.scrollTop ?? 0);

  onMount(() => {
    viewport?.addEventListener("scroll", onScroll, { passive: true });
  });
  onCleanup(() => viewport?.removeEventListener("scroll", onScroll));

  return (
    <div
      ref={viewport}
      class="virtual-viewport"
      style={{ height: `${props.height}px` }}
    >
      {/* A spacer of the full height gives the scrollbar the right size… */}
      <div style={{ height: `${total() * props.rowHeight}px`, position: "relative" }}>
        {/* …while the rendered window is offset into place. */}
        <div
          style={{
            position: "absolute",
            top: `${firstVisible() * props.rowHeight}px`,
            left: 0,
            right: 0,
          }}
        >
          <For each={slice()}>
            {(item, index) => (
              <div style={{ height: `${props.rowHeight}px` }}>
                {props.children(item, firstVisible() + index())}
              </div>
            )}
          </For>
        </div>
      </div>
    </div>
  );
}
