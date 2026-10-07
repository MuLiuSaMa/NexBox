package com.nexbox.app.ui.screen

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ArrowBack
import androidx.compose.material.icons.rounded.Casino
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import coil3.compose.AsyncImage
import com.nexbox.app.data.RouletteGear
import com.nexbox.app.data.RouletteMap
import com.nexbox.app.data.RouletteWeapon
import com.nexbox.app.data.isClassifiedDifficulty
import com.nexbox.app.data.isPistolWeapon
import com.nexbox.app.data.rouletteArmors
import com.nexbox.app.data.rouletteHelmets
import com.nexbox.app.data.rouletteMaps
import com.nexbox.app.data.rouletteOperators
import com.nexbox.app.data.rouletteTasks
import com.nexbox.app.data.rouletteWeapons
import com.nexbox.app.ui.AppBackground
import com.nexbox.app.ui.AppCard
import com.nexbox.app.ui.theme.MetricMemory
import com.nexbox.app.ui.theme.MetricStorage
import com.nexbox.app.ui.theme.TextSecondary
import kotlinx.coroutines.delay

/** 单次抽取的完整结果 */
private data class RouletteDraw(
    val map: RouletteMap,
    val operatorName: String,
    val operatorPic: String,
    val weapon: RouletteWeapon,
    val helmet: RouletteGear,
    val armor: RouletteGear,
    val task: String,
)

/**
 * 随机装备（参考 PC 端 /delta-force/random-equipment）：
 * 干员 / 武器 / 头盔 / 护甲 / 地图 / 特殊任务各槽独立随机，
 * 「只玩绝密」过滤地图池，「不用手枪」过滤武器池；
 * 生成时先滚动一串随机结果再定格，模拟 PC 端的槽机动画。
 */
@Composable
fun DeltaRouletteScreen(onClose: () -> Unit) {
    var onlyClassified by remember { mutableStateOf(false) }
    var noPistol by remember { mutableStateOf(false) }
    var rolling by remember { mutableStateOf(false) }
    var shown by remember { mutableStateOf<RouletteDraw?>(null) }
    var spinKey by remember { mutableIntStateOf(0) }

    // 槽位池跟着筛选走：过滤后的池子闭包进生成函数，滚动期间也用同一套池子
    val randomDraw: () -> RouletteDraw = {
        val mapPool =
            if (onlyClassified) rouletteMaps.filter { isClassifiedDifficulty(it.difficulty) }
            else rouletteMaps
        val weaponPool =
            if (noPistol) rouletteWeapons.filterNot { isPistolWeapon(it.pic) } else rouletteWeapons
        val op = rouletteOperators.random()
        RouletteDraw(
            map = mapPool.random(),
            operatorName = op.name,
            operatorPic = op.pic,
            weapon = weaponPool.random(),
            helmet = rouletteHelmets.random(),
            armor = rouletteArmors.random(),
            task = rouletteTasks.random(),
        )
    }

    // 回合制滚动：连跳 10 次随机结果，每 60ms 一步，最后定格在最终结果
    LaunchedEffect(spinKey) {
        if (spinKey == 0) return@LaunchedEffect
        val final = randomDraw()
        repeat(10) {
            shown = randomDraw()
            delay(60)
        }
        shown = final
        rolling = false
    }

    BackHandler(enabled = true, onBack = onClose)

    // 整页覆盖层：底下还画着三角洲主页，必须自己铺完整背景才盖得住。
    // 用 AppBackground（与 NexBoxApp 背景层同一份渲染）：液态玻璃卡片采样的是背景层，
    // 两边逐像素一致，卡片透出的背景才和页面背景对得上，而不是「卡片里有背景图、页面纯色」
    Box(Modifier.fillMaxSize()) {
        AppBackground(Modifier.fillMaxSize())
        Column(
            Modifier
                .fillMaxSize()
                .statusBarsPadding(),
        ) {
            Row(
                verticalAlignment = Alignment.CenterVertically,
                modifier = Modifier.padding(start = 6.dp, end = 18.dp, top = 8.dp, bottom = 8.dp),
            ) {
                Icon(
                    Icons.AutoMirrored.Rounded.ArrowBack,
                    contentDescription = "返回",
                    tint = MaterialTheme.colorScheme.onBackground,
                    modifier = Modifier
                        .clip(RoundedCornerShape(50))
                        .clickable(onClick = onClose)
                        .padding(10.dp)
                        .size(24.dp),
                )
                Spacer(Modifier.width(4.dp))
                Text(
                    "随机装备",
                    fontSize = 20.sp,
                    fontWeight = FontWeight.SemiBold,
                    color = MaterialTheme.colorScheme.onBackground,
                )
            }

            Column(
                Modifier
                    .fillMaxSize()
                    .verticalScroll(rememberScrollState())
                    .padding(horizontal = 18.dp)
                    .navigationBarsPadding()
                    .padding(bottom = 108.dp),
                verticalArrangement = Arrangement.spacedBy(14.dp),
            ) {
                // 筛选：与 PC 端一致的「只玩绝密」「不用手枪」
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    RouletteOption("只玩绝密", onlyClassified) { onlyClassified = it }
                    RouletteOption("不用手枪", noPistol) { noPistol = it }
                }

                // 生成按钮
                Box(
                    modifier = Modifier
                        .fillMaxWidth()
                        .clip(RoundedCornerShape(50))
                        .background(MaterialTheme.colorScheme.primary)
                        .clickable(enabled = !rolling) {
                            rolling = true
                            spinKey++
                        }
                        .padding(vertical = 13.dp),
                    contentAlignment = Alignment.Center,
                ) {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        Icon(
                            Icons.Rounded.Casino,
                            contentDescription = null,
                            tint = MaterialTheme.colorScheme.onPrimary,
                            modifier = Modifier.size(19.dp),
                        )
                        Text(
                            if (rolling) "抽取中…" else "生成随机装备",
                            fontSize = 14.5.sp,
                            fontWeight = FontWeight.SemiBold,
                            color = MaterialTheme.colorScheme.onPrimary,
                        )
                    }
                }

                shown?.let { draw ->
                    RouletteResultView(draw)
                }

                if (shown == null) {
                    AppCard(modifier = Modifier.fillMaxWidth(), shape = RoundedCornerShape(16.dp)) {
                        Text(
                            "点「生成随机装备」开一局：地图、干员、武器、防具和一条特殊任务一次抽齐。",
                            fontSize = 12.sp,
                            lineHeight = 18.sp,
                            color = TextSecondary,
                            modifier = Modifier.padding(16.dp),
                        )
                    }
                }
            }
        }
    }
}

