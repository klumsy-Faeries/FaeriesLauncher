import type { JSX } from "solid-js";

// Inline stroke icons keyed by nav id. Themes can restyle them freely since
// they inherit currentColor; a future asset system can swap them wholesale.
const PATHS: Record<string, JSX.Element> = {
  home: (
    <>
      <path d="M3 11l9-8 9 8" />
      <path d="M5 10v10h14V10" />
    </>
  ),
  instances: (
    <>
      <path d="M21 8l-9-5-9 5 9 5 9-5z" />
      <path d="M3 8v8l9 5 9-5V8" />
    </>
  ),
  mods: (
    <>
      <rect x="4" y="4" width="7" height="7" rx="1" />
      <rect x="13" y="4" width="7" height="7" rx="1" />
      <rect x="4" y="13" width="7" height="7" rx="1" />
      <rect x="13" y="13" width="7" height="7" rx="1" />
    </>
  ),
  versions: (
    <>
      <path d="M20 12l-8 8-9-9V4h7l10 8z" />
      <circle cx="7.5" cy="7.5" r="1.5" />
    </>
  ),
  downloads: (
    <>
      <path d="M12 3v12" />
      <path d="M6 11l6 6 6-6" />
      <path d="M4 21h16" />
    </>
  ),
  accounts: (
    <>
      <circle cx="12" cy="8" r="4" />
      <path d="M4 21c0-4 4-6 8-6s8 2 8 6" />
    </>
  ),
  settings: (
    <>
      <path d="M4 6h16" />
      <path d="M4 12h16" />
      <path d="M4 18h16" />
      <circle cx="9" cy="6" r="2" />
      <circle cx="15" cy="12" r="2" />
      <circle cx="7" cy="18" r="2" />
    </>
  ),
};

/**
 * A theme can replace any icon by dropping `assets/icons/<name>.svg|png`
 * into its folder, which becomes the `--fae-icon-<name>` token. When that
 * token is set we render it; otherwise the built-in line drawing is used,
 * so a theme can override one icon and inherit the rest.
 */
export function Icon(props: { name: string }) {
  const themed = () => {
    const value = getComputedStyle(document.documentElement)
      .getPropertyValue(`--fae-icon-${props.name}`)
      .trim();
    return value && value !== "none" ? value : null;
  };

  const custom = themed();
  if (custom) {
    return (
      <span
        class="icon icon-themed"
        aria-hidden="true"
        style={{ "background-image": custom }}
      />
    );
  }

  return (
    <svg
      class="icon"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      stroke-width="2"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      {PATHS[props.name]}
    </svg>
  );
}
