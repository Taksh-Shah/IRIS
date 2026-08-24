plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
    id("com.google.devtools.ksp")
    id("com.google.dagger.hilt.android")
}

android {
    namespace = "iriscore"
    compileSdk = 35

    defaultConfig {
        applicationId = "org.iris.mesh"
        minSdk = 26
        targetSdk = 34 // AC-9: stays 34 — avoids the SDK-37 ACCESS_LOCAL_NETWORK cliff.
        versionCode = 1
        versionName = "0.1.0"

        // libiriscode.so ABIs (D-5). armeabi-v7a = size/tradeoff recorded G-AND-6.
        ndk {
            abiFilters += listOf("arm64-v8a", "armeabi-v7a", "x86_64")
        }

        // rust-android-gradle merges target/*/release into jniLibs at build
        // time on the toolchain host; the .so is produced by `cargo ndk build`
        // (AC-1) and shipped under src/main/jniLibs/{abi}/ on release builds.
    }

    buildTypes {
        release {
            isMinifyEnabled = false
            proguardFiles(
                getDefaultProguardFile("proguard-android-optimize.txt"),
                "proguard-rules.pro",
            )
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    packaging {
        resources.excludes += "/META-INF/{AL2.0,LGPL2.1}"
    }

    testOptions {
        unitTests.isReturnDefaultValues = true
        unitTests.all { it.useJUnitPlatform() }
    }
}

// DesignTokenParityTest reads the desktop design mirror (tokens.css,
// commands.js) to assert the two platforms have not drifted. Those files are
// outside the Gradle source set, so without declaring them as inputs the test
// task reports UP-TO-DATE and silently keeps a stale pass after the mirror
// changes — which is exactly the drift the test exists to catch.
tasks.withType<Test>().configureEach {
    inputs
        .files(
            rootProject.projectDir.parentFile
                .resolve("crates/iris-desktop/ui")
                .let { dir -> files(dir.resolve("tokens.css"), dir.resolve("commands.js")) },
        )
        .withPropertyName("irisDesktopDesignMirror")
        .withPathSensitivity(PathSensitivity.RELATIVE)
}

kotlin {
    compilerOptions {
        jvmTarget.set(org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17)
    }
    sourceSets["main"].kotlin.srcDir(file("../../kotlin/src/main/kotlin"))
}

dependencies {
    // --- Compose (Kotlin 2.x compose compiler via plugin; strong-skipping on) ---
    implementation(platform("androidx.compose:compose-bom:2024.12.01"))
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-graphics")
    implementation("androidx.compose.foundation:foundation")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-extended")

    // --- Lifecycle / ViewModel / collectAsStateWithLifecycle ---
    implementation("androidx.core:core-ktx:1.15.0")
    implementation("androidx.activity:activity-compose:1.9.3")
    implementation("androidx.lifecycle:lifecycle-runtime-ktx:2.8.7")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.8.7")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.8.7")

    // --- Hilt (D-6 compile-time DI) ---
    implementation("com.google.dagger:hilt-android:2.57.2")
    ksp("com.google.dagger:hilt-android-compiler:2.57.2")
    implementation("androidx.hilt:hilt-navigation-compose:1.2.0")

    // --- WorkManager (AC-7 cadence) ---
    implementation("androidx.work:work-runtime-ktx:2.9.1")
    implementation("androidx.hilt:hilt-work:1.2.0")
    ksp("androidx.hilt:hilt-compiler:1.2.0")

    // --- Coroutines ---
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.9.0")

    // --- UniFFI generated Kotlin (kotlin/src/main/kotlin, committed) ---
    // The bindings are pure Kotlin + JNA (com.sun.jna); the native bridge is
    // libiriscode.so (AC-1 cargo-ndk build). The `iriscode` package facade
    // (kotlin/.../iriscode/api.kt) re-exports `uniffi.iriscode` for the
    // adapter + shell source sets (AC-11 FQCN surface).
    implementation("net.java.dev.jna:jna:5.14.0")
    
    // --- Unit tests (pure JVM leg; also runs AC-5 AdapterLifecycleTest) ---
    testImplementation("org.junit.jupiter:junit-jupiter:5.10.3")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
    testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.9.0")
    testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.9.0")
    testImplementation("com.google.truth:truth:1.4.4")
}