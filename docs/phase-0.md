# Fase 0: prueba técnica

Objetivo: saber si Notepad Studio puede compilarse para Android sin reescribirlo, y dejar un APK
que se pueda probar en DeX.

## Qué se hizo

1. **GPUI nuevo en el fork.** Notepad Studio traía un GPUI sin plataforma conectable. Al pasar a
   la revisión `294169ae` de Zed salieron 128 errores en 53 archivos, todos mecánicos; corregidos,
   la app compila y arranca en Linux con el renderizador wgpu.
2. **Plataforma Android.** gpui-mobile compiló contra ese GPUI sin cambios de código (solo su
   `Cargo.toml`). `gpui_platform` lo devuelve en Android.
3. **Notepad Studio para Android.** Hizo falta: el sustituto de `rfd`, dejar fuera la papelera,
   y funciones vacías para terminal y gestor de archivos. Además la app pasó a ser una biblioteca
   para que este repositorio pueda arrancarla.
4. **Este repositorio.** `apps/dexnotas`: `android_main` prepara `HOME` y las carpetas XDG en el
   almacenamiento privado, crea la plataforma de GPUI y llama a `notepad_studio::main()`. La
   actividad es `dev.gpui.mobile.GpuiInputActivity` (una `NativeActivity`).

Detalle de los cambios del fork: [fork.md](fork.md).

## Qué se sabe y qué no

- ✅ Compila para `aarch64-linux-android` (cargo-ndk, NDK r29).
- ⬜ **No se ha ejecutado en ningún dispositivo ni emulador.** No se sabe todavía si arranca, si
  dibuja, ni cómo responden el ratón, el teclado y el IME en DeX.
- ⬜ Los diálogos de abrir y guardar no tienen backend aún: se cancelan solos (fase 2).
- ⬜ Los cuadros de mensaje («¿guardar los cambios?») tampoco: responden «cancelar».

## Qué mirar en la primera prueba en DeX

1. ¿Arranca y se ve la interfaz (menú, barra de herramientas, pestañas, editor)?
2. ¿Se puede escribir con el teclado físico? ¿Funcionan Ctrl+C/V/Z y las flechas?
3. ¿Ratón: clic, clic derecho, rueda, selección arrastrando?
4. ¿Cambiar el tamaño de la ventana y pasar de pantalla del móvil a monitor?
5. ¿Cerrar y volver a abrir? ¿Vuelven las pestañas?

Si falla el arranque, `adb logcat -s dexnotas gpui-android` muestra el registro.
