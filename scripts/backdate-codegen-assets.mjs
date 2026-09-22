// Stops the launcher from relinking a second time after an interface change.
//
// Cargo decides whether the launcher is up to date by comparing every file
// the last compile read against the moment that compile started. Tauri's
// code generator writes its embedded-asset cache
// (target/<profile>/build/faerie-launcher-*/out/tauri-codegen-assets/<sha256>.<ext>)
// *during* the compile, so after any change to ui/dist those files come out
// newer than the reference, and the next build relinks the launcher again
// (~1.5 min with LTO) for nothing — whoever runs cargo second pays it.
// The files are content-addressed and never change once written, so dating
// them back is safe and makes the next build the no-op it should be.
//
// Usage: node scripts/backdate-codegen-assets.mjs [release|debug]

import { existsSync, readdirSync, statSync, utimesSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const repo = join(dirname(fileURLToPath(import.meta.url)), "..");
const profile = process.argv[2] ?? "release";
const buildDir = join(repo, "target", profile, "build");
if (!existsSync(buildDir)) process.exit(0);

const settled = new Date("2000-01-01T00:00:00Z");
const contentAddressed = /^[0-9a-f]{64}(\.[a-z0-9]+)?$/;
let touched = 0;

function settle(path) {
  const info = statSync(path);
  if (!info.isFile() || info.mtime <= settled) return;
  utimesSync(path, settled, settled);
  touched += 1;
}

for (const entry of readdirSync(buildDir)) {
  if (!entry.startsWith("faerie-launcher-")) continue;
  const out = join(buildDir, entry, "out");
  if (!existsSync(out)) continue;
  for (const name of readdirSync(out)) {
    if (contentAddressed.test(name)) settle(join(out, name));
  }
  const assets = join(out, "tauri-codegen-assets");
  if (!existsSync(assets)) continue;
  for (const name of readdirSync(assets)) settle(join(assets, name));
}

if (touched > 0) {
  console.log(
    `Settled ${touched} generated asset file(s); the next build will not relink.`,
  );
}
