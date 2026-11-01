# Mods, translation patches, cheats, licences and extras per system

Research for TODO 2.1 (2026-11-01). Four researchers plus one adversarial verifier per group, who opened every cited source (libretro docs and database, Mesen, FCEUX, Snes9x/bsnes, Dolphin, PCSX2, DuckStation, PPSSPP, Azahar, Atmosphère and Switch emulators, Cemu, RPCS3, Vita3K, shadPS4, Xenia and others) and corrected or dropped what did not hold. Coverly only reads plaintext metadata and never needs keys.

## Retro cartridge family (shared rules for NES, SNES, Genesis, GB/GBC, GBA, N64, PCE, SMS/GG, NDS)

**Mods and texture packs.** Beyond ROM hacks there are few mod systems: SNES MSU-1 audio packs, N64 hi-res texture packs, NES HD Packs (Mesen), Genesis MSU-MD, and GB palettes and SGB borders. ROM hacks come as patches and belong under Mods, not Cheats.

**Translation patches and ROM hacks.** Soft-patch formats. IPS: magic 'PATCH', ends with 'EOF', uses 24-bit offsets so it cannot reach past 16 MB, and has no checksum or source-ROM ID, so it can only be matched by readme or filename. UPS: magic 'UPS1', varint source and target sizes, footer with CRC32 of source, target and patch. BPS: magic 'BPS1', varint source/target/metadata sizes plus an optional metadata block (often XML or text with the title), and a 12-byte footer holding source CRC32, target CRC32 and patch CRC32. BPS is the best format for auto-linking: read the last 12 bytes and compare the source CRC32 with No-Intro DAT CRCs. xdelta3/VCDIFF: magic D6 C3 C4 00; the xdelta3 app header often stores source and target filenames; it has optional per-window Adler32 checksums but no full-ROM CRC. APS: 'APS1' for GBA and 'APS10' for N64; the N64 variant stores the cart ID, country and header CRC. PPF is mostly for discs. RetroArch (verified in the libretro softpatching doc): supports UPS, IPS, BPS, and xdelta only when enabled at compile time. A patch named like the ROM (rom.ups/rom.ips/rom.bps) is applied automatically when the ROM loads. IPS patches can be chained as rom.ips, rom.ips1, rom.ips2 and are applied in that order. This only works when the core loads the ROM from memory. Cores marked as unsupported: SameBoy (GB/GBC), Genesis Plus GX and PicoDrive for Master System, PicoDrive for Mega Drive, and RACE (NGP). Hacks and translations usually come as a .zip with the patch and a readme.txt naming the target ROM (No-Intro name plus CRC32/SHA-1, and whether it must be headered or headerless). Romhacking.net stopped taking submissions in 2024 and is now archived and read-only; together with No-Intro DATs it is the reference. Linking heuristic: BPS/UPS source CRC32 matched against the No-Intro DAT, then patch basename == ROM basename, then readme parsing, then a fuzzy title match.

**Cheats and emulator patches.** RetroArch .cht files (libretro-database/cht/<No-Intro system name>/<No-Intro game name>.cht). Verified format: a 'cheats = N' line, then for each cheat cheatN_desc, cheatN_code (several codes joined with '+', e.g. "0754:00+0756:01") and cheatN_enable. Cheats are either emulator-handled (passed to the core) or RetroArch-handled (RetroArch scans and writes memory itself). Found by matching the game name in the cheats/ directory. Device code types: Game Genie (NES/SNES/Genesis/GB/GG), Pro Action Replay (SNES/Genesis/SMS/GG), GameShark (GB/GBC/GBA/N64), Action Replay/CodeBreaker (GBA/NDS). Standalone emulator files are listed per system. Link a cheat file to a game by filename stem == ROM stem or No-Intro name, or by internal keys (N64 CRC1-CRC2-C:country, NDS gamecode + CRC32, GBA gamecode).

**Official extras.** None. Cartridge systems never had system-package themes, avatars or digital soundtracks, so every extra is user-supplied: manuals as PDF/CBZ, soundtrack rips as VGM/VGZ/SPC/NSF/NSFe/GBS/GSF/USF/2SF/HES/MP3/FLAC, and box art. Chiptune rips can still be linked automatically, because many store the game title as plain text (SPC ID666 tag, NSF/NSFe header, GBS header, PSF-family tags for GSF/USF/2SF, VGM GD3 tag). For this family Extras should be free-form, with tag-based suggestions.

**Licences.** none

**Not games (firmware, caches, configs).** BIOS and firmware, not games: disksys.rom (FDS), gba_bios.bin, gb_bios.bin/gbc_bios.bin (Gambatte), dmg_boot.bin/cgb_boot.bin/sgb_boot.bin/sgb2_boot.bin (SameBoy), bios7.bin/bios9.bin/firmware.bin (DS), DSi BIOS/NAND, syscard1/2/3.pce, gexpress.pce, bios_CD_U/E/J.bin, bios_MD.bin, bios_E/U/J.sms, bios.gg, 64DD IPL. Saves and states: .srm, .sav, .eep, .fla, .mpk, .sra, .dsv, .state*, .ss*. RetroArch per-game config: .opt core options, .cfg overrides, .rmp remaps, .slangp/.glslp shader presets, .lpl playlists. Caches: GLideN64 .htc/.hts in cache/ (these are texture-pack caches, so treat them as mods, not noise), shader caches. Patch files (.ips/.bps) next to a ROM are mods, and .cht files are cheats.

**Confidence.** high for patch formats and the RetroArch softpatch and .cht rules (checked against the docs and a database file); medium for standalone emulator paths

**Sources:**
- <https://docs.libretro.com/guides/softpatching/>
- <https://docs.libretro.com/guides/cheat-codes/>
- <https://github.com/libretro/libretro-database/tree/master/cht>
- <https://www.romhacking.net/>
- <https://datomatic.no-intro.org/>

## NES / Famicom / FDS

**Mods and texture packs.** Mesen/Mesen2 HD Packs (verified): extract into a subfolder of HdPacks/ in Mesen's data folder, named the same as the ROM file. Zipped packs from the HD Pack Builder can go directly into HdPacks/. The definition file is hires.txt with <ver> (currently 105), <scale> and <tile> tags, plus PNG images. Packs can also replace music and sound: <bgm>/<sfx> tags point to .ogg files, controlled through registers $4100-$4106 in an MSU-1-like way. Detect a pack by a hires.txt containing <ver>/<tile>. ROM hacks come as patches.

**Translation patches and ROM hacks.** Mostly IPS, with BPS increasingly common. The 16-byte iNES/NES 2.0 header matters: No-Intro hashes the ROM without the header, but most patches expect the headered file, and the readme says 'headered'. FDS patches target .fds files, with or without the fwNES header. Source: Romhacking.net and No-Intro naming.

**Cheats and emulator patches.** Game Genie (6 or 8 letters, e.g. SXIOPO) and raw address:value codes. RetroArch .cht files live under 'Nintendo - Nintendo Entertainment System' (format verified, e.g. cheat0_code = "0754:00+0756:01"). FCEUX: plain-text .cht files in its cheats/ subfolder, named exactly like the ROM (Contra.nes -> Contra.cht), with one memory patch per line in the form prefix:address:value[:compare]:name. Mesen/Mesen2 has a built-in cheat database.

**Official extras.** none (user extras only: NSF/NSFe music rips, whose header holds the game name in plain text; manual scans)

**Licences.** none

**Not games (firmware, caches, configs).** disksys.rom (FDS BIOS), .sav/.srm, FCEUX .fc0-.fc9 states, Mesen save states, NstDatabase.xml, palette files (.pal)

**Confidence.** high for HD packs and .cht; medium for the exact FCEUX line prefix

**Sources:**
- <https://www.mesen.ca/docs/hdpacks.html>
- <https://sources.debian.org/src/fceux/2.6.5%2Bdfsg1-2/documentation/cheat.html>
- <https://raw.githubusercontent.com/libretro/libretro-database/master/cht/Nintendo%20-%20Nintendo%20Entertainment%20System/Super%20Mario%20Bros.%20(World).cht>
- <https://docs.libretro.com/guides/softpatching/>

## SNES / Super Famicom

**Mods and texture packs.** MSU-1 packs: <name>.msu (data file, may be 0 bytes), <name>-N.pcm tracks (16-bit stereo 44.1 kHz with an 'MSU1' header and a 4-byte loop point), and usually a patched ROM <name>.sfc. All files share the ROM basename and sit in one folder; this works in Snes9x, bsnes and their RetroArch cores, and on FXPak/SD2SNES. higan needs a manifest, bsnes does not. Detect by .pcm files with the 'MSU1' magic next to a .msu file. Snes9x (since 1.55) also loads the '.msu1' pack format: a zip renamed to .msu1 holding program.rom, the .msu file and .pcm or .ogg tracks, loaded next to a same-named ROM. Mesen2 PR #58, which would add .ogg and .msu1 support, was never merged and the repository is archived, so do not assume Mesen2 supports them. bsnes-hd's HD Mode 7 is a setting only, with no files.

**Translation patches and ROM hacks.** IPS for older patches and BPS as the modern standard (byuu designed BPS; its source CRC32 allows auto-linking). The 512-byte copier header (.smc vs .sfc) matters: No-Intro is headerless, and the readme says whether a header is needed. Large translations use BPS or xdelta. Satellaview (.bs) patches exist.

**Cheats and emulator patches.** Game Genie (XXXX-XXXX), Pro Action Replay (7E1234XX), Gold Finger. RetroArch .cht files under 'Nintendo - Super Nintendo Entertainment System'. Snes9x and bsnes standalone each store per-game cheat files in a Cheats folder (text/BML in recent versions); the exact formats depend on the version and were not verified.

**Official extras.** none (user: SPC rips with ID666 tags holding the game title; .rsn = RAR archive of SPCs)

**Licences.** none

**Not games (firmware, caches, configs).** .srm saves, .bml manifests (but a .bml next to .msu/.pcm files belongs to an MSU-1 pack), BS-X.bin (Satellaview BIOS), SGB BIOS, coprocessor firmware used by bsnes (dsp1.rom, cx4.rom, st010.rom and similar), Snes9x .000-.009/.frz states

**Confidence.** high for patches and MSU-1 naming; medium for standalone cheat file formats

**Sources:**
- <https://docs.libretro.com/guides/softpatching/>
- <https://docs.libretro.com/library/snes9x/>
- <https://www.zeldix.net/t1607-msu1-getting-started-guide>
- <https://github.com/SourMesen/Mesen2/pull/58>
- <https://forums.launchbox-app.com/topic/46563-zipped-msu1-pack-for-snes9x-tutorial>

## Sega Genesis / Mega Drive (incl. 32X, Mega-CD noted)

**Mods and texture packs.** MSU-MD: ROM hacks that use Mega-CD audio for CD-quality music. Shipped as a patched ROM plus a .cue and audio tracks, and loaded as a Mega-CD disc (Genesis Plus GX, MiSTer). Detect by a .cue whose first track is the patched ROM. No texture packs.

