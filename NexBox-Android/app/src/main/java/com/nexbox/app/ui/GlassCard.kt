package com.nexbox.app.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.kyant.backdrop.backdrops.LayerBackdrop
import com.kyant.backdrop.drawBackdrop
import com.kyant.backdrop.effects.blur
import com.kyant.backdrop.effects.lens
import com.kyant.backdrop.effects.vibrancy
import com.nexbox.app.data.AppearanceStore

/**
 * 卡片玻璃的取样源（背景层专用）。
 * 由 NexBoxApp 挂在背景层（壁纸/渐变）上——卡片只采样背景，不采样自己，
 * 避免「把自己的内容糊进自己」的重影。
 */
val LocalCardBackdrop = staticCompositionLocalOf<LayerBackdrop?> { null }

/**
 * 全局卡片容器（设置页「液态玻璃」开关控制）：
 * - 关：普通 surface 卡片（原样式，零变化）
 * - 开：backdrop 液态玻璃卡片——背景透过卡片时在边缘被折射弯曲 + 整面轻模糊，
 *   顶缘自带一道高光（Highlight.Default），卡片内容照常清晰
 *
 * 全应用的 Card 都走这里，改这一处即可全局生效。
 */
@Composable
fun AppCard(
    modifier: Modifier = Modifier,
    shape: RoundedCornerShape = RoundedCornerShape(16.dp),
    content: @Composable () -> Unit,
) {
    val backdrop = LocalCardBackdrop.current
    if (AppearanceStore.glassEnabled && backdrop != null) {
        Box(
            modifier = modifier.drawBackdrop(
                backdrop = backdrop,
                shape = { shape },
                effects = {
                    vibrancy()
                    blur(8f.dp.toPx())
                    // 边缘折射：折射带宽 16dp、最大位移 24dp，与官方演示的卡片一致
                    lens(16f.dp.toPx(), 24f.dp.toPx())
                },
                // 不压任何底色：纯透明 + 模糊 + 边缘折射，与官方演示的卡片一致；
                // 可读性由背景层的遮罩（imageScrim）负责
            )
        ) {
            content()
        }
    } else {
        Card(
            colors = CardDefaults.cardColors(containerColor = MaterialTheme.colorScheme.surface),
            shape = shape,
            modifier = modifier,
        ) {
            content()
        }
    }
}
