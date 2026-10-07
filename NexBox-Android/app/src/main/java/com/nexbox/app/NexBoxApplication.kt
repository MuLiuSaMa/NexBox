package com.nexbox.app

import android.app.Application
import com.nexbox.app.data.AppearanceStore
import com.nexbox.app.data.BackgroundStore
import com.nexbox.app.data.CrosshairStore
import com.nexbox.app.data.NetworkModule
import com.nexbox.app.data.OverlayStore
import com.nexbox.app.data.StatisticsReporter

/** 全局只做初始化：网络单例容器、恢复外观/背景/悬浮设置、首次启动统计（不引 DI 框架） */
class NexBoxApplication : Application() {
    override fun onCreate() {
        super.onCreate()
        NetworkModule.init(this)
        StatisticsReporter.maybeReport(this)
        BackgroundStore.init(this)
        AppearanceStore.init(this)
        OverlayStore.init(this)
        CrosshairStore.init(this)
    }
}
