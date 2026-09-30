plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "ir.ratatosk.app"
    compileSdk = 35

    defaultConfig {
        applicationId = "ir.ratatosk.app"
        minSdk = 29          // saving to Downloads needs no storage permission from here up
        targetSdk = 35
        versionCode = 1
        versionName = "1.0.0"
        // yt-dlp and ffmpeg ship as native code; keep to the usual phone CPUs.
        ndk { abiFilters += listOf("arm64-v8a", "armeabi-v7a", "x86_64") }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
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
}

dependencies {
    implementation("androidx.core:core-ktx:1.15.0")
    implementation("androidx.appcompat:appcompat:1.7.0")
    implementation("com.google.android.material:material:1.12.0")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.9.0")
    // yt-dlp (YouTube, Instagram and many more sites) and ffmpeg for Android.
    implementation("io.github.junkfood02.youtubedl-android:library:0.17.2")
    implementation("io.github.junkfood02.youtubedl-android:ffmpeg:0.17.2")

    testImplementation("junit:junit:4.13.2")
}
