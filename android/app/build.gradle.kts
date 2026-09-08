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

        // Tier-5 relay endpoints, supplied at build time only for controlled
        // lab/deployment variants. Empty is safe: the IP transport stays
        // unavailable until a trusted endpoint is configured.
        val relayEndpoints = (project.findProperty("IRIS_RELAY_ENDPOINTS") as String?) ?: ""
        buildConfigField("String", "IRIS_RELAY_ENDPOINTS", "\"${relayEndpoints.replace("\\", "\\\\").replace("\"", "\\\"")}\"")
        val relayServerName = (project.findProperty("IRIS_RELAY_SERVER_NAME") as String?) ?: ""
        buildConfigField("String", "IRIS_RELAY_SERVER_NAME", "\"${relayServerName.replace("\\", "\\\\").replace("\"", "\\\"")}\"")

        // libiriscode.so ABIs (D-5). armeabi-v7a = size/tradeoff recorded G-AND-6.
        ndk {
            abiFilters += listOf("arm64-v8a", "armeabi-v7a", "x86_64")
        }

        // HV-2: the androidTest APK is the Mobly `iris_bench` snippet — driven
        // from the laptop via `adb shell am instrument`, not JUnit.
        testInstrumentationRunner = "com.google.android.mobly.snippet.SnippetRunner"

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
        // bcprov-jdk18on ships multi-release-jar OSGi metadata that collides
        // with another dependency's copy of the same path; neither is
        // needed at runtime (OSGi framework metadata, not code).
        resources.excludes += "/META-INF/versions/9/OSGI-INF/MANIFEST.MF"
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

    // --- Offline trusted-peer QR pairing ---
    val cameraXVersion = "1.5.3"
    implementation("androidx.camera:camera-core:$cameraXVersion")
    implementation("androidx.camera:camera-camera2:$cameraXVersion")
    implementation("androidx.camera:camera-lifecycle:$cameraXVersion")
    implementation("androidx.camera:camera-view:$cameraXVersion")
    // Bundled model: scanning works immediately after sideload/install with no
    // Play Services or first-use network download.
    implementation("com.google.mlkit:barcode-scanning:17.3.0")
    implementation("com.google.zxing:core:3.5.3")

    // --- Hilt (D-6 compile-time DI) ---
    implementation("com.google.dagger:hilt-android:2.57.2")
    ksp("com.google.dagger:hilt-android-compiler:2.57.2")
    implementation("androidx.hilt:hilt-navigation-compose:1.2.0")
    implementation("androidx.navigation:navigation-compose:2.9.0")

    // --- WorkManager (AC-7 cadence) ---
    implementation("androidx.work:work-runtime-ktx:2.9.1")
    implementation("androidx.hilt:hilt-work:1.2.0")
    ksp("androidx.hilt:hilt-compiler:1.2.0")

    // --- Room (WP2: durable message + pending-send store) ---
    val roomVersion = "2.7.0"
    implementation("androidx.room:room-runtime:$roomVersion")
    implementation("androidx.room:room-ktx:$roomVersion")
    ksp("androidx.room:room-compiler:$roomVersion")

    // --- Coroutines ---
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.9.0")

    // --- UniFFI generated Kotlin (kotlin/src/main/kotlin, committed) ---
    // The bindings are pure Kotlin + JNA (com.sun.jna); the native bridge is
    // libiriscode.so (AC-1 cargo-ndk build). The `iriscode` package facade
    // (kotlin/.../iriscode/api.kt) re-exports `uniffi.iriscode` for the
    // adapter + shell source sets (AC-11 FQCN surface).
    // Ed25519 provider for pre-API-33 devices (KeystoreEd25519.SoftwareBackend):
    // confirmed on a real API-31 device that Android's built-in Conscrypt JCA
    // provider does not implement Ed25519 below API 33 - `KeyPairGenerator
    // .getInstance("Ed25519")` throws NoSuchAlgorithmException with no
    // provider argument, since none of the default-registered providers
    // support it on that OS version. BouncyCastle is a pure-Java Ed25519
    // implementation that works on every API level; registered explicitly by
    // name ("BC") rather than relying on provider search order, since
    // Android also ships its own stripped internal BC used by the TLS stack
    // that does not expose full public-key crypto to apps and can otherwise
    // shadow this one.
    implementation("org.bouncycastle:bcprov-jdk18on:1.78.1")

    // @aar (not the plain jar) is required for Android: the default jar
    // artifact bundles native dispatch libs for darwin/linux/win32 desktop
    // ABIs only (com/sun/jna/{darwin,linux,win32}-aarch64/...) - no
    // android-aarch64 at all. The AAR variant is the one that packages
    // jniLibs/<abi>/libjnidispatch.so for Android ABIs, which AGP then
    // merges into the APK the normal way. Without @aar this crashes on
    // first launch on every device: UnsatisfiedLinkError, native library
    // (com/sun/jna/android-aarch64/libjnidispatch.so) not found.
    implementation("net.java.dev.jna:jna:5.14.0@aar")
    
    // --- Unit tests (pure JVM leg; also runs AC-5 AdapterLifecycleTest) ---
    testImplementation("org.junit.jupiter:junit-jupiter:5.10.3")
    testRuntimeOnly("org.junit.platform:junit-platform-launcher")
    testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-test:1.9.0")
    testImplementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.9.0")
    testImplementation("com.google.truth:truth:1.4.4")
    testImplementation("org.json:json:20240303")

    // --- HV-2: Mobly snippet (androidTest APK = the iris_bench device surface) ---
    androidTestImplementation("com.google.android.mobly:mobly-snippet-lib:1.4.0")
    androidTestImplementation("androidx.test:runner:1.6.2")
    androidTestImplementation("androidx.test:core:1.6.1")

    // --- WP13: Compose UI tests (run with AndroidJUnitRunner, not SnippetRunner) ---
    androidTestImplementation("androidx.compose.ui:ui-test-junit4")
    androidTestImplementation("androidx.test.ext:junit:1.2.1")
    debugImplementation("androidx.compose.ui:ui-test-manifest")
}