**Translation patches and ROM hacks.** IPS and BPS; some hacks ship as complete ROMs. The internal header at 0x100 holds 'SEGA MEGA DRIVE'/'SEGA GENESIS', the domestic and overseas titles, a serial, and a checksum at 0x18E. A patch can invalidate that checksum. Correction (verified in the libretro doc): Genesis Plus GX DOES soft-patch Mega Drive ROMs; PicoDrive does not.

**Cheats and emulator patches.** Game Genie (XXXX-XXXX) and Pro Action Replay (address:value). RetroArch .cht files under 'Sega - Mega Drive - Genesis'. Genesis Plus GX supports RetroArch cheats but has no native cheat files. Standalone Kega Fusion and BlastEm formats were not verified and were dropped.

**Official extras.** none (user: VGM/VGZ rips with GD3 tags)

**Licences.** none

**Not games (firmware, caches, configs).** Mega-CD BIOS bios_CD_U.bin/bios_CD_E.bin/bios_CD_J.bin; TMSS boot ROM bios_MD.bin (Genesis Plus GX name, corrected from bios.md); 32X BIOS 32X_G_BIOS.BIN/32X_M_BIOS.BIN/32X_S_BIOS.BIN (PicoDrive); .srm saves; save states

**Confidence.** medium-high

**Sources:**
- <https://docs.libretro.com/guides/softpatching/>
- <https://docs.libretro.com/library/genesis_plus_gx/>
- <https://github.com/libretro/libretro-database/tree/master/cht>

## Game Boy / Game Boy Color

**Mods and texture packs.** Gambatte custom palettes (verified): set GB Colorization to custom and put .pal files in system/palettes/. Per-game files are named INTERNALROMNAME.pal or <rom-name>.pal, and default.pal applies to every game. Also Super Game Boy borders (cores with SGB support), and colorization (DX) or other hacks as IPS/BPS.

**Translation patches and ROM hacks.** IPS and BPS, sometimes UPS. The header at 0x134 holds the title, the CGB flag (0x143), the header checksum (0x14D) and the global checksum (0x14E-0x14F). SameBoy (libretro) does not soft-patch (verified); Gambatte and mGBA do.

**Cheats and emulator patches.** GameShark (01VVAAAA, RAM writes) and Game Genie (XXX-XXX-XXX, ROM patches). RetroArch .cht files under 'Nintendo - Game Boy' / 'Nintendo - Game Boy Color'. mGBA standalone saves a <rom>.cheats file in its cheats directory, which defaults to next to the ROM (verified in source: core.c/cheats.c use the '.cheats' suffix).

**Official extras.** none (user: GBS rips, whose header holds the title)

**Licences.** none

**Not games (firmware, caches, configs).** gb_bios.bin/gbc_bios.bin (Gambatte), dmg_boot.bin/cgb_boot.bin/sgb_boot.bin/sgb2_boot.bin (SameBoy), .sav/.srm/.rtc, .pal palette files

**Confidence.** high

**Sources:**
- <https://docs.libretro.com/guides/softpatching/>
- <https://docs.libretro.com/library/gambatte/>
- <https://github.com/mgba-emu/mgba/blob/master/src/core/cheats.c>

## Game Boy Advance

**Mods and texture packs.** ROM hacks as patches. No texture or audio packs. e-Reader card dumps (.raw/.bin) load in mGBA as a niche extra.

**Translation patches and ROM hacks.** IPS (cannot reach past 16 MB, so 32 MB ROMs need UPS or BPS), UPS (popular on GBA, e.g. the Mother 3 fan translation), BPS, xdelta. The header at 0xA0 has a 12-character title, then the 4-character game code at 0xAC (e.g. A2NE) and the maker code at 0xB0, all plain text and useful for linking. APS (magic 'APS1') is rare. Pokemon hacks are usually BPS/UPS/IPS against a specific No-Intro ROM such as 'Pokemon - FireRed Version (USA)', with the CRC given in the readme.

**Cheats and emulator patches.** GameShark/Action Replay v1-v2, Action Replay v3, CodeBreaker. mGBA: built-in cheat manager that saves <rom>.cheats in its cheats directory (next to the ROM by default; verified in source) and can import other formats. RetroArch .cht files under 'Nintendo - Game Boy Advance' (mGBA, gpSP, VBA-M cores). VBA-M standalone uses .clt/.cht (not verified).

**Official extras.** none (user: GSF/minigsf rips with PSF tags)

**Licences.** none

**Not games (firmware, caches, configs).** gba_bios.bin, .sav/.srm saves, mGBA .ss0-.ss9 states, .cheats files (cheats, not games)

**Confidence.** high

**Sources:**
- <https://github.com/mgba-emu/mgba/blob/master/src/core/core.c>
- <https://docs.libretro.com/guides/softpatching/>
- <https://www.romhacking.net/>

## Nintendo 64

**Mods and texture packs.** Hi-res texture packs. (1) Rice format: an uncompressed folder named after the ROM's internal name (case-sensitive), with PNGs named <NAME>#<CRC>#<fmt>#<size>_all.png (or _rgb/_a). (2) GLideN64 compiled caches: <INTERNAL NAME>_HIRESTEXTURES.htc, or .hts when enhanced storage is used. RetroArch Mupen64Plus-Next (verified): ready-made .htc packs go in system/Mupen64plus/cache/, and Rice folders go in system/Mupen64plus/hires_texture/<internal name>/; the cache is built on first launch. Standalone Mupen64Plus uses ~/.local/share/mupen64plus/hires_texture/. Project64 with GLideN64 uses a hires_texture folder under its plugin folder (not verified). ParaLLEl-RDP and Angrylion do not support texture packs. Detect packs by PNGs with '#' CRC naming, or .htc/.hts files whose basename is an N64 internal name. 64DD disk images (.ndd) are games, not mods.

**Translation patches and ROM hacks.** IPS (most common, usually against big-endian .z64), BPS, xdelta (large hacks), and APS10 (N64-specific; stores the cart ID, country and CRC). Byte order matters: .z64 is big-endian (magic 80 37 12 40), .v64 is byte-swapped (37 80 40 12), .n64 is little-endian (40 12 37 80). Patches usually need .z64, and the readme says so. Linking keys in the header: CRC1/CRC2 at 0x10/0x14, internal name at 0x20 (20 bytes), game ID at 0x3B-0x3E (media type, 2-char ID, region; e.g. NSME). Mupen64Plus-Next supports soft-patching (verified).

**Cheats and emulator patches.** GameShark/Action Replay Pro (80XXXXXX YYYY codes, D0 conditionals). Project64 3.x (verified in repo): one file per game at Config/Cheats/<Game Name>.cht, with a [CRC1-CRC2-C:XX] section header, a Name= line, then entries of '$Cheat name' followed by code lines. Enhancements (.enh) are kept separately in Config/Enhancements/. Mupen64Plus (verified): a single mupencheat.txt with 'crc CRC1-CRC2-C:XX', 'gn <game name>', ' cn <cheat name>' and indented code lines; RMG and simple64 inherit this format. RetroArch .cht files under 'Nintendo - Nintendo 64'; Mupen64Plus-Next supports RetroArch cheats but has no native cheat files.

**Official extras.** none (user: USF/miniusf rips with PSF tags; manual scans)

**Licences.** none

