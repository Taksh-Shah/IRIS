// Add project specific ProGuard rules here.
// IRIS keeps the Rust engine (libiriscode.so) uncompressed & unrenamed so the
// JNI entry point and UniFFI-generated stubs keep their exact FQCNs.
-keep class iriscode.** { *; }
-dontwarn iriscode.**
-keepclassmembers class * {
    @dagger.internal.* <methods>;
}