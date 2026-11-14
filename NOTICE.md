# Créditos y licencias

DexNotas se publica bajo **MIT o Apache-2.0**, a elegir (`LICENSE-MIT`, `LICENSE-APACHE`).

- **Notepad Studio** (https://github.com/alesloa/notepad-studio), Apache-2.0, de los colaboradores
  de Notepad Studio: la lista de lenguajes, su detección y la configuración de las gramáticas de
  tree-sitter (`apps/dexnotas/src/syntax/`, con el origen indicado en cada archivo).
- **craft-android** (https://github.com/manoloGith/craft-android), MIT o Apache-2.0: la capa
  Android, como submódulo.
- **egui/eframe** (MIT o Apache-2.0), en el fork de storytold que usan todas las apps Dex.
- **highlight.js** (https://github.com/highlightjs/highlight.js), BSD-3-Clause, de sus autores:
  el resaltado de los lenguajes que no tienen gramática de tree-sitter, empaquetado en
  `apps/dexnotas/assets/highlightjs/` (con `LICENSE`; se rehace con `tools/highlightjs/build.sh`).
- **QuickJS** (MIT, Fabrice Bellard y Charlie Gordon) mediante **rquickjs** (MIT): ejecuta
  highlight.js dentro de la app.
- **Inter** (https://github.com/rsms/inter), SIL Open Font License 1.1, de The Inter Project
  Authors: la letra de la vista previa (`apps/dexnotas/assets/fonts/`, con `OFL.txt`).
- **pulldown-cmark** (MIT) para leer Markdown; **image** y **egui_extras** (MIT o Apache-2.0)
  para las imágenes de la vista previa.
- Las gramáticas de **tree-sitter** de cada lenguaje (MIT), como dependencias.

DexNotas no está afiliado a Notepad Studio ni a Samsung. «Samsung DeX» es una marca de Samsung
Electronics.
