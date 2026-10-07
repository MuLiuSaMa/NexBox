package com.nexbox.app.ui

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInHorizontally
import androidx.compose.animation.slideOutHorizontally
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import kotlinx.coroutines.delay

/**
 * 从屏幕右侧滑入、停留片刻后自动向右缩回的轻提示。
 * 取代原来常驻顶部、必须手动点关闭的 MessageBar。
 *
 * message 传 null 即触发缩回动画；退出动画期间用最后一条非空文案占位，避免内容闪空。
 * 由调用方用 `Modifier.align(Alignment.TopEnd)` 放到 Box 顶层，宽度用 fillMaxWidth 控制。
 */
@Composable
fun MessageToast(
    message: String?,
    isError: Boolean,
    onDismiss: () -> Unit,
    modifier: Modifier = Modifier,
    durationMs: Long = 3200L,
) {
    var shown by remember { mutableStateOf(message.orEmpty()) }
    var error by remember { mutableStateOf(isError) }

    // message 变化即重置计时；新提示会取消上一条的延时，避免旧定时器误关新提示
    LaunchedEffect(message) {
        if (message != null) {
            shown = message
            error = isError
            delay(durationMs)
            onDismiss()
        }
    }

    AnimatedVisibility(
        visible = message != null,
        enter = slideInHorizontally(tween(280)) { it } + fadeIn(tween(280)),
        exit = slideOutHorizontally(tween(240)) { it } + fadeOut(tween(240)),
        modifier = modifier,
    ) {
        val color = if (error) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.primary
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(16.dp))
                .background(MaterialTheme.colorScheme.surface)
                .background(color.copy(alpha = 0.14f))
                .clickable(onClick = onDismiss)
                .padding(start = 16.dp, end = 18.dp, top = 13.dp, bottom = 13.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(Modifier.size(8.dp).clip(CircleShape).background(color))
            Spacer(Modifier.width(10.dp))
            Text(
                shown,
                fontSize = 13.sp,
                lineHeight = 18.sp,
                fontWeight = FontWeight.Medium,
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 4,
            )
        }
    }
}
