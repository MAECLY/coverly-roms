# Layouts, systems and art sources

[Coverly ROMs](../../README.md) › Layouts, systems and art sources · **English** · [Español](layouts-and-systems.es.md)

## Supported layouts

Eight firmware and frontend layouts, detected when you open the folder. You can
override the detection, and build a custom profile for any other firmware.

| ID               | Name                                 | ROMs folder | Art it writes                                                                                                 |
| ---------------- | ------------------------------------ | ----------- | ------------------------------------------------------------------------------------------------------------- |
| `anbernic-stock` | Anbernic stock                       | `ROMS`      | cover in `<SYS>/Imgs/<stem>.png`                                                                              |
| `garlicos`       | GarlicOS                             | `Roms`      | cover in `Imgs/`                                                                                              |
| `onion`          | Onion OS                             | `Roms`      | cover in `Imgs/`                                                                                              |
| `muos`           | muOS                                 | `ROMS`      | cover and screenshot in `MUOS/info/catalogue/<sys>/box` and `preview`                                         |
| `minui`          | MinUI / NextUI                       | `Roms`      | cover in `.media/`                                                                                            |
| `esde`           | ES-DE                                | `ROMs`      | cover, screenshot and title in `downloaded_media/<sys>/covers`, `screenshots` and `titlescreens`              |
| `batocera`       | Batocera / Knulli / EmulationStation | `roms`      | cover, screenshot and title as `images/<stem>-image`, `-screenshot` and `-titleshot.png`, plus `gamelist.xml` |
| `retroarch`      | RetroArch                            | `.`         | cover, screenshot and title in `thumbnails/<db>/Named_Boxarts`, `Named_Snaps` and `Named_Titles`              |

`coverly profiles` lists them, with your custom profiles.

## Supported systems

133 systems, each under the folder names the layouts use (`GBA`, `gba` and
`gameboyadvance` are the same system). Among them: Game Boy, Game Boy Color,
Game Boy Advance, NES/Famicom, Super Nintendo, Mega Drive, Master System, Game
Gear, PlayStation 1 to 5, PSP, PS Vita, Nintendo 64, GameCube, Wii, Wii U,
Nintendo DS, 3DS, Switch, Dreamcast, Saturn, Xbox, Xbox 360, Xbox One, Neo Geo,
CPS1–3, MAME, FinalBurn Neo, ZX Spectrum, Amstrad CPC, PICO-8, ScummVM and DOS.

### All 133 systems and their folder aliases (`coverly systems`)

