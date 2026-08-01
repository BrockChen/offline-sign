plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "com.ecohash.btcwallate"
    compileSdk = 34

    defaultConfig {
        applicationId = "com.ecohash.btcwallate"
        minSdk = 23              // 设备 SM901 = Android 6.0.1 (API 23)
        targetSdk = 34
        versionCode = 1
        versionName = "1.0"
        ndk { abiFilters += listOf("arm64-v8a") }   // 设备主 ABI
    }
    buildTypes {
        release { isMinifyEnabled = false }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
}

dependencies {
    implementation("androidx.appcompat:appcompat:1.6.1")
    implementation("com.google.android.material:material:1.11.0")
    implementation("androidx.security:security-crypto:1.1.0-alpha06")  // EncryptedSharedPreferences（对标 iOS Keychain）
    implementation("androidx.biometric:biometric:1.1.0")               // 指纹（对标 Touch ID）
    implementation("com.google.zxing:core:3.5.3")                      // 纯解码/编码（无 support 依赖）
    implementation("androidx.camera:camera-camera2:1.3.4")             // 相机扫码（CameraX，minSdk 21 兼容 23）
    implementation("androidx.camera:camera-lifecycle:1.3.4")
    implementation("androidx.camera:camera-view:1.3.4")
}
