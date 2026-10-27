## English

Coverly ROMs <version>. **Download** the file for your computer below and check it against `SHA256SUMS`: `shasum -a 256 -c SHA256SUMS --ignore-missing` (macOS), `sha256sum -c SHA256SUMS --ignore-missing` (Linux) or `Get-FileHash <file>` (Windows PowerShell).

| System                                         | File                                                                                                                                                                             |
| ---------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| macOS 11 or later, Apple silicon (M1 or newer) | `Coverly-ROMs_<version>_mac_arm.dmg`                                                                                                                                             |
| macOS 11 or later, Intel                       | `Coverly-ROMs_<version>_mac_intel.dmg`                                                                                                                                           |
| Windows 10/11 (x64)                            | `Coverly-ROMs_<version>_windows_x64-setup.exe` (recommended); stable releases also `Coverly-ROMs_<version>_windows_x64.msi`                                                      |
| Windows 11 (ARM64)                             | `Coverly-ROMs_<version>_windows_arm-setup.exe` (recommended); stable releases also `Coverly-ROMs_<version>_windows_arm.msi`                                                      |
| Linux x64                                      | `Coverly-ROMs_<version>_linux_x64.AppImage` or `Coverly-ROMs_<version>_linux_x64.deb`                                                                                            |
| Linux ARM64                                    | `Coverly-ROMs_<version>_linux_arm.AppImage` or `Coverly-ROMs_<version>_linux_arm.deb`                                                                                            |
| Command line                                   | `coverly_<version>_<system>` (`mac_arm`, `mac_intel`, `windows_x64.exe`, `windows_arm.exe`, `linux_x64`, `linux_arm`); once installed, `coverly update install` keeps it current |

Not sure which Mac you have? Apple menu → About This Mac: "Chip" means Apple silicon, "Processor" means Intel.

On a Windows ARM64 PC, install the ARM64 build. A copy of the x64 build installed earlier keeps working and updating as x64.

The builds are not signed with an Apple or Microsoft certificate yet, so macOS and Windows warn you the first time. Only follow these steps for a file you took from this release page and checked against `SHA256SUMS`.

### macOS will say the app is damaged, or that Apple could not verify it. It is not.

The app carries no Apple Developer ID, so macOS quarantines it. Move Coverly ROMs to Applications, then clear the flag once in Terminal (the quotes matter: the name has a space):

```bash
xattr -rd com.apple.quarantine "/Applications/Coverly ROMs.app"
```

Or try to open it once, then go to **System Settings → Privacy & Security**, scroll down and click **Open Anyway**. That quarantine flag is the check that protects you from a tampered download. A `coverly` binary downloaded with a browser is quarantined too: `xattr -d com.apple.quarantine coverly_<version>_mac_arm` (or `_mac_intel`).

### SmartScreen will interrupt the installer.

Windows shows "Windows protected your PC": choose **More info**, then **Run anyway**. If you see a **Security Warning** with **Publisher: Unknown Publisher** instead, choose **Run**. With Smart App Control turned on there is no **Run anyway**: it blocks unsigned apps. The `-setup.exe` installs for your user without administrator rights; the `.msi` installs for every user. Both use Microsoft Edge WebView2: Windows 11 has it, and on Windows 10 the installer downloads it if it is missing.

### Linux

Mark the AppImage executable (`chmod +x`). The `.deb` depends on `libwebkit2gtk-4.1-0` and `libgtk-3-0`, which `sudo apt install ./<file>.deb` pulls in. Built on Ubuntu 22.04: it needs glibc 2.35 or newer (Ubuntu 22.04, Debian 12 or later).

Installed apps update themselves (Settings → About). Every update is verified with the project's own signing key before it is installed; an update that was not signed by it is refused.

---

## Español

Coverly ROMs <versión>. **Descarga** el archivo para tu ordenador y compruébalo con `SHA256SUMS`: `shasum -a 256 -c SHA256SUMS --ignore-missing` (macOS), `sha256sum -c SHA256SUMS --ignore-missing` (Linux) o `Get-FileHash <archivo>` (PowerShell en Windows).