**Not games (firmware, caches, configs).** 64DD IPL (IPL.n64 in system/Mupen64plus/ for Mupen64Plus-Next; not verified this run), PIF ROMs (pif.ntsc.rom/pif.pal.rom, used by some emulators), saves .eep/.sra/.fla/.mpk/.srm, mupen64plus.cfg, Project64 Config/Project64.rdb/.rdx and Audio.rdb/Video.rdb (databases), Config/Enhancements/*.enh, GLideN64 shader caches, texture_dump/ folders

**Confidence.** high for cheat formats and RetroArch texture paths; medium for Rice PNG naming and standalone paths

**Sources:**
- <https://docs.libretro.com/library/mupen64plus/>
- <https://github.com/project64/project64/tree/develop/Config/Cheats>
- <https://raw.githubusercontent.com/mupen64plus/mupen64plus-core/master/data/mupencheat.txt>
- <https://github.com/libretro/libretro-database/tree/master/cht>
- <https://docs.libretro.com/guides/softpatching/>

## PC Engine / TurboGrafx-16 (HuCard; CD noted)

**Mods and texture packs.** none notable (CD audio track replacement for CD games only)

**Translation patches and ROM hacks.** IPS for HuCards. Some .pce dumps carry a 512-byte header, while No-Intro is headerless. Most PCE translations are for CD games and ship as xdelta/PPF patches, replacement tracks or rebuild scripts. HuCards have no internal title header, so linking relies on CRC (No-Intro) or the filename.

**Cheats and emulator patches.** No widespread commercial cheat device. RetroArch .cht files under 'NEC - PC Engine - TurboGrafx 16' (raw memory codes). Mednafen stores its own cheats in cheats/pce.cht.

**Official extras.** none (user: HES rips; manuals)

**Licences.** none

**Not games (firmware, caches, configs).** syscard1.pce/syscard2.pce/syscard3.pce, gexpress.pce, .sav/.srm, Mednafen .mc0-.mc9 states

**Confidence.** medium

**Sources:**
- <https://github.com/libretro/libretro-database/tree/master/cht>
- <https://docs.libretro.com/library/beetle_pce_fast/>

## Sega Master System / Game Gear (incl. SG-1000)

**Mods and texture packs.** none notable (FM-sound hacks are patches)

**Translation patches and ROM hacks.** Mostly IPS, some BPS; Mark III and Game Gear translations target No-Intro names. The 'TMR SEGA' header at 0x7FF0 holds a product code, version, region and checksum, but no title. Verified: Genesis Plus GX and PicoDrive do NOT soft-patch Master System content in RetroArch, so apply the patch beforehand or use another core (soft-patch support in other cores was not verified).

**Cheats and emulator patches.** Pro Action Replay (00XXXX:YY) and Game Genie for Game Gear (XXX-XXX-XXX). RetroArch .cht files under 'Sega - Master System - Mark III' / 'Sega - Game Gear'.

**Official extras.** none (user: VGM rips with GD3 tags)

**Licences.** none

**Not games (firmware, caches, configs).** bios_E.sms/bios_U.sms/bios_J.sms, bios.gg (Genesis Plus GX names), .srm saves, save states

**Confidence.** medium-high

**Sources:**
- <https://docs.libretro.com/guides/softpatching/>
- <https://docs.libretro.com/library/genesis_plus_gx/>
- <https://github.com/libretro/libretro-database/tree/master/cht>

## Nintendo DS

**Mods and texture packs.** ROM hacks and translations as patches. melonDS and DeSmuME have no standard texture or audio pack system.

**Translation patches and ROM hacks.** Mostly xdelta3, plus BPS; IPS is rare because it cannot reach past 16 MB. Sometimes a patcher .exe instead. The plain-text header has the 12-character title at 0x00, the 4-character game code at 0x0C (e.g. AMCE), the maker code at 0x10, and the header CRC16 at 0x15E. Readmes give the No-Intro name/CRC32 of the clean ROM; some patches need a trimmed or untrimmed dump. Main sources are Romhacking.net and GBAtemp. Pre-patching is the safe default for melonDS/DeSmuME cores.

**Cheats and emulator patches.** Action Replay DS ('XXXXXXXX YYYYYYYY') and CodeBreaker DS. usrcheat.dat is a binary database keyed by game code and header CRC32, used by R4, TWiLight Menu++ and nds-bootstrap (sd:/_nds/TWiLightMenu/extras/usrcheat.dat). melonDS (verified in source, EmuInstance.cpp): a per-game .mch text file found through the CheatFilePath setting, which defaults to next to the ROM. DeSmuME: per-game .dct files in its cheats folder, and it can import usrcheat.dat (not verified). RetroArch .cht files under 'Nintendo - Nintendo DS'. Link by game code + CRC32 from the header.

**Official extras.** none (Download Play demos are games; user extras: 2SF/mini2SF rips with PSF tags; manuals)

**Licences.** none for DS cartridges (DSiWare tickets/TMD belong to DSi and are out of scope)

**Not games (firmware, caches, configs).** bios7.bin (16 KB), bios9.bin (4 KB), firmware.bin (128/256/512 KB) per the melonDS FAQ; DSi BIOS, firmware and NAND images; melonDS.ini/melonDS.toml; .dsv/.sav saves; melonDS .ml1-.ml8 states; usrcheat.dat and .mch/.dct (cheats, not games); TWiLight/nds-bootstrap files in the _nds/ folder

**Confidence.** medium-high (melonDS .mch and BIOS sizes verified; DeSmuME .dct not verified)

**Sources:**
- <https://melonds.kuribo64.net/faq.php>
- <https://github.com/melonDS-emu/melonDS/blob/master/src/frontend/qt_sdl/EmuInstance.cpp>
- <https://wiki.ds-homebrew.com/twilightmenu/>
- <https://github.com/libretro/libretro-database/tree/master/cht>

## PlayStation (PS1)

**Mods and texture packs.** DuckStation texture replacement. Packs go in <DataDir>/textures/<SERIAL>/replacements/ and dumps in textures/<SERIAL>/dumps/. A config.yaml is created in the game's textures/<SERIAL>/ folder when dumping is first enabled. The folder is named by the disc serial (e.g. SLUS-00594), which links a pack to its game. Replacement file names follow texupload-P4-<texhash16>-<palhash16>-<WxH>-<x>-<y>-<WxH>-P<range>; direct C16 textures have no palette fields. Formats: .png, .jpg, .webp. Beetle PSX HW and SwanStation have no comparable pack system. Other mods ship as patches or rebuilt disc images.

**Translation patches and ROM hacks.** Mostly xdelta3 (VCDIFF, magic D6 C3 C4) against a .bin track, plus PPF (magic PPF10/PPF20/PPF30). PPF2/3 can carry a FILE_ID.DIZ and a 1024-byte block check of the source image that helps identify the target. DuckStation can apply a .ppf on the fly when it sits next to the image with the same base name. Small hacks sometimes use IPS on extracted files. Identify the target from the Redump name or serial in the readme or patch file name, or from the source file name in the xdelta app header (xdelta3 printhdr). LibCrypt .sbi files are companion subchannel data, not patches.

**Cheats and emulator patches.** DuckStation uses one INI-like .cht file per game, named SERIAL.cht or SERIAL-HASH.cht (hyphen; HASH is the XXHash DuckStation shows, used only when revisions differ). Cheats go in <DataDir>/cheats/ and patches (widescreen, 60fps, overrides) in <DataDir>/patches/. Each code has a [Name] header, then Key = Value lines (Type=Gameshark|Assembly, Activation=Manual|EndFrame|StartFrame, Description, Author, Option/OptionRange, Ignore), then the code body. Patch-only keys include OverrideAspectRatio and Enable8MBRAM. The emulator bundles the chtdb database. RetroArch keeps .cht files in cheats/<system>/<content name>.cht (cheatN_desc/cheatN_code), keyed by content file name.

**Official extras.** none as installable packages. Some discs have CD-DA bonus audio tracks inside the bin/cue, which are part of the game image. PS1 Classics (EBOOT.PBP) are games.

**Licences.** none for discs. PSN PS1 Classics on PSP/Vita/PS3 use those host systems' .rif/.rap files; recognise them by extension only.

**Not games (firmware, caches, configs).** BIOS files (scph1001.bin, scph5501.bin, scph7001.bin, psxonpsp660.bin, ps1_rom.bin), memory cards (.mcd, .mcr, .srm), save states, DuckStation cache/, gamesettings/<SERIAL>.ini, covers/, screenshots/, textures/ dumps, RetroArch .opt/.cfg overrides. .sbi/.lsd LibCrypt files and .m3u playlists belong to a game but are not games.

**Confidence.** high for texture and cheat formats (DuckStation docs read; the replacements subfolder name comes from community guides, the wiki only names dumps). Medium for PPF details.

**Sources:**
- <https://github.com/stenzek/duckstation/wiki/Texture-Replacement>
- <https://github.com/duckstation/chtdb/blob/master/cheat-format.md>
- <https://docs.libretro.com/guides/cheat-codes/>

## PlayStation 2

**Mods and texture packs.** PCSX2 texture replacement: <DataDir>/textures/<SERIAL>/replacements/ (dumps in textures/<SERIAL>/dumps/). Community guides say the serial must be upper case (e.g. SLUS-20420). Files are hash-named .png/.dds. The serial folder links a pack to its game. Many mods ship as .pnach patches or rebuilt ISOs.

**Translation patches and ROM hacks.** Mostly xdelta3 against the ISO, sometimes PPF3, sometimes .pnach keyed by CRC. Identify the target from the Redump name or serial in the readme or the xdelta source file name.

**Cheats and emulator patches.** PCSX2 .pnach text files. User cheats go in <DataDir>/cheats/ and user patches in <DataDir>/patches/. The pcsx2_patches repo names files SERIAL_CRC.pnach (e.g. PAPX-90203_55CE5111.pnach), and some legacy files are CRC-only (6D980D22.pnach). CRC is the 8-hex ELF CRC. Content: gametitle=, comment=, [Section] groups, patch=1,EE,addr,type,value lines. PCSX2 builds ship these bundled as patches.zip. Old builds used cheats_ws/. Raw Action Replay/CodeBreaker codes are encrypted and must be converted first.

**Official extras.** none as separate packages. Demo discs are games. PS2 Classics belong to PS3 PKGs.

**Licences.** none for discs. PS2 Classics .rap/EDAT belong to PS3.

**Not games (firmware, caches, configs).** BIOS dumps (SCPH-*.bin plus .rom1/.rom2/.erom/.nvm/.mec), memory cards (Mcd001.ps2, folder memcards), cache/, gamesettings/*.ini, snaps/, sstates/ (.p2s), covers/, patches.zip, textures/ dumps. OPL artifacts: ART/, CFG/, VMC/, ul.cfg, ul.* split parts (game data, not separate games).

**Confidence.** high for pnach naming (repo listing checked); medium for the texture path (community guides only, the official docs page returned 404)

**Sources:**
- <https://github.com/PCSX2/pcsx2_patches>
- <https://www.moddb.com/tutorials/pcsx2-qt-texture-replacement-tutorial>
- <https://forums.pcsx2.net/archive/index.php/thread-68472.html>

## Sega Saturn

**Mods and texture packs.** none. No texture replacement in Beetle Saturn, Kronos or YabaSanshiro. Mods ship as patched disc images.

**Translation patches and ROM hacks.** xdelta3 against the data-track .bin, sometimes PPF or a dedicated patcher tool. Identify the target from the Redump name in the readme.

**Cheats and emulator patches.** Action Replay codes as text. RetroArch cheats/<system>/<content>.cht, keyed by content file name. Mednafen and Kronos/Yaba have their own cheat files (format not verified).

**Official extras.** none

**Licences.** none

**Not games (firmware, caches, configs).** BIOS files (sega_101.bin, mpr-17933.bin, saturn_bios.bin, ST-V BIOS), backup RAM (.bkr/.bcr), save states, core options.

**Confidence.** medium

**Sources:**
- <https://docs.libretro.com/library/beetle_saturn/>
- <https://docs.libretro.com/guides/cheat-codes/>

## Sega Dreamcast

**Mods and texture packs.** Flycast custom textures. The default load path is <data>/textures/<GAMEID>/, and user-defined texture paths can be set (Flycast checks <path>/<GAMEID> and <path>/textures/<GAMEID>). Dumps go to <data>/texdump/ (per game ID). In RetroArch the path is system/dc/textures/<GAMEID>/. GAMEID is the product ID from the disc header with trailing spaces trimmed (e.g. 17707N, T1401N). Files are hash-named .png. Redream has no texture packs.

**Translation patches and ROM hacks.** xdelta3 against GDI track files or a CDI, or a fully rebuilt GDI/CDI. Identify the target from the Redump/TOSEC name in the readme.

**Cheats and emulator patches.** Flycast loads RetroArch-style .cht files (cheatN_desc/cheatN_code/cheatN_enable), keyed by file name, and has built-in widescreen cheats. Codes are CodeBreaker/GameShark/Xploder text.

**Official extras.** VMU downloadable content (.VMS/.VMI/.DCI): saves or DLC, not games. Bonus audio is inside the GDI.

**Licences.** none

**Not games (firmware, caches, configs).** dc_boot.bin, dc_flash.bin, Naomi/Atomiswave BIOS, VMU saves (vmu_save_A1.bin), emu.cfg, Flycast data/ and texdump/, RetroArch system/dc/.

**Confidence.** high for the texture path (Flycast source read); medium for the rest

**Sources:**
- <https://github.com/flyinghead/flycast/blob/master/core/oslib/oslib.cpp>
- <https://github.com/flyinghead/flycast/blob/master/core/rend/CustomTexture.cpp>
- <https://emudeck.github.io/emulators/steamos/flycast>

## Sega CD / Mega-CD

**Mods and texture packs.** none. MSU-MD targets Mega Drive ROMs, not Sega CD.

**Translation patches and ROM hacks.** xdelta3 or PPF against the bin track, or a full image. Identify the target from the Redump name in the readme.

**Cheats and emulator patches.** Game Genie/Action Replay codes as text. RetroArch .cht files (Genesis Plus GX, PicoDrive), keyed by content file name.

**Official extras.** none

**Licences.** none

**Not games (firmware, caches, configs).** BIOS files (bios_CD_U.bin, bios_CD_E.bin, bios_CD_J.bin), backup RAM (.brm), save states, core options.

**Confidence.** medium

**Sources:**
- <https://docs.libretro.com/library/genesis_plus_gx/>

## PC Engine CD / TurboGrafx-CD

**Mods and texture packs.** none

**Translation patches and ROM hacks.** IPS on the data track (historically common), or xdelta/PPF on the track, often shipped with a replacement .cue and audio tracks. Identify the target from the Redump name in the readme.

**Cheats and emulator patches.** RetroArch .cht files for Beetle PCE / PCE Fast, keyed by content file name.

**Official extras.** none

**Licences.** none

**Not games (firmware, caches, configs).** System Card BIOS (syscard3.pce, syscard2.pce, syscard1.pce, gexpress.pce), save RAM, core options.

**Confidence.** medium

**Sources:**
- <https://docs.libretro.com/library/beetle_pce_fast/>

## Nintendo GameCube

**Mods and texture packs.** Dolphin custom textures in User/Load/Textures/<GAMEID>/ (a 6-character ID like GALE01, or a 3-character prefix for all regions). Files are named tex1_WxH_<hash>_<fmt>.png/.dds, with _mipN/_arb variants. Graphics Mods live in User/Load/GraphicMods/<Name>/metadata.json, keyed by game ID. Resource Packs are .zip files in ResourcePacks/.

**Translation patches and ROM hacks.** xdelta3 against the ISO, BPS (magic BPS1, with CRC32s of source, target and patch in the footer), or Riivolution XML. Identify the target from the game ID or Redump name.

**Cheats and emulator patches.** Dolphin User/GameSettings/<GAMEID>.ini (plus 3-character and revision variants), with [Gecko]/[Gecko_Enabled], [ActionReplay]/[ActionReplay_Enabled] and [OnFrame]/[OnFrame_Enabled] sections. Defaults are bundled in Data/Sys/GameSettings. On hardware, Nintendont/Swiss use codes/<GAMEID>.gct.

**Official extras.** none

**Licences.** none

**Not games (firmware, caches, configs).** IPL.bin (User/GC/<region>/), memory cards (MemoryCardA.*.raw, GCI folders), User/Cache, User/ShaderCache, User/Dump, User/GameSettings, User/StateSaves, Load/Textures and GraphicMods (mods, not games).

**Confidence.** medium-high (well established, but the wiki page could not be fetched)

**Sources:**
- <https://github.com/dolphin-emu/dolphin/tree/master/Data/Sys/GameSettings>
- <https://dolphin-emu.org/docs/guides/>

## Nintendo Wii

**Mods and texture packs.** Riivolution: riivolution/*.xml plus data folders, on the SD card or in Dolphin's User/Load/Riivolution/. <id game="RMC"> in the XML links a patch to its game. Custom textures and Graphics Mods work as on GameCube.

**Translation patches and ROM hacks.** Riivolution XML (most common), xdelta3 against the ISO/WBFS, or BPS. Identify the target from the game ID in the XML or file name.

**Cheats and emulator patches.** Same as GameCube in Dolphin. USB Loader GX/WiiFlow use codes/<GAMEID>.gct and txtcodes/<GAMEID>.txt.

**Official extras.** none as themes. WiiWare/VC WADs are games or channels. Menu themes (.csm) are homebrew.

**Licences.** Tickets and TMDs in WADs and NAND (ticket/, title/). Recognise them by path or extension and never open them.

**Not games (firmware, caches, configs).** Dolphin User/Wii NAND (sys, shared1, title, ticket), IOS WADs, apps/*/boot.dol + meta.xml homebrew, WBFS split parts (.wbf1-3), Cache/GameSettings.

