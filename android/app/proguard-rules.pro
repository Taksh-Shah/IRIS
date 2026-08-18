// Add project specific ProGuard rules here.
// IRIS keeps the Rust engine (libiriscode.so) uncompressed & unrenamed so the
// JNI entry point and UniFFI-generated stubs keep their exact FQCNs.
-keep class iriscore.uniffi.iriscode.** { *; }
-dontwarn iriscore.uniffi.iriscode.**
-keepclassmembers class * {
    @dagger.internal.* <methods>;
}