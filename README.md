# DexNotas

Un editor de texto y código para **Samsung DeX** y el modo escritorio de Android (monitor, ratón y
teclado), que también funciona en el móvil y en Linux.

Está hecho en Rust con egui, igual que [DexFiles](https://github.com/manoloGith/DexFiles) y
[DexPdfCraft](https://github.com/manoloGith/DexPdfCraft), sobre la misma capa Android compartida,
[craft-android](https://github.com/manoloGith/craft-android): ratón completo, teclado físico,
teclado en pantalla, selector de archivos con escritura de vuelta al documento, cierre y
reapertura fiables y un informe de diagnóstico que se copia sin adb.

- **Pestañas que no se pierden**: las pestañas abiertas y el texto sin guardar vuelven al abrir la
  app, aunque se haya cerrado con la X de DeX o la haya parado el sistema. Se guardan un momento
  después de cada cambio y al perder el foco. Opcionalmente, también los archivos.
- **Buscar y reemplazar**: texto o expresión regular, mayúsculas, palabra completa, grupos `$1` al
  reemplazar, todas las coincidencias marcadas, buscar en todas las pestañas, ir a línea.
- **Resaltado de unos 195 lenguajes**: 25 con tree-sitter (las gramáticas de
  [Notepad Studio](https://github.com/alesloa/notepad-studio)) y 169 más con
  [highlight.js](https://github.com/highlightjs/highlight.js) ejecutado con QuickJS; detectados por
  la extensión, el nombre del archivo o `#!`, y elegibles a mano en el menú Lenguaje (por iniciales).
- **Formatear**: JSON y XML/HTML/SVG (formatear, compactar, validar); YAML y TOML (validar), con
  la línea del error.
- **Vista previa de Markdown y HTML**: en doble panel (lado a lado, o arriba y abajo en una
  pantalla estrecha) con el divisor arrastrable, o sola (Ctrl+Mayús+V o la barra de estado). Sigue
  al editor al desplazarse, muestra tablas, listas de tareas, código con colores e imágenes
  locales, y los enlaces abren la web o el archivo `.md` enlazado. En Android se dibuja con el
  WebView del sistema (el motor de Chrome): el HTML se ve completo, con su CSS y su JavaScript;
  Ver ▸ Vista previa ▸ «Motor web» lo cambia por la vista propia de la app, que es la del escritorio.
- **Codificaciones**: UTF-8 (con o sin BOM), UTF-16 LE/BE (con o sin BOM), ANSI (Windows-1252) y
  otras páginas de códigos; reabrir con otra, convertir, fin de línea CRLF/LF/CR.
- **Macros**: grabar (Ctrl+Mayús+R), reproducir una vez, N veces o hasta el final, guardarlas con
  nombre y atajo Ctrl+Alt+1…9.
- **Plegado de código**: − y + en el margen para contraer y expandir funciones y bloques (por
  sangría, en cualquier lenguaje); Ctrl+Alt+F / Ctrl+Alt+Mayús+F, Alt+0 / Alt+Mayús+0 para todo.
- **Menú contextual** (clic derecho o pulsación larga): deshacer, cortar, copiar, pegar, eliminar,
  seleccionar todo, buscar, comentar, mayúsculas/minúsculas, plegar.
- **Edición**: números de línea, línea actual, ajuste de línea, zoom, comentar, duplicar, eliminar
  y ordenar líneas, mayúsculas/minúsculas, sangría con Tab/Mayús+Tab, mantener la sangría.
- **Ecosistema Dex**: DexFiles la pone como app predeterminada para unas 350 extensiones de
  texto y código, y aparece siempre en «Abrir con», para cualquier archivo; «Compartir» desde
  otras apps.

Estado y fases: [docs/plan.md](docs/plan.md). Qué probar: [docs/testing.md](docs/testing.md).

## Estructura

```
DexNotas/
├── craft-android/          submódulo: la capa Android compartida (y winit con los parches de DeX)
├── apps/dexnotas/          la app (cdylib para Android + binario de escritorio)
│   ├── src/text.rs         codificaciones y fines de línea
│   ├── src/document.rs     un documento abierto: abrir, guardar, recargar
│   ├── src/session.rs      la sesión: pestañas y texto sin guardar, ajustes, macros
│   ├── src/find.rs         buscar y reemplazar
│   ├── src/format.rs       JSON, XML, YAML, TOML
│   ├── src/edit_ops.rs     operaciones del menú Editar
│   ├── src/macros.rs       grabar y reproducir
│   ├── src/syntax/         resaltado (tree-sitter y highlight.js)
│   ├── assets/highlightjs/ highlight.js empaquetado (tools/highlightjs/build.sh lo rehace)
│   ├── src/preview/        Markdown y HTML para la vista previa
│   ├── src/ui/             la interfaz egui
│   ├── src/android.rs      android_main y el puente con MainActivity
│   ├── strings.tsv         textos en inglés y español
│   └── android/            proyecto Gradle (MainActivity : CraftActivity)
└── docs/
```

## Compilar

```bash
git clone --recursive https://github.com/manoloGith/DexNotas
```

Escritorio (para iterar sin dispositivo):

```bash
cargo run -p dexnotas --features desktop -- archivo.txt
```

Pruebas:

```bash
cargo test -p dexnotas --features desktop
```

APK (Rust según `rust-toolchain.toml`, `cargo-ndk` 4.1.2, NDK 29.0.14206865, JDK 17, SDK 36):

```bash
cd apps/dexnotas/android && ./gradlew assembleDebug -Pdexnotas.abis=arm64-v8a
```

## Licencia

MIT o Apache-2.0, a elegir. Créditos: [NOTICE.md](NOTICE.md).

El intento anterior (Notepad Studio compilado para Android con GPUI) está en la rama
`notepad-studio-gpui`.
