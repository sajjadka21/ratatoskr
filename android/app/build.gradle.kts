plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "app.ratatoskr.android"
    compileSdk = 35

    defaultConfig {
        applicationId = "app.ratatoskr.android"
        minSdk = 29          // saving to Downloads needs no storage permission from here up
        targetSdk = 35
        versionCode = 5
        versionName = "1.1.1"
        // yt-dlp and ffmpeg ship as native code; keep to the usual phone CPUs.
        ndk { abiFilters += listOf("arm64-v8a", "armeabi-v7a", "x86_64") }
    }

    splits {
        abi {
            isEnable = true
            reset()
            include("arm64-v8a", "armeabi-v7a", "x86_64")
            isUniversalApk = true
        }
    }

    // Public releases always use the same private keystore. Debug builds keep
    // their own key; never fall back to it for a release.
    val releaseKeystore = System.getenv("ANDROID_KEYSTORE_PATH")?.takeIf { it.isNotBlank() }
    signingConfigs {
        create("release") {
            storeFile = releaseKeystore?.let { file(it) }
            storePassword = System.getenv("ANDROID_KEYSTORE_PASSWORD")
            keyAlias = System.getenv("ANDROID_KEY_ALIAS")
            keyPassword = System.getenv("ANDROID_KEY_PASSWORD")
        }
    }
    buildTypes {
        release {
            isMinifyEnabled = false
            signingConfig = signingConfigs.getByName("release")
        }
    }
    packaging {
        jniLibs { useLegacyPackaging = true }   // the bundled yt-dlp runs from extracted libraries
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
    testOptions { unitTests.isIncludeAndroidResources = true }
}

// Build the official FFmpeg AAR with source-built, ABI-checked WebP libraries.
val nativeAar = rootProject.layout.buildDirectory.file("native/ffmpeg-0.18.1-16k.aar")
val prepareNative by tasks.registering(Exec::class) {
    val sdk = android.sdkDirectory.absolutePath
    inputs.files(rootProject.file("../scripts/build_android_native.py"), rootProject.file("../scripts/verify_android_native.py"))
    outputs.file(nativeAar)
    workingDir(rootProject.projectDir.parentFile)
    val python = System.getenv("PYTHON") ?: if (System.getProperty("os.name").startsWith("Windows")) "python" else "python3"
    commandLine(python, "scripts/build_android_native.py", "--sdk", sdk, "--output", nativeAar.get().asFile.absolutePath)
}
dependencies {
    implementation("com.squareup.okhttp3:okhttp:4.12.0")
    implementation("androidx.core:core-ktx:1.15.0")
    implementation("androidx.appcompat:appcompat:1.7.0")
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.8.7")
    implementation("androidx.lifecycle:lifecycle-viewmodel-ktx:2.8.7")
    implementation("com.google.android.material:material:1.12.0")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.9.0")
    // yt-dlp (YouTube, Instagram and many more sites) and ffmpeg for Android.
    implementation("io.github.junkfood02.youtubedl-android:library:0.18.1")
    implementation(files(nativeAar).builtBy(prepareNative))
    implementation("io.github.junkfood02.youtubedl-android:common:0.18.1")
    implementation("commons-io:commons-io:2.5")

    testImplementation("junit:junit:4.13.2")
    testImplementation("org.robolectric:robolectric:4.14.1")
}