| ID                | Name                          | Folder aliases                                           |
| ----------------- | ----------------------------- | -------------------------------------------------------- |
| `a2600`           | Atari 2600                    | `A2600`, `atari2600`, `2600`                             |
| `a5200`           | Atari 5200                    | `A5200`, `atari5200`, `5200`                             |
| `a7800`           | Atari 7800                    | `A7800`, `atari7800`, `7800`                             |
| `a800`            | Atari 8-bit                   | `A800`, `atari800`, `atari8bit`                          |
| `amiga`           | Commodore Amiga               | `AMIGA`                                                  |
| `atarist`         | Atari ST                      | `ATARIST`, `atari st`                                    |
| `atomiswave`      | Atomiswave                    | `ATOMISWAVE`, `awave`                                    |
| `c64`             | Commodore 64                  | `C64`, `commodore64`                                     |
| `cps1`            | Capcom Play System            | `CPS1`                                                   |
| `cps2`            | Capcom Play System II         | `CPS2`                                                   |
| `cps3`            | Capcom Play System III        | `CPS3`                                                   |
| `dos`             | DOS                           | `DOS`, `msdos`                                           |
| `dreamcast`       | Sega Dreamcast                | `DREAMCAST`, `dc`                                        |
| `easyrpg`         | EasyRPG                       | `EASYRPG`, `rpgmaker`                                    |
| `fbneo`           | FinalBurn Neo                 | `FBNEO`, `fba`, `fbalpha`, `finalburn`                   |
| `nes`             | Nintendo Entertainment System | `FC`, `NES`, `famicom`                                   |
| `fds`             | Famicom Disk System           | `FDS`, `famicomdisk`                                     |
| `gb`              | Game Boy                      | `GB`, `gameboy`                                          |
| `gba`             | Game Boy Advance              | `GBA`, `gameboyadvance`                                  |
| `gbc`             | Game Boy Color                | `GBC`, `gameboycolor`                                    |
| `gamegear`        | Sega Game Gear                | `GG`, `gamegear`                                         |
| `gw`              | Game & Watch                  | `GW`, `gameandwatch`                                     |
| `hbmame`          | HBMAME                        | `HBMAME`                                                 |
| `lynx`            | Atari Lynx                    | `LYNX`, `atarilynx`                                      |
| `mame`            | MAME                          | `MAME`, `arcade`, `mame2003`, `mame2003plus`, `mame2010` |
| `megadrive`       | Sega Mega Drive / Genesis     | `MD`, `megadrive`, `genesis`, `Mega Drive`               |
| `segacd`          | Sega CD / Mega-CD             | `MDCD`, `segacd`, `megacd`, `Mega CD`                    |
| `msx`             | MSX                           | `MSX`, `msx1`                                            |
| `n64`             | Nintendo 64                   | `N64`, `nintendo64`                                      |
| `naomi`           | Sega NAOMI                    | `NAOMI`                                                  |
| `nds`             | Nintendo DS                   | `NDS`, `nintendods`, `dsi`, `nintendodsi`                |
| `neocd`           | Neo Geo CD                    | `NEOCD`, `neogeocd`                                      |
| `neogeo`          | Neo Geo                       | `NEOGEO`, `Neo Geo`                                      |
| `ngp`             | Neo Geo Pocket                | `NGP`, `neogeopocket`                                    |
| `ngpc`            | Neo Geo Pocket Color          | `NGPC`, `neogeopocketcolor`                              |
| `ons`             | ONScripter                    | `ONS`, `onscripter`                                      |
| `openbor`         | OpenBOR                       | `OPENBOR`                                                |
| `pcengine`        | PC Engine / TurboGrafx-16     | `PCE`, `pcengine`, `tg16`, `turbografx16`, `PC Engine`   |
| `pcenginecd`      | PC Engine CD / TurboGrafx-CD  | `PCECD`, `pcenginecd`, `tgcd`, `turbografxcd`            |
| `pgm2`            | IGS PGM2                      | `PGM2`                                                   |
| `pico8`           | PICO-8                        | `PICO`, `pico8`, `pico-8`                                |
| `pokemini`        | Pokemon Mini                  | `POKE`, `pokemini`                                       |
| `psx`             | Sony PlayStation              | `PS`, `psx`, `ps1`, `playstation`                        |
| `psp`             | Sony PSP                      | `PSP`                                                    |
| `saturn`          | Sega Saturn                   | `SATURN`, `segasaturn`                                   |
| `scummvm`         | ScummVM                       | `SCUMMVM`, `scumm`                                       |
| `sega32x`         | Sega 32X                      | `SEGA32X`, `32x`, `sega32`                               |
| `snes`            | Super Nintendo                | `SFC`, `snes`, `Super Nintendo`, `supernintendo`         |
| `mastersystem`    | Sega Master System            | `SMS`, `mastersystem`, `Master System`                   |
| `varcade`         | Arcade (various)              | `VARCADE`                                                |
| `virtualboy`      | Virtual Boy                   | `VB`, `virtualboy`                                       |
| `vic20`           | Commodore VIC-20              | `VIC20`, `vic-20`                                        |
| `wonderswan`      | WonderSwan                    | `WS`, `wonderswan`                                       |
| `wonderswancolor` | WonderSwan Color              | `WSC`, `wonderswancolor`                                 |
| `ps2`             | Sony PlayStation 2            | `PS2`, `playstation2`                                    |
| `3do`             | 3DO Interactive Multiplayer   | `3DO`, `3d0`                                             |
| `pcfx`            | NEC PC-FX                     | `PCFX`, `pc-fx`                                          |
| `cdi`             | Philips CD-i                  | `CDI`, `cdimono1`, `cd-i`                                |
| `amigacd32`       | Commodore Amiga CD32          | `CD32`, `amigacd32`                                      |
| `cdtv`            | Commodore CDTV                | `CDTV`, `amigacdtv`                                      |
| `jaguarcd`        | Atari Jaguar CD               | `JAGUARCD`, `atarijaguarcd`                              |
| `fmtowns`         | Fujitsu FM Towns              | `FMTOWNS`, `fm-towns`                                    |
| `colecovision`    | ColecoVision                  | `COLECO`, `colecovision`                                 |
| `intellivision`   | Mattel Intellivision          | `INTELLIVISION`, `intv`                                  |
| `vectrex`         | GCE Vectrex                   | `VECTREX`, `gcevectrex`                                  |
| `odyssey2`        | Magnavox Odyssey² / Videopac  | `ODYSSEY`, `odyssey2`, `videopac`, `o2em`                |
| `channelf`        | Fairchild Channel F           | `FAIRCHILD`, `channelf`                                  |
| `arcadia`         | Emerson Arcadia 2001          | `ARCADIA`, `arcadia2001`                                 |
| `scv`             | Epoch Super Cassette Vision   | `SCV`, `supercassettevision`                             |
| `jaguar`          | Atari Jaguar                  | `JAGUAR`, `atarijaguar`                                  |
| `sg1000`          | Sega SG-1000 / SC-3000        | `SEGASGONE`, `sg1000`, `sg-1000`, `sc3000`               |
| `pico`            | Sega Pico                     | `SEGAPICO`                                               |
| `supergrafx`      | PC Engine SuperGrafx          | `SGFX`, `supergrafx`                                     |
| `n64dd`           | Nintendo 64DD                 | `N64DD`, `64dd`                                          |
| `satellaview`     | Nintendo Satellaview          | `SATELLAVIEW`, `bsx`, `bs-x`                             |
| `sufami`          | Bandai Sufami Turbo           | `SUFAMI`, `sufamiturbo`                                  |
| `supervision`     | Watara Supervision            | `SUPERVISION`                                            |
| `gamecom`         | Tiger Game.com                | `GAMECOM`                                                |
| `megaduck`        | Mega Duck / Cougar Boy        | `MEGADUCK`, `cougarboy`                                  |
| `zxspectrum`      | Sinclair ZX Spectrum          | `ZXS`, `zxspectrum`, `spectrum`                          |
| `zx81`            | Sinclair ZX81                 | `ZX81`                                                   |
| `amstradcpc`      | Amstrad CPC                   | `CPC`, `amstradcpc`                                      |
| `bbcmicro`        | Acorn BBC Micro               | `BBCMICRO`, `bbc`                                        |
| `thomson`         | Thomson MO/TO                 | `THOMSON`, `moto`, `to8`, `thomsonmoto`, `theodore`      |
| `dragon`          | Dragon 32/64                  | `DRAGON`, `dragon32`, `dragon64`                         |
| `coco`            | Tandy TRS-80 Color Computer   | `COCO`, `trs80coco`                                      |
| `ti99`            | Texas Instruments TI-99/4A    | `TI99`, `ti994a`                                         |
| `x68000`          | Sharp X68000                  | `X68000`, `x68k`                                         |
| `x1`              | Sharp X1                      | `X1`                                                     |
| `pc98`            | NEC PC-98                     | `PC98`, `pc9800`                                         |
| `pc88`            | NEC PC-88                     | `PC88`, `pc8800`, `pc8000`                               |
| `plus4`           | Commodore Plus/4              | `PLUS4`, `cplus4`, `commodoreplus4`                      |
| `pet`             | Commodore PET                 | `PET`, `commodorepet`                                    |
| `c128`            | Commodore 128                 | `C128`, `commodore128`                                   |
| `atarixe`         | Atari XE / XEGS               | `ATARIXE`, `xegs`                                        |
| `msx2`            | MSX2                          | `MSX2`                                                   |
| `msxturbor`       | MSX turbo R                   | `MSXTURBOR`, `turbor`                                    |
| `apple2`          | Apple II                      | `APPLE2`, `appleii`                                      |
| `tic80`           | TIC-80                        | `TIC`, `tic80`                                           |
| `wasm4`           | WASM-4                        | `WASM4`                                                  |
| `arduboy`         | Arduboy                       | `ARDUBOY`                                                |
| `uzebox`          | Uzebox                        | `UZEBOX`                                                 |
| `lowresnx`        | LowRes NX                     | `LOWRESNX`                                               |
| `vircon32`        | Vircon32                      | `VIRCON32`                                               |
| `doom`            | Doom (PrBoom)                 | `PRBOOM`, `doom`                                         |
| `quake`           | Quake                         | `QUAKE`, `tyrquake`                                      |
| `cavestory`       | Cave Story (NXEngine)         | `CAVESTORY`, `nxengine`                                  |
| `wolf3d`          | Wolfenstein 3D (ECWolf)       | `ECWOLF`, `wolf3d`, `wolfenstein3d`                      |
| `mrboom`          | Mr. Boom                      | `MRBOOM`                                                 |
| `dinothawr`       | Dinothawr                     | `DINOTHAWR`                                              |
| `lutro`           | Lutro                         | `LUTRO`                                                  |
| `solarus`         | Solarus                       | `SOLARUS`                                                |
| `flashback`       | Flashback (REminiscence)      | `REMINISCENCE`, `flashback`                              |
| `xrick`           | Rick Dangerous (XRick)        | `XRICK`, `rickdangerous`                                 |
| `model2`          | Sega Model 2                  | `MODEL2`                                                 |
| `model3`          | Sega Model 3                  | `MODEL3`                                                 |
| `naomi2`          | Sega NAOMI 2                  | `NAOMI2`                                                 |
| `triforce`        | Namco Sega Nintendo Triforce  | `TRIFORCE`                                               |
| `cave3rd`         | Cave CV1000                   | `CAVE3RD`                                                |
| `typex`           | Taito Type X                  | `TYPEX`, `type-x`                                        |
| `gamecube`        | Nintendo GameCube             | `GAMECUBE`, `gc`, `ngc`, `gcn`                           |
| `wii`             | Nintendo Wii                  | `WII`                                                    |
| `3ds`             | Nintendo 3DS                  | `3DS`, `n3ds`, `nintendo3ds`                             |
| `wiiu`            | Nintendo Wii U                | `WIIU`                                                   |
| `switch`          | Nintendo Switch               | `SWITCH`, `nintendoswitch`, `nsw`                        |
| `psvita`          | Sony PlayStation Vita         | `PSVITA`, `vita`                                         |
| `ps3`             | Sony PlayStation 3            | `PS3`, `playstation3`                                    |
| `xbox`            | Microsoft Xbox                | `XBOX`                                                   |
| `xbox360`         | Microsoft Xbox 360            | `XBOX360`, `x360`                                        |
| `ps4`             | Sony PlayStation 4            | `PS4`, `playstation4`                                    |
| `ps5`             | Sony PlayStation 5            | `PS5`, `playstation5`                                    |
| `xboxone`         | Microsoft Xbox One            | `XBOXONE`, `xb1`                                         |
| `xboxseries`      | Microsoft Xbox Series X\|S    | `XBOXSERIES`, `xsx`, `xboxseriesx`                       |

## Art sources

- **[libretro-thumbnails](https://github.com/libretro-thumbnails/libretro-thumbnails)**
  (`thumbnails.libretro.com`): the default. No account.
- **[SteamGridDB](https://www.steamgriddb.com/)**: optional, with your own API
  key. Its community votes feed the Popularity strategy. Set it in Settings →
  Integrations, where it is kept in the OS keychain; the CLI reads
  `COVERLY_SGDB_KEY` first, then the key the app saved.
- **ScreenScraper**: planned for a later version, not available as an art
  source yet. Today it is only used to identify files when organising, when it
  is configured.
- **Identification data** comes from
  [libretro-database](https://github.com/libretro/libretro-database),
  downloaded at runtime, not bundled.

More detail: [docs/04-architecture/art-sources.md](../04-architecture/art-sources.md).
