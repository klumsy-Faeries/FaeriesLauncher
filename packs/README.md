# Bundled resource packs

Zips in this folder are compiled into the launcher and enabled by the
**Optimized** preset in every instance it is installed into: the file is
copied to `<instance>/resourcepacks/` and put at the top of the pack stack in
`options.txt` (existing entries stay; the file is created if the game has
not written one yet).

| File | Pack | Notes |
|---|---|---|
| `faeries-smp.zip` | Faeries SMP pack | Nexo's generated server pack (27.5 MB; `pack_format` 64 with overlays through 26.2). SHA-1 `03ee86b58e95aff2deffe1c673aacd8591075f1b`, added 2026-09-09. Also seeded into the Pack Vault under that hash with the hermes URL, so a join to `mc.faeriessmp.com` announcing this build needs no download. When Nexo regenerates the pack, replace the file and rebuild. |

To add one:

1. Drop the pack here, e.g. `faeries.zip` (a normal resource pack: a zip
   with `pack.mcmeta` at its root).
2. Add an entry to `FAERIES_PACKS` in `crates/faerie-modding/src/presets.rs`
   pointing at the file name, with a one-sentence reason.
3. Rebuild. The bytes are embedded at compile time, so the launcher grows by
   the pack's size — fine for a few megabytes; a very large pack is better
   fetched from a URL at install time (not implemented yet).
