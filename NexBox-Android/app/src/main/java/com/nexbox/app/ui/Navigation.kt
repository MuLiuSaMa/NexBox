package com.nexbox.app.ui

import androidx.annotation.DrawableRes
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Gamepad
import androidx.compose.material.icons.rounded.GridView
import androidx.compose.material.icons.rounded.Home
import androidx.compose.material.icons.rounded.Settings
import androidx.compose.material.icons.rounded.Smartphone
import androidx.compose.ui.graphics.vector.ImageVector
import com.nexbox.app.R

/**
 * 底部导航的分页。
 *
 * [bitmapIcon] 非 0 时优先于 [icon]：三角洲的图标沿用 PC 端那份白色剪影 logo，
 * Material 图标库里没有对应的矢量图，只能走位图资源。
 */
enum class NexBoxTab(
    val label: String,
    val icon: ImageVector,
    @param:DrawableRes val bitmapIcon: Int,
) {
    Home("主页", Icons.Rounded.Home, 0),
    Config("本机", Icons.Rounded.Smartphone, 0),
    Function("工具", Icons.Rounded.GridView, 0),
    Delta("三角洲", Icons.Rounded.Gamepad, R.drawable.ic_tab_deltaforce),
    Settings("设置", Icons.Rounded.Settings, 0),
}
