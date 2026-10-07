package com.nexbox.app.ui.screen

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.ExpandMore
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.nexbox.app.ui.AppCard

/**
 * 投稿页：与 PC 端上传弹窗同一套字段与校验（分类 → 武器联动、改枪码、描述、费用、昵称）。
 *
 * 走整屏而不是浮层：底部那条悬浮玻璃导航条是页面之外的兄弟节点，浮层盖不住它，
 * 半屏弹窗的「提交」按钮会被压在导航条下面。
 */
@Composable
internal fun UploadFormScreen(state: DeltaUiState, vm: DeltaForceViewModel) {
    val categories = state.categories
    val selectedCategory = categories.firstOrNull { it.id == state.uploadCategoryId }
    val selectedWeapon = state.uploadWeapon

    Column(
        modifier = Modifier
            .fillMaxSize()
            .statusBarsPadding()
            .verticalScroll(rememberScrollState())
            .imePadding()
            .padding(horizontal = 18.dp),
        verticalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        DetailTopBar(title = "上传改枪码", onBack = vm::closeUpload)

        AppCard(modifier = Modifier.fillMaxWidth(), shape = RoundedCornerShape(16.dp)) {
            Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
                FormChoice(
                    label = "分类",
                    placeholder = "请选择分类",
                    options = categories.map { it.name },
                    selected = selectedCategory?.name,
                    onSelect = { name ->
                        categories.firstOrNull { it.name == name }?.let { vm.selectUploadCategory(it.id) }
                    },
                )
                FormChoice(
                    label = "武器名称",
                    placeholder = if (state.uploadWeapons.isEmpty()) "请先选择分类" else "请选择武器",
                    options = state.uploadWeapons.map { it.weaponName },
                    selected = selectedWeapon.ifEmpty { null },
                    enabled = state.uploadWeapons.isNotEmpty(),
                    onSelect = vm::selectUploadWeapon,
                )
                FormField(
                    label = "改枪码",
                    value = state.uploadCode,
                    placeholder = "请输入改枪码，最多500字符",
                    singleLine = false,
                    onValueChange = vm::onUploadCode,
                )
                FormField(
                    label = "描述",
                    value = state.uploadDescription,
                    placeholder = "请输入描述（可选）",
                    onValueChange = vm::onUploadDescription,
                )
                FormField(
                    label = "改枪费用",
                    value = state.uploadCost,
                    placeholder = "0",
                    keyboardType = KeyboardType.Number,
                    onValueChange = vm::onUploadCost,
                )
                FormField(
                    label = "作者",
                    value = state.uploadAuthor,
                    placeholder = "输入昵称（可选）",
                    onValueChange = vm::onUploadAuthor,
                )

                if (state.uploadError != null) {
                    Text(state.uploadError.orEmpty(), fontSize = 12.sp, color = MaterialTheme.colorScheme.error)
                }

                Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
                    OutlinedButton(
                        onClick = vm::closeUpload,
                        modifier = Modifier.weight(1f),
                        shape = RoundedCornerShape(12.dp),
                    ) { Text("取消") }
                    Button(
                        onClick = vm::submitUpload,
                        enabled = !state.submitting,
                        modifier = Modifier.weight(1f),
                        shape = RoundedCornerShape(12.dp),
                    ) {
                        if (state.submitting) {
                            CircularProgressIndicator(
                                modifier = Modifier.size(14.dp),
                                strokeWidth = 2.dp,
                                color = MaterialTheme.colorScheme.onPrimary,
                            )
                            Spacer(Modifier.width(8.dp))
                            Text("提交中…")
                        } else {
                            Text("提交审核")
                        }
                    }
                }
            }
        }

        Text(
            "提交的改枪码先进审核队列，管理员通过后才会出现在列表里。",
            fontSize = 11.5.sp,
            lineHeight = 17.sp,
            color = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.55f),
        )

        Spacer(Modifier.height(8.dp))
        // 悬浮玻璃导航条的余量
        Spacer(Modifier.navigationBarsPadding().height(96.dp))
    }
}

@Composable
private fun FormField(
    label: String,
    value: String,
    placeholder: String,
    onValueChange: (String) -> Unit,
    singleLine: Boolean = true,
    keyboardType: KeyboardType = KeyboardType.Text,
) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(
            label,
            fontSize = 12.5.sp,
            fontWeight = FontWeight.Medium,
            color = MaterialTheme.colorScheme.onSurface,
        )
        OutlinedTextField(
            value = value,
            onValueChange = onValueChange,
            placeholder = { Text(placeholder, fontSize = 12.5.sp) },
            singleLine = singleLine,
            textStyle = MaterialTheme.typography.bodyMedium.copy(fontSize = 13.sp),
            keyboardOptions = KeyboardOptions(keyboardType = keyboardType),
            modifier = Modifier.fillMaxWidth(),
            shape = RoundedCornerShape(12.dp),
        )
    }
}

/** 下拉选择：分类与武器都是后端给的固定清单，不给自由输入，否则同一把枪会写出好几个名字 */
@Composable
private fun FormChoice(
    label: String,
    placeholder: String,
    options: List<String>,
    selected: String?,
    onSelect: (String) -> Unit,
    enabled: Boolean = true,
) {
    var expanded by remember { mutableStateOf(false) }
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(
            label,
            fontSize = 12.5.sp,
            fontWeight = FontWeight.Medium,
            color = MaterialTheme.colorScheme.onSurface,
        )
        Box {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier
                    .fillMaxWidth()
                    .clip(RoundedCornerShape(12.dp))
                    .background(MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.45f))
                    .clickable(enabled = enabled && options.isNotEmpty()) { expanded = true }
                    .padding(horizontal = 14.dp, vertical = 15.dp),
            ) {
                Text(
                    selected ?: placeholder,
                    fontSize = 13.sp,
                    color = if (selected != null) MaterialTheme.colorScheme.onSurface
                    else MaterialTheme.colorScheme.onSurface.copy(alpha = 0.45f),
                    modifier = Modifier.weight(1f),
                )
                Icon(
                    Icons.Rounded.ExpandMore,
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.5f),
                    modifier = Modifier.size(18.dp),
                )
            }
            DropdownMenu(expanded = expanded, onDismissRequest = { expanded = false }) {
                options.forEach { option ->
                    DropdownMenuItem(
                        text = {
                            Text(
                                option,
                                fontSize = 13.sp,
                                color = if (option == selected) MaterialTheme.colorScheme.primary
                                else MaterialTheme.colorScheme.onSurface,
                            )
                        },
                        onClick = {
                            expanded = false
                            onSelect(option)
                        },
                    )
                }
            }
        }
    }
}
