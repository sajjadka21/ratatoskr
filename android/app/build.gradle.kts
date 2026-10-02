plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    buildFeatures { compose = true }
    namespace = "app.ratatoskr.android"
    compileSdk = 35

    defaultConfig {
        applicationId = "app.ratatoskr.android"
        minSdk = 26          // Android 8; from Android 10 saving to Downloads needs no storage permission
        targetSdk = 35
        versionCode = 10
        versionName = "1.3.0"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
        resourceConfigurations += listOf("en", "fa")   // library strings in other languages are dead weight
        // The CPUs shipped are chosen by the per-ABI splits below; abiFilters cannot be combined with them.
    }

    splits {
        abi {
            isEnable = true
            reset()
            include("arm64-v8a", "armeabi-v7a", "x86_64")
            isUniversalApk = false   // one APK per CPU: each is about a third of the size of a universal one
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
    implementation("androidx.activity:activity-compose:1.9.3")
    implementation("androidx.compose.ui:ui:1.7.6")
    implementation("androidx.compose.foundation:foundation:1.7.6")
    implementation("androidx.compose.material3:material3:1.3.1")
    androidTestImplementation("androidx.compose.ui:ui-test-junit4:1.7.6")
    debugImplementation("androidx.compose.ui:ui-test-manifest:1.7.6")
    implementation("com.squareup.okhttp3:okhttp:4.12.0")
    implementation("androidx.core:core-ktx:1.15.0")
    implementation("androidx.appcompat:appcompat:1.7.0")
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.8.7")
    implementation("androidx.lifecycle:lifecycle-viewmodel-ktx:2.8.7")
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.recyclerview:recyclerview:1.3.2")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.9.0")
    // yt-dlp (YouTube, Instagram and many more sites) and ffmpeg for Android.
    implementation("io.github.junkfood02.youtubedl-android:library:0.18.1")
    implementation(files(nativeAar).builtBy(prepareNative))
    implementation("io.github.junkfood02.youtubedl-android:common:0.18.1")
    implementation("commons-io:commons-io:2.5")

    testImplementation("junit:junit:4.13.2")
    testImplementation("org.robolectric:robolectric:4.14.1")
    // Real-screen smoke and accessibility tests, run on an emulator by .github/workflows/android-ui.yml
    androidTestImplementation("androidx.test:runner:1.6.2")
    androidTestImplementation("androidx.test.ext:junit:1.2.1")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.6.1")
    androidTestImplementation("androidx.test.espresso:espresso-contrib:3.6.1")
    androidTestImplementation("androidx.test.espresso:espresso-accessibility:3.6.1")
}
