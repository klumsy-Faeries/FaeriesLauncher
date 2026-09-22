# Launcher website

A one-page static site for Faeries Launcher, in the launcher's own two
themes (the `themes/smp` and `themes/skyblock` tokens, the emblem, the
castle painting and the pixel sky) and linking to the SMP site at
faeriessmp.com. A switch in the header flips between SMP and Skyblock; the
choice is remembered in the visitor's browser.

## Files

| Path | What |
|---|---|
| `index.html` | The whole page: markup, styles, and the copy-address script |
| `assets/logo.png` | Emblem, 768 px (same file the launcher embeds) |
| `assets/mark.png` | Emblem, 256 px: header, footer, favicon |
| `assets/castle.jpg` | The castle painting, the page's background scene |
| `assets/launcher-home.jpg` | Screenshot of Home (status bar cropped off) |

Fonts load from Google Fonts (Fredoka, Nunito, JetBrains Mono) with system
fallbacks, so the page needs no build step.

## Hosting

Upload the `site` folder as-is to any static host: the web space behind
faeriessmp.com, GitHub Pages, Netlify, Cloudflare Pages. `index.html` must
sit next to `assets/`.

## Before publishing

- **Download link.** The two download buttons point at `#download` until
  there is a release. Set `href` on the anchor with `id="download-link"`
  (and the hero's `✦ Download ✦` button, if it should go straight to the
  file) to the installer's URL.
- **Version.** `v0.7.2` appears in the hero meta line, the download button,
  and the footer; update all three on a release.
- **Screenshot.** Regenerate `assets/launcher-home.jpg` from a fresh capture
  when Home changes; keep the status bar cropped so no local path shows.

## Theme

The colours are the launcher's `themes/smp/colors.json` and
`themes/skyblock/colors.json` verbatim, applied through CSS custom
properties at the top of `index.html`. SMP is the default;
`<html data-theme="skyblock">` selects the other, which is what the header
switch sets (and stores under `faeries-theme` in localStorage).

## Hosting

The page is published by `.github/workflows/pages.yml` to GitHub Pages at
https://klumsy-faeries.github.io/FaeriesLauncher/ on every push that touches
this folder. Pages must be enabled once on GitHub (Settings -> Pages ->
Source: GitHub Actions); the workflow cannot enable it by itself. The
download button links to the repository's latest release, which the
`release.yml` workflow publishes when a version tag is pushed.
