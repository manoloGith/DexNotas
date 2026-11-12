# El fork de Notepad Studio

`upstream/` es la rama `dexnotas/android` de
[manoloGith/notepad-studio](https://github.com/manoloGith/notepad-studio): Notepad Studio
(`main` de alesloa, v0.0.7, commit `4460678`) más estos commits.

| Commit | Qué hace |
|---|---|
| Move to GPUI at Zed 294169ae | `vendor/gpui` pasa de la revisión `acd8f23` de Zed (un solo crate, renderizador blade, plataforma privada) a la `294169ae`, con las plataformas en crates separados y el renderizador wgpu. Es un workspace propio con las claves de Zed. El código de Notepad Studio sigue los cambios de la API (unas 100 líneas). Se conserva el parche de Notepad Studio a GPUI (`InputContextToken`). |
| Android platform: gpui-mobile behind gpui_platform | `vendor/gpui-mobile` (aidant/gpui-mobile `4288a9a`, bajo Apache-2.0) compilado contra `vendor/gpui`; `gpui_platform::current_platform` lo devuelve en Android. |
| Build for Android | Notepad Studio pasa a ser una biblioteca (`src/lib.rs`, el antiguo `main.rs`) con un binario mínimo; `rfd` se sustituye por `android/rfd-shim`; la papelera, la terminal y el gestor de archivos no hacen nada en Android. |
| The Java host of gpui-mobile, no single-instance gate | Las clases Java que usa la plataforma (`vendor/gpui-mobile/android/java`) y sin bloqueo de instancia única en Android. |

## Qué está comprobado

- Linux: `cargo check`, `cargo build`, y la app abre un archivo con resaltado en una pantalla
  virtual (Xvfb).
- Android: `cargo ndk -t arm64-v8a check` y la compilación de la biblioteca nativa.
- **Sin comprobar:** los tests de Notepad Studio (faltan por portar dos ayudas de test que el
  GPUI anterior tenía parcheadas), macOS, Windows, y cualquier ejecución en un dispositivo.

## Actualizar

Rebasar `dexnotas/android` sobre el `main` de alesloa. Los conflictos esperables están en
`Cargo.toml` y en el código que use las APIs de GPUI que cambiaron. Después, copiar los
`[patch]` de `upstream/Cargo.toml` al `Cargo.toml` de la raíz de este repositorio y volver a
tomar su `Cargo.lock`.
