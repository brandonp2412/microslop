import java.io.FileInputStream
import java.util.Properties

plugins {
    id("com.android.application")
    id("kotlin-android")
    // The Flutter Gradle Plugin must be applied after the Android and Kotlin Gradle plugins.
    id("dev.flutter.flutter-gradle-plugin")
}

val keystoreProperties = Properties()
val keystorePropertiesFile =
    listOfNotNull(
        rootProject.file("key.properties"),
        System.getenv("ANDROID_SIGNING_PROPERTIES")?.let(::file),
        file("${System.getProperty("user.home")}/.config/android-signing/fdroid.properties"),
        if (System.getProperty("os.name").startsWith("Windows")) {
            file("\\\\host.lan\\Data\\.android-signing\\fdroid.properties")
        } else {
            null
        },
    ).firstOrNull { it.exists() }
keystorePropertiesFile?.let { keystoreProperties.load(FileInputStream(it)) }

val signingStoreFile = keystorePropertiesFile?.let {
    val configuredStoreFile = file(keystoreProperties["storeFile"] as String)
    if (configuredStoreFile.exists()) {
        configuredStoreFile
    } else {
        it.parentFile.resolve(configuredStoreFile.name)
    }
}

android {
    namespace = "app.microslop"
    compileSdk = flutter.compileSdkVersion
    ndkVersion = flutter.ndkVersion

    compileOptions {
        isCoreLibraryDesugaringEnabled = true
        sourceCompatibility = JavaVersion.VERSION_11
        targetCompatibility = JavaVersion.VERSION_11
    }

    kotlinOptions {
        jvmTarget = JavaVersion.VERSION_11.toString()
    }

    defaultConfig {
        applicationId = "app.microslop"
        minSdk = flutter.minSdkVersion
        targetSdk = flutter.targetSdkVersion
        versionCode = flutter.versionCode
        versionName = flutter.versionName
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    signingConfigs {
        if (signingStoreFile != null) {
            create("release") {
                keyAlias = keystoreProperties["keyAlias"] as String
                keyPassword = keystoreProperties["keyPassword"] as String
                storeFile = signingStoreFile
                storePassword = keystoreProperties["storePassword"] as String
            }
        }
    }

    buildTypes {
        debug {
            signingConfig = signingConfigs.getByName("debug")
        }
        getByName("profile") {
            signingConfig = signingConfigs.getByName(if (signingStoreFile != null) "release" else "debug")
        }
        release {
            signingConfig = signingConfigs.getByName(if (signingStoreFile != null) "release" else "debug")
        }
    }

    sourceSets.getByName("main").jniLibs.srcDir(
        layout.buildDirectory.dir("generated/cxxRuntime"),
    )
}

val copyCxxRuntime by tasks.registering(Sync::class) {
    val hostTag =
        when {
            System.getProperty("os.name").startsWith("Mac") -> "darwin-x86_64"
            System.getProperty("os.name").startsWith("Windows") -> "windows-x86_64"
            else -> "linux-x86_64"
        }
    val llvmLibDir =
        android.ndkDirectory.resolve("toolchains/llvm/prebuilt/$hostTag/sysroot/usr/lib")
    val abiTargets =
        mapOf(
            "arm64-v8a" to "aarch64-linux-android",
            "armeabi-v7a" to "arm-linux-androideabi",
            "x86" to "i686-linux-android",
            "x86_64" to "x86_64-linux-android",
        )

    into(layout.buildDirectory.dir("generated/cxxRuntime"))
    abiTargets.forEach { (abi, target) ->
        from(llvmLibDir.resolve("$target/libc++_shared.so")) {
            into(abi)
        }
    }
}

tasks.named("preBuild").configure {
    dependsOn(copyCxxRuntime)
}

dependencies {
    coreLibraryDesugaring("com.android.tools:desugar_jdk_libs:2.1.4")
    androidTestImplementation("androidx.test:runner:1.3.0")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.3.0")
}

flutter {
    source = "../.."
}
