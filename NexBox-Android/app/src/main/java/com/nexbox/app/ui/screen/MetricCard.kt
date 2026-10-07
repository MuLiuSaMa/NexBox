package com.nexbox.app.ui.screen

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.nexbox.app.ui.AppCard
import com.nexbox.app.ui.theme.Danger
import com.nexbox.app.ui.theme.FillColor
import com.nexbox.app.ui.theme.TempWarn
import com.nexbox.app.ui.theme.TextSecondary
import kotlin.math.roundToInt

/**
 * 硬件指标卡：彩色圆点 + 标签 + 右上角温度 + 大数字占用率 + 进度条 + 脚注。
 *
 * 首页（PC 侧读数）与配置页（本机读数）共用，所以按 `SpecRow.kt` 的先例单独成文件。
 */
@Composable
internal fun MetricCard(
    label: String,
    color: Color,
    percent: Int?,
    temp: Double?,
    footnote: String,
    modifier: Modifier = Modifier,
    onClick: (() -> Unit)? = null,
    hint: String? = null,
    tempWarn: Double = 80.0,
    tempDanger: Double = 90.0,
) {
    AppCard(
        modifier = modifier.then(if (onClick != null) Modifier.clickable(onClick = onClick) else Modifier),
        shape = RoundedCornerShape(16.dp),
    ) {
        MetricGauge(
            label = label,
            color = color,
            percent = percent,
            temp = temp,
            footnote = footnote,
            hint = hint,
            tempWarn = tempWarn,
            tempDanger = tempDanger,
        )
    }
}

/** 卡片内容本体，不带 [AppCard] 外壳：一张卡里并排放两个指标时用这个 */
@Composable
internal fun MetricGauge(
    label: String,
    color: Color,
    percent: Int?,
    temp: Double?,
    footnote: String?,
    modifier: Modifier = Modifier,
    hint: String? = null,
    tempWarn: Double = 80.0,
    tempDanger: Double = 90.0,
) {
    Column(
        modifier = modifier.padding(14.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(6.dp).clip(CircleShape).background(color))
            Spacer(Modifier.width(6.dp))
            Text(
                label,
                fontSize = 12.sp,
                fontWeight = FontWeight.SemiBold,
                color = TextSecondary,
            )
            Spacer(Modifier.weight(1f))
            if (temp != null) {
                Text(
                    "${temp.roundToInt()}°C",
                    fontSize = 12.sp,
                    fontWeight = FontWeight.Medium,
                    color = tempColor(temp, tempWarn, tempDanger),
                )
            }
        }

        Text(
            percent?.let { "$it%" } ?: "—",
            fontSize = 26.sp,
            fontWeight = FontWeight.Bold,
            color = if (percent == null) TextSecondary else MaterialTheme.colorScheme.onSurface,
        )

        MetricBar(percent = percent, color = color)

        if (!footnote.isNullOrBlank() || hint != null) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    footnote.orEmpty(),
                    fontSize = 11.sp,
                    color = TextSecondary,
                    maxLines = 1,
                    modifier = Modifier.weight(1f),
                )
                if (hint != null) {
                    Text(hint, fontSize = 10.5.sp, fontWeight = FontWeight.Medium, color = color)
                }
            }
        }
    }
}

/** 占用条。percent 为 null（无数据）时只留底槽，不画进度 */
@Composable
internal fun MetricBar(percent: Int?, color: Color, modifier: Modifier = Modifier) {
    Box(
        modifier = modifier
            .fillMaxWidth()
            .height(5.dp)
            .clip(RoundedCornerShape(50))
            .background(FillColor),
    ) {
        if (percent != null) {
            Box(
                modifier = Modifier
                    .fillMaxWidth(percent.coerceIn(0, 100) / 100f)
                    .fillMaxHeight()
                    .clip(RoundedCornerShape(50))
                    .background(color),
            )
        }
    }
}

/**
 * 温度配色。默认 80/90°C 是 PC 硬件的口径；手机 SoC 常态就偏高，
 * 配置页要另传更低的阈值，否则永远不亮、等于没有配色。
 */
@Composable
internal fun tempColor(t: Double, warn: Double = 80.0, danger: Double = 90.0): Color = when {
    t >= danger -> Danger
    t >= warn -> TempWarn
    else -> TextSecondary
}
