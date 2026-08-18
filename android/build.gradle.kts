// IRIS Android shell — root build (ANDROID-001 AC-6 scaffold).
//
// Toolchain pins (D-4 / D-5 / G-AND-4): Gradle-Gradle-plugin pair pinned at
// scaffold time.
//   - Gradle 8.9 / AGP 8.7.3
//   - Kotlin 2.2.10 + org.jetbrains.kotlin.plugin.compose (Compose compiler
//     shipped with Kotlin)
//   - KSP 2.2.10-2.0.2 for Dagger/Hilt
//   - Hilt 2.55 (compile-time DI, D-6)
plugins {
    id("com.android.application") version "8.7.3" apply false
    id("org.jetbrains.kotlin.android") version "2.2.10" apply false
    id("org.jetbrains.kotlin.plugin.compose") version "2.2.10" apply false
    id("com.google.devtools.ksp") version "2.2.10-2.0.2" apply false
    id("com.google.dagger.hilt.android") version "2.55" apply false
}