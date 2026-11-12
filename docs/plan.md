# Plan por fases

Enfoque: **compilar para Android el propio Notepad Studio** (unas 186.000 líneas de Rust sobre
GPUI) en lugar de reescribirlo, y añadir solo la capa de plataforma Android. Así se conservan
todas sus funciones y se sigue a upstream moviendo un submódulo.

| Fase | Contenido | Estado |
|---|---|---|
| 0 | Prueba técnica ([phase-0.md](phase-0.md)): GPUI actualizado en el fork, plataforma Android de gpui-mobile, Notepad Studio como biblioteca, este repositorio, Gradle + cargo-ndk, CI | ✅ la interfaz se ve en DeX (Galaxy S24, Android 17) |
| 1 | Lo que falle en el dispositivo: arranque, ventana, texto, ratón, teclado físico, cerrar y reabrir | ⬜ |
| 2 | Archivos: abrir, guardar y guardar como con el selector de Android (backend de `rfd`), cuadros de mensaje, portapapeles, «Abrir con» y «Compartir» | ⬜ |
| 3 | Escritorio DeX: cursores, título de la ventana, arrastrar y soltar, preguntar al cerrar, autoguardado | ⬜ |
| 4 | Móvil y tablet sin DeX: teclado en pantalla (IME), barras del sistema y notch | ⬜ |
| 5 | Imprimir, enlaces, DexFiles (`io.github.dexfiles.action.OPEN`) | ⬜ |
| 6 | Tamaño del APK, clave de release, releases en GitHub, metadatos de F-Droid | ⬜ |

## Riesgos conocidos

1. **La plataforma Android de GPUI es de la comunidad** (gpui-mobile, «experimental»), no de
   Zed. Su IME y su ciclo de vida están incompletos; lo que falle en DeX habrá que arreglarlo en
   el fork, con lo aprendido en [craft-android](https://github.com/manoloGith/craft-android)
   (ratón, teclado físico, cerrar y reabrir).
2. **Notepad Studio es joven** (v0.0.7, un autor). Si upstream se para, el mantenimiento es
   nuestro.
3. **El fork es grande**: cambia la versión de GPUI de upstream. Rebasarlo cuesta más que en
   DexPdfCraft, salvo que upstream adopte el GPUI nuevo.
4. **Drivers GPU** (Mali, Adreno, Xclipse) con wgpu: solo se sabrá en el dispositivo.
5. **Los tests de Notepad Studio** no se han ejecutado contra el GPUI nuevo.
