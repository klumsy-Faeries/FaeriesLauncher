// Builds the interface only when something it depends on changed since the
// last build. Used by run.cmd: an unconditional `vite build` rewrites
// ui/dist on every run, and because Tauri embeds that folder, the launcher
// then relinks (~1.5 min with LTO) even when nothing changed.
//
// Freshness = every input file is older than dist/index.html.

import { spawnSync } from "node:child_process";
import { readdirSync, statSync, existsSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const ui = join(dirname(fileURLToPath(import.meta.url)), "..");
const repo = join(ui, "..");
const stamp = join(ui, "dist", "index.html");

// Everything the bundle is built from. Theme assets and locales are imported
// by the mock and by the styles, so they count too.
const inputs = [
  join(ui, "src"),
  join(ui, "index.html"),
  join(ui, "package.json"),
  join(ui, "package-lock.json"),
  join(ui, "vite.config.ts"),
  join(ui, "tsconfig.json"),
  join(repo, "locales"),
  join(repo, "themes"),
  join(repo, "CHANGELOG.md"),
];

function newestMtime(path) {
  if (!existsSync(path)) return 0;
  const info = statSync(path);
  if (!info.isDirectory()) return info.mtimeMs;
  let newest = info.mtimeMs;
  for (const entry of readdirSync(path)) {
    newest = Math.max(newest, newestMtime(join(path, entry)));
  }
  return newest;
}

const built = existsSync(stamp) ? statSync(stamp).mtimeMs : 0;
const changed = inputs.filter((p) => newestMtime(p) > built);

if (built > 0 && changed.length === 0) {
  console.log("Interface is up to date; skipping build.");
  process.exit(0);
}

console.log(
  built === 0
    ? "Building the interface (first build)..."
    : `Building the interface (changed: ${changed.map((p) => p.slice(repo.length + 1)).join(", ")})...`,
);
const npm = process.platform === "win32" ? "npm.cmd" : "npm";
const result = spawnSync(npm, ["run", "build"], { cwd: ui, stdio: "inherit", shell: true });
process.exit(result.status ?? 1);
