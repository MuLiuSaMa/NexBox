package com.nexbox.app.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.layout.ContentScale
import coil3.compose.AsyncImage
import com.nexbox.app.data.BackgroundStore
import com.nexbox.app.ui.theme.LocalAppPalette
import com.nexbox.app.ui.theme.PageBg
import com.nexbox.app.ui.theme.PageBgTop

/**
 * 应用的统一背景：设置页选的背景图优先（上面叠一层压暗遮罩保正文可读），
 * 没选就是默认的深色渐变。
 *
 * 两处共用：NexBoxApp 的背景层（液态玻璃卡片的采样源），以及三角洲随机装备这类
 * **整页覆盖层**——覆盖层底下还画着三角洲主页，必须自己铺完整背景才能盖住，
 * 自绘的又必须和背景层逐像素一致（同图同裁剪同遮罩），玻璃卡片采样背景层时
 * 才不会和页面里透出的背景错位。
 */
@Composable
fun AppBackground(modifier: Modifier = Modifier) {
    val bgFile = BackgroundStore.file
    val palette = LocalAppPalette.current
    Box(modifier) {
        if (bgFile != null) {
            AsyncImage(
                model = bgFile,
                contentDescription = null,
                contentScale = ContentScale.Crop,
                modifier = Modifier.fillMaxSize(),
            )
            // 薄薄一层遮罩：背景图再花，正文也还读得清（深色压黑、浅色压白）
            Box(Modifier.fillMaxSize().background(palette.imageScrim))
        } else {
            Box(Modifier.fillMaxSize().background(Brush.verticalGradient(listOf(PageBgTop, PageBg))))
        }
    }
}
