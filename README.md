# DexNotas

[Notepad Studio](https://github.com/alesloa/notepad-studio), un editor de texto al estilo de
Notepad++, en Android: pensado para **Samsung DeX** y para el modo de ventanas de escritorio de
Android (monitor, ratón y teclado).

No se reescribe nada: se compila para Android el propio Notepad Studio, con todo lo que trae
(pestañas y sesiones, resaltado de 25 lenguajes, buscar y reemplazar con regex, buscar en
archivos, vista previa de Markdown, RTF/DOCX/ODT, CSV, macros, corrector), y se le añade la capa
de plataforma Android.

> Estado: **fase 0** (prueba técnica). El APK se compila; **todavía no se ha probado en ningún
> dispositivo**. Plan y estado: [docs/plan.md](docs/plan.md).

## Cómo está hecho

```
DexNotas/
├── upstream/              submódulo: fork manoloGith/notepad-studio, rama dexnotas/android
│   ├── src/               Notepad Studio (una biblioteca, `notepad_studio`, y su binario)
│   ├── vendor/gpui/       GPUI de Zed (revisión 294169ae): interfaz, renderizador wgpu
│   ├── vendor/gpui-mobile/  la plataforma Android de GPUI (Rust y Java)
│   └── android/rfd-shim/  diálogos de archivo: rfd en escritorio, un backend en Android
├── apps/dexnotas/         el crate Rust de la app (cdylib que carga Android)
│   ├── src/android.rs     android_main: entorno, plataforma GPUI, notepad_studio::main()
│   ├── src/main.rs        la misma app en escritorio, para iterar sin dispositivo
│   └── android/           proyecto Gradle: NativeActivity (GpuiInputActivity) y manifiesto para DeX
└── docs/                  plan por fases y notas de cada fase
```

Notepad Studio usa GPUI, la biblioteca de interfaz de Zed, que no tiene plataforma Android
oficial. El fork hace tres cosas ([docs/fork.md](docs/fork.md)): actualiza GPUI a una revisión
con las plataformas separadas, le conecta la plataforma Android de
[gpui-mobile](https://github.com/aidant/gpui-mobile) y quita o sustituye lo que Android no tiene
(diálogos nativos, papelera, terminal).

## Compilar

Requisitos: `rustup` (instala la versión y los targets de `rust-toolchain.toml`),
`cargo install cargo-ndk --version 4.1.2 --locked`, Android SDK (plataforma 36, build-tools
36.0.0), NDK 29.0.14206865 y JDK 17 o superior.

```bash
git clone --recursive https://github.com/manoloGith/DexNotas
```

El APK de depuración (arm64-v8a para el dispositivo, x86_64 para el emulador):

```bash
cd DexNotas/apps/dexnotas/android && ./gradlew assembleDebug
```

Solo arm64, que es más rápido:

```bash
./gradlew assembleDebug -Pdexnotas.abis=arm64-v8a
```

Instalar en un dispositivo conectado:

```bash
adb install -r app/build/outputs/apk/debug/app-debug.apk
```

La misma app en escritorio Linux, sin dispositivo:

```bash
cargo run -p dexnotas
```

## Licencia

MIT o Apache-2.0, a elegir. Notepad Studio, GPUI y gpui-mobile conservan las suyas:
[NOTICE.md](NOTICE.md).
