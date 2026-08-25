import java.util.Properties
import java.io.FileInputStream
import groovy.json.JsonSlurper

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("rust")
}

val tauriProperties = Properties().apply {
    val propFile = file("tauri.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}

val versionPattern = Regex("""^(\d+)\.(\d+)\.(\d+)$""")

val tauriVersionName = tauriProperties.getProperty("tauri.android.versionName")?.ifBlank { null } ?: run {
    val tauriConfFile = file("../../../tauri.conf.json")
    check(tauriConfFile.exists()) { "Missing tauri.conf.json at ${tauriConfFile.path}" }

    val tauriConf = tauriConfFile.reader(Charsets.UTF_8).use { JsonSlurper().parse(it) as? Map<*, *> }
        ?: error("Failed to parse ${tauriConfFile.path} as a JSON object")
    tauriConf["version"] as? String
        ?: error("Missing string field \"version\" in ${tauriConfFile.path}")
}

val tauriVersionMatch = versionPattern.matchEntire(tauriVersionName)
    ?: error("Android version must use x.y.z format, but got \"$tauriVersionName\"")

val tauriVersionCode = if (tauriProperties.getProperty("tauri.android.versionName")?.ifBlank { null } != null) {
    val versionCodeProp = tauriProperties.getProperty("tauri.android.versionCode")
    if (versionCodeProp != null) {
        versionCodeProp.toIntOrNull()
            ?: error("Property \"tauri.android.versionCode\" must be an integer, but got \"$versionCodeProp\"")
    } else {
        val (major, minor, patch) = tauriVersionMatch.destructured
        major.toInt() * 1_000_000 + minor.toInt() * 1_000 + patch.toInt()
    }
} else {
    val (major, minor, patch) = tauriVersionMatch.destructured
    major.toInt() * 1_000_000 + minor.toInt() * 1_000 + patch.toInt()
}

android {
    compileSdk = 34
    namespace = "com.anidev.anfeasytier"
    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "false"
        applicationId = "com.anidev.anfeasytier"
        minSdk = 24
        targetSdk = 34
        versionCode = tauriVersionCode
        versionName = tauriVersionName
    }
    signingConfigs {
        create("release") {
            // 签名凭据优先取环境变量（CI Secrets），避免依赖入库文件。
            // 环境变量：ANDROID_KEYSTORE_STORE_FILE / ANDROID_KEYSTORE_STORE_PASSWORD /
            //           ANDROID_KEYSTORE_KEY_ALIAS / ANDROID_KEYSTORE_KEY_PASSWORD
            val keystorePropertiesFile = rootProject.file("keystore.properties")
            val keystoreProperties = Properties()
            if (keystorePropertiesFile.exists()) {
                keystoreProperties.load(FileInputStream(keystorePropertiesFile))
            }
            fun env(name: String) = System.getenv(name)

            keyAlias = env("ANDROID_KEYSTORE_KEY_ALIAS")
                ?: keystoreProperties["keyAlias"] as? String
                ?: "upload"
            keyPassword = env("ANDROID_KEYSTORE_KEY_PASSWORD")
                ?: keystoreProperties["keyPassword"] as? String
                ?: ""
            storeFile = file(
                env("ANDROID_KEYSTORE_STORE_FILE")
                    ?: keystoreProperties["storeFile"] as? String
                    ?: "upload-keystore.jks"
            )
            storePassword = env("ANDROID_KEYSTORE_STORE_PASSWORD")
                ?: keystoreProperties["storePassword"] as? String
                ?: ""
        }
    }
    buildTypes {
        getByName("debug") {
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            isDebuggable = true
            isJniDebuggable = true
            isMinifyEnabled = false
            packaging {                jniLibs.keepDebugSymbols.add("*/arm64-v8a/*.so")
                jniLibs.keepDebugSymbols.add("*/armeabi-v7a/*.so")
                jniLibs.keepDebugSymbols.add("*/x86/*.so")
                jniLibs.keepDebugSymbols.add("*/x86_64/*.so")
            }
        }
        getByName("release") {
            isMinifyEnabled = true
            proguardFiles(
                *fileTree(".") { include("**/*.pro") }
                    .plus(getDefaultProguardFile("proguard-android-optimize.txt"))
                    .toList().toTypedArray()
            )
            signingConfig = signingConfigs.getByName("release")
        }
    }
    kotlinOptions {
        jvmTarget = "1.8"
    }
    buildFeatures {
        buildConfig = true
    }
}

rust {
    rootDirRel = "../../../"
}

dependencies {
    implementation("androidx.webkit:webkit:1.6.1")
    implementation("androidx.appcompat:appcompat:1.6.1")
    implementation("com.google.android.material:material:1.8.0")
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.4")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.0")
}

apply(from = "tauri.build.gradle.kts")
