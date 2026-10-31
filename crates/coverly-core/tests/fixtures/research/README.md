# Research listings for systems batch 1

Committed evidence for the names in `src/systems/systems.toml` (plan P-5,
spec §16). `tests/systems_batch1.rs` checks every libretro-thumbnails
folder and every libretro-database DAT path against these files, so a
typo fails offline. Fetched on 2026-10-31 from public endpoints, with no
credentials.

| File                              | Source                                                                                                                                                                      | Pinned at                                  |
| --------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------ |
| `thumbnails-root.html`            | Live root listing of <https://thumbnails.libretro.com/> (`curl -fsS https://thumbnails.libretro.com/`); 127 playlist folders                                                | 2026-10-31                                 |
| `libretro-database-dats.txt`      | Every `.dat` path of <https://github.com/libretro/libretro-database> (`gh api repos/libretro/libretro-database/git/trees/<commit>?recursive=1`, tree not truncated), sorted | `2c93952f3cdf02837d2b34112aea5ee844d42b8b` |
| `retroarch-assets-monochrome.txt` | Every path under `src/xmb/monochrome/` of <https://github.com/libretro/retroarch-assets> (same call, tree not truncated), sorted                                            | `d9f969054dc7fbb6fa89519036d2b971e0855b51` |

To refresh, run the commands of plan Task 3 Step 1 again and review the
diff: a folder that disappears from the thumbnail server must also leave
`systems.toml`.
