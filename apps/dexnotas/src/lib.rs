//! DexNotas: Notepad Studio on Android.
//!
//! The Android activity (a `NativeActivity`, `dev.gpui.mobile.GpuiInputActivity`) loads this
//! library and android-activity calls [`android_main`] on its own thread. It prepares what the
//! desktop app expects (a home directory, the GPUI platform) and then starts Notepad Studio the
//! way its desktop binary does.

#[cfg(target_os = "android")]
mod android;
