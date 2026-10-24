# R8 rules for Cantara's Android build. Copied next to dx's own rules by
# packaging/scripts/build-android-apk.sh; Gradle reads every *.pro there.
#
# The FileProvider is only ever called from Rust, through JNI
# (src/logic/android_share.rs). R8 sees no Java caller of getUriForFile and
# would remove it, and the share sheet would then fail at the first export.
-keep class androidx.core.content.FileProvider { *; }
