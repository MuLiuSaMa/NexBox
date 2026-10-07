package com.nexbox.app.ui.screen

import android.annotation.SuppressLint
import android.webkit.WebChromeClient
import android.webkit.WebResourceError
import android.webkit.WebResourceRequest
import android.webkit.WebSettings
import android.webkit.WebView
import android.webkit.WebViewClient
import android.view.View
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.viewinterop.AndroidView
import com.nexbox.app.ui.theme.PageBg
import com.nexbox.app.ui.theme.TextSecondary
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import androidx.compose.ui.unit.dp

/** 心境页地址：与 PC 端 MoodPage 内嵌的 iframe 同一个 */
private const val MOOD_URL = "https://www.nexbox.top/love"

/**
 * 心境：App 内嵌网页版（原先是跳系统浏览器）。
 *
 * WebView 只加载自家站点，且刻意**不注册** `addJavascriptInterface`：
 * 网页里拿不到任何原生能力，被注入也碰不到文件系统。
 */
@SuppressLint("SetJavaScriptEnabled")
@Composable
fun MoodScreen(onBack: () -> Unit) {
    var webView by remember { mutableStateOf<WebView?>(null) }
    var canGoBack by remember { mutableStateOf(false) }
    var loading by remember { mutableStateOf(true) }
    var loadError by remember { mutableStateOf<String?>(null) }
    // 主题底色要在组合期取好：AndroidView 的 factory 不是组合作用域，读不到 @Composable 语义色
    val canvasColor = PageBg.toArgb()

    Column(
        modifier = Modifier
            .fillMaxSize()
            .statusBarsPadding()
            .background(PageBg),
    ) {
        DetailTopBar(
            title = "心境",
            onBack = onBack,
            trailingText = when {
                loading -> "加载中…"
                else -> null
            },
        )
        Box(Modifier.weight(1f)) {
            @Suppress("DEPRECATION")
            AndroidView(
                modifier = Modifier.fillMaxSize(),
                factory = { ctx ->
                    WebView(ctx).apply {
                        // 液态玻璃开启时，整个内容层每帧会被 backdrop 库画两遍（显示 + 给玻璃
                        // 导航条录采样层）。WebView 的合成帧是异步提交的，一帧内画两次会拿到
                        // 空白/过期帧，页面与玻璃条跟着高频闪烁；固定到自己的硬件层后两次绘制
                        // 都取同一份稳定纹理，闪烁消失
                        setLayerType(View.LAYER_TYPE_HARDWARE, null)
                        settings.apply {
                            javaScriptEnabled = true          // 心境页是 Web 应用，不开 JS 就是白屏
                            domStorageEnabled = true          // 登录态与草稿依赖 localStorage
                            useWideViewPort = true
                            loadWithOverviewMode = true       // 窄屏按内容自适应缩放
                            mixedContentMode = WebSettings.MIXED_CONTENT_COMPATIBILITY_MODE
                            cacheMode = WebSettings.LOAD_DEFAULT
                        }
                        // 用主题底色当画布底色，避免加载瞬间整块闪白
                        setBackgroundColor(canvasColor)
                        isVerticalScrollBarEnabled = false

                        webViewClient = object : WebViewClient() {
                            override fun onPageFinished(view: WebView, url: String?) {
                                loading = false
                            }

                            override fun doUpdateVisitedHistory(view: WebView, url: String?, isReload: Boolean) {
                                canGoBack = view.canGoBack()
                            }

                            override fun onReceivedError(
                                view: WebView?,
                                request: WebResourceRequest?,
                                error: WebResourceError?,
                            ) {
                                // 只认主文档失败：页面里某个小资源 404 不该把整页判死
                                if (request?.isForMainFrame == true) {
                                    loading = false
                                    loadError = error?.description?.toString() ?: "页面加载失败"
                                }
                            }
                        }
                        webChromeClient = WebChromeClient()
                        loadUrl(MOOD_URL)
                        webView = this
                    }
                },
                // 中途切主题只改画布底色，不重新加载网页
                update = { view -> view.setBackgroundColor(canvasColor) },
                // 离开页面必须 destroy：WebView 持有原生资源，只靠 GC 回收会留内存残渣
                onRelease = { view ->
                    runCatching {
                        view.stopLoading()
                        view.destroy()
                    }
                    webView = null
                },
            )

            loadError?.let { message ->
                Column(
                    modifier = Modifier
                        .align(Alignment.Center)
                        .padding(horizontal = 32.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Text(
                        "心境页打不开",
                        fontSize = 15.sp,
                        fontWeight = FontWeight.Bold,
                        color = MaterialTheme.colorScheme.onBackground,
                    )
                    Text(
                        "$message\n请确认手机能访问外网后重试",
                        fontSize = 12.5.sp,
                        color = TextSecondary,
                    )
                }
            }
        }
    }

    // 系统返回键：网页还能后退就先在网页里后退，退无可退才关掉头心境页
    BackHandler {
        val view = webView
        if (view != null && view.canGoBack()) view.goBack() else onBack()
    }
}
