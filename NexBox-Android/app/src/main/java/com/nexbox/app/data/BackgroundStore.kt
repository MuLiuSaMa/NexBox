package com.nexbox.app.data

import android.content.Context
import android.net.Uri
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import java.io.File

/**
 * 应用背景图：选中后替换默认的深色渐变背景，整个应用生效。
 *
 * 图片拷贝进应用私有目录，重启后仍然生效；文件不存在 = 默认深色渐变。
 * 状态用 Compose State 持有，改完立即重组背景层。
 *
 * 注意：换图时必须换一个「文件名」。Compose 的 mutableStateOf 默认按结构化相等
 * 比较，而 File.equals 只比路径——若沿用固定文件名，新图和旧图路径相同，
 * 赋值会被判定为「值没变」而不触发重组，表现为「换了背景图但界面还是旧图」。
 * 因此这里用时间戳命名，并保证目录内只留当前这一张。
 */
object BackgroundStore {
    /** 旧版固定文件名，仅用于兼容升级前已设置的背景 */
    private const val LEGACY_FILE_NAME = "app_background.jpg"
    private const val PREFIX = "app_background_"
    private const val SUFFIX = ".jpg"

    /** 中转文件名：不能落在 isManaged 的匹配范围内，否则会被当成背景图 */
    private const val TMP_FILE_NAME = "app_background.tmp.jpg"

    /** 当前背景图文件；null = 默认深色渐变 */
    var file: File? by mutableStateOf(null)
        private set

    /** 启动时调用：恢复上次选择的背景 */
    fun init(context: Context) {
        file = managedFiles(context).maxByOrNull { it.lastModified() }
    }

    /** 从相册选中的 Uri 拷贝进私有目录并立即生效（拷贝是为了不依赖相册的临时读权限） */
    fun setFromUri(context: Context, uri: Uri): Boolean {
        val dir = context.filesDir
        val tmp = File(dir, TMP_FILE_NAME)
        return try {
            tmp.delete()
            context.contentResolver.openInputStream(uri)?.use { input ->
                tmp.outputStream().use { output -> input.copyTo(output) }
            } ?: return false
            if (tmp.length() <= 0L) return false
            // 先落到新名字再换状态：中途失败时旧背景保持完好
            val target = File(dir, uniqueName())
            target.delete()
            if (!tmp.renameTo(target)) return false
            val previous = file
            file = target
            prune(context, keep = target)
            if (previous != null && previous != target) previous.delete()
            true
        } catch (_: Exception) {
            false
        } finally {
            tmp.delete()
        }
    }

    /** 恢复默认深色渐变背景 */
    fun clear() {
        val current = file
        file = null
        current?.delete()
    }

    /** 只保留 keep，其余背景文件（含旧版固定名）全部清掉 */
    private fun prune(context: Context, keep: File) {
        managedFiles(context).filter { it.absolutePath != keep.absolutePath }.forEach { it.delete() }
    }

    private fun managedFiles(context: Context): List<File> =
        context.filesDir.listFiles()?.filter { f ->
            f.isFile && f.length() > 0 && isManaged(f.name)
        }.orEmpty()

    private fun isManaged(name: String): Boolean =
        name == LEGACY_FILE_NAME || (name.startsWith(PREFIX) && name.endsWith(SUFFIX))

    private fun uniqueName(): String = PREFIX + System.currentTimeMillis() + SUFFIX
}
