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
        ndk { abiFilters += listOf("arm64-v8a") }   // 阶段 A 只打 arm64（设备主 ABI）
    }
    buildTypes {
        release { isMinifyEnabled = false }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }
    // libbtcwallate_jni.so 由 cargo-ndk 输出到 src/main/jniLibs/<abi>/
}

dependencies {
    // 阶段 A 最小验证：纯 android.app.Activity + 系统 Material 主题，无需 AndroidX。
    // 阶段 B 做完整 UI（相机/ZXing 扫码、动画二维码等）时再引入所需依赖。
}
