# Bundled resource packs

Zips in this folder are compiled into the launcher and enabled by the
**Optimized** preset in every instance it is installed into: the file is
copied to `<instance>/resourcepacks/` and put at the top of the pack stack in
`options.txt` (existing entries stay; the file is created if the game has
not written one yet).

| File | Pack | Notes |
|---|---|---|
| `faeries-smp.zip` | Faeries SMP pack | Nexo's generated server pack (27.5 MB; `pack_format` 64 with overlays through 26.2). SHA-1 `03ee86b58e95aff2deffe1c673aacd8591075f1b`, added 2026-09-09. Also seeded into the Pack Vault under that hash with the hermes URL, so a join to `mc.faeriessmp.com` announcing this build needs no download. When Nexo regenerates the pack, replace the file and rebuild. |
| `fairy-castle-gui.zip` | Fairy Castle GUI | Pastel HUD and menu textures (221 KB, 196 files; formats 84 to 99, so 26.x). SHA-1 `0e906c3d2424bff67947aa7f9ccc01845802cd4d`, added 2026-09-24 from the author's own copy. Listed last, so it sits on top of the server pack. |

Packs are enabled in the order listed in `FAERIES_PACKS`; each goes on top
of the previous one, so the last entry wins where two packs touch the same
texture.

To add one:

1. Drop the pack here, e.g. `faeries.zip` (a normal resource pack: a zip
   with `pack.mcmeta` at its root).
2. Add an entry to `FAERIES_PACKS` in `crates/faerie-modding/src/presets.rs`
   pointing at the file name, with a one-sentence reason.
3. Rebuild. The bytes are embedded at compile time, so the launcher grows by
   the pack's size — fine for a few megabytes; a very large pack is better
   fetched from a URL at install time (not implemented yet).
