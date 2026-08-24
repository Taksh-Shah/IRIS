// IRIS Android shell — root build (ANDROID-001 AC-6 scaffold).
//
// Toolchain pins (D-4 / D-5 / G-AND-4): Gradle-Gradle-plugin pair pinned at
// scaffold time.
//   - Gradle 8.9 / AGP 8.7.3
//   - Kotlin 2.2.10 + org.jetbrains.kotlin.plugin.compose (Compose compiler
//     shipped with Kotlin)
//   - KSP 2.2.10-2.0.2 for Dagger/Hilt
//   - Hilt 2.57.2 (compile-time DI, D-6). Must be >= 2.57: earlier releases
//     bundle kotlin-metadata-jvm 2.1.0, which rejects the 2.2.0 metadata
//     emitted by Kotlin 2.2.10 ("maximum supported version is 2.1.0") and
//     fails :app:hiltJavaCompile*.
plugins {
    id("com.android.application") version "8.7.3" apply false
    id("org.jetbrains.kotlin.android") version "2.2.10" apply false
    id("org.jetbrains.kotlin.plugin.compose") version "2.2.10" apply false
    id("com.google.devtools.ksp") version "2.2.10-2.0.2" apply false
    id("com.google.dagger.hilt.android") version "2.57.2" apply false
}