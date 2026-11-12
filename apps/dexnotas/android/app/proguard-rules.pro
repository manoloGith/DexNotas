# The activity is named in the manifest and the rest of dev.gpui.mobile is called from Rust
# through JNI, by name.
-keep class dev.gpui.mobile.** { *; }
