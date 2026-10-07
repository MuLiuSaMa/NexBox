import com.android.build.gradle.internal.api.BaseVariantOutputImpl
import org.jetbrains.kotlin.gradle.dsl.JvmTarget
import java.util.Properties

// 发布签名只从本地 local.properties 取（该文件已在 .gitignore 里），代码仓库不存证书也不存口令。
// 四项缺任何一项就退回不签名：CI 和没拿到证书的人照样能构建，不会把别人卡死。
val keystoreProps = Properties().apply {
    val f = rootProject.file("local.properties")
    if (f.exists()) f.inputStream().use { load(it) }
}
fun signingProp(key: String): String? = keystoreProps.getProperty(key)?.trim()?.takeIf { it.isNotEmpty() }
val signingReady = listOf(
    "nexbox.store.file", "nexbox.store.password", "nexbox.key.alias", "nexbox.key.password",
).all { signingProp(it) != null }

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("org.jetbrains.kotlin.plugin.compose")
    id("org.jetbrains.kotlin.plugin.serialization")
}

android {
    namespace = "com.nexbox.app"
    // 37：玻璃效果（GraphicsLayer + RenderEffect/AGSL）与 Compose 1.8 图形栈需要的较新 SDK
    compileSdk = 37

    defaultConfig {
        applicationId = "com.nexbox.app"
        minSdk = 26
        targetSdk = 36
        versionCode = 2
        versionName = "0.1.1"
    }

    signingConfigs {
        if (signingReady) {
            create("release") {
                storeFile = file(signingProp("nexbox.store.file")!!)
                storePassword = signingProp("nexbox.store.password")
                keyAlias = signingProp("nexbox.key.alias")
                keyPassword = signingProp("nexbox.key.password")
            }
        }
    }

    buildTypes {
        debug {
            // 与正式版并存：正式版用的是另一份签名，debug 包无法覆盖安装，
            // 加个后缀就能单独装上去做真机排查（不影响正式版与其数据）
            applicationIdSuffix = ".dbg"
        }
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            if (signingReady) signingConfig = signingConfigs.getByName("release")
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    // 安装包统一命名：nexbox_android_<版本号>.apk（版本号跟 defaultConfig 走，改名只需动一处）
    applicationVariants.all {
        val vName = versionName
        outputs.all {
            (this as BaseVariantOutputImpl).outputFileName = "nexbox_android_${vName}.apk"
        }
    }
}

kotlin {
    compilerOptions {
        jvmTarget.set(JvmTarget.JVM_17)
    }
}

dependencies {
    implementation(platform("androidx.compose:compose-bom:2025.05.01"))
    implementation("androidx.activity:activity-compose:1.10.1")
    implementation("androidx.core:core-ktx:1.16.0")
    implementation("androidx.lifecycle:lifecycle-runtime-compose:2.10.0")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.10.0")
    implementation("androidx.compose.foundation:foundation")
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.material:material-icons-extended")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.compose.ui:ui-tooling-preview")
    implementation("io.coil-kt.coil3:coil-compose:3.3.0")
    // Coil 3 把网络能力拆成了独立 artifact：只装 coil-compose 时 https:// 图片没有可用
    // Fetcher（本地 File 能用），Epic 封面就是因此始终加载不出来
    implementation("io.coil-kt.coil3:coil-network-okhttp:3.3.0")
    debugImplementation("androidx.compose.ui:ui-tooling")

    implementation("top.yukonga.miuix.kmp:miuix:0.8.8")

    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.11.0")
    implementation("org.jetbrains.kotlinx:kotlinx-serialization-json:1.11.0")

    // 后端对接：REST + WebSocket
    implementation("com.squareup.okhttp3:okhttp:4.12.0")
    // 本地持久化 host / token / deviceId
    implementation("androidx.datastore:datastore-preferences:1.1.7")
    // 扫码配对：ZXing 纯本地解码，不依赖 GMS（国内可用）
    implementation("com.journeyapps:zxing-android-embedded:4.3.0")

    // 液态玻璃：真·背景折射 + 模糊 + 高光，vendor 的 Kyant0 backdrop 库源码（:backdrop 模块）。
    // Maven 包 io.github.kyant0:backdrop:2.0.1 要 CMP 1.12 → AGP 9.1+，与本项目 AGP 8.13 不兼容
    implementation(project(":backdrop"))
}

// 部分依赖（曾为 Haze Glass，现防其他传递依赖）会把 lifecycle 拖到 2.11，而 2.11 要求 AGP 9.1+。
// 压回 2.10.0（minAGP 8.6 / minCompileSdk 35，与本项目工具链兼容；2.9→2.10 API 兼容）
configurations.all {
    resolutionStrategy {
        force("androidx.lifecycle:lifecycle-runtime-compose:2.10.0")
        force("androidx.lifecycle:lifecycle-viewmodel-compose:2.10.0")
        force("androidx.lifecycle:lifecycle-runtime-compose-android:2.10.0")
        force("androidx.lifecycle:lifecycle-viewmodel-compose-android:2.10.0")
    }
}
