# Cabeceras en claro para clasificar ejecutables y archivos sueltos de consola (versión verificada)

Fecha: 2026-11-01. Revisé cada afirmación contra la fuente primaria: wikitext crudo de switchbrew, wiibrew, wiiubrew, 3dbrew y xboxdevwiki; código actual en `master` de Dolphin, Xenia, decaf-emu, PPSSPP, scetool, vita-toolchain, psvimgtools y Azahar; la API de GitHub; y el PDF de la sentencia de Yuzu en CourtListener. psdevwiki y azahar-emu.org/blog devuelven 403. **[inferencia]** marca lo que deduzco y no aparece literalmente en la fuente.

## 0. Correcciones al informe anterior

| # | Antes | Ahora | Fuente |
|---|---|---|---|
| C1 | Azahar exige `keys.txt` para juegos cifrados y no trae claves | **Falso hoy.** Azahar rechaza todo NCCH cifrado (`ErrorEncrypted`) desde el commit "Remove encrypted game support" (2025-02-27). Además compila por defecto un bloque de claves incluido (`ENABLE_BUILTIN_KEYBLOB` ON); `keys.txt` solo lo sustituye. | [ncch_container.cpp](https://github.com/azahar-emu/azahar/blob/master/src/core/file_sys/ncch_container.cpp), [CMakeLists.txt L162](https://github.com/azahar-emu/azahar/blob/master/CMakeLists.txt), [key.cpp](https://github.com/azahar-emu/azahar/blob/master/src/core/hw/aes/key.cpp), commits dc1ebb63c y 336d871a7 |
| C2 | Que el usuario aporte `prod.keys` es el modelo seguro | **Es exactamente el modelo de Yuzu**, y la sentencia lo declara infracción (ver §10) | [Sentencia 1:24-cv-00082 D.R.I.](https://storage.courtlistener.com/recap/gov.uscourts.rid.56980/gov.uscourts.rid.56980.10.1.pdf) |
| C3 | Icono del NRO: JPEG **[inferencia]** | Confirmado: "256x256 JPEG" | [switchbrew NRO0](https://switchbrew.org/wiki/NRO) |
| C4 | En 3DS, el bit 0x4 indica AddOn (DLC) | **Erróneo.** Patch vale 0x6 y contiene el bit 0x4, así que las actualizaciones (0004000E) saldrían como DLC. Hay que comparar valores exactos. | [3dbrew Titles](https://3dbrew.org/wiki/Titles) |
| C5 | Plataforma por la parte alta: 0x0001xxxx Wii, 0x0004xxxx 3DS, 0x0005xxxx Wii U | Incompleto. El sistema de Wii (IOS) es 00000001, vWii usa 00000007, 00070002 y 00070008, y DSi usa 0003 | [wiibrew TMD](https://wiibrew.org/wiki/Title_metadata), [wiiubrew Title database](https://wiiubrew.org/wiki/Title_database), [3dbrew Titles](https://3dbrew.org/wiki/Titles) |
| C6 | El TMD no menciona cifrado | El TMD va en claro, pero en 3DS cada contenido trae el flag `1 = Encrypted` | [3dbrew TMD](https://3dbrew.org/wiki/Title_metadata) |
| C7 | authid homebrew de Vita: 0x2F00000000000000 o …01 | Son **…01** (por defecto), **…02** (`-s`, safe) y **…03** (`-ss`, secret-safe). El valor …00 no existe. | [vita-make-fself.c L49-61, L184-190](https://github.com/vitasdk/vita-toolchain/blob/master/src/vita-make-fself/vita-make-fself.c) |
| C8 | psvimg: el usuario aporta el AID | La clave es la *backup key*, que está "tied to your PSN AID" pero no es el AID. Se obtiene con `psvimg-keyfind` a partir de un `partials.bin` generado en una Vita con HENkaku. | [psvimgtools README](https://github.com/yifanlu/psvimgtools) |
| C9 | PSP: "homebrew is a plain ELF" | La frase de PPSSPP se refiere al **DATA.PSP dentro del EBOOT.PBP**. Además, un PBP puede ser un clásico de PS1 o un ISO de PSN (ver §8). | [sceKernelModule.cpp L1556](https://github.com/hrydgard/ppsspp/blob/master/Core/HLE/sceKernelModule.cpp), [Loaders.cpp L251-282](https://github.com/hrydgard/ppsspp/blob/master/Core/Loaders.cpp) |
| C10 | DOL: offsets y tamaños alineados a 32 | wiibrew dice que el IPL y el IOS rechazan lo no alineado. Dolphin solo avisa y compara `offset + AlignUp(size, 32) ≤ tamaño del archivo`. | [wiibrew DOL](https://wiibrew.org/wiki/DOL), [DolReader.cpp L61-70](https://github.com/dolphin-emu/dolphin/blob/master/Source/Core/Core/Boot/DolReader.cpp) |
| C11 | titledb: `{REGION}.{lang}.json` indexado por nsuId | Correcto, pero el id → nombre se saca del campo `id` de cada entrada. `titles.US.en.json` ya no está en el repo. Cada archivo de región pesa unos 90 MB. | [README](https://github.com/blawar/titledb/blob/master/README.md), US.en.json descargado |
| C12 | Nota sobre la página de caustik (0x114) | Eliminada porque no se puede verificar. Cxbx confirma el certificado en 0x118 y el timestamp en 0x114. | [Cxbx Xbe.h L104-105](https://github.com/Cxbx-Reloaded/Cxbx-Reloaded/blob/master/src/common/xbe/Xbe.h) |
| C13 | — (no estaba) | **Nuevo:** `ncas.json` de titledb relaciona id de NCA → titleId (236.650 entradas), así que un .nca suelto se puede identificar **sin claves** | [titledb ncas.json](https://github.com/blawar/titledb/blob/master/ncas.json) |

## Tabla resumen para el clasificador (corregida)

| Ext. | Magia u offset | Sistema | Tipo | Metadatos legibles sin claves |
|---|---|---|---|---|
| .nro | `NRO0` @0x10; `ASET` @ NroHeader.size (u32 @0x18) | Switch | Homebrew | NACP (nombre, editor, DisplayVersion) e icono JPEG de 256x256 |
| .nso | `NSO0` @0x0 | Switch | Módulo de ExeFS | Solo ModuleId (build id) @0x40 |
| main.npdm | `META` @0x0; `ACI0` → ProgramId @+0x10 | Switch | Parte de ExeFS | ProgramId (el Name casi siempre es "Application") |
| .nca | Nada en claro salvo la sección logo | Switch | Contenido | Nombre de archivo = ContentId **[inferencia]** → `ncas.json` → titleId |
| .dol | Sin magia; se valida por estructura | GC / Wii | Ejecutable | Nada; Wii si aparece `mtspr HID4` |
| .xbe | `XBEH` @0x0 | Xbox | Juego o homebrew | TitleId, TitleName UTF-16, medios, región, DiscNumber |
| .xex | `XEX2` @0x0 | Xbox 360 | Juego, DLL o parche | media_id, title_id, disc_number/count, región |
| .rpx/.rpl | ELF con OSABI 0xCA y ABIVer 0xFE | Wii U | Ejecutable o biblioteca | Nada; están en `meta/meta.xml` |
| .tmd | Tipo de firma @0x0; Title ID @0x18C con RSA-2048 | Wii / 3DS / Wii U | Metadatos de título | Title ID, versión, número de contenidos y flags |
| .psvimg | Opaco | Vita (copia CMA) | Copia de seguridad | Title id en la ruta |
| .self/eboot.bin | `SCE\0` @0x0 | PS3 / Vita | App, NPDRM o sistema | app_info y control info (content_id) |
| PSP | `~PSP` / `\0PBP` / ELF | PSP (y PS1 en PBP) | Juego, PS1, PSN, homebrew o módulo | modname @0x0A (~PSP); PARAM.SFO (PBP) |
| .3dsx | `3DSX` @0x0 | 3DS | Homebrew | SMDH: nombre, editor, icono y región |

---

## 1. Switch

### .nro
- **Cabecera:** RocrtHeader de 0x10 bytes, luego `"NRO0"` en 0x10, versión en 0x14 y tamaño total del NRO en 0x18 (u32). ([switchbrew NRO0](https://switchbrew.org/wiki/NRO))
- **ASET:** opcional y en el offset de archivo `{NroHeader.size}`. Los offsets internos cuentan desde el inicio del AssetHeader.
  - Magia `ASET` en +0x0 y versión en +0x4 (actualmente 0).
  - AssetSections {u64 offset, u64 tamaño}: icono en +0x8, NACP en +0x18 y RomFS en +0x28. Un tamaño 0 significa sección vacía.
  - El icono es "256x256 JPEG". ([switchbrew NRO0](https://switchbrew.org/wiki/NRO))
- **NACP:** ocupa 0x4000 bytes ([switchbrew NACP](https://switchbrew.org/wiki/NACP)).
  - Desde 0x0 hay 16 ApplicationTitle de 0x300 bytes: Name de 0x200 y Publisher de 0x100, en UTF-8.
  - DisplayVersion en 0x3060 (0x10 bytes).
- **Nota sin efecto en la detección:** desde 23.0.0 el bit 1 de Flags (0x1C) indica Compress y existe una AdditionalNroHeader con magia `NRA0`. La magia `NRO0` no cambia. ([switchbrew NRO0](https://switchbrew.org/wiki/NRO))

### .nso y ExeFS
- **Cabecera:** `"NSO0"` en 0x0 y Flags en 0xC ([switchbrew NSO0](https://switchbrew.org/wiki/NSO)):
  - bits 0 a 2: .text, .rodata y .data comprimidos;
  - bits 3 a 5: comprobación de hash;
  - bit 6: ExecuteOnlyMemory (20.0.0+);
  - bit 7: UseZbicCompression, "zstd-based compression instead of LZ4" (22.0.0+).
- **Identificación:** ModuleId en 0x40 (0x20 bytes, build id). No hay título.
- **Contexto:** un ExeFS contiene `main` y `main.npdm`, y a veces `rtld`, `sdk` y `subsdk0-6`. ([switchbrew ExeFS](https://switchbrew.org/wiki/ExeFS))
- **main.npdm** ([switchbrew NPDM](https://switchbrew.org/wiki/NPDM)):
  - `META` en 0x0;
  - Name en 0x20, descrito como "usually/always 'Application'", así que no sirve como nombre del juego;
  - AciOffset en 0x70;
  - ACI0 con ProgramId en +0x10 (8 bytes).
- **Clasificación:** un NSO o NPDM suelto es un componente de ExeFS extraído. El title id sale de `main.npdm` y el nombre se busca en titledb.

### .nca
- **Cifrado:** según switchbrew, "The entire raw NCAs are encrypted", y lo único en claro es la sección logo cuando existe ([switchbrew NCA](https://switchbrew.org/wiki/NCA)):
  - los primeros 0xC00 bytes van con AES-XTS (sector de 0x200, tweak no estándar);
  - una vez descifrado: magia `NCA3/2/1/0` en 0x200, ContentType en 0x205 (0 Program, 1 Meta, 2 Control, 3 Manual, 4 Data, 5 PublicData) y ProgramId en 0x210.
- **Identificación sin claves (nuevo):**
  - el CNMT referencia cada contenido por un ContentId de 0x10 bytes ([switchbrew CNMT](https://switchbrew.org/wiki/CNMT));
  - los .nca de NSP y SD se llaman `<ContentId>.nca` **[inferencia, convención de la comunidad]**;
  - `ncas.json` de titledb está indexado por esos ids de 32 hex y da `titleId`, `contentType` y `size` (236.650 entradas, comprobado).
  - Con eso un .nca se clasifica sin descifrar nada.
- **Recomendación:** no descifrar nunca, ni siquiera con `prod.keys` del usuario (ver §10).

### Catálogo de ids: blawar/titledb (decisión 1)
- **Licencia:** `LICENSE` es MIT, "Copyright (c) 2019 Blake Warner". La API de GitHub da `license.spdx_id = MIT`, `archived = false` y último push el 2026-10-16T05:24:17Z. ([repo](https://github.com/blawar/titledb), [LICENSE](https://github.com/blawar/titledb/blob/master/LICENSE))
- **Archivos según el README:**
  - `{region}.{language}.json` indexado por nsuId;
  - `cnmts.json` y `versions.json` por titleId;
  - `ncas.json` por NCA;
  - `cheats.json` por titleId > buildId;
  - `titles.US.en.json` ya no se incluye. ([README](https://github.com/blawar/titledb/blob/master/README.md))
- **Datos comprobados (US.en.json, 90,7 MB):**
  - 37.473 entradas, de las que 29.774 tienen `id` (titleId);
  - el campo `key` vale `null` en las 37.473, así que este archivo no trae claves de título;
  - las imágenes apuntan a `img-eshop.cdn.nintendo.net`.
- **Matiz legal [inferencia]:**
  - el MIT cubre lo que aporta el autor; nombres, descripciones e imágenes son datos de la eShop de Nintendo;
  - lo más defendible es descargar en tiempo de ejecución y guardar solo `id → name` (más `ncas.json` → titleId);
  - no conviene empaquetar descripciones ni imágenes en el binario.
- **Alternativa:** `nsuId` no hace falta. Basta con filtrar `{id, name, publisher}` y generar un índice compacto local.

## 2. GameCube / Wii: .dol
- **Cabecera:** 0x100 bytes en big-endian ([wiibrew DOL](https://wiibrew.org/wiki/DOL)):
  - offsets de Text0-6 en 0x00 y de Data0-10 en 0x1C;
  - direcciones de carga en 0x48 y tamaños en 0x90;
  - BSS en 0xD8/0xDC, entry point en 0xE0 y relleno de 0xE4 a 0xFF;
  - wiibrew: "each section should be 32 byte aligned … or they will be rejected by the Gamecube's IPL and Wii's IOS".
- **Validación de Dolphin:** cada sección no vacía debe cumplir `offset + AlignUp(size, 32) ≤ tamaño del archivo`. Si no está alineada solo avisa. ([DolReader.cpp L54-111](https://github.com/dolphin-emu/dolphin/blob/master/Source/Core/Core/Boot/DolReader.cpp))
- **GC o Wii:** `m_is_wii = true` si alguna palabra de una sección de texto cumple `(w & 0xFC1FFFFF) == 0x7C13FBA6` (mtspr HID4). ([DolReader.cpp L48-85](https://github.com/dolphin-emu/dolphin/blob/master/Source/Core/Core/Boot/DolReader.cpp))
- **Heurística extra [inferencia]:** direcciones y entry dentro de 0x80000000–0x81800000.
- **Homebrew Channel** ([wiibrew Homebrew Channel](https://wiibrew.org/wiki/Homebrew_Channel)):
  - carga `apps/<nombre>/boot.dol` o `boot.elf`;
  - los metadatos van en `meta.xml` (opcional);
  - el icono es `icon.png` de 128x48.

## 3. Xbox: .xbe
- **Cabecera** ([Cxbx-Reloaded Xbe.h](https://github.com/Cxbx-Reloaded/Cxbx-Reloaded/blob/master/src/common/xbe/Xbe.h)):
  - `XBEH` en 0x0 y BaseAddr en 0x104;
  - TimeDate en 0x114 y **CertificateAddr en 0x118**;
  - offset del certificado en el archivo = `cert_addr − base_addr`.
- **Certificado:**
  - TitleId en +0x08 y TitleName en +0x0C (wchar[40]);
  - AllowedMedia en +0x9C, GameRegion en +0xA0 y GameRatings en +0xA4;
  - **DiscNumber en +0xA8** y Version en +0xAC.
- **TitleId:** "usually 2 ASCII letters for the publisher, followed by a u16". Por ejemplo, `4D530004` = MS-004. ([xboxdevwiki Xbe](https://xboxdevwiki.net/Xbe))
- **Medios:** `HARD_DISK = 0x00000001` y `NONSECURE_HARD_DISK = 0x40000000`. ([xboxdevwiki Xbe](https://xboxdevwiki.net/Xbe))
- **Homebrew [inferencia]:** TitleId 0 o arbitrario y medio de disco duro. Es una heurística.

## 4. Xbox 360: .xex
- **Cabecera** en big-endian ([Xenia xex2_info.h L548-554](https://github.com/xenia-project/xenia/blob/master/src/xenia/kernel/util/xex2_info.h)):
  - `XEX2` en 0x0 y module_flags en 0x4;
  - header_size en 0x8, security_offset en 0x10 y header_count en 0x14.
- **module_flags:** 0x1 TITLE, 0x8 DLL, 0x10 MODULE_PATCH, 0x20 PATCH_FULL y 0x40 PATCH_DELTA (L113-121).
- **EXECUTION_INFO (0x00040006):**
  - media_id en +0x0, version en +0x4, base_version en +0x8 y **title_id en +0xC**;
  - platform en +0x10 y executable_table en +0x11;
  - **disc_number en +0x12** y **disc_count en +0x13**;
  - savegame_id en +0x14 (L479-488).
- **Security info:** region en +0x178 y allowed_media_types en +0x17C (L584-585). Flags de región: NTSCU 0xFF, NTSCJ 0xFF00, PAL 0xFF0000 y ALL 0xFFFFFFFF (L102-111).
- **Cifrado:** "Section contents are encrypted with CBC AES, with the key changing for each file" ([free60 XEX](https://free60.org/System-Software/Formats/XEX/)). Las cabeceras van en claro.

## 5. Wii U: .rpx/.rpl
- **ELF:** `EABI_CAFE = 0xCA` en el offset 0x07 y `EABI_VERSION_CAFE = 0xFE` en el 0x08. ([decaf-emu cafe_loader_rpl.h L28-33, L178-179](https://github.com/decaf-emu/decaf-emu/blob/master/src/libdecaf/src/cafe/loader/cafe_loader_rpl.h))
- **Otras señales:** secciones comprimidas con zlib, sin program headers, y presencia de `.fexports`/`.dexports` y `.fimport_*`/`.dimport_*`. ([wiiubrew RPL](https://wiiubrew.org/wiki/RPL))
- **meta.xml:** `title_id`, `product_code` ("WUP-X-****"), `longname_en`, `publisher_en` y `region` ("FFFFFFFF for region free"). ([wiiubrew meta.xml](https://wiiubrew.org/wiki/Meta.xml))
- **Clasificación [inferencia]:** `code/*.rpx` junto a `meta/` es un juego desempaquetado; un .rpx suelto es homebrew; un .rpl es una biblioteca.

## 6. .tmd (Wii, 3DS, Wii U)
- **Wii** ([wiibrew TMD](https://wiibrew.org/wiki/Title_metadata)):
  - firma 0x10001 en 0x0, Title ID en 0x18C y Region en 0x19C;
  - versión en 0x1DC y número de contenidos en 0x1DE;
  - registros CMD de 0x24 bytes desde 0x1E4, con el tipo 0x4001 = DLC.
  - Parte alta del Title ID: 00000001 System, 00010000 Game, 00010001 Channel, 00010002 SystemChannel, 00010004 GameWithChannel, 00010005 DLC, 00010008 HiddenChannel.
- **3DS** ([3dbrew TMD](https://3dbrew.org/wiki/Title_metadata)):
  - la cabecera empieza en Y, que depende del tipo de firma; con RSA-2048 (firma de 0x100 y relleno hasta 0x40) Y = 0x140;
  - Title ID en Y+0x4C, que da **0x18C, igual que en Wii y Wii U**;
  - versión en Y+0x9C y número de contenidos en Y+0x9E;
  - los content chunks tienen el flag `1 = Encrypted`.
- **Categorías de 3DS (bitmask)** ([3dbrew Titles](https://3dbrew.org/wiki/Titles)): Normal 0x0, DlpChild 0x1, Demo 0x2, AddOnContents 0x4, Patch 0x6, CannotExecution 0x8, System 0x10, TWL 0x8000.
  - Hay que **comparar valores completos**, no bits sueltos:
    - 00040000 app y 00040002 demo;
    - 0004000E actualización **[inferencia: Patch|CannotExecution]**;
    - 0004008C DLC (citado en 3dbrew Titles);
    - 00040010 app de sistema y 00040001 Download Play. ([3dbrew Title list](https://3dbrew.org/wiki/Title_list))
- **Wii U:**
  - Title ID en 0x18C, versión en 0x1DC y número de contenidos en 0x1DE ([wiiubrew TMD](https://wiiubrew.org/wiki/Title_metadata));
  - 00050000 juego, 0005000C DLC, 0005000E actualización, 00050002 demo, 00050010 app de sistema, 0005001B datos compartidos y 0005004E paquete de actualización de disco;
  - vWii usa 00000007, 00070002 y 00070008. ([wiiubrew Title database](https://wiiubrew.org/wiki/Title_database))
- **Plataforma** ("CCCC: 5=WiiU, 4=3DS, 3=DSi, 1=Wii", [3dbrew Titles](https://3dbrew.org/wiki/Titles)), con estas excepciones:
  - 00000001 es sistema de Wii;
  - 00000007 y 0007xxxx son vWii, es decir, Wii U.

## 7. Vita: .psvimg
- **Archivos:** `.psvimg` y `.psvmd`, cifrados con una backup key "tied to your PSN AID".
- **Cómo se obtiene la clave:** con `psvimg-keyfind` (fuerza bruta) a partir de un `partials.bin` que genera el homebrew "dump_partials" en una Vita con HENkaku. ([psvimgtools README](https://github.com/yifanlu/psvimgtools))
- **Estructura descifrada:** constantes `0xFEE1900D` y `0xFEE1900E`, bloques de 0x8000 y marcadores `EndOfHeader\n` y `EndOfTailer\n`. ([psvimg.h](https://github.com/yifanlu/psvimgtools/blob/master/psvimg.h))
- **Title id:** solo está en la ruta, `PS Vita/PGAME/<AID 16 hex>/NPJH00053/game/game.psvimg`. ([README](https://github.com/yifanlu/psvimgtools))
- **Clasificación:** "Copia de seguridad de Vita (cifrada)", con el title id tomado de la carpeta. No se descifra.

## 8. PS3, Vita y PSP
- **SCE header** ([scetool sce.h L21-63, L160-177](https://github.com/naehrwert/scetool/blob/master/sce.h)):
  - magia `0x53434500` en 0x0 y version (u32) en 0x4;
  - key_revision (u16) en 0x8 y header_type (u16) en 0xA: 1 SELF, 2 RVK, 3 PKG, 4 SPP.
- **app_info:** auth_id, vendor_id, self_type y version.
- **self_type:** 1 LV0, 2 LV1, 3 LV2, 4 APP, 5 ISO ("Isolated SPU module"), 6 LDR y 8 NPDRM (L129-143).
- **Control info NPDRM:** magia `"NPD\0"` y `content_id[0x30]` (L379-399).
- **En claro:** scetool parsea `self.ai` (app_info) directamente del buffer crudo antes de `sce_decrypt_header`. ([sce.cpp L163-171, L808](https://github.com/naehrwert/scetool/blob/master/sce.cpp))
- **PS3 o Vita:**
  - scetool escribe `SCE_HEADER_VERSION_2` en big-endian, es decir `00 00 00 02`;
  - vita-make-fself escribe `magic = 0x454353` y `version = 3` en little-endian, es decir `03 00 00 00` ([L152-153](https://github.com/vitasdk/vita-toolchain/blob/master/src/vita-make-fself/vita-make-fself.c)).
  - Es una heurística válida **[inferencia]**.
- **Homebrew de Vita:** authid `0x2F00000000000001`, `…02` (`-s`) o `…03` (`-ss`). El self_type por defecto es NPDRM en el enum de Vita. ([vita-make-fself.c L49-61, L184-193](https://github.com/vitasdk/vita-toolchain/blob/master/src/vita-make-fself/vita-make-fself.c)) **[inferencia]** Es buen indicador de homebrew.
- **PSP, magias:**
  - `PSP_MAGIC = 0x5053507e` ("~PSP") y `ELF_MAGIC = 0x464c457f` ([sceKernelModule.cpp L1351-1352](https://github.com/hrydgard/ppsspp/blob/master/Core/HLE/sceKernelModule.cpp));
  - los módulos `~SCE` también se tratan en L1286+;
  - `PSP_Header`: signature en 0x0, comp_attribute en 0x6, module_ver en 0x8/0x9 y **modname[28] en 0x0A**. ([PrxDecrypter.h L33-38](https://github.com/hrydgard/ppsspp/blob/master/Core/ELF/PrxDecrypter.h))
- **PBP:** magia `"\0PBP"`; si es ELF, se trata como ELF ([PBPReader.cpp L36-41](https://github.com/hrydgard/ppsspp/blob/master/Core/ELF/PBPReader.cpp)). PPSSPP lo clasifica así ([Loaders.cpp L225-282](https://github.com/hrydgard/ppsspp/blob/master/Core/Loaders.cpp)):
  - PARAM.SFO con `CATEGORY == "ME"`, o PSAR que empieza por `PSISOIMG0000`: **clásico de PS1**;
  - PSAR con `NPUMDIMG`: ISO de PSN de PSP;
  - en cualquier otro caso: PBP de PSP;
  - el offset del PSAR está en 0x24.
- **"Homebrew is a plain ELF":** se refiere al DATA.PSP dentro del PBP (L1556).

## 9. 3DS: .3dsx
- **Cabecera:** `3DSX` en 0x0 y header size en 0x4. Hay cabecera extendida si ese tamaño es mayor de 32; la cabecera base mide 0x20, así que la extendida empieza en 0x20.
- **Cabecera extendida:** SMDH offset en +0x0, SMDH size en +0x4 y RomFS offset en +0x8. ([3dbrew 3DSX](https://3dbrew.org/wiki/3DSX_Format))
- **SMDH** ([3dbrew SMDH](https://3dbrew.org/wiki/SMDH)):
  - `SMDH` en 0x0;
  - 16 títulos de 0x200 bytes desde 0x8, cada uno con descripción corta de 0x80, larga de 0x100 y editor de 0x80, en UTF-16;
  - región en 0x2018 (0x7FFFFFFF = sin región);
  - icono de 24x24 en 0x2040 (0x480 bytes) y de 48x48 en 0x24C0 (0x1200 bytes), en RGB565 con tiles de 8x8.

## 10. Claves y marco legal (la duda de fondo)
- **Yuzu.** En Nintendo of America v. Tropic Haze, caso 1:24-cv-00082-JJM-LDA (D.R.I.), la sentencia acordada declara ([PDF de la sentencia](https://storage.courtlistener.com/recap/gov.uscourts.rid.56980/gov.uscourts.rid.56980.10.1.pdf)):
  - "Yuzu requires the Nintendo Switch's proprietary cryptographic keys to gain access to and play Nintendo Switch games";
  - "Developing or distributing software … that in its ordinary course functions only when cryptographic keys are integrated without authorization, violates the [DMCA's] prohibition on trafficking … § 1201(a)(2)(A)";
  - condena de US$2.400.000;
  - orden de destruir "all copies of Nintendo cryptographic keys including the prod.keys".
  - Por tanto, "el usuario pone las claves" **no** protege por sí mismo.
- **Azahar.** El modelo que se mencionaba como referencia ya no existe:
  - no carga NCCH cifrado ("Encrypted NCCH are not supported");
  - trae un bloque de claves propio, ofuscado, activado por defecto, que `keys.txt` sustituye ([key.cpp](https://github.com/azahar-emu/azahar/blob/master/src/core/hw/aes/key.cpp), [CMakeLists.txt](https://github.com/azahar-emu/azahar/blob/master/CMakeLists.txt));
  - según la prensa, el motivo es legal y cita el caso Yuzu ([Android Authority](https://www.androidauthority.com/3ds-emulator-legal-standing-3508926/)), pero el blog primario no se pudo abrir (403).
- **Recomendación [inferencia; no es asesoría legal]:**
  - un organizador que **no descifra** queda fuera del supuesto "functions only when keys are integrated";
  - se clasifica con lo que va en claro (§1-9), con el nombre de archivo o la carpeta, y con `ncas.json` y `{region}.json` de titledb;
  - no se piden, no se leen y no se distribuyen `prod.keys`, `aes_keys.txt` ni backup keys de Vita;
  - .nca y .psvimg se etiquetan como "contenido cifrado" e identificado cuando sea posible.

## 11. Qué implica para el clasificador
1. **Detección:** todos tienen magia o valores fijos salvo .dol (validación por estructura) y .nca/.psvimg (opacos). Un .nca se identifica igualmente por su nombre y `ncas.json`.
2. **Juego, actualización, DLC u homebrew:**
   - por la parte alta del Title ID del TMD, comparando valores completos (C4, C5);
   - por los flags del XEX;
   - por self_type, por authid 0x2F… en Vita y por la categoría del PBP ("ME" = PS1).
3. **Agrupar discos:** XEX con disc_number y disc_count (+0x12/+0x13 de 0x40006) y XBE con DiscNumber (+0xA8).
4. **Duplicados:** usar title id más versión, sacados de TMD, XEX, NPDM o titledb, como clave para el sistema de duplicados que ya existe **[inferencia]**.

