package com.nexbox.app.ui.theme

import androidx.compose.material3.Typography
import androidx.compose.ui.text.font.Font
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import com.nexbox.app.R

// MiSans 原样打包（res/font/misans.ttf），作为全局默认字体。
// 说明：当前 Material3 版本的 Typography 没有 defaultFontFamily 参数，
// 这里显式给每个文字角色套上 MiSans，保证全界面统一使用。
val MiSans = FontFamily(
    Font(R.font.misans, FontWeight.Normal),
    Font(R.font.misans, FontWeight.Medium),
    Font(R.font.misans, FontWeight.Bold),
)

fun NexBoxTypography(family: FontFamily = MiSans): Typography = Typography().run {
    Typography(
        displayLarge = displayLarge.copy(fontFamily = family),
        displayMedium = displayMedium.copy(fontFamily = family),
        displaySmall = displaySmall.copy(fontFamily = family),
        headlineLarge = headlineLarge.copy(fontFamily = family),
        headlineMedium = headlineMedium.copy(fontFamily = family),
        headlineSmall = headlineSmall.copy(fontFamily = family),
        titleLarge = titleLarge.copy(fontFamily = family),
        titleMedium = titleMedium.copy(fontFamily = family),
        titleSmall = titleSmall.copy(fontFamily = family),
        bodyLarge = bodyLarge.copy(fontFamily = family),
        bodyMedium = bodyMedium.copy(fontFamily = family),
        bodySmall = bodySmall.copy(fontFamily = family),
        labelLarge = labelLarge.copy(fontFamily = family),
        labelMedium = labelMedium.copy(fontFamily = family),
        labelSmall = labelSmall.copy(fontFamily = family),
    )
}
