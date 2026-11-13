# Plan por fases

DexNotas es una app propia en egui con la estructura de DexFiles: sin fork de otra app, con la
capa Android de `craft-android` y el resaltado de Notepad Studio.

| Fase | Contenido | Estado |
|---|---|---|
| 0 | Repositorio, `craft-android`, Gradle, CI, ventana egui en Android y escritorio | ✅ |
| 1 | Pestañas, sesión persistente con autoguardado, abrir/guardar (selector de Android), codificaciones UTF-8/UTF-16/ANSI y fines de línea | ✅ |
| 2 | Resaltado de 25 lenguajes, números de línea, línea actual, lenguaje a mano | ✅ |
| 3 | Buscar y reemplazar (regex, grupos, palabra, mayúsculas), marcar coincidencias, buscar en pestañas, ir a línea | ✅ |
| 4 | JSON y XML: formatear, compactar, validar; YAML y TOML: validar | ✅ |
| 5 | Macros: grabar, reproducir 1/N veces/hasta el final, guardar con nombre y atajo | ✅ |
| 6 | «Abrir con», DexFiles, compartir texto, metadatos de F-Droid, workflow de release | ✅ |

✅ quiere decir hecho y probado en Linux (pruebas automáticas y la ventana bajo Xvfb). **En
Android solo se ha comprobado que compila y se empaqueta**; la lista de qué probar en DeX está en
[testing.md](testing.md).

## Decisiones

- **Las pestañas no se cierran al salir.** Cerrar la ventana (Atrás, la X de DeX, Salir) nunca
  pregunta: el texto sin guardar se queda en la sesión. Solo cerrar una pestaña con cambios
  pregunta si guardar.
- **El texto en memoria usa `\n`**; el fin de línea del archivo se recuerda y se aplica al guardar.
- **ANSI** es Windows-1252 al detectar; otras páginas de códigos se eligen con «Reabrir con». Al
  guardar en ANSI, lo que no cabe se escribe como `?` (avisa antes).
- **YAML y TOML solo se validan**: formatearlos con las bibliotecas disponibles pierde los
  comentarios.
- **El editor es propio** (`ui/code_editor.rs`) sobre una cuerda (`ropey`, `buffer.rs`): solo se
  maquetan las líneas visibles, así que no hay límite de tamaño al abrir ni para los colores.
  Hasta 4 MB se colorea el documento entero; por encima, una ventana de ±256 KB alrededor de lo
  que se ve, en un hilo aparte. Las líneas muy largas (JSON en una línea) se parten en trozos al
  pintarlas. Sin selección por columnas.

## Pendiente o posible

- Teclado en pantalla en el móvil: depende de craft-android, sin probar aquí.
- Imprimir, vista previa de Markdown, selección por columnas, buscar en archivos de una carpeta.
