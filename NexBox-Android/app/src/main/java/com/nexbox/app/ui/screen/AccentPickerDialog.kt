package com.nexbox.app.ui.screen

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Brush
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.nexbox.app.ui.theme.accentContentColor
import com.nexbox.app.ui.theme.parseHexColor
import com.nexbox.app.ui.theme.toHexColorString

/**
 * 调色盘弹窗：HSV 取色。
 *
 * 走 [Dialog] 而不是页内浮层 —— 底部那条悬浮导航条是内容层之外的兄弟节点，
 * 页内浮层盖不住它，弹窗自己的窗口才压得住。
 */
@Composable
internal fun AccentPickerDialog(
    initialHex: String,
    defaultHex: String,
    onApply: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    val fallback = parseHexColor(defaultHex) ?: Color(0xFF4C8DFF)
    var hsv by remember { mutableStateOf((parseHexColor(initialHex) ?: fallback).toHsv()) }
    val candidate = hsv.toColor()

    Dialog(
        onDismissRequest = onDismiss,
        // 默认宽度只有 310dp 左右，色相带会被挤成一条细线
        properties = DialogProperties(usePlatformDefaultWidth = false),
    ) {
        Box(
            modifier = Modifier
                .fillMaxSize()
                .padding(horizontal = 18.dp),
            contentAlignment = Alignment.Center,
        ) {
            Column(
                modifier = Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(22.dp))
                    .background(MaterialTheme.colorScheme.surface)
                    .border(1.dp, MaterialTheme.colorScheme.outlineVariant, RoundedCornerShape(22.dp))
                    .padding(18.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        "调色盘",
                        fontSize = 16.sp,
                        fontWeight = FontWeight.SemiBold,
                        color = MaterialTheme.colorScheme.onSurface,
                        modifier = Modifier.weight(1f),
                    )
                    Text(
                        "重置",
                        fontSize = 12.sp,
                        color = MaterialTheme.colorScheme.primary,
                        modifier = Modifier
                            .clip(RoundedCornerShape(8.dp))
                            .clickable(onClick = { hsv = fallback.toHsv() })
                            .padding(horizontal = 6.dp, vertical = 4.dp),
                    )
                }

                PreviewRow(candidate)
                SaturationValuePanel(hsv = hsv, onChange = { s, v -> hsv = hsv.copy(s = s, v = v) })
                HueBar(hue = hsv.h) { h -> hsv = hsv.copy(h = h) }

                Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    OutlinedButton(
                        onClick = onDismiss,
                        modifier = Modifier.weight(1f),
                        shape = RoundedCornerShape(12.dp),
                    ) { Text("取消") }
                    Button(
                        onClick = { onApply(candidate.toHexColorString()) },
                        modifier = Modifier.weight(1f),
                        shape = RoundedCornerShape(12.dp),
                        colors = ButtonDefaults.buttonColors(
                            containerColor = candidate,
                            contentColor = accentContentColor(candidate),
                        ),
                    ) { Text("应用", fontWeight = FontWeight.SemiBold) }
                }

                // 弹窗窗口不消费手势区，留一点底部余量避免贴住导航条
                Spacer(Modifier.navigationBarsPadding().height(0.dp))
            }
        }
    }
}

/** 预览：色块 + 色值 + 两种真实用法（彩色标签、色底正文），选色时直接看对比 */
@Composable
private fun PreviewRow(candidate: Color) {
    Row(
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Box(
            Modifier
                .size(44.dp)
                .clip(CircleShape)
                .background(candidate),
        )
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            Text(
                candidate.toHexColorString(),
                fontSize = 15.sp,
                fontWeight = FontWeight.SemiBold,
                fontFamily = FontFamily.Monospace,
                color = MaterialTheme.colorScheme.onSurface,
            )
            Text("标签与按钮都会换成这个色", fontSize = 11.sp, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        Text(
            "正文",
            fontSize = 11.5.sp,
            color = accentContentColor(candidate),
            modifier = Modifier
                .clip(RoundedCornerShape(8.dp))
                .background(candidate)
                .padding(horizontal = 10.dp, vertical = 7.dp),
        )
    }
}

/** 饱和度（横）× 明度（纵）面板 */
@Composable
private fun SaturationValuePanel(hsv: Hsv, onChange: (Float, Float) -> Unit) {
    val hueColor = Hsv(h = hsv.h, s = 1f, v = 1f).toColor()
    BoxWithConstraints(
        modifier = Modifier
            .fillMaxWidth()
            .height(160.dp)
            .clip(RoundedCornerShape(12.dp))
            // 两层线性渐变叠出 HSV 平面：横向白→纯色管饱和，纵向透明→黑管明度
            .background(Brush.horizontalGradient(listOf(Color.White, hueColor)))
            .background(Brush.verticalGradient(listOf(Color.Transparent, Color.Black)))
            .scrub { fx, fy -> onChange(fx, 1f - fy) },
    ) {
        ScrubThumb(x = maxWidth * hsv.s, y = maxHeight * (1f - hsv.v))
    }
}

/** 色相带 */
@Composable
private fun HueBar(hue: Float, onChange: (Float) -> Unit) {
    val spectrum = remember { (0..360 step 30).map { Hsv(h = it.toFloat(), s = 1f, v = 1f).toColor() } }
    BoxWithConstraints(
        modifier = Modifier
            .fillMaxWidth()
            .height(18.dp)
            .clip(CircleShape)
            .background(Brush.horizontalGradient(spectrum))
            .scrub { fx, _ -> onChange(fx * 360f) },
    ) {
        ScrubThumb(x = maxWidth * (hue / 360f), y = maxHeight / 2, size = 22.dp)
    }
}

/** 拖拽把手：白圈压深色描边，落在任何颜色上都看得见 */
@Composable
private fun ScrubThumb(x: Dp, y: Dp, size: Dp = 20.dp) {
    Box(
        modifier = Modifier
            .offset {
                IntOffset(
                    x = x.roundToPx() - size.roundToPx() / 2,
                    y = y.roundToPx() - size.roundToPx() / 2,
                )
            }
            .size(size)
            .clip(CircleShape)
            .background(Color.White)
            .border(2.dp, Color.Black.copy(alpha = 0.35f), CircleShape),
    )
}

/** 按下即取值、拖动连续取值、抬手结束本次手势 */
private fun Modifier.scrub(onFraction: (x: Float, y: Float) -> Unit): Modifier = pointerInput(Unit) {
    awaitEachGesture {
        val down = awaitFirstDown(requireUnconsumed = false)
        reportScrub(down.position, size, onFraction)
        while (true) {
            val active = awaitPointerEvent().changes.firstOrNull { it.pressed } ?: break
            reportScrub(active.position, size, onFraction)
            active.consume()
        }
    }
}

private fun reportScrub(position: Offset, size: IntSize, onFraction: (Float, Float) -> Unit) {
    if (size.width == 0 || size.height == 0) return
    onFraction(
        (position.x / size.width).coerceIn(0f, 1f),
        (position.y / size.height).coerceIn(0f, 1f),
    )
}

/** 取色器状态：拖拽每帧只改一个通道，所以存 HSV 而不是 Color */
internal data class Hsv(val h: Float, val s: Float, val v: Float) {
    fun toColor(): Color = Color(android.graphics.Color.HSVToColor(floatArrayOf(h, s, v)))
}

private fun Color.toHsv(): Hsv {
    val out = FloatArray(3)
    android.graphics.Color.colorToHSV(toArgb(), out)
    return Hsv(out[0].mod(360f), out[1], out[2])
}
