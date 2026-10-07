package com.nexbox.app.ui.screen

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.nexbox.app.data.CapabilityAction
import com.nexbox.app.data.CapabilityGroup
import com.nexbox.app.data.CapabilityParam
import com.nexbox.app.data.NetworkModule
import com.nexbox.app.data.toUserMessage
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.contentOrNull

/** 单个动作的参数草稿：paramName -> 原始输入文本 */
typealias ArgDraft = Map<String, String>

data class ControlUiState(
    val loading: Boolean = false,
    val loaded: Boolean = false,
    val groups: List<CapabilityGroup> = emptyList(),
    val error: String? = null,
    /** actionKey -> 参数草稿 */
    val drafts: Map<String, ArgDraft> = emptyMap(),
    /** queryKey -> 最近一次读取结果（格式化后的文本） */
    val queryResults: Map<String, String> = emptyMap(),
    /** 正在执行的 action / 正在读取的 query，用于禁用按钮 */
    val running: String? = null,
    /** needsConfirm 的动作：待用户确认 */
    val pendingConfirm: CapabilityAction? = null,
    /** 一次性提示 */
    val message: String? = null,
    val messageIsError: Boolean = false,
)

/**
 * 控制页数据源：能力清单完全来自 `GET /api/capabilities`，
 * 安卓端不硬编码任何动作，危险项由 PC 端白名单天然隔离。
 */
class ControlViewModel : ViewModel() {

    private val _state = MutableStateFlow(ControlUiState())
    val state: StateFlow<ControlUiState> = _state.asStateFlow()

    fun load(force: Boolean = false) {
        val s = _state.value
        if (s.loading || (s.loaded && !force)) return
        viewModelScope.launch {
            _state.update { it.copy(loading = true, error = null) }
            runCatching { NetworkModule.api.capabilities() }
                .onSuccess { caps ->
                    _state.update { it.copy(loading = false, loaded = true, groups = caps.groups) }
                }
                .onFailure { e ->
                    _state.update { it.copy(loading = false, loaded = false, error = e.toUserMessage()) }
                }
        }
    }

    fun reset() = _state.update {
        it.copy(loaded = false, groups = emptyList(), error = null, queryResults = emptyMap())
    }

    fun onArgChange(actionKey: String, param: String, value: String) = _state.update { s ->
        val draft = s.drafts[actionKey].orEmpty().toMutableMap()
        draft[param] = value
        s.copy(drafts = s.drafts + (actionKey to draft))
    }

    fun argValue(actionKey: String, param: CapabilityParam): String =
        _state.value.drafts[actionKey]?.get(param.name) ?: defaultArg(param)

    /** 点击执行：needsConfirm 的先弹确认框，其余直接发 */
    fun run(action: CapabilityAction, confirm: Boolean = false) {
        if (action.needsConfirm && !confirm) {
            _state.update { it.copy(pendingConfirm = action) }
            return
        }
        _state.update { it.copy(pendingConfirm = null) }
        viewModelScope.launch {
            _state.update { it.copy(running = action.key, message = null) }
            val args = buildArgs(action)
            runCatching { NetworkModule.api.action(action.key, args, confirm = action.needsConfirm) }
                .onSuccess {
                    _state.update {
                        it.copy(running = null, message = "${action.title} 已执行", messageIsError = false)
                    }
                }
                .onFailure { e ->
                    _state.update { it.copy(running = null, message = e.toUserMessage(), messageIsError = true) }
                }
        }
    }

    fun cancelConfirm() = _state.update { it.copy(pendingConfirm = null) }

    fun readQuery(key: String) {
        viewModelScope.launch {
            _state.update { it.copy(running = key, message = null) }
            runCatching { NetworkModule.api.query(key) }
                .onSuccess { data ->
                    _state.update {
                        it.copy(running = null, queryResults = it.queryResults + (key to pretty(data)))
                    }
                }
                .onFailure { e ->
                    _state.update { it.copy(running = null, message = e.toUserMessage(), messageIsError = true) }
                }
        }
    }

    fun consumeMessage() = _state.update { it.copy(message = null) }

    // ───────────────────────── 内部 ─────────────────────────

    private fun buildArgs(action: CapabilityAction): JsonObject {
        val draft = _state.value.drafts[action.key].orEmpty()
        val entries = action.params.mapNotNull { p ->
            val raw = draft[p.name] ?: defaultArg(p)
            val value = toJson(p, raw) ?: return@mapNotNull null
            p.name to value
        }
        return JsonObject(entries.toMap())
    }

    private fun defaultArg(p: CapabilityParam): String = when {
        p.type == "bool" -> "false"
        p.enumValues?.isNotEmpty() == true -> p.enumValues.first()
        else -> ""
    }

    private fun toJson(p: CapabilityParam, raw: String): JsonElement? = when {
        p.type == "bool" -> JsonPrimitive(raw.toBoolean())
        p.type == "int" || p.type == "number" -> raw.toIntOrNull()?.let { JsonPrimitive(it) }
            ?: raw.toDoubleOrNull()?.let { JsonPrimitive(it) }
            ?: if (p.required) JsonNull else null
        else -> if (raw.isBlank() && !p.required) null else JsonPrimitive(raw)
    }

    /** 结果格式化：字符串直出，对象/数组转成 JSON 文本，够用且不引额外依赖 */
    private fun pretty(element: JsonElement): String = when (element) {
        is JsonPrimitive -> element.contentOrNull ?: element.toString()
        is JsonArray, is JsonObject -> element.toString()
    }
}
