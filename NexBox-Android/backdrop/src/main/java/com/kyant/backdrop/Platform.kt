package com.kyant.backdrop

import android.os.Build
import androidx.annotation.ChecksSdkIntAtLeast

/* 由 KMP 源合并而来：commonMain 的 expect + androidMain 的 actual，
 * 本模块只面向 Android，直接落成普通函数。
 * RenderEffect 需要 API 31+，AGSL RuntimeShader 需要 API 33+；低于这两档时
 * 相关效果会在调用方守卫下静默降级（只剩 surface 着色）。 */

@ChecksSdkIntAtLeast(Build.VERSION_CODES.S)
fun isRenderEffectSupported(): Boolean = Build.VERSION.SDK_INT >= Build.VERSION_CODES.S

@ChecksSdkIntAtLeast(Build.VERSION_CODES.TIRAMISU)
fun isRuntimeShaderSupported(): Boolean = Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU
