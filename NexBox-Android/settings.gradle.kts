pluginManagement {
    repositories {
        // 国内镜像优先，官方源兜底
        maven("https://maven.aliyun.com/repository/google")
        maven("https://maven.aliyun.com/repository/public")
        maven("https://maven.aliyun.com/repository/gradle-plugin")
        maven("https://mirrors.cloud.tencent.com/nexus/repository/maven-public/")
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        // 国内镜像优先，官方源兜底
        maven("https://maven.aliyun.com/repository/google")
        maven("https://maven.aliyun.com/repository/public")
        maven("https://mirrors.cloud.tencent.com/nexus/repository/maven-public/")
        google()
        mavenCentral()
    }
}

rootProject.name = "NexBox-Android"
include(":app")
// 液态玻璃库：vendor 自 Kyant0/AndroidLiquidGlass 的 backdrop 模块（2.0.1 源码），
// 官方 Maven 包要求 CMP 1.12 / AGP 9.1+，与本项目工具链不兼容，故本地编译
include(":backdrop")