**Confidence.** medium-high

**Sources:**
- <https://riivolution.github.io/>
- <https://wiibrew.org/wiki/Riivolution>

## Microsoft Xbox (original)

**Mods and texture packs.** none in xemu. Mods replace files inside the XISO or game folder.

**Translation patches and ROM hacks.** xdelta3 against the XISO/Redump ISO, or replaced files. Identify the target from the Redump name or the XBE title ID.

**Cheats and emulator patches.** none in xemu. Hardware trainers (.etm/.xbtf) or patched XBEs.

**Official extras.** Xbox Live DLC installed in the HDD image under TDATA/UDATA/<TitleID>. Dashboard skins are unofficial.

**Licences.** none needed. eeprom.bin and the HDD image contain keys; recognise them by name only.

**Not games (firmware, caches, configs).** mcpx_1.0.bin, flash BIOS (e.g. Complex_4627.bin), xbox_hdd.qcow2, eeprom.bin, xemu.toml, dashboard xboxdash.xbe, partition folders C/E/X/Y/Z.

**Confidence.** medium

**Sources:**
- <https://xemu.app/docs/required-files/>

## PlayStation Portable (PSP)

**Mods and texture packs.** PPSSPP texture replacement in memstick/PSP/TEXTURES/<GAMEID>/ (the folder must exist). Each pack has a textures.ini with [options] (version, hash=quick|xxh32|xxh64, ignoreAddress, reduceHash), [hashes], [hashranges] and [filtering] sections. Formats: PNG, DDS (BC1/2/3/7, not on mobile), KTX2 (Basis/UASTC, 1.15+) and ZIM. A zipped pack loads as textures.zip inside the game folder (stored uncompressed is faster); a root-level <GAMEID>.zip was not confirmed and is dropped. PRX plugins go in PSP/PLUGINS/.

**Translation patches and ROM hacks.** Mostly xdelta3 against the ISO, sometimes PPF, a patched EBOOT/BOOT.BIN, or a CWCheat/plugin. Identify the target from the UMD ID (ULUS/ULES/ULJM/NPJH) in the readme or file name, or the xdelta source file name.

**Cheats and emulator patches.** PPSSPP: memstick/PSP/Cheats/<GAMEID>.ini, with GAMEID from PARAM.SFO without the dash (e.g. ULUS10336.ini). The community database downloads to PSP/Cheats/cheat.db. Format: _S id (dashes ignored), _G title (ignored), _C0/_C1-9 name (disabled/enabled), _L 0xAAAAAAAA 0xBBBBBBBB, _M (TempAR, unsupported). UTF-8; // and # start comments. Real hardware uses CWCheat in seplugins/cwcheat/.

**Official extras.** PSP themes are .PTF files in PSP/THEME/. PSP/PHOTO, MUSIC and VIDEO hold user media. DLC and updates live under PSP/GAME/<ID> (EDAT, PBOOT.PBP). Use PARAM.SFO CATEGORY to separate them: UG = UMD game, MG/EG = memory stick game, PG = patch/update, ME = PS1 Classic.

**Licences.** PSN .rif (PSP/LICENSE), act.dat and EDAT. Recognise them by extension and location only.

**Not games (firmware, caches, configs).** Firmware update EBOOT (PSP/GAME/UPDATE), PSP/SYSTEM (ppsspp.ini, controls.ini, per-game <GAMEID>_ppsspp.ini, CACHE), PSP/SAVEDATA, PSP/PPSSPP_STATE, PSP/TEXTURES (including the new/ dump folder), PSP/Cheats, PSP/PLUGINS, flash0/.

**Confidence.** high for cheats, textures.ini and the folder/zip layout (docs and source read); medium for the CATEGORY values

**Sources:**
- <https://ppsspp.org/docs/reference/cwcheats/>
- <https://ppsspp.org/docs/reference/texture-replacement/>
- <https://github.com/hrydgard/ppsspp/blob/master/GPU/Common/TextureReplacer.cpp>
- <https://www.psdevwiki.com/psp/>

## Nintendo 3DS

**Mods and texture packs.** Mods are LayeredFS folders keyed by the 16-hex-digit Title ID (00040000XXXXXXXX for retail apps).
(1) Azahar/Citra: <user_dir>/load/mods/<TitleID>/. It is also reachable from the game's right-click menu. All parts are optional and empty folders are ignored:
- exefs/ holds a replacement code.bin, or a code.ips or code.bps patch.
- romfs/ holds replacement assets. It must mirror the game's RomFS tree and is layered over the base.
- romfs_ext/: a <file>.stub deletes that base file, and a <file>.ips or <file>.bps patches it.
- exheader.bin overrides the ExHeader.
Azahar's docs say the framework 'offers drop-in compatibility with Luma3DS mods'. code.bin and code.ips can also sit directly in the TitleID folder.
(2) Console, Luma3DS game patching: sd:/luma/titles/<TitleID>/ with romfs/, code.bin|code.ips and exheader.bin.
(3) 3GX plugins are binary CTRPluginFramework plugins, not data mods. Paths: sdmc/luma/plugins/<TitleID>/<name>.3gx (per game) or luma/plugins/default.3gx (all games).
Texture packs are Azahar only. Use the game's 'Open Custom Texture Location' menu item to find the load path, which is load/textures/<TitleID>/. Dumped and replacement files are named tex1_<W>x<H>_<16-hex hash>_<pixel format>_mip<N>.png. The docs' example is tex1_256x256_543624189C94B105_12_mip0.png. Normal maps add '.norm' before the extension (texture.norm.png) and need fragment lighting. pack.json is optional. It holds name and author, use_new_hash (new hash by default; the old hash is used without pack.json or when it is disabled), a 'textures' hash-to-filename map, and mipmap and flip options. The dump folder (load path 'dump/textures/<TitleID>') ships a template pack.json.
How to recognise a mod: a folder named by a 16-hex TitleID that contains romfs/, exefs/, romfs_ext/, exheader.bin, code.ips|code.bps|code.bin or tex1_*.png.
Link it to the game by the program ID in the NCCH header, or by hShop/3dsdb metadata. TitleIDs differ by region, so a mod is region-specific.

**Translation patches and ROM hacks.** (1) Most common: LayeredFS folders (romfs/ plus code.ips or code.bps) under <TitleID>, shipped as ZIPs with a readme that names the TitleID and region.
(2) xdelta3 (.xdelta/.vcdiff) against a decrypted .3ds/.cci/.cia. IPS is unusable for whole ROMs because of its 24-bit offsets (16 MB limit).
(3) BPS: the header holds source size, target size and metadata. The footer holds the source CRC32, target CRC32 and patch CRC32, so a target can be matched against No-Intro CRCs.
(4) Some groups ship pre-patched CIAs.
About xdelta3: it stores Adler-32 checksums of the target windows, not a source checksum. Its application header (on by default; -A sets it) usually records the source and target file names, which often name the No-Intro ROM.
Identify a patch's target in this order: BPS source CRC32, the TitleID folder name, the xdelta app header, the readme, then the romhacking.net page (platform and required ROM checksum).

**Cheats and emulator patches.** Plain-text Gateway / Action Replay format, one file per title, named by the TitleID as 16 upper-case hex digits plus .txt. Each cheat is a [Cheat Name] line followed by 8+8 hex code lines. Lines starting with '*' are notes or flags.
- Azahar/Citra: <user_dir>/cheats/<TitleID>.txt. Desktop: File > Open Azahar Folder.
- Luma3DS Rosalina on console: sd:/cheats/<TitleID>.txt, same format.
- CTRPluginFramework .3gx plugins live under luma/plugins/ (see mods). They are binary, not text cheats.
- RetroArch has no maintained 3DS core with .cht support.

