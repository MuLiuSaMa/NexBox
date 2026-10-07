package com.nexbox.app

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.core.view.WindowCompat
import com.nexbox.app.ui.NexBoxApp

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // edge-to-edge：让系统栏与内容共存，配合 WindowInsets 处理
        WindowCompat.setDecorFitsSystemWindows(window, false)
        setContent {
            NexBoxApp()
        }
    }
}