/** 筛选胶囊 */
@Composable
private fun RouletteOption(text: String, checked: Boolean, onChange: (Boolean) -> Unit) {
    Box(
        modifier = Modifier
            .clip(RoundedCornerShape(50))
            .background(
                if (checked) MaterialTheme.colorScheme.primary
                else MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.45f),
            )
            .clickable { onChange(!checked) }
            .padding(horizontal = 14.dp, vertical = 8.dp),
    ) {
        Text(
            text,
            fontSize = 12.5.sp,
            fontWeight = FontWeight.Medium,
            color = if (checked) MaterialTheme.colorScheme.onPrimary else TextSecondary,
        )
    }
}

/** 结果区：地图 / 干员+武器 / 头盔+护甲 / 特殊任务 */
@Composable
private fun RouletteResultView(draw: RouletteDraw) {
    Column(verticalArrangement = Arrangement.spacedBy(14.dp)) {
        // 地图：大图 + 左下角名称与难度
        Box(
            Modifier
                .fillMaxWidth()
                .height(140.dp)
                .clip(RoundedCornerShape(16.dp)),
        ) {
            AsyncImage(
                model = draw.map.pic,
                contentDescription = draw.map.map,
                contentScale = ContentScale.Crop,
                modifier = Modifier.fillMaxSize(),
            )
            Box(
                Modifier
                    .fillMaxSize()
                    .background(Color.Black.copy(alpha = 0.28f)),
            )
            Column(
                Modifier
                    .align(Alignment.BottomStart)
                    .padding(12.dp),
                verticalArrangement = Arrangement.spacedBy(4.dp),
            ) {
                DifficultyBadge(draw.map.difficulty)
                Text(
                    draw.map.map,
                    fontSize = 18.sp,
                    fontWeight = FontWeight.Bold,
                    color = Color.White,
                )
            }
        }

        // 干员 + 武器：两卡同高。Row 包内容时高度约束是无界的，fillMaxHeight 不生效，
        // 必须 IntrinsicSize.Min 让行高取最高子项（武器卡），干员卡才能拉到同高
        Row(
            horizontalArrangement = Arrangement.spacedBy(10.dp),
            modifier = Modifier.height(IntrinsicSize.Min),
        ) {
            AppCard(modifier = Modifier.weight(0.42f).fillMaxHeight(), shape = RoundedCornerShape(16.dp)) {
                Column(
                    Modifier
                        .fillMaxWidth()
                        .padding(vertical = 12.dp, horizontal = 8.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    SlotLabel("干员")
                    Box(Modifier.weight(1f), contentAlignment = Alignment.Center) {
                        Column(
                            horizontalAlignment = Alignment.CenterHorizontally,
                            verticalArrangement = Arrangement.spacedBy(6.dp),
                        ) {
                            AsyncImage(
                                model = draw.operatorPic,
                                contentDescription = draw.operatorName,
                                contentScale = ContentScale.Fit,
                                modifier = Modifier.size(64.dp),
                            )
                            Text(
                                draw.operatorName,
                                fontSize = 13.5.sp,
                                fontWeight = FontWeight.SemiBold,
                                color = MaterialTheme.colorScheme.onSurface,
                                maxLines = 1,
                                overflow = TextOverflow.Ellipsis,
                            )
                        }
                    }
                }
            }
            AppCard(modifier = Modifier.weight(0.58f), shape = RoundedCornerShape(16.dp)) {
                Column(
                    Modifier
                        .fillMaxWidth()
                        .padding(vertical = 12.dp, horizontal = 10.dp),
                    horizontalAlignment = Alignment.CenterHorizontally,
                    verticalArrangement = Arrangement.spacedBy(6.dp),
                ) {
                    SlotLabel("武器")
                    AsyncImage(
                        model = draw.weapon.pic,
                        contentDescription = draw.weapon.objectName,
                        contentScale = ContentScale.Fit,
                        modifier = Modifier
                            .fillMaxWidth()
                            .height(58.dp),
                    )
                    Text(
                        draw.weapon.objectName,
                        fontSize = 13.5.sp,
                        fontWeight = FontWeight.SemiBold,
                        color = MaterialTheme.colorScheme.onSurface,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                    Text(
                        "${draw.weapon.fireMode} · ${draw.weapon.caliber}",
                        fontSize = 10.5.sp,
                        color = TextSecondary,
                    )
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(5.dp)) {
                        draw.weapon.ammoPic?.let { ammo ->
                            AsyncImage(
                                model = ammo,
                                contentDescription = null,
                                contentScale = ContentScale.Fit,
                                modifier = Modifier.size(16.dp),
                            )
                        }
                        Text(
                            "${draw.weapon.selectedAmmoGrade} 级弹",
                            fontSize = 10.5.sp,
                            fontWeight = FontWeight.Medium,
                            color = MaterialTheme.colorScheme.primary,
                        )
                    }
                }
            }
        }

        // 头盔 + 护甲：与上行同一分法（0.42/0.58），两行中间缝隙上下对齐
        Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            GearSlot("头盔", draw.helmet, Modifier.weight(0.42f))
            GearSlot("护甲", draw.armor, Modifier.weight(0.58f))
        }

        // 特殊任务
        AppCard(modifier = Modifier.fillMaxWidth(), shape = RoundedCornerShape(16.dp)) {
            Column(Modifier.padding(14.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Icon(
                        Icons.Rounded.Casino,
                        contentDescription = null,
                        tint = MaterialTheme.colorScheme.primary,
                        modifier = Modifier.size(15.dp),
                    )
                    Text(
                        "特殊任务",
                        fontSize = 12.sp,
                        fontWeight = FontWeight.Bold,
                        color = MaterialTheme.colorScheme.primary,
                    )
                }
                Text(
                    draw.task,
                    fontSize = 13.sp,
                    lineHeight = 19.sp,
                    color = MaterialTheme.colorScheme.onSurface,
                )
            }
        }
    }
}

