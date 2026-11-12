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

## Lo que falló en el dispositivo (Galaxy S24, DeX, Android 17)

1. **Ventana vacía.** gpui-mobile ejecutaba las tareas del hilo principal con su cerrojo de
   estado tomado. Notepad Studio abre su ventana desde una de esas tareas (después de leer la
   sesión en segundo plano), `open_window` pide el mismo cerrojo y el hilo se bloqueaba.
   Arreglo en el fork: `flush_main_thread_tasks` suelta el cerrojo antes de ejecutarlas.
2. **Cierre al abrir la ventana.** El editor pedía la fuente `monospace` y ninguna de las de
   reserva de GPUI existe en Android. Arreglo en el fork: candidatas de Android (Droid Sans
   Mono, Roboto) y, si faltan, cualquier fuente instalada.
3. **Cierre inmediato en el siguiente arranque.** El pánico solo terminaba el hilo de
   `android_main`; Android reutilizaba el proceso y volvía a llamarlo en otro hilo, y GPUI
   exige seguir en el hilo inicial. Arreglo aquí: `panic = "abort"`, terminar el proceso al
   volver de `notepad_studio::main()` y rechazar una segunda llamada.

Los tres se diagnosticaron con el registro `dexnotas.log` y la pantalla «DexNotas diagnóstico»
de las compilaciones de depuración (no hay adb en el dispositivo de pruebas).

## Estado

- ✅ Compila para `aarch64-linux-android` y `x86_64-linux-android` (cargo-ndk, NDK r29).
- ✅ Arranca y muestra la interfaz en DeX, con Vulkan (Adreno 750).
- ⬜ Sin comprobar: ratón, teclado físico, IME, cambio de tamaño, cerrar y reabrir.
- ⬜ Los diálogos de abrir y guardar no tienen backend aún: se cancelan solos (fase 2).
- ⬜ Los cuadros de mensaje («¿guardar los cambios?») tampoco: responden «cancelar».
- ⬜ En cada reanudación se registra un error al buscar `GpuiPlatformView` (una clase Java de
  gpui-mobile que no se incluye); no afecta.

## Qué mirar en DeX (fase 1)

1. ¿Se puede escribir con el teclado físico? ¿Funcionan Ctrl+C/V/Z y las flechas?
2. ¿Ratón: clic, clic derecho, rueda, selección arrastrando?
3. ¿Cambiar el tamaño de la ventana y pasar de pantalla del móvil a monitor?
4. ¿Cerrar y volver a abrir? ¿Vuelven las pestañas?

Si algo falla, el registro está en «DexNotas diagnóstico» (Copiar o Compartir).