**Official extras.** Types are told apart by TitleID high word, with no keys needed: 00040000 app, 00040002 demo, 0004000E update, 0004008C DLC, 00048004 DSiWare port.
(1) E-manuals: not a separate package. NCCH content index 1 is the e-manual CFA (Manual.bcma) and index 2 is the Download Play child. Show the manual as an extra of the parent game.
(2) Home Menu themes: sold as 0004008C DLC attached to the Home Menu. The exact per-region theme-DLC IDs are not confirmed by 3dbrew; its DLC page only notes that themes use several DLC indexes. Custom themes are folders or ZIPs in sd:/Themes/ (Anemone3DS/ThemePlaza format) with body_LZ.bin plus optional bgm.bcstm, info.smdh and preview.png.
(3) Badges: no installable package. They live in Badge Arcade extdata, so treat them as save data.
(4) Video and music apps: Nintendo Video, trailers and similar shipped as ordinary 00040000 CIAs. Tell them from games by SMDH title or hShop category ('Videos' or 'Extras').
Loose .bcstm, .mp3, .m4a or .ogg files are user-added media. Nintendo 3DS Sound and Camera are 00040010 system apps.

**Licences.** Recognise these by name only and never read them: *.tik and ticket.db (tickets), seeddb.bin (seeds), aes_keys.txt (keys for older Citra builds; current Azahar requires decrypted games), boot9.bin, boot11.bin, movable.sed, otp.bin, secret_sector.bin and GodMode9's essential.exefs backup. A CIA embeds its own ticket and TMD; these are fine to skip.
Coverly needs two things:
- The program ID and product code from the NCCH header. These are plaintext even on encrypted dumps.
- The SMDH (icon and titles). It sits inside the ExeFS, so it is readable only from decrypted CIA/3DS/CXI files.
On an encrypted file, fall back to the TitleID and product code plus external metadata.

**Not games (firmware, caches, configs).** Never treat these as games. TitleID high words (3dbrew):
- 00040010: system applications
- 0004001B: system data archives
- 00040030: system applets
- 0004009B: shared data archives
- 000400DB: system data (auto-update)
- 00040130: system modules
- 00040138: system firmware
Also noise: System Update CIAs.
Add-ons rather than separate games: 0004000E patches/updates and 0004008C DLC. 00040001 (Download Play child) is also not a game in its own right.
Azahar noise: shaders/, per-game configs under config/custom/<TitleID>.ini, nand/, sysdata/, the sdmc/Nintendo 3DS/<id0>/<id1>/ tree, states/, dump/ and load/ (mods).
Console noise: luma/ (config.ini, titles/, plugins/), gm9/, boot.firm, boot.3dsx, cheats/, Themes/, and .3dsx homebrew.

**Confidence.** High: mod, texture and plugin paths (official Azahar docs, fetched) and the TitleID categories (3dbrew Title_list, fetched).
Medium: the cheat file locations, taken from known Citra and Rosalina source behaviour (not re-fetched).
Low: theme-DLC TitleIDs. 3dbrew's DLC list does not name them.
Dropped: claims about specific RetroDECK and EmuDeck paths (not verified).

**Sources:**
- <https://citra.azahar-emu.org/help/feature/game-modding/>
- <https://citra.azahar-emu.org/help/feature/custom-textures/>
- <https://www.3dbrew.org/wiki/Title_list>
- <https://www.3dbrew.org/wiki/Title_list/DLC>
- <https://www.3dbrew.org/wiki/CIA>
- <https://www.3dbrew.org/wiki/SMDH>

## Nintendo Wii U

**Mods and texture packs.** Cemu: every mod is a 'graphic pack' folder in <Cemu>/graphicPacks/. Community packs from the cemu_graphic_packs repo download into graphicPacks/downloadedGraphicPacks/.
Each pack has a rules.txt with a [Definition] section:
- titleIds: comma-separated 16-hex IDs (BotW is 00050000101C9300 JP, 101C9400 US, 101C9500 EU)
- name
- path: the place in the Graphic packs window
- description
- version: must be set; 2 since Cemu 1.8.0, 6 for Cemu newer than 1.23.1a; ZeldaMods says content mods need at least 3
- fsPriority: optional; higher wins, default 100
Optional parts:
- [Preset] blocks
- [TextureRedefine] rules. These only change texture resolution and format; Cemu has no custom texture replacement.
- Shader replacements
- Assembly patches. Native since Cemu 1.17.0; formerly Cemuhook patches.txt.
- content/ and aoc/ file-replacement folders next to rules.txt
On console, SDCafiine: legacy sd:/sdcafiine/<TitleID>/<ModName>/content|aoc. The Aroma SDCafiine/ContentRedirection plugin uses sd:/wiiu/sdcafiine/<TitleID>/<ModName>/content|aoc.
Link a mod to its game by the titleIds in rules.txt (plain key=value) or by the SDCafiine TitleID folder name.

**Translation patches and ROM hacks.** Translations are mostly content/ file replacements shipped as a Cemu graphic pack (rules.txt) or an SDCafiine folder, inside ZIPs. A few xdelta patches against extracted files exist. IPS and BPS are impractical at Wii U sizes.
Link a patch to its game by titleIds, the SDCafiine TitleID folder or the readme. Redump's Wii U DAT gives disc checksums and serials (WUP-P-XXXX).

**Cheats and emulator patches.** There is no standard per-game text cheat file, and Cemu has no cheat engine. Cheats ship as graphic packs (rules.txt plus assembly patches), usually with a path or name containing 'Cheats'. Classify them as cheats with the same titleIds rule as mods.
Console: TCPGecko/JGecko U code lists (XML keyed by TitleID) and Aroma plugins (.wps in sd:/wiiu/environments/aroma/plugins/). Plugins are system-wide, not per game.

**Official extras.** None as separate packages: no themes, avatars or soundtrack packages. The Wii U Menu has no themes, and Mii/Miiverse data is system data.
E-manuals ship inside each title (meta/Manual.bfma), so show them as part of the game.
Virtual Console titles are ordinary 00050000 apps.
Anything else is user-added media.

**Licences.** Recognise these by name only and never read them: title.tik and *.tik, title.tmd, title.cert (NUS folders of *.app and *.h3 files), Cemu's keys.txt, otp.bin, seeprom.bin and account.dat.
For metadata, read meta/meta.xml (plaintext title_id, longname_*, product_code, company_code) and meta/iconTex.tga from decrypted title folders (code/, content/, meta/) or from .wua archives.
Encrypted .wud/.wux discs and NUS packages are opaque, so match them by filename or TitleID only.

**Not games (firmware, caches, configs).** Never treat these as games. TitleID high words: 00050010 system apps, 00050030 applets, 0005001B system data, plus system firmware.
Add-ons of the base game: 0005000E updates and 0005000C DLC. 00050002 is demo/kiosk content.
Cemu noise: shaderCache/ (transferable *_shaders.bin, *_vkpipeline.bin), gameProfiles/<TitleID>.ini plus the shipped gameProfiles/default/, mlc01/sys/, mlc01/usr/save/, mlc01/usr/title/0005000E|0005000C (updates and DLC), controllerProfiles/, graphicPacks/, settings.xml and log.txt.

**Confidence.** High: the rules.txt fields, version, fsPriority, BotW TitleIDs and the legacy SDCafiine path (Cemu wiki and ZeldaMods, fetched).
Medium: the Aroma SDCafiine path, the cheat ecosystem and Manual.bfma.
Dropped: the specific patch_*.asm/moduleMatches syntax (not on the fetched pages).

**Sources:**
- <https://wiki.cemu.info/wiki/Graphic_packs>
- <https://wiki.cemu.info/wiki/Graphic_packs_creation>
- <https://github.com/cemu-project/cemu_graphic_packs>
- <https://zeldamods.org/wiki/Help:Using_mods>
- <https://wiiubrew.org/wiki/Title_database>
- <https://wiiubrew.org/wiki/Meta.xml>

## Nintendo Switch

**Mods and texture packs.** Mods are keyed by the 16-hex Program ID (base 0100XXXXXXXXX000).
- Updates: base + 0x800.
- DLC: (base & ~0xFFF) + 0x1000 + index.
Console, Atmosphere LayeredFS: sd:/atmosphere/contents/<TitleID>/romfs/ and exefs/ (or exefs.nsp), plus flags/. Before Atmosphere 0.10 the folder was atmosphere/titles/. ExeFS patches: atmosphere/exefs_patches/<PatchSet>/<BuildID>.ips, in IPS or IPS32 ('IPS32' magic, 'EEOF' end). Also nro_patches/, and kip_patches/ (system modules, so noise).
Ryujinx/Ryubing (verified in ModLoader.cs):
- Mods: mods/contents/<titleid>/. Subfolders are scanned recursively, so <ModName>/romfs|exefs|cheats works, as do romfs.bin and exefs.nsp containers and .stub deletion files.
- Patches: mods/exefs_patches/<name>/ and mods/nro_patches/<name>/, holding .ips or IPSwitch .pchtxt.
- It also reads the SD mods base, sdcard/atmosphere/.
- Enable states: games/<titleid>/mods.json.
Yuzu forks (Eden, Citron, Sudachi): <user>/load/<TitleID>/<ModName>/ with exefs/ (<BuildID>.ips or .pchtxt), romfs/, romfs_ext/ and cheats/.
How to recognise a mod: a 0100... TitleID folder containing romfs/, exefs/, romfs.bin or exefs.nsp, or a <BuildID>.ips or .pchtxt file. An exefs patch is tied to one game version by its BuildID.

**Translation patches and ROM hacks.** Translations ship as LayeredFS folders (contents/<TitleID>/romfs, sometimes with exefs .ips or .pchtxt keyed by BuildID). Whole-file xdelta patches against an NSP/XCI are rare, and IPS/BPS ROM patches are not used.
The target is the TitleID folder name, plus the BuildID for exefs parts (which pins the version), plus the readme. GameBanana and GitHub are the main sources; romhacking.net has few Switch titles.

**Cheats and emulator patches.** Atmosphere dmnt: sd:/atmosphere/contents/<program_id>/cheats/<build_id>.txt. build_id is the hex form of the first 8 bytes of the main executable's build ID, so 16 hex characters.
The text has [Cheat Name] headers followed by 32-bit hex opcode words. Atmosphere's parser treats a {Name} brace header as the master code; this comes from dmnt source and is not in cheats.md.
Loading: cheats load at launch unless L is held (cheat_enable_key default '!L'). All cheats in the file are toggled on by default (setting atmosphere!dmnt_cheats_enabled_by_default).
Ryujinx: any cheats/ folder under mods/contents/<titleid>/, with files named <16-hex BuildID>.txt. Enabled cheats are recorded in contents/<titleid>/cheats/enabled.txt.
Yuzu forks: load/<TitleID>/<ModName>/cheats/<BuildID>.txt.
All use the same format, so one parser covers them. A cheat file matches exactly one game version.
IPSwitch .pchtxt: an '@nsobid-<BuildID>' line, optional '@flag offset_shift', then '@enabled'/'@disabled' blocks of 'offset hexbytes' lines. Used for 60 FPS and resolution patches.

