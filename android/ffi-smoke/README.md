# Rasm Studio Android FFI smoke host

This small Android app exists only to verify the Rust/Kotlin boundary at runtime. It is not the product shell and does not include Compose UI, stylus capture, or a renderer.

## Prerequisites

- JDK 17
- Android Gradle Plugin 9.3.1 (with built-in Kotlin support)
- Gradle 9.5.0
- Rust 1.87 or newer
- Android SDK and NDK 28.2.13676358
- Android target `aarch64-linux-android` and `x86_64-linux-android`
- `cargo-ndk 4.1.2`

Set `ANDROID_NDK_HOME` to the installed NDK directory and ensure `cargo`, `cargo-ndk`, and `gradle` are available on `PATH`.

## Build and run

From the repository root:

```sh
gradle -p android/ffi-smoke :app:assembleDebug
gradle -p android/ffi-smoke :app:connectedDebugAndroidTest
```

The app's Gradle build tasks compile `rasm-engine-ffi` for Android ARM64 and x86_64, generate Kotlin bindings from the x86_64 library metadata, and package the shared libraries into the APK.

The instrumentation tests verify that the generated Kotlin bindings load the native library, call `bridgeVersion()`, call `previewStroke()`, and surface rejected input.

## Scope limits

Passing this smoke test proves the Kotlin/JNA/UniFFI call path on the selected emulator ABI. It does not prove physical stylus input, pressure/tilt delivery, Android GPU surface rendering, input latency, or a production app lifecycle.
