# Fact-checked report: how emulators and tools handle user-supplied console keys, and the legal background for metadata-only use

Date: 2026-11-01. This is not legal advice. The report contains no key values and names no sources for keys. Claims marked **[verified]** were checked against the primary source in this session. Claims marked **[secondary]** rest on press or third-party pages only. Claims marked **[corrected]** were wrong or overstated in the draft.

## Corrections made to the draft

1. **[corrected] "Only Dolphin and Azahar carry key material in their own source" is false.**
   - RPCS3 embeds key constants in `rpcs3/Crypto/key_vault.h`, for example `PKG_AES_KEY` and `PKG_AES_KEY_VITA_1`/`_2`/`_3`.
   - Vita3K embeds PKG keys in `vita3k/packages/include/packages/pkg.h` (`pkg_vita_2`, and others used by `pkg.cpp`).
   - Sources: [key_vault.h](https://github.com/RPCS3/rpcs3/blob/master/rpcs3/Crypto/key_vault.h), [pkg.h](https://github.com/Vita3K/Vita3K/blob/master/vita3k/packages/include/packages/pkg.h)
   - Only the Switch tools (Ryubing/LibHac and hactool) and Cemu ship no real keys.
2. **[corrected] Dolphin and Steam.** The draft said "cease-and-desist, not a DMCA takedown" and quoted "not authorized by Nintendo". Both need changing.
   - Valve contacted Nintendo of America. A Nintendo lawyer then asked Valve to block the release, citing the DMCA anti-circumvention provisions.
   - Dolphin's statement called this "a cease and desist citing the DMCA". Others described it as a DMCA opinion, not a formal takedown.
   - The wording actually quoted is "incorporating these cryptographic keys without Nintendo's authorization".
   - Dolphin argued the key does not change its §1201(f) position, and kept it.
   - Sources [secondary]: [TheSixthAxis 2023-05-30](https://www.thesixthaxis.com/2023/05/30/dolphin-emulator-steam-release-blocked-by-nintendo-dmca-notice/), [TheSixthAxis 2023-07-20](https://www.thesixthaxis.com/2023/07/20/heres-what-really-happened-to-dolphin-steam-release/), [Dolphin blog](https://dolphin-emu.org/blog/2023/07/20/what-happened-to-dolphin-on-steam/) (HTTP 403 here)
   - The key is still in source today **[verified]**, see section 1.
3. **[corrected] Ryubing key-path order.**
   - `~/.switch` (`KeysDirPathUser`) is checked only when `AppDataManager.Mode == LaunchMode.UserProfile`.
   - `<data>/system` is checked next. Because later matches overwrite earlier ones, a file in `system/` wins.
4. **[corrected] 3DS "content" group key slots.**
   - When ncchflag[3] = 0x00, the content group also uses slot 0x2C, the same key as the primary key.
   - 0x25, 0x18 and 0x1B apply only when the flag is 0x01, 0x0A or 0x0B.
5. **[corrected] "Needs a single keyX slot" is incomplete.**
   - Deriving the normal key in software also needs the AES-engine scrambler constant. Azahar loads it as `generatorConstant` from the key file (`key.cpp`, lines 31-37, 116, 217).
6. **[corrected] nxdumptool.**
   - The repo is now **archived**, and its default branch is `rewrite`. The cited `main` README still resolves.
   - The README explicitly links a key-generation tool by name. So "no links" is not a universal norm in the ecosystem; it is a choice our project should make.
   - 37,473 entries in total.
   - `key` is non-null in 0 entries.
   - `rightsId` is set in 2,777 entries.
   - `iconUrl` is set in 19,601 entries, all on `img-eshop.cdn.nintendo.net`.
   - **Only 29,774 entries have an `id` (titleId).** The other 7,699 cannot be used for a titleId lookup.
8. **[corrected] Tinfoil context.** The titledb README does not mention Tinfoil, so the draft's "according to the README" is wrong. If the claim stays, it is reputational and unsourced.
9. **[corrected] RPCS3 citations.**
   - PR #10581 is "Fully implement sceNpDrmVerifyUpgradeLicense", which is not a source for paths. It is replaced by source code.
   - `unself.cpp` (around line 1500) says: "Ensure the .rap license file is placed in the dev_hdd0/home/%s/exdata folder with a lowercase file extension." [unself.cpp](https://github.com/RPCS3/rpcs3/blob/master/rpcs3/Crypto/unself.cpp)
   - `main_window.cpp` `InstallFileInExData` copies files to `home/<usr>/exdata/`.
10. **[corrected] Yuzu "or any similar software".**
    - This is Engadget's paraphrase, not quoted order text.
    - TorrentFreak describes the injunction as covering Yuzu and "any other software or device that circumvents Nintendo's technical protection measures", including through Nintendo's cryptographic keys.

## 0. Premise check (3DS / Azahar): confirmed

- **Release notes [verified].** Azahar 2120-rc1 says: "Support for encrypted applications has been dropped. Applications must now be dumped in a decrypted format to be used." It also drops `.3ds` "for historical reasons related to the origin of the extension", in favour of `.cci`. The stable 2120 was published on 2025-03-21. [Release 2120-rc1](https://github.com/azahar-emu/azahar/releases/tag/2120-rc1)
- **Loader [verified].** `ncch_container.cpp` (lines 261-263 and 292-294) returns `ErrorEncrypted` when `!ncch_header.no_crypto`, with the comment "Encrypted NCCH are not supported".
  - `DumpRomFS` (line 678) refuses `CryptoFile` titles with the message "built-in romfs dumping of eShop titles is not allowed".
  - Azahar does open its own console-unique "CryptoFile" wrapper (lines 171-174). That is Azahar's own container format, not Nintendo NCCH crypto.
  - [ncch_container.cpp](https://github.com/azahar-emu/azahar/blob/master/src/core/file_sys/ncch_container.cpp)
- **Game list [verified].** `game_list_worker.cpp` (lines 62 and 100) still lists files that return `ErrorEncrypted` and passes an encrypted flag. [game_list_worker.cpp](https://github.com/azahar-emu/azahar/blob/master/src/citra_qt/game_list_worker.cpp)
- **Key file [verified].** `common_paths.h:83` has `#define KEYS_FILE "keys.txt"`. `key.cpp:293` builds the path as `GetUserPath(SysDataDir) + KEYS_FILE`.
- **Built-in blob [verified].**
  - `CMakeLists.txt:162` has `option(ENABLE_BUILTIN_KEYBLOB "Enable the inclusion of the default crypto keys blob" ON)`.
  - In `key.cpp:302-312`, the blob is used only when `keys.txt` cannot be opened. The comment reads "The key data is encrypted in the source to prevent easy access to it for unintended purposes."
- **Legacy Citra wiki [verified].**
  - It documents `sysdata/aes_keys.txt`: "These keys are not officially provided by Citra due to legal considerations."
  - The Azahar disclaimer reads: "We cannot provide information on dumping AES keys."
  - [citra.azahar-emu.org/wiki/aes-keys](https://citra.azahar-emu.org/wiki/aes-keys/)
- **Fork [secondary].** AzaharPlus keeps encrypted-game support and is not official.

## 1. Per emulator or tool

| Tool | What it reads | Where | Ships keys in source? | Source |
|---|---|---|---|---|
| **Azahar (3DS)** | `keys.txt`, which overrides the built-in blob. Encrypted NCCH is rejected. | `<user>/sysdata/keys.txt` | **Yes**: an obfuscated blob, default ON | Section 0 [verified] |
| **Citra (legacy)** | `aes_keys.txt`, plus `seeddb.bin` | `<user>/sysdata/` | Not provided "due to legal considerations" | [Citra wiki](https://citra.azahar-emu.org/wiki/aes-keys/) [verified] |
| **Ryubing (Switch)** | `prod.keys`, `title.keys`, `console.keys`, `dev.keys` | `~/.switch/` (UserProfile mode only), then `<data>/system/`, which wins | **No.** `KeySet.CreateDefaultKeySet()` fills from `DefaultKeySet`, and every array in the public `DefaultKeySet.Empty.cs` is `Array.Empty<byte>()`. | [VirtualFileSystem.cs](https://git.ryujinx.app/ryubing/ryujinx/src/branch/master/src/Ryujinx.HLE/FileSystem/VirtualFileSystem.cs) lines 222-252, [AppDataManager.cs](https://git.ryujinx.app/ryubing/ryujinx/src/branch/master/src/Ryujinx.Common/Configuration/AppDataManager.cs) lines 17, 47, 229, [LibHac DefaultKeySet.Empty.cs](https://git.ryujinx.app/projects/LibHac/src/branch/master/src/LibHac/Common/Keys/DefaultKeySet.Empty.cs) [verified] |
| **Ryujinx (original)** | Same files | `system` folder | No | [old wiki](https://github.com/Ryujinx/Ryujinx/wiki/Keys) (404) [secondary] |
| **hactool** (ISC [verified]) | A text keyset with lines of the form `key_name = HEX` | `-k/--keyset`, otherwise `$HOME/.switch/prod.keys` (or `dev.keys` with `-d`) | No. KEYS.md lists key names and where their sources are found, but gives no values. | [README](https://github.com/SciresM/hactool/blob/master/README.md) lines 104-109, [KEYS.md](https://github.com/SciresM/hactool/blob/master/KEYS.md) [verified] |
| **nxdumptool** (GPL-3.0, archived) | Gamecard operations need no keys file; it collects keys at runtime from console memory. SD/eMMC title operations need `sdmc:/switch/prod.keys`. | On the console's SD card | No | [README](https://github.com/DarkMatterCore/nxdumptool/blob/main/README.md): line 44, v1.1.0 notes, credits line 57 [verified] |
| **Cemu (Wii U)** (MPL-2.0) | `keys.txt` for WUD/WUX disc crypto. `#` or `;` starts a comment. "the emulator will automatically pick the right key". | `ActiveSettings::GetUserDataPath("keys.txt")`. A template is created if the file is missing. | No real keys. The template has one line labelled "example key (can be deleted)". | [KeyCache.cpp](https://github.com/cemu-project/Cemu/blob/main/src/Cafe/Filesystem/FST/KeyCache.cpp) lines 63-91 [verified] |
| **Dolphin (GC/Wii)** | Wii: optional `keys.bin` | `GetUserPath(D_WIIROOT_IDX) + "keys.bin"` | **Yes.** `LoadDefaultEntries()` sets `HANDLE_COMMON_KEY`. Without `keys.bin` it logs "keys.bin could not be found. Default values will be used." | [IOSC.cpp](https://github.com/dolphin-emu/dolphin/blob/master/Source/Core/Core/IOS/IOSC.cpp) lines 210-211, 557-632 [verified] |
| **RPCS3 (PS3)** (GPL-2.0-only for most files, per the README) | `.rap` and `.edat` licence files, plus the official firmware | `dev_hdd0/home/<user>/exdata/` | **Yes**: PKG/NPDRM-related constants in `key_vault.h` | [unself.cpp](https://github.com/RPCS3/rpcs3/blob/master/rpcs3/Crypto/unself.cpp), [key_vault.h](https://github.com/RPCS3/rpcs3/blob/master/rpcs3/Crypto/key_vault.h) [verified] |
| **Vita3K (PS Vita)** (GPL-2.0) | `.pkg` plus a zRIF or `work.bin`; decrypted dumps; firmware | Through the File menu, or `ux0/app` | **Yes**: PKG keys in `pkg.h`. The docs say: "Vita3K does not condone piracy, therefore, you are required to dump your own games." | [quickstart](https://vita3k.org/quickstart.html), [pkg.h](https://github.com/Vita3K/Vita3K/blob/master/vita3k/packages/include/packages/pkg.h) [verified] |

**Common pattern (corrected).**
- The Nintendo Switch and Wii U tools take user-supplied key files from fixed per-user paths and ship no keys.
- Dolphin, Azahar, RPCS3 and Vita3K embed some key material.
- Of these, Dolphin has drawn an explicit Nintendo objection (Steam, 2023).
- None of the tools checked downloads keys.

## 2. Where metadata lives and which keys it needs

**3DS NCCH (CCI/CXI)** [verified] [3dbrew NCCH](https://www.3dbrew.org/wiki/NCCH)
- Unencrypted fields:
  - The ExHeader, ExeFS and RomFS are AES-CTR encrypted "unless the NoCrypto flag is set in ncchflag[7]".
  - The ncchflag[7] bit masks are FixedCryptoKey = 0x1, NoMountRomFs = 0x2, NoCrypto = 0x4, and 0x20 for the new keyY generator (seed).
- Key groups:
  - The "menu info" group (ExHeader, ExeFS header, and the ExeFS files `icon` and `banner`) uses the primary key.
  - The primary key comes from keyX slot 0x2C plus a keyY taken from the first 0x10 bytes of the NCCH signature.
  - The "content" group uses keyX 0x2C, 0x25, 0x18 or 0x1B, selected by ncchflag[3] values 0x00, 0x01, 0x0A or 0x0B.
- The normal key is generated by the AES engine scrambler. A software implementation also needs the generator constant (see Azahar `key.cpp`).
- Seed crypto (flag 0x20) affects only the secondary key, so it does not touch the icon. This is an inference from the 3dbrew text.
- Consequence: NoCrypto or decrypted dumps need no keys for name and icon. That is exactly what Azahar now requires anyway.

**3DS CIA** [verified] [3dbrew CIA](https://www.3dbrew.org/wiki/CIA)
- The contents are AES-CBC encrypted with the decrypted title key from the ticket.
- An optional Meta section ("Not a necessary component") sits after the content. It holds "Icon Data (.ICN) – Taken from the application's ExeFS" at offset 0x400, with size 0x36C0. The header field at 0x14 gives the Meta size, which is 0 when the section is absent.
- Inference, to test on real files: with a Meta section, name and icon can be read without keys. Without one, an encrypted CIA may carry two layers of encryption, CIA title-key crypto and NCCH crypto.

**Switch NCA** [verified] [switchbrew NCA](https://switchbrew.org/wiki/NCA)
- The header and section data:
  - "The first 0xC00 bytes are encrypted with AES-XTS".
  - "The only known area which is not encrypted in the raw NCA is the logo section".
- The fields that select the keys:
  - ContentType 0x02 is Control, which holds the NACP and icons.
  - KeyAreaEncryptionKeyIndex is 0 for Application, 1 for Ocean, 2 for System.
  - KeyGeneration and RightsId (header offset 0x230) are also in the header.
- In hactool terms, this needs `header_key` plus `key_area_key_application_xx`, or a titlekey derived through `titlekek_xx` ([KEYS.md](https://github.com/SciresM/hactool/blob/master/KEYS.md)).
- Keyless titleId sources (inference, not verified on real files):
  - In an NSP, the outer PFS0 table is plaintext. Ticket files are named after the rightsId, and by community convention the first 8 bytes of a rightsId are the titleId.
  - In an XCI, the gamecard header exposes a PackageId, not a titleId ([switchbrew XCI](https://switchbrew.org/wiki/XCI)). In practice, keyless XCI identification usually depends on the filename (`[0100…]` tags).
- Consequence: there is no keyless path to the NACP. The keyless route is titleId (from the filename or a ticket name) plus a catalogue lookup.

## 3. Legal background

**US: DMCA 17 U.S.C. §1201** [Cornell LII](https://www.law.cornell.edu/uscode/text/17/1201)
- §1201(a)(1)(A) prohibits circumventing an access control.
- §1201(a)(2) and (b)(1) prohibit trafficking.
- §1201(f) allows circumvention for interoperability. The citation is standard; the text was not re-fetched in this session.

**Nintendo of America v. Tropic Haze (Yuzu), No. 1:24-cv-00082 (D.R.I.)**
- Filed on 2024-02-26 [secondary, TorrentFreak].
- The complaint alleges that the Yuzu developers "encouraged Yuzu users to obtain their own encryption keys", knowing the keys could come only from Switch consoles [secondary, TorrentFreak].
- Joint final judgment and permanent injunction (Doc. 11, March 2024), [secondary]:
  - $2.4M payment.
  - A ban on offering or trafficking in Yuzu and other software that circumvents Nintendo's TPMs.
  - Deletion of "all circumvention devices, tools and Nintendo cryptographic keys".
  - Surrender of the domain, "including any variants or successors".
  - An irrevocable waiver of appeal.
  - Sources: [DocketAlarm #11](https://www.docketalarm.com/cases/Rhode_Island_District_Court/1--24-cv-00082/Nintendo_of_America_Inc._v._Tropic_Haze_LLC/11/), [Engadget](https://www.engadget.com/makers-of-switch-emulator-yuzu-quickly-settle-with-nintendo-for-24-million-203204698.html), [TorrentFreak](https://torrentfreak.com/nintendos-yuzu-lawsuit-is-all-but-done-price-2-4m-cost-to-emulation-tbd-240305/)
- It is a consent judgment, so it sets no precedent on the merits.

**Citra** [secondary]
- Taken offline in March 2024 alongside Yuzu because they shared developers. There was no separate suit. [PC Gamer](https://pcgamer.com/software/nintendo-3ds-emulator-citra-taken-offline-as-collateral-damage-in-yuzu-settlement/), [GameSpot](https://gamespot.com/articles/nintendo-lawsuit-shuts-down-switch-emulator-yuzu-and-3ds-emulator-citra/1100-6521589/)
- Azahar's move to decrypted-only content is plausibly related. That is my inference; the release notes give no reason.

**Ryujinx** [secondary]
- Went offline on 2024-10-01 after a reported agreement with Nintendo whose terms are undisclosed. [PC Gamer](https://www.pcgamer.com/gaming-industry/switch-emulator-ryujinx-goes-offline-after-creator-gets-an-offer-from-nintendo-they-cant-refuse)

**Dolphin and Steam (2023)**
- See correction 2.
- Lesson: an embedded key was the explicit hook of Nintendo's objection, and Valve required Nintendo's agreement before it would list the emulator.

**EU** (texts not fetched in this session; standard citations)
- Directive 2001/29/EC, Art. 6 bans circumvention and trafficking.
- CJEU C-355/12 *Nintendo v PC Box* (2014-01-23):
  - Hardware and software encryption can be an effective technological measure.
  - Legal protection must be proportionate and aimed at preventing unauthorised acts.
  - National courts weigh the device's purpose and its actual lawful versus infringing use.
  - Sources [secondary]: [CURIA press release 9/14](https://curia.europa.eu/jcms/jcms/P_117967/nl), [SCL](https://www.scl.org/3014-game-over-for-excessive-tpm-the-nintendo-ruling/)
- Directive 2009/24/EC, Art. 6 allows decompilation for interoperability under strict conditions.

## 4. The Switch catalogue (blawar/titledb) [verified via the GitHub API]

- **Licence and status.** The licence is MIT, "Copyright (c) 2019 Blake Warner". The repo is not archived, was last pushed on 2026-10-16, and its size is about 45 GB (`size` = 45,499,924 KB).
- **README.** It says "The repository provides metadata only". It also lists `cheats.json`, `cnmts.json`, `ncas.json`, `versions.json` and `versions.txt`, links three historical snapshot repos, and notes that `titles.US.en.json` was removed.
- **`US.en.json` fields.** The file is keyed by nsuId and has these fields: `bannerUrl`, `category`, `description`, `developer`, `frontBoxArt`, `iconUrl`, `id`, `intro`, `isDemo`, `key`, `languages`, `name`, `nsuId`, `numberOfPlayers`, `publisher`, `rating`, `ratingContent`, `region`, `releaseDate`, `rightsId`, `screenshots`, `size`, `version`. Counts are in correction 7.
- **Practical gaps.**
  - About 20% of entries have no titleId.
  - One region/language file does not cover every title, so a build should merge several region files and dedupe by `id`.
- **Licensing of contents.**
  - MIT covers the compilation only.
  - Descriptions, screenshots and icons are Nintendo's and publishers' material. Icon URLs point at Nintendo's CDN.
- **Safer use.**
  - Build a minimal map at build time, `id` to `name`, `publisher` and `region`.
  - Drop `key`, `rightsId`, `cnmts`, `ncas` and `cheats`.
  - Include the MIT notice.
  - Do not redistribute images. If icons are wanted, fetch them on demand, opt-in.
  - Pin to a commit, because the data changes often.
- **Reputational context.** The author is widely associated with Tinfoil. This is unsourced here; the README does not say it.

## 5. Practices that look safer for a metadata-only feature

These are a synthesis of the sources above, not legal advice.

1. **Keyless first.**
   - 3DS: NoCrypto or decrypted NCCH, and the CIA Meta section.
   - Switch: titleId from the filename or ticket name, then the catalogue.
   - Anything unresolved gets a plain "encrypted" label, as the Azahar game list does.
2. **Never bundle keys,** in any form, obfuscated or as default values.
   - Several emulators do bundle keys (Dolphin, Azahar, RPCS3, Vita3K).
   - The only documented platform-level consequence is Dolphin and Steam, where the key was the explicit hook. So "others do it" offers no protection.
3. **Never download keys** or any file that carries them. Strip the catalogue's `key` and `rightsId` fields at build time.
4. **Never link to, name or describe key sources or how to dump keys.**
   - Azahar removed the dumping steps from the Citra wiki.
   - nxdumptool did link a key tool, which shows this is a choice, and the conservative one is not to link.
   - The Yuzu complaint relied on the developers "encouraging" users to obtain keys.
5. **User-supplied only, read in place,** through an explicit file picker. The known locations can be offered as hints: `~/.switch/prod.keys`, Ryubing's `system/`, Azahar's `sysdata/keys.txt`. Do not copy the file into the app.
6. **Opt-in and off by default,** with copy that states the purpose ("only to read the name and icon").
7. **Local only.** Keys stay in memory. Never send them over the network, log them, include them in crash reports or telemetry, or sync them.
8. **Minimal decryption.**
   - Decrypt only the NCA header and the Control NCA's NACP and icon, or the 3DS menu-info group (keyX 0x2C plus the generator constant).
   - Never write decrypted data to disk, and offer no export or decrypt function.
   - Store only the name and icon.
9. **No circumvention features.** No conversion, installation or ticket handling.
10. **Documentation framed the way Vita3K and the Citra wiki do it,** for example "dump your own games", and never mention piracy.

**Residual risk.**
- Even metadata-only decryption technically circumvents an access control.
- §1201(f) and the 2009/24 Art. 6 exception are narrow, and no court has ruled on a metadata-only case.
- The lowest-risk design is keyless parsing plus the catalogue, with the user-keys mode as an opt-in extra. For 3DS this is moot in practice, because Azahar itself requires decrypted dumps.

**Working files.**
