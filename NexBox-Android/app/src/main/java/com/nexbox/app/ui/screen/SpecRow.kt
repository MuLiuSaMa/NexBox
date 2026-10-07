package com.nexbox.app.ui.screen

import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import com.nexbox.app.ui.theme.TextSecondary

/**
 * 规格信息行：值缺失时整行不显示，比一排「—」干净。
 *
 * 主页硬件详情页（[HardwareDetailScreen]）与配置页的本机信息卡共用一份。
 */
@Composable
internal fun SpecRow(label: String, value: String?) {
    if (value == null) return
    Row(verticalAlignment = Alignment.CenterVertically) {
        Text(label, fontSize = 11.5.sp, color = TextSecondary)
        Spacer(Modifier.weight(1f))
        Text(
            value,
            fontSize = 12.sp,
            fontWeight = FontWeight.Medium,
            color = MaterialTheme.colorScheme.onSurface,
        )
    }
}