@Composable
private fun SlotLabel(text: String) {
    Text(
        text,
        fontSize = 11.sp,
        fontWeight = FontWeight.Bold,
        color = MaterialTheme.colorScheme.primary,
    )
}

@Composable
private fun GearSlot(label: String, gear: RouletteGear, modifier: Modifier = Modifier) {
    AppCard(modifier = modifier, shape = RoundedCornerShape(16.dp)) {
        Column(
            Modifier
                .fillMaxWidth()
                .padding(vertical = 12.dp, horizontal = 8.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            SlotLabel(label)
            AsyncImage(
                model = gear.pic,
                contentDescription = gear.objectName,
                contentScale = ContentScale.Fit,
                modifier = Modifier.size(56.dp),
            )
            Text(
                gear.objectName,
                fontSize = 12.sp,
                fontWeight = FontWeight.SemiBold,
                color = MaterialTheme.colorScheme.onSurface,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                textAlign = TextAlign.Center,
            )
            Text(
                "${gear.protectLevel} 级",
                fontSize = 10.5.sp,
                fontWeight = FontWeight.Medium,
                color = TextSecondary,
            )
        }
    }
}

/** 难度徽标：绝密红 / 机密橙 / 永夜紫 / 常规绿 */
@Composable
private fun DifficultyBadge(difficulty: String) {
    val color = when (difficulty) {
        "绝密" -> MaterialTheme.colorScheme.error
        "机密" -> MetricStorage
        "永夜" -> MetricMemory
        else -> Color(0xFF3FBF6F)
    }
    Text(
        difficulty,
        fontSize = 10.5.sp,
        fontWeight = FontWeight.Bold,
        color = Color.White,
        modifier = Modifier
            .clip(RoundedCornerShape(6.dp))
            .background(color)
            .padding(horizontal = 7.dp, vertical = 2.dp),
    )
}