| Sistema                                              | Archivo                                                                                                                                                                      |
| ---------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| macOS 11 o posterior, Apple silicon (M1 o posterior) | `Coverly-ROMs_<versión>_mac_arm.dmg`                                                                                                                                         |
| macOS 11 o posterior, Intel                          | `Coverly-ROMs_<versión>_mac_intel.dmg`                                                                                                                                       |
| Windows 10/11 (x64)                                  | `Coverly-ROMs_<versión>_windows_x64-setup.exe` (recomendado); las versiones estables también `Coverly-ROMs_<versión>_windows_x64.msi`                                        |
| Windows 11 (ARM64)                                   | `Coverly-ROMs_<versión>_windows_arm-setup.exe` (recomendado); las versiones estables también `Coverly-ROMs_<versión>_windows_arm.msi`                                        |
| Linux x64                                            | `Coverly-ROMs_<versión>_linux_x64.AppImage` o `Coverly-ROMs_<versión>_linux_x64.deb`                                                                                         |
| Linux ARM64                                          | `Coverly-ROMs_<versión>_linux_arm.AppImage` o `Coverly-ROMs_<versión>_linux_arm.deb`                                                                                         |
| Línea de comandos                                    | `coverly_<versión>_<sistema>` (`mac_arm`, `mac_intel`, `windows_x64.exe`, `windows_arm.exe`, `linux_x64`, `linux_arm`); después, `coverly update install` la mantiene al día |

¿No sabes qué Mac tienes? Menú Apple → Acerca de este Mac: «Chip» es Apple silicon, «Procesador» es Intel.

En un PC con Windows ARM64, instala la versión ARM64. Una copia de la versión x64 instalada antes sigue funcionando y actualizándose como x64.

Las versiones todavía no están firmadas con un certificado de Apple ni de Microsoft, así que macOS y Windows te avisan la primera vez. Sigue estos pasos solo con un archivo que hayas bajado de esta página y comprobado con `SHA256SUMS`.

### macOS dirá que la app está dañada, o que Apple no puede comprobarla. No lo está.

La app no lleva Apple Developer ID, así que macOS la pone en cuarentena. Mueve Coverly ROMs a Aplicaciones y quita la marca una vez, en Terminal (las comillas importan: el nombre lleva un espacio):

```bash
xattr -rd com.apple.quarantine "/Applications/Coverly ROMs.app"
```

O intenta abrirla una vez, ve a **Ajustes del Sistema → Privacidad y seguridad**, baja y pulsa **Abrir igualmente**. Esa marca de cuarentena es la comprobación que te protege de una descarga manipulada. Un binario `coverly` bajado con el navegador también queda en cuarentena: `xattr -d com.apple.quarantine coverly_<versión>_mac_arm` (o `_mac_intel`).

### SmartScreen interrumpirá el instalador.

Windows muestra «Windows protegió su PC»: elige **Más información** y luego **Ejecutar de todas formas**. Si en su lugar ves una **Advertencia de seguridad** con **Editor: Editor desconocido**, elige **Ejecutar**. Con Smart App Control activado no hay **Ejecutar de todas formas**: bloquea las apps sin firmar. El `-setup.exe` instala para tu usuario sin permisos de administrador; el `.msi` instala para todos los usuarios. Los dos usan Microsoft Edge WebView2: Windows 11 ya lo trae, y en Windows 10 el instalador lo descarga si falta.

### Linux

Marca el AppImage como ejecutable (`chmod +x`). El `.deb` depende de `libwebkit2gtk-4.1-0` y `libgtk-3-0`, que `sudo apt install ./<archivo>.deb` instala. Compilado en Ubuntu 22.04: necesita glibc 2.35 o posterior (Ubuntu 22.04, Debian 12 o posteriores).

Las apps instaladas se actualizan solas (Configuración → Acerca de). Cada actualización se verifica con la clave de firma del propio proyecto antes de instalarse; una actualización que no esté firmada con ella se rechaza.