**Official extras.** None as separate packages. There are no purchasable Home Menu themes (only the base black and white), no avatar packs and no soundtrack packages; Nintendo Music is a mobile app.
E-manuals and legal pages ship inside the game as HtmlDocument NCAs.
Official installables beyond the game are DLC (AddOnContent, which can include soundtracks and costumes) and updates. Classify them as DLC with the base game as parent; what a DLC contains shows only in its name.
Custom themes are .nxtheme files, usually in sd:/themes/, installed with NXThemes Installer and carrying an embedded info.json. Profile icons are system data.

**Licences.** Recognise these by name only and never read them: prod.keys, title.keys, dev.keys, *.tik and *.cert, PRODINFO, BOOT0 and BOOT1, rawnand.bin and eMMC backups.
Metadata without keys:
- NSP files are PFS0 containers whose file list (*.nca, *.tik, *.cert, optional *.cnmt.xml) is plaintext. The .cnmt.xml gives TitleID, version and type (Application / Patch / AddOnContent).
- Filename conventions: [TitleID][vVersion].
- Homebrew-exported control.nacp and icon files.
NCA contents are encrypted.

**Not games (firmware, caches, configs).** Never treat these as games: firmware dumps (folders or ZIPs of hundreds of .nca files), the SystemUpdate title 0100000000000816 and other 010000000000xxxx system titles.
Add-ons, not separate games: updates (TitleID ending 800, type Patch) and DLC (AddOnContent).
Atmosphere noise: atmosphere/config/, system exefs_patches (es/fs/nifm patches), kip_patches/, hbl.nsp, bootloader/ (Hekate) and .nro homebrew in /switch.
Ryujinx noise: games/<id>/cache/shader/, games/<id>/cache/cpu/ (PPTC), games/<id>/mods.json, bis/, system/ and sdcard/.
Yuzu-fork noise: shader/, custom/<TitleID>.ini, nand/, load/ and dump/.

**Confidence.** High: Atmosphere cheat path, build ID rule and enable defaults (cheats.md and override_config.ini, fetched); the Ryujinx mod, cheat and patch layout (ModLoader.cs source, fetched).
Medium: the yuzu-fork layout (archived yuzu docs).
Corrected: the earlier claim that cheats load when L is held. They load unless L is held.

**Sources:**
- <https://raw.githubusercontent.com/Atmosphere-NX/Atmosphere/master/docs/features/cheats.md>
- <https://raw.githubusercontent.com/Atmosphere-NX/Atmosphere/master/config_templates/override_config.ini>
- <https://git.ryujinx.app/api/v1/repos/ryubing/ryujinx/raw/src/Ryujinx.HLE/HOS/ModLoader.cs>
- <https://yuzu-mirror.github.io/help/feature/game-modding>
- <https://switchbrew.org/wiki/Title_list>
- <https://switchbrew.org/wiki/NCA>

## Nintendo DSi

**Mods and texture packs.** There is no LayeredFS or runtime file-replacement framework for DS/DSi (TWiLight Menu++, nds-bootstrap and melonDS have none), and no texture packs: melonDS only upscales and cannot replace textures. Mods are ROM hacks shipped as patches, or pre-patched .nds/.dsi files whose header keeps the original game code.
DSiWare (00030004XXXXXXXX) can run from SD through TWiLight Menu++ / nds-bootstrap.

**Translation patches and ROM hacks.** NDS ROMs run from 8 to 512 MB, so xdelta3 (.xdelta/.vcdiff) is most common.
- xdelta3 stores target-window Adler-32 checksums and, usually, source and target file names in its app header. It has no source checksum.
- BPS comes next. Flips handles files over 16 MB, and the footer holds source, target and patch CRC32.
- UPS (source and target CRC32) appears on older patches.
- IPS only suits small hacks: 16 MB limit, no checksum.
- PPF and APS are not used for DS.
Identify the target by: BPS/UPS source CRC32 matched against the No-Intro DAT, the header game code plus revision (0x0C, 0x1E), the xdelta app-header file name, the readme, or the romhacking.net page (required No-Intro name and CRC32/SHA-1).
Release names follow No-Intro conventions, e.g. 'Title (Japan) [T-En by Group v1.0]'.

**Cheats and emulator patches.** (1) usrcheat.dat: the R4-style cheat database for all games, at sd:/_nds/TWiLightMenu/extras/usrcheat.dat (TWiLight FAQ; older versions used _nds/TWiLightMenu/cheats). Entries are keyed by game code plus header CRC32. It is a single global file, so treat it as system data, not a per-game file.
(2) melonDS: a <romname>.mch file next to the ROM (plain-text AR DS codes).
(3) DeSmuME: <romname>.dct.
(4) RetroArch (melonDS DS / DeSmuME cores): cheats/<core>/<content name>.cht in libretro .cht format (cheats = N, cheatN_desc, cheatN_code, cheatN_enable), keyed by ROM file name.
AR DS codes are 8+8 hex words.

**Official extras.** No official themes, avatars, e-manuals or soundtrack packages.
DSi Shop downloads (00030004 DSiWare, e.g. Flipnote Studio and DSi Browser) are titles in No-Intro's DSiWare DAT. Treat the non-game ones as 'apps'.
DSi Camera and DSi Sound are 00030005 built-in apps, not extras. AAC/M4A files that DSi Sound plays from SD are user-added media.

**Licences.** Recognise these by name only and never read them: bios7i.bin, bios9i.bin, bios7.bin, bios9.bin, firmware.bin, nand.bin and dsi_nand.bin backups, DSiWare *.tik and title.tmd, and console-encrypted DSiWare .bin SD exports.
The plaintext NDS header is enough for Coverly:
- title at 0x00
- game code at 0x0C
- maker code at 0x10
- unit code at 0x12 (0x02 DSi-enhanced, 0x03 DSi-exclusive)
- banner offset at 0x68
- DSi TitleID at 0x230

**Not games (firmware, caches, configs).** Never treat these as games. DSi system titles (dsibrew):
- 00030005: built-in apps (DSi Camera, DSi Sound)
- 00030015: title management (System Settings, DSi Shop)
- 00030017: System Menu (launcher)
- 0003000F: non-executable data files
Also noise: system updates, NAND dumps, the TWiLight Menu++ _nds/ tree (settings, boxart cache, usrcheat.dat), nds-bootstrap.nds, emulator configs, .sav/.dsv saves, savestates and homebrew .nds files.

**Confidence.** High: title categories (dsibrew, fetched), the usrcheat.dat path (TWiLight FAQ via search), header fields and patch formats.
Medium: melonDS .mch and DeSmuME .dct naming.
Corrected: the system title categories.

**Sources:**
- <https://dsibrew.org/wiki/Title_list>
- <https://dsibrew.org/wiki/DSi_cartridge_header>
- <https://wiki.ds-homebrew.com/twilightmenu/faq>
- <https://melonds.kuribo64.net/faq.php>
- <https://docs.libretro.com/guides/cheat-codes/>
- <https://github.com/Alcaro/Flips>
- <https://www.romhacking.net/>

## PS3 (RPCS3)

**Mods and texture packs.** RPCS3 has no overlay or mod folder. Mods replace files in place: dev_hdd0/game/<TITLEID>/USRDIR/ for PSN/HDD games, or <disc folder>/PS3_GAME/USRDIR/ for folder-format disc games. Some games load packed mod archives through their own loaders, for example Persona 5 via mod.cpk with Aemulus/Reloaded-II (ShrineFox). RPCS3 does not support texture replacement, so there are no texture packs. To identify a mod: a zip or folder that mirrors USRDIR or PS3_GAME paths, or a serial (BLUS/BLES/BCES/NPUB/NPEB...) in the folder name or readme. Updates and DLC are .pkg files (magic 0x7F 'PKG', ContentID in plaintext at header 0x30) that install to dev_hdd0/game/<TITLEID>/ with PARAM.SFO CATEGORY 'GD'. They are updates/DLC, not mods.

**Translation patches and ROM hacks.** Most fan translations ship as replacement files for USRDIR or as an unsigned PKG/update. Some are xdelta patches for specific archives or EBOOT.BIN. None use BPS/UPS checksum headers. The target game comes from the serial (BLUS/BLJM/NPJB...) in the folder or readme, plus the required game version. Redump identifies the base disc. RPCS3 patch.yml (or <TITLEID>_patch.yml) is also used for small text or hack patches.

**Cheats and emulator patches.** RPCS3 patches act as both cheats and enhancements. They live in <config_dir>/patches/: patch.yml is the official downloaded file, imported_patch.yml holds the user's own patches, and optional per-title <TITLEID>_patch.yml files are also loaded (bin_patch.cpp). Each file is YAML and needs a 'Version' entry equal to the engine version "1.2"; files with a missing or different version are rejected. The top-level key of each entry is the executable hash, written as 'PPU-<sha1>' (PPUModule.cpp builds the 'PPU-' + 40-hex string; SPU- keys exist for SPU patches). Under the hash come the game title, serial(s) (BLUS/BLES/NPUB...) and app version. To link a patch to a game, match the serial plus APP_VER from PARAM.SFO. The hash itself is printed in RPCS3.log. Which patches are enabled is stored separately in <config_dir>/patch_config.yml. RPCS3's built-in Cheat Manager saves to patches/cheats.yml (cheat_manager.cpp). On real hardware the formats are Artemis (.ncl) and webMAN cheats, which RPCS3 does not use.

**Official extras.** Themes are .p3t files (dev_hdd0/theme/, or /PS3/THEME/ on USB). Avatars and themes are also sold as PSN .pkg files. The PKG content_type is a 32-bit value in the PKG metadata block, which is plaintext and readable without keys. pkg2zip and Vita3K source confirm: 0x06 PS1 classic, 0x07 PSP/PC Engine, 0x0E PSP go, 0x0F PSP minis, 0x10 NeoGeo, 0x15 Vita app, 0x16 Vita DLC, 0x18/0x1D PSM, 0x1F Vita theme. PS3-specific values from recall (medium, check before relying on them): 0x04 GameData (patch/DLC), 0x05 GameExec, 0x09 Theme, 0x0B License, 0x0D PSN Avatar, 0x12 PS2 Classic. The plaintext ContentID plus content_type is enough to route a PKG to Extras/Themes or Extras/Avatars. Soundtracks have no dedicated type (they are usually DLC/GameData PKGs), so they need name/ContentID heuristics. Discs can carry PS3_EXTRA/ (bonus videos, trailers, soundtracks as PKGs), which belongs in Extras. Manuals are not shipped as separate packages.

