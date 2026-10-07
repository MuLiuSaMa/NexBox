package com.nexbox.app.ui

import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.dp
import com.nexbox.app.R

/**
 * 开屏画面：纯黑背景，中央为标语（tagline），底部为左右排列的新境盒双 Logo。
 * 画面本身无入场动画，仅在结束时由外层 AnimatedVisibility 做淡出。
 */
@Composable
fun SplashScreen() {
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(Color.Black),
    ) {
        // 中间：标语
        Image(
            painter = painterResource(R.drawable.tagline),
            contentDescription = "标语",
            contentScale = ContentScale.Fit,
            modifier = Modifier
                .align(Alignment.Center)
                .padding(horizontal = 48.dp)
                .size(width = 280.dp, height = 120.dp),
        )
        // 底部：新境盒双 Logo（六边形标识在前、中文标准字在后，左右排列）
        // 两个 Image 的尺寸严格按素材实际宽高比给出，避免 box 内出现多余空白，
        // 这样 spacedBy 的 12.dp 就是肉眼看到的真实间距。
        Row(
            modifier = Modifier
                .align(Alignment.BottomCenter)
                .navigationBarsPadding()
                .padding(bottom = 56.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Image(
                painter = painterResource(R.drawable.logo_nexbox),
                contentDescription = "新境盒标识",
                contentScale = ContentScale.Fit,
                modifier = Modifier.size(width = 15.95.dp, height = 18.3.dp),
            )
            Image(
                painter = painterResource(R.drawable.logo_chinese),
                contentDescription = "新境盒",
                contentScale = ContentScale.Fit,
                modifier = Modifier.size(width = 64.86.dp, height = 16.8.dp),
            )
        }
    }
}
