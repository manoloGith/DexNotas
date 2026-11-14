# Qué comprobar en el dispositivo

Probado en Linux: las 37 pruebas automáticas y la ventana bajo Xvfb (pestañas, resaltado, buscar
con regex y grupos, buscar en pestañas, formatear JSON, guardar en ANSI, macros, sesión al
reabrir, diálogo de cerrar con cambios). **En Android nada se ha ejecutado todavía.**

Si algo falla: el icono «DexNotas diagnostics» (solo en el APK de depuración) muestra el informe
para copiarlo.

## 1. Arranque y ventana (DeX)

- [ ] La app abre y se ve la interfaz: menú, pestañas, editor, barra de estado.
- [ ] Al cambiar el tamaño de la ventana, maximizar y pasar del móvil al monitor, todo se recoloca.
- [ ] El texto se ve nítido y de buen tamaño; Ctrl+rueda, Ctrl++ y Ctrl+- cambian el tamaño.

## 2. Teclado y ratón

- [ ] Escribir: letras, mayúsculas, ñ, tildes (´ + vocal), AltGr (@ # [ { €).
- [ ] Intro mantiene la sangría; Tab inserta 4 espacios; Tab/Mayús+Tab con varias líneas
      seleccionadas sangran o quitan sangría.
- [ ] Flechas, Inicio, Fin, RePág/AvPág, Ctrl+flechas, Mayús+flechas para seleccionar.
- [ ] Ctrl+C, Ctrl+X, Ctrl+V con otras apps; Ctrl+Z y Ctrl+Mayús+Z.
- [ ] Ratón: clic, doble clic (palabra), arrastrar para seleccionar, rueda, menús.
- [ ] Clic derecho en una pestaña: menú (cerrar, cerrar otras, copiar ruta).

## 3. Pestañas y sesión

- [ ] Ctrl+N abre «nuevo 2», «nuevo 3»…; el + de la tira también.
- [ ] Escribir en varias pestañas sin guardar, cerrar la app con la X de DeX y volver a abrirla:
      vuelven todas, con su texto, el punto de «sin guardar» y el cursor donde estaba.
- [ ] Lo mismo cerrando con Atrás y con Archivo ▸ Salir.
- [ ] Cerrar una pestaña con cambios (Ctrl+W o ×) pregunta Guardar / No guardar / Cancelar.
- [ ] Ctrl+Tab y Ctrl+Mayús+Tab cambian de pestaña.

## 4. Abrir y guardar

- [ ] Ctrl+O abre el selector de Android; se pueden elegir varios archivos y abren en pestañas.
- [ ] Ctrl+S en un archivo abierto guarda en el original (comprobar con DexFiles u otra app).
- [ ] Ctrl+S en «nuevo 1» pide nombre y carpeta; el nombre sugerido sale de la primera línea.
- [ ] Guardar como… a otra carpeta.
- [ ] Archivo ▸ Archivos recientes los vuelve a abrir (también tras reiniciar la app).
- [ ] Modificar un archivo abierto desde otra app: se recarga solo si no tenía cambios; si los
      tenía, pregunta.
- [ ] Herramientas ▸ «Guardar también los archivos automáticamente»: tras escribir, el archivo
      se guarda solo a los ~2 s.

## 5. Codificaciones

- [ ] Un archivo UTF-8 con BOM, uno UTF-16 (por ejemplo exportado del Bloc de notas de Windows)
      y uno ANSI con ñ y €: se leen bien y la barra de estado dice la codificación y el fin de
      línea.
- [ ] Codificación ▸ Reabrir con: un archivo mal detectado se lee con otra.
- [ ] Codificación ▸ Convertir a + guardar; BOM sí/no; CRLF/LF/CR. Abrir el resultado en otra app.
- [ ] Guardar en ANSI un texto con caracteres que no caben (日本) avisa y escribe `?`.

## 6. Buscar y reemplazar

- [ ] Ctrl+F: las coincidencias se marcan; Intro y F3 van a la siguiente, Mayús+Intro y
      Mayús+F3 a la anterior; el contador dice cuántas hay.
- [ ] Aa (mayúsculas), W (palabra completa), .* (regex); una regex mal escrita se explica en rojo.
- [ ] Ctrl+H: Reemplazar (primero selecciona, luego reemplaza) y Reemplazar todo; con regex,
      `$1`, `$2` y `\n` en el reemplazo.
- [ ] «En todas las pestañas» (Ctrl+Mayús+F): lista abajo; un clic lleva a la línea.
- [ ] Ctrl+G: ir a una línea.

## 7. Resaltado

- [ ] Abrir .py, .rs, .js, .json, .xml, .html, .css, .md, .sql, .sh, .yaml: colores correctos.
- [ ] Un archivo sin extensión con `#!/bin/bash` se colorea como Shell.
- [ ] Lenguaje ▸ elegir otro a mano; Texto plano lo quita.
- [ ] Un archivo grande (1–2 MB) se mueve con fluidez al escribir y desplazar.
- [ ] Un archivo enorme (50–100 MB): se abre, tiene colores también al final (Ctrl+End),
      escribir y deshacer van fluidos.
- [ ] Un JSON de varios MB en una sola línea: Fin lleva al final con colores; con Ajuste de
      línea se ve partido; Formatear JSON funciona.
- [ ] Toque/clic simple y doble clic seleccionan bien (en el móvil, un toque mueve el cursor).

## 8. Formatear y validar

- [ ] JSON en una línea → Herramientas ▸ Formatear JSON (Ctrl+Alt+J); Compactar; Ctrl+Z lo
      deshace.
- [ ] Un JSON con error → Validar JSON: mensaje con línea y el cursor en ella.
- [ ] XML / HTML / SVG: Formatear (Ctrl+Alt+X), Compactar, Validar.
- [ ] YAML y TOML con un error: Validar indica la línea.
- [ ] Con una selección, solo se formatea lo seleccionado.

## 8b. Vista previa de Markdown y HTML

- [ ] Abrir un .md: Ctrl+Mayús+V (o «Doble panel» en la barra de estado, o Ver ▸ Vista previa)
      abre la vista previa al lado; otra vez Ctrl+Mayús+V la cierra; «Vista previa» la deja sola.
- [ ] Arrastrar el divisor (los tres puntos) cambia el tamaño; doble clic lo pone a la mitad; el
      tamaño se recuerda.
- [ ] En el móvil en vertical, los paneles quedan uno encima del otro.
- [ ] Escribir en el editor actualiza la vista previa; al desplazar el editor, la vista previa le
      sigue.
- [ ] Negrita, cursiva, tachado, código, enlaces, listas (anidadas, numeradas, de tareas), citas,
      tablas con alineación, bloques de código con colores, notas al pie.
- [ ] Una imagen local (`![](imagen.png)` junto al .md) se ve; una de la web muestra su texto.
- [ ] Un enlace web abre el navegador; un enlace a otro .md lo abre en una pestaña.
- [ ] Un .html: títulos, párrafos, listas, tablas, `<pre>`, imágenes; sin CSS ni scripts.
- [ ] La vista previa de cada pestaña vuelve como estaba al reabrir la app.

Motor web (Android, WebView; Ver ▸ Vista previa ▸ «Motor web (CSS y JavaScript)», activado de
serie):

- [ ] Un .html con `<style>` y `<script>`: se ve con sus colores y su JavaScript funciona.
- [ ] Un .md: estilo tipo GitHub, negrita, tablas, casillas, código con los colores del editor.
- [ ] El WebView ocupa exactamente su panel: ni tapa el divisor, ni la barra de estado, ni las
      pestañas; al arrastrar el divisor o cambiar el tamaño de la ventana de DeX se recoloca.
- [ ] Abrir un menú que cae sobre la vista previa, un diálogo (Ctrl+G) o el menú de una pestaña:
      la página se oculta mientras tanto (se ve la vista propia) y vuelve al cerrarlo.
- [ ] Escribir en el editor: el Markdown se actualiza sin parpadeo ni salto; un HTML se recarga
      tras una pausa y conserva el desplazamiento.
- [ ] Desplazar el editor: la página le sigue.
- [ ] Rueda del ratón, clic y selección de texto dentro de la página; con la página enfocada,
      flechas/AvPág/Inicio/Fin la desplazan y Ctrl+C copia; un clic en el editor devuelve el
      teclado al editor; Ctrl+S, Ctrl+Mayús+V, etc. siguen funcionando.
- [ ] Un enlace web abre el navegador; uno a otro .md abre una pestaña; uno a `#seccion` baja a
      ese título.
- [ ] Imágenes locales y de internet en un .md y en un .html.
- [ ] Cambiar a tema claro/oscuro o el zoom (Ctrl++): la página se rehace con ese tema y tamaño.
- [ ] Desactivar «Motor web»: vuelve la vista propia; activarlo otra vez: vuelve la página.

## 9. Macros

- [ ] Ctrl+Mayús+R, escribir y moverse (por ejemplo: Inicio, «- », Abajo), Ctrl+Mayús+R otra vez;
      la barra de estado muestra «Grabando» mientras tanto.
- [ ] Ctrl+Mayús+P la reproduce; Macros ▸ Reproducir varias veces (10); Reproducir hasta el
      final del archivo.
- [ ] Una macro con Buscar siguiente / Reemplazar del panel de búsqueda se repite igual.
- [ ] Macros ▸ Guardar la macro grabada: aparece en el menú con Ctrl+Alt+1, que la reproduce;
      sigue ahí tras reiniciar. Gestionar: renombrar, cambiar atajo, borrar.

## 10. Ecosistema y móvil

- [ ] Desde DexFiles, «Abrir» un .txt, .json o .py: DexNotas aparece y lo abre en una pestaña.
- [ ] DexFiles ▸ Ajustes ▸ Aplicaciones predeterminadas: DexNotas aparece para las extensiones de
      los lenguajes (.rs, .py, .ts, .vue, .jsonc, .mdx, .xsd…) y de texto (.log, .env, .srt,
      .diff, .kt, .gradle…).
- [ ] En DexFiles, «Abrir con» de un archivo con una extensión rara (.foo) o sin extensión:
      DexNotas está en la lista y lo abre como texto.
- [ ] «Abrir con» y «Compartir» un archivo de texto desde otra app.
- [ ] Archivo ▸ Compartir texto: abre el menú de compartir de Android.
- [ ] En el móvil sin DeX: el teclado en pantalla aparece al tocar el editor y se puede escribir.
- [ ] Ayuda ▸ Acerca de; Herramientas ▸ Idioma de la interfaz (Español/English).
