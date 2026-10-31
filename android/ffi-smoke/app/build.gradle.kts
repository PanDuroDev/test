plugins {
    id("com.android.application")
}

val repositoryRoot = rootProject.projectDir.resolve("../..").canonicalFile
val generatedFfiRoot = layout.buildDirectory.dir("generated/ffi").get().asFile
val generatedJniLibs = generatedFfiRoot.resolve("jniLibs")
val generatedKotlin = generatedFfiRoot.resolve("kotlin")
val x86_64Library = generatedJniLibs.resolve("x86_64/librasm_engine_ffi.so")

android {
    namespace = "com.pandurodev.rasm.ffismoke"
    compileSdk = 35

    defaultConfig {
        applicationId = "com.pandurodev.rasm.ffismoke"
        minSdk = 23
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    sourceSets.getByName("main") {
        // AGP 9 built-in Kotlin requires generated .kt sources in AndroidSourceSet.kotlin.
        kotlin.directories.add(generatedKotlin)
        jniLibs.srcDir(generatedJniLibs)
    }

    testOptions {
        animationsDisabled = true
    }
}

dependencies {
    // UniFFI's Kotlin runtime uses JNA to load and call the Rust shared library.
    implementation("net.java.dev.jna:jna:5.12.0@aar")

    androidTestImplementation("androidx.test:runner:1.6.2")
    androidTestImplementation("androidx.test.ext:junit:1.2.1")
}

val buildRustFfi by tasks.registering(Exec::class) {
    group = "build"
    description = "Build the Rust FFI shared libraries for Android ARM64 and x86_64."
    workingDir(repositoryRoot)
    commandLine(
        "cargo",
        "ndk",
        "-t", "arm64-v8a",
        "-t", "x86_64",
        "-P", "23",
        "-o", generatedJniLibs.absolutePath,
        "build",
        "--release",
        "-p", "rasm-engine-ffi"
    )
    inputs.dir(repositoryRoot.resolve("crates/engine-ffi"))
    inputs.dir(repositoryRoot.resolve("crates/engine-ink"))
    inputs.dir(repositoryRoot.resolve("crates/engine-tiles"))
    outputs.dir(generatedJniLibs)
}

val generateUniFfiKotlin by tasks.registering(Exec::class) {
    group = "build"
    description = "Generate Kotlin sources from the Android x86_64 FFI library metadata."
    dependsOn(buildRustFfi)
    workingDir(repositoryRoot)
    commandLine(
        "cargo",
        "run",
        "-p", "uniffi-bindgen",
        "--",
        "generate",
        "--library", x86_64Library.absolutePath,
        "--language", "kotlin",
        "--out-dir", generatedKotlin.absolutePath
    )
    inputs.file(x86_64Library)
    inputs.file(repositoryRoot.resolve("crates/engine-ffi/uniffi.toml"))
    outputs.dir(generatedKotlin)
}

tasks.named("preBuild").configure {
    dependsOn(generateUniFfiKotlin)
}
