# Theming

Everything visual in the launcher flows through design tokens. A theme is a
folder of five JSON files; no code, no rebuild.

## Create a theme

1. Open your data directory (shown in Settings → About), then `themes/`.
2. Create a folder — its name is the theme name, e.g. `themes/midnight/`.
3. Add any of the files below. **You only need the values you want to
   change** — everything else keeps the default (SMP) value.
4. Select the theme in Settings → Appearance. It appears automatically.

## Live editing

The launcher watches your themes folder. Save a change to any theme file or
image and the running window updates — no restart, no reload button. Rapid
saves are coalesced, so an editor that writes several times per save still
produces one update.

## Artwork

A theme can supply any of these under `assets/`, in `.svg`, `.png`,
`.webp`, `.jpg`, or `.gif` (SVG wins when several formats exist):

| File | Token | Used for |
|---|---|---|
| `background.*` | `--fae-background-image` | the full-window scene |
| `logo.*` | `--fae-asset-logo` | the wordmark |
| `mark.*` | `--fae-asset-mark` | the small sidebar badge |
| `icons/<name>.*` | `--fae-icon-<name>` | one navigation icon |

Two themes are built in, one per Faeries world:

- **`smp`** (the default): a JPEG copy of the castle painting at
  `art/background-5296.png`, pink frosted glass.
- **`skyblock`**: a pixel-art sky with clouds, floating islands, the
  rainbow strip and grass, after the Skyblock GUI's pink slot frames;
  squarer corners and 2px borders to match. Painted by a script, so it
  stays flat-colour and small (`themes/skyblock/assets/background.png`).

Both use the emblem as logo and mark: sized copies (768px logo, 256px mark)
of `art/logo-2500.png` in `themes/smp/assets/`. A user theme's `logo.png`
and `mark.png` override them. Keep supplied artwork roughly square with a
transparent background — the hero box is height-driven and the mark sits on
the brand card with no tile behind it.

Icon names match the sidebar items: `home`, `instances`, `mods`, `versions`,
`downloads`, `accounts`, `settings`. Supply only the ones you want to
change; the rest keep their built-in line drawings.

```
themes/mytheme/
├── colors.json
└── assets/
    ├── background.png
    ├── logo.png
    └── icons/
        └── mods.svg
```

Assets are embedded when the theme loads, so a theme is still one folder you
can zip and send to someone. Individual files are capped at 8 MB.

## Layout

Sidebar items and home dashboard cards can be shown, hidden, and reordered
from **Settings → Layout**. Those changes are saved as *your* overrides in
`config/ui.json`, merged over whatever the theme declares — so they survive
switching themes, and selecting a built-in theme never rewrites it. "Reset
to theme" clears them.

A theme still ships its own `layout.json` as the starting point.

## Background artwork

A theme can paint a full-window scene behind the whole launcher. Drop an
image at `assets/background.<ext>` inside your theme folder:

```
themes/mytheme/
├── colors.json
└── assets/
    └── background.png      (or .svg, .jpg, .jpeg, .webp)
```

The launcher embeds it as the `--fae-background-image` token, so the picture
travels with the theme folder — sharing a theme means sharing one folder,
with no separate asset install step.

Two things make artwork readable underneath the interface:

- `colors.json: scrim` — a translucent wash laid over the scene. Raise its
  alpha if your art is busy or high-contrast.
- `colors.json: surface` / `surfaceAlt` — cards are drawn with these, so an
  alpha around `0.9` lets a hint of the scene through while keeping text
  crisp. Combine with `theme.json: blur.card` for the frosted-glass look.

A user theme with no `assets/background.*` keeps the SMP castle; to show a
plain colour instead, supply a tiny transparent PNG as the background.

The built-in scenes live at
[`themes/smp/assets/background.jpg`](../themes/smp/assets/background.jpg)
and
[`themes/skyblock/assets/background.png`](../themes/skyblock/assets/background.png).
Replace either with your own painting by dropping an image in a user theme
of the same name.

## Files and their token prefixes

| File | Prefix | Examples |
|---|---|---|
| `theme.json` | *(root)* | `radius.small` → `--fae-radius-small`, `shadow.card`, `shadow.glow`, `blur.card`, `motion.fast`, `border.width` |
| `colors.json` | `color` | `background` → `--fae-color-background`, `primaryHover` → `--fae-color-primary-hover` |
| `typography.json` | `font` | `family.base` → `--fae-font-family-base`, `size.title`, `weight.bold` |
| `spacing.json` | `space` | `md` → `--fae-space-md` |
| `assets/background.*` | — | the full-window scene (see above) |
| `layout.json` | *(not tokens)* | structural config: sidebar enabled/width/item order, status bar, home page sections |

Nested keys join with `-`; camelCase becomes kebab-case. Values are plain CSS
values (`"#ec4899"`, `"14px"`, `"200ms"`, `"0 4px 20px rgba(0,0,0,.2)"`).

## Example: change only the accent color and sidebar width

`themes/mytheme/colors.json`

```json
{ "primary": "#7c3aed", "primaryHover": "#6d28d9", "sidebarActive": "#7c3aed" }
```

`themes/mytheme/layout.json`

```json
{ "sidebar": { "enabled": true, "width": "280px",
    "items": ["home", "instances", "settings"] } }
```

## Behavior

- A user theme with the same name as a built-in (`smp`, `skyblock`) overrides it.
- A broken file never breaks the UI: bad files are skipped with a warning
  toast, and every token always has a value from the SMP base.
- `theme.json`'s `meta` block (`name`, `author`, `dark`) is informational.
- The retired names `faerie` (the SMP look's first name) and `dark` still
  resolve, to SMP, so an older settings file keeps working.

The full token list is the union of the files in `themes/smp/` in the
repository — that folder is the reference implementation.