**Licences.** .rap files: 16-byte binaries named <ContentID>.rap (e.g. UP0001-NPUB30001_00-0000000000000001.rap), placed in dev_hdd0/home/<userid>/exdata/ (RPCS3: dev_hdd0/home/00000001/exdata/). Related files are .rif/.edat licence files in exdata and act.dat. Recognise them by extension, by the fixed 16-byte size of .rap, and by the ContentID pattern in the filename (XXYYYY-AAAA#####_00-16 chars). Never open or parse them. Classify them as 'licence' and keep them out of the library.

**Not games (firmware, caches, configs).** Firmware: PS3UPDAT.PUP ('SCEUF' magic), found on discs at PS3_UPDATE/PS3UPDAT.PUP; RPCS3 installs it into dev_flash/. RPCS3 data that is not games: cache/<SERIAL>/ (PPU/SPU/shader caches), config/custom_configs/config_<SERIAL>.yml, patches/ (patch.yml, imported_patch.yml, cheats.yml), patch_config.yml, RPCS3.log. Inside games: TROPDIR/ trophies, PS3_GAME/LICDIR, PS3_DISC.SFB. Saves: dev_hdd0/home/*/savedata/. dev_hdd0/game/<TITLEID> folders with CATEGORY 'GD' are updates/DLC, not standalone games.

**Confidence.** high, checked against RPCS3 source: patch folder and file names, the 'Version: 1.2' header (patch_engine_version = "1.2"), the PPU- hash keys, and cheats.yml. High for the PKG ContentID at header 0x30 (pkg2zip). Medium for the PS3-only PKG content_type values 0x04/0x05/0x09/0x0B/0x0D/0x12: psdevwiki blocks fetching (Cloudflare) and these come from recall. 0x06/0x07/0x0E/0x0F/0x10/0x15/0x16/0x18/0x1D/0x1F are confirmed in pkg2zip and Vita3K source.

**Sources:**
- <https://raw.githubusercontent.com/RPCS3/rpcs3/master/Utilities/bin_patch.cpp>
- <https://raw.githubusercontent.com/RPCS3/rpcs3/master/Utilities/bin_patch.h>
- <https://raw.githubusercontent.com/RPCS3/rpcs3/master/rpcs3/rpcs3qt/cheat_manager.cpp>
- <https://raw.githubusercontent.com/RPCS3/rpcs3/master/rpcs3/Emu/Cell/PPUModule.cpp>
- <https://raw.githubusercontent.com/mmozeiko/pkg2zip/master/pkg2zip.c>
- <https://psdevwiki.com/ps3/PlayStation_3_Theme_(P3T)>
- <https://www.psdevwiki.com/ps3/PKG_files>
- <https://shrinefox.com/guides/persona-5-rpcs3-modding-guide-1-downloads-and-setup>

## PS Vita (Vita3K)

**Mods and texture packs.** rePatch: ux0:rePatch/<TITLEID>/ holds modified copies of files from app/patch, excluding sce_sys. reAddcont: ux0:reAddcont/<TITLEID>/<DLCID>/ for DLC mods. rePatch reDux0 and reLoaded are variants. Vita3K installs updates into ux0/patch/<TITLEID>/ (interface.cpp); for mods in Vita3K the usual method is to merge files into ux0/app/<TITLEID>/ or ux0/patch/<TITLEID>/ inside the pref path. The Vita3K wiki has a 'Using taiHEN plugins' page (.suprx), with limited support. To identify a mod: a folder named after the TITLE_ID with no sce_sys/param.sfo of its own, or a readme that mentions rePatch. Some games have their own mod tooling, for example Persona 4 Golden via ShrineFox.

**Translation patches and ROM hacks.** Usually shipped as rePatch folders (ux0:rePatch/<TITLEID>/...). Sometimes xdelta patches for eboot.bin or specific data files, or a full modified VPK. None use checksum-headed formats. The target game comes from the TITLE_ID in the folder name or readme plus the required game version (e.g. 'requires 1.03 patch'). No-Intro has no set for Vita digital titles; NoPayStation TSVs list TITLE_ID and ContentID.

**Cheats and emulator patches.** On real hardware the format is VitaCheat .psv text files in ux0:vitacheat/db/<TITLEID>.psv (newer versions: ux0:data/vitacheat). Vita3K has no cheat engine. It has a patch system for 60fps/resolution patches, inspired by VitaGrafix but not compatible with it. Patches live in the 'patch' folder under Vita3K's pref/shared path (app_init.cpp: set_patch_path(... / "patch")). A file is loaded if its name contains the TITLE_ID and ends in .txt (for example PCSE00999_60FPS.txt), or if it is a global patchlist.txt (patch.cpp). Headers look like [PCSE00999, eboot.bin]; the binary defaults to eboot.bin, so [PCSE00999] is also valid. Patch lines are 'seg:0xOFFSET bytes'; lines starting with # or @ are skipped. Link a patch to a game by TITLE_ID (PCSG/PCSE/PCSB...) plus the binary name.

**Official extras.** Themes: Vita theme PKGs have content_type 0x1F. Vita3K installs them to ux0/theme/<ContentID>/ and adds ' (Theme)' to the title (pkg.cpp). Avatars are not shipped as packages. E-manuals ship inside each game at sce_sys/manual/ (numbered PNGs, plaintext), so Coverly can show them as an official 'Manual' extra without keys. Soundtracks have no dedicated type; they are sometimes DLC (content_type 0x16, CATEGORY 'ac') or music apps. LiveArea files (sce_sys/livearea) are game assets, not extras.

**Licences.** zRIF strings: base64 of a zlib-compressed .rif, often starting with 'KO5ifR1d...'. work.bin: 512 bytes, in sce_sys/package/work.bin of NoNpDrm dumps. .rif files: ux0:license/app/<TITLEID>/ or ux0:license/addcont/. When installing a PKG, Vita3K asks for a zRIF or work.bin, converts it with rif2zrif and writes the licence itself (pkg.cpp create_license). Recognise these by filename/path (work.bin, *.rif, license/), by size, or by a text file containing zRIF strings. Never decode them.

**Not games (firmware, caches, configs).** Firmware: PSP2UPDAT.PUP. Vita3K also needs a separate font PUP. Updates have PARAM.SFO CATEGORY 'gp' (ux0:patch/<TITLEID>), DLC has 'ac' (ux0:addcont/<TITLEID>/<id>); only 'gd' is a game. Also not games: Vita3K's patch/ folder, shader caches and per-game configs (cache/ and config/ under the pref path, keyed by TITLEID), sce_sys/trophy, savedata (ux0:user/00/savedata), sce_pfs, sce_sys/package (head.bin/work.bin), and VitaCheat/taiHEN config folders (ur0:tai).

**Confidence.** high, checked against Vita3K source: the patch folder (<pref/shared path>/patch/), the per-title .txt and patchlist.txt rules, PKG content_type 0x15/0x16/0x1F, theme install path, and zRIF/work.bin handling. High for the rePatch/reAddcont layout. Medium for whether Vita3K honours rePatch natively (no code found; merge files instead), and for the exact cache/config file names.

**Sources:**
- <https://raw.githubusercontent.com/Vita3K/Vita3K/master/vita3k/packages/src/pkg.cpp>
- <https://raw.githubusercontent.com/Vita3K/Vita3K/master/vita3k/patch/src/patch.cpp>
- <https://raw.githubusercontent.com/Vita3K/Vita3K/master/vita3k/app/src/app_init.cpp>
- <https://lavaforge.org/preserve-emulation/Vita3K/wiki/Patches>
- <https://gamebrew.org/wiki/RePatch_reLoaded_Vita>
- <https://consolemods.org/wiki/Vita:Applying_ROM_Patches>
- <https://docs.shrinefox.com/getting-started/persona-4-golden-psvita-mod-support>

## PS4 (shadPS4)

**Mods and texture packs.** For folder-format games, mods replace files in the game folder. For Bloodborne these are dvdroot_ps4/ subfolders such as sfx, map, parts. Community mod enablers (JSGME) or the Vortex extension stage mods in dvdroot_ps4/MODS/<mod name>/ and then copy them in; removing a mod means restoring the original files. shadPS4 itself has no mod overlay folder (none in path_util.h); user/custom_modules/ is for replacement system modules, not game mods. On console: GoldHEN plugins (.prx in /data/GoldHEN/plugins/) and the AFR (Application File Redirector) plugin, which reads /data/GoldHEN/AFR/<CUSA>/. To identify a mod: a zip mirroring the game's internal paths, a CUSA ID in the name or readme, or a per-game Nexus page. Updates are not mods: PKG content_type 0x1E (DP) or param.sfo CATEGORY 'gp', or a sibling folder <CUSA>-UPDATE / <CUSA>-patch next to the game folder. DLC is 'ac'.

**Translation patches and ROM hacks.** Fan translations ship as replacement files for folder-format dumps, as fake PKG updates (CATEGORY gp), or as GoldHEN/shadPS4 XML patches. Some are xdelta patches for specific archives. None use checksum-headed formats. The target comes from the CUSA/PLJM/PCJS serial plus the required APP_VER stated in the readme or in the XML Metadata AppVer.

**Cheats and emulator patches.** shadPS4 uses the shared ps4_cheats repo, which GoldHEN also uses. Cheats are JSON named SERIAL_VERSION.json (e.g. CUSA07023_01.03.json, with an optional _2 suffix for extra files for the same version). Fields: name, id, version, process (e.g. eboot.bin), mods[] (name, hint, type checkbox|button, memory[{offset,on,off}]), credits. Each file is listed in CHEATS_JSON.txt as '<file>.json=<game name>'. Patches are XML named after the game title (e.g. GravityRush2.xml): a <Patch> root, <TitleID><ID>… listing the serials, and <Metadata Title Name Author PatchVer AppVer AppElf isEnabled> with <PatchList><Line Type Address Value>. Several authors can share one file. shadPS4 downloads both into user/cheats/ and user/patches/ (path_util.h CHEATS_DIR="cheats", PATCHES_DIR="patches"). The user dir is %APPDATA%\shadPS4, ~/Library/Application Support/shadPS4 or ~/.local/share/shadPS4, or ./user in portable mode. On console, GoldHEN keeps cheats in /data/GoldHEN/cheats/{json,shn,mc4}/ and patches in /data/GoldHEN/patches/xml/. Key = CUSA serial plus APP_VER from param.sfo.

**Official extras.** PS4 PKGs have magic '\x7FCNT' and a plaintext ContentID at header 0x40. content_type values: 0x1A GD (game), 0x1B AC (add-on), 0x1C AL (additional licence), 0x1E DP (delta patch); 0x1F is uncertain. The param.sfo and icon0.png entries inside a PKG can be read without keys (PS4 PKG Tool does this). Dynamic/static themes and avatars ship as add-on PKGs and have no unique content_type. Tell them apart by param.sfo TITLE/CATEGORY text ('Theme', 'Avatar'), the ContentID label, and the absence of game data. Soundtracks are usually add-ons (AC) or separate music apps. Games have no on-disc e-manuals; manuals are online.

**Licences.** Real hardware: .rif files in /user/license/ and act.dat; fake PKGs carry fake licences. shadPS4 mostly runs decrypted dumps (a folder with eboot.bin + sce_sys/), but recent builds also create a user/licenses/ folder (path_util.h LICENSES_DIR). Recognise licence files by the *.rif extension, a license/ or licenses/ path, or act.dat. Never parse them.

**Not games (firmware, caches, configs).** Firmware: PS4UPDATE.PUP (on discs as PS4UPDATE/PS4UPDATE.PUP; there is also a recovery PUP). shadPS4 user-folder entries that are not games (path_util.h): sys_modules (optional LLE firmware modules), custom_modules, custom_configs (per-game .toml), shader, cache, data, game_data (metadata), temp, log, screenshots, captures, download, fonts, trophy, custom_trophy, home (user/saves), licenses, cheats, patches. Inside games: sce_sys/trophy, sce_sys/keystone, sce_sys/nptitle.dat. <CUSA>-UPDATE/-patch folders and 'ac'/'gp' PKGs are not games.

**Confidence.** high for the cheat/patch formats (ps4_cheats README) and the shadPS4 user-folder names (path_util.h). High for PS4 PKG content_type 0x1A/0x1B/0x1C/0x1E (SocraticBliss PKG template; 0x1F is marked uncertain there). No native shadPS4 '<CUSA>-MODS' overlay was found, so that claim is dropped. Medium for how to tell themes and avatars apart.

**Sources:**
- <https://github.com/shadps4-emu/ps4_cheats/blob/main/README.md>
- <https://raw.githubusercontent.com/shadps4-emu/shadPS4/main/src/common/path_util.h>
- <https://raw.githubusercontent.com/shadps4-emu/shadPS4/main/src/common/path_util.cpp>
- <https://gist.github.com/SocraticBliss/094077a377b67a3947c3b46da808fcaa>
- <https://consolemods.org/wiki/PS4:Using_Cheats_Patches_and_Plugins>
- <https://www.nexusmods.com/bloodborne/mods/64>
- <https://shadps4.net/quickstart/>

## PS5

**Mods and texture packs.** With no emulator there is no mod folder convention. Jailbreak homebrew allows file replacement in decrypted folder dumps and some per-game plugins via etaHEN. Identify a mod by a PPSA serial in its name or readme. In practice, store these as user mods linked by PPSA ID.

**Translation patches and ROM hacks.** Rare. When they exist they are replacement files for folder dumps, keyed by PPSA serial and version in the readme. None use checksum-headed formats.

**Cheats and emulator patches.** No emulator cheats. On jailbroken consoles, etaHEN uses GoldHEN-style cheat formats (.json/.shn/.mc4), reportedly under /data/etaHEN/cheats/ (unverified), keyed by PPSA/CUSA serial plus version. PS4 titles running on PS5 use the PS4 cheat files.

**Official extras.** PS5 has no installable custom themes and no avatar packages (avatars are bought into the account). Soundtracks and digital artbooks are add-on content, sometimes standalone apps. Decrypted dumps carry sce_sys/param.json (plaintext JSON with fields such as titleId, contentId, contentVersion, applicationCategoryType and localized titles; field names unverified), which can tell a game from an add-on without keys. Retail PS5 PKGs are encrypted beyond the header.

**Licences.** Real hardware licences (.rif, act.dat) cannot be used. Treat any license/ path, .rif or act.dat as a licence and never read it. Coverly never needs them.

**Not games (firmware, caches, configs).** Firmware: PS5UPDATE.PUP. Also not games: sce_sys/trophy2, savedata, etaHEN/kstuff config folders, and PS4 backward-compatible titles (CUSA) mixed in. 'ac'-type add-ons and patches are not games.

**Confidence.** low-medium. There is no working PS5 emulator as of 2026-11. These points cover homebrew dump structure and etaHEN conventions and were not verified against primary sources (psdevwiki blocks automated fetches). The etaHEN cheat path and the param.json field names in particular need checking.

**Sources:**
- <https://www.psdevwiki.com/ps5/>
- <https://consolemods.org/wiki/PS4:Using_Cheats_Patches_and_Plugins>

## Xbox 360 (Xenia Canary)

**Mods and texture packs.** Xenia Canary supports plugins/trainers. With the allow_plugins cvar set (default false), it loads plugins/<TITLEID as 8 hex digits>/plugins.toml. That file holds title_name, title_id and [[plugin]] entries, each with hash(es), an enable flag and a .xex file (plugin_loader.cc). Other mods are file replacements on extracted (XEX folder) games or edited STFS DLC packages. Title updates are not mods: they are STFS packages with content type 0x000B0000 (Installer/TU) in content/<profile-or-0>/<TITLEID>/000B0000/. To identify a mod: a TITLEID in the filename or readme, a plugins/<TITLEID>/ structure, or a layout mirroring the game folder.

**Translation patches and ROM hacks.** Usually xdelta against the ISO or specific files (default.xex, archives), or replacement files for extracted folders. Sometimes a Xenia patch.toml for small text fixes. None use BPS/UPS checksum headers. The target comes from the TITLEID plus the media_id/Redump disc hash cited in the readme or toml.

**Cheats and emulator patches.** Xenia Canary patches are files named '<TITLEID> - <Game Title>.patch.toml' (e.g. '4D5307DF - Blue Dragon.patch.toml') in a 'patches' folder next to xenia_canary.exe. PatchDB loads <root>/patches/; patch_db.cc requires title_name, title_id and hash. TOML fields: title_name; title_id (uppercase hex); hash (uppercase module hash of default.xex etc., can be an array); optional media_id (can be an array, optionally with a Redump link). Each patch is a [[patch]] block with name, author, optional desc and is_enabled, plus [[patch.be8|be16|be32|be64|f32|f64|string|u16string|array]] entries with address and value. The apply_patches cvar defaults to true; the title bar shows '[Patches Applied]' when patches load. Key = title_id plus module hash (the hash is shown as 'Module hash:' in xenia.log), which pins the exact title-update version. There is no separate cheat engine; patches serve as cheats. The older emoose fork used '<TITLEID>.patch'.

**Official extras.** Everything is an STFS package ('CON '/'LIVE'/'PIRS' magic). The plaintext metadata has content type at 0x344 (4 bytes), title ID at 0x360 and display name at 0x411 (0x900 bytes, 0x80 per locale). Packages can be classified without keys. Content types (Free60): 0x00000001 saved game, 0x00000002 Marketplace content (DLC), 0x00000003 publisher, 0x00001000 Xbox 360 title, 0x00004000 installed game, 0x00005000 original Xbox game, 0x00007000 Games on Demand, 0x00009000 avatar item, 0x00010000 profile, 0x00020000 gamer picture, 0x00030000 theme, 0x00040000 cache file, 0x00080000 game demo, 0x00090000 video, 0x000A0000 game title, 0x000B0000 installer/title update, 0x000C0000 game trailer, 0x000D0000 Arcade title (XBLA), 0x000E0000 XNA, 0x000F0000 License Store, 0x00100000 movie, 0x00200000 TV, 0x00300000 music video, 0x00400000 game video, 0x00500000 podcast video, 0x00600000 viral video, 0x02000000 community game. On-device layout is Content/<profile or 0000000000000000>/<TITLEID>/<CONTENTTYPE>/<file>, so the folder name also gives the type. Themes, gamer pictures and avatar items go to Extras; Arcade (0x000D0000), GoD (0x00007000) and demos are games; videos, trailers and music videos go to Extras/Videos. There are no e-manuals.

**Licences.** There are no standalone licence files like .rap. STFS packages (LIVE/PIRS) carry licence entries inside their header; Xenia ignores them or overrides them via the license_mask cvar. Content type 0x000F0000 (License Store) packages and console KV/keyvault files (kv.bin) are sensitive: classify them as licence/system and never read them.

**Not games (firmware, caches, configs).** System updates: the $SystemUpdate folder on discs/USB (su20076000_00000000 and similar). Xenia data that is not games: per-game configs config/<TITLEID>.config.toml, shader/pipeline caches (cache/, cache_host/), xenia.log, the patches/ and plugins/ folders. STFS types that are not games: profile (0x00010000), saved game (0x00000001), cache file (0x00040000), License Store (0x000F0000), title updates (0x000B0000). Also kv.bin.

**Confidence.** high, checked against xenia-canary source: patch.toml format and location, the apply_patches cvar (default true), and the plugin loader (allow_plugins, plugins/<TITLEID>/plugins.toml). High for the STFS header offsets and the content-type list (Free60 table, re-read). Medium for the display-name encoding: Free60 lists it as a string with 0x80-byte locale slots, and tools read it as UTF-16BE.

**Sources:**
- <https://github.com/xenia-canary/game-patches>
- <https://raw.githubusercontent.com/xenia-canary/xenia-canary/canary_experimental/src/xenia/patcher/patch_db.cc>
- <https://raw.githubusercontent.com/xenia-canary/xenia-canary/canary_experimental/src/xenia/patcher/plugin_loader.cc>
- <https://free60.org/System-Software/Formats/STFS/>
- <https://gist.github.com/InvoxiPlayGames/b7ee7483876efe606f8ed4a92b7c780f>
- <https://github.com/emoose/game-patches>

## Xbox One

**Mods and texture packs.** No file mods. Some games (Fallout 4, Skyrim SE) offer in-game Bethesda.net/Creations mods that live inside the game's storage. Dev Mode UWP apps (.appx/.msix/.appxbundle, e.g. RetroArch UWP) are homebrew apps, not games or mods. For Coverly: user mods only, linked manually.

**Translation patches and ROM hacks.** none (no accessible format; fan translations are effectively nonexistent).

**Cheats and emulator patches.** none (no emulator, no cheat format).

**Official extras.** No installable theme or gamerpic packages (backgrounds are user images; gamerpics are cloud-side). Avatars are managed by an app. Soundtracks and artbooks are sold as add-ons inside encrypted XVCs and cannot be identified offline. Dev Mode .appx packages have a plaintext AppxManifest.xml (Identity Name, DisplayName), which can classify them as apps.

**Licences.** Licences live in the console's encrypted licence store and in XVC packages; there are no user-handleable licence files. Classify any .xvd/.xvc/licence blobs as system files and do not read them.

**Not games (firmware, caches, configs).** Offline system update (OSU1/OSU2/OSU3 zip containing a $SystemUpdate folder with .xvd files). Also not games: external-drive 'MSXC' folders, .xvd/.xvc containers, and the Connected Storage cache. None of these are games for Coverly unless the user adds them manually.

**Confidence.** low-medium. There is no working Xbox One emulator, and XVC/MSIXVC packages are encrypted. Based on general knowledge, not verified against primary docs in this pass (the cited wikis mostly cover Xbox/360).

**Sources:**
- <https://free60.org/>
- <https://xboxdevwiki.net/>
