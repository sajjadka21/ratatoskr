# Android native payloads

The official youtubedl-android 0.18.1 AAR still contains five WebP libraries linked for 4 KB pages inside libffmpeg.zip.so. Upstream issue: https://github.com/yausername/youtubedl-android/issues/365

Gradle automatically runs scripts/build_android_native.py before resolving the local FFmpeg AAR. It downloads SHA-256-pinned official libwebp 1.6.0 source and FFmpeg 0.18.1 AAR, rebuilds all five WebP libraries for arm64-v8a/armeabi-v7a/x86_64, preserves SONAMEs and verifies the existing public WebP/SharpYuv exports remain available. It never alters an ELF alignment header in place. Source licence, patent grant and provenance are included inside the APK.

Install SDK packages with sdkmanager: ndk;27.2.12479018 and cmake;3.22.1. Use Python 3.12+; set PYTHON to its executable on Windows when necessary. No signing key is needed for a debug build. Native inputs/build outputs remain in the ignored android/build/native directory.

CI runs scripts/verify_android_native.py on the final APK, checking every nested ELF executable/library, not just the outer jni directory. The 64-bit PT_LOAD alignment must be at least 16384 and file offsets/virtual addresses must be congruent. 32-bit Android continues to use 4 KB pages.

This verifies binary layout, not physical-device behaviour. Before broad promotion, test install/update, a complete media download and FFmpeg remux on 4 KB and 16 KB devices.
