//! 白名单动作/查询的实际执行：直接调用现有模块的 `pub async fn`。
//!
//! 这里是控制能力的**唯一权威入口**：match 分支即允许的集合，
//! 未列出的命令永远无法被远程触达。

use serde_json::{json, Value};

/// 将 `Result<T: Serialize, String>` 转为 `Result<Value, String>`。
fn v<T: serde::Serialize>(r: Result<T, String>) -> Result<Value, String> {
    r.and_then(|x| serde_json::to_value(x).map_err(|e| e.to_string()))
}

fn get_str(args: &Value, key: &str) -> Option<String> {
    args.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}
fn get_bool(args: &Value, key: &str) -> Option<bool> {
    args.get(key).and_then(|v| v.as_bool())
}
fn get_usize(args: &Value, key: &str) -> Option<usize> {
    args.get(key).and_then(|v| v.as_u64()).map(|v| v as usize)
}

/// 只读状态查询。
pub async fn exec_query(app: &tauri::AppHandle, key: &str, args: &Value) -> Result<Value, String> {
    match key {
        "stats.hw" => {
            let data = tokio::task::spawn_blocking(crate::overlay_panel::collect_hardware_data)
                .await
                .map_err(|e| e.to_string())?;
            v(Ok(data))
        }
        "mem.status" => v(crate::optimization::get_memory_status().await),
        // 磁盘占用：复用已有的 hardware::get_disk_status（内部 spawn_blocking + sysinfo）
        "disk.status" => v(crate::hardware::get_disk_status().await),
        // 型号信息：CPU / GPU 的名称、驱动、显存类型等静态身份信息。
        // 只回 UI 要的字段——完整 HardwareInfo 里带硬盘序列号、内存序列号等隐私数据，不外发。
        "hw.info" => {
            let info = tokio::task::spawn_blocking(crate::hardware::get_hardware_info)
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string())?;
            let gpus: Vec<Value> = info
                .gpu
                .iter()
                .map(|g| {
                    json!({
                        "name": g.name,
                        "vendor": g.vendor,
                        "memory_gb": g.memory_gb,
                        "driver_version": g.driver_version,
                        "driver_date": g.driver_date,
                        "video_memory_type": g.video_memory_type,
                        "resolution_width": g.resolution_width,
                        "resolution_height": g.resolution_height,
                        "refresh_rate": g.refresh_rate,
                    })
                })
                .collect();
            Ok(json!({
                "cpu": {
                    "name": info.cpu.name,
                    "manufacturer": info.cpu.manufacturer,
                    "cores": info.cpu.cores,
                    "threads": info.cpu.threads,
                    "max_clock_speed": info.cpu.max_clock_speed,
                    "socket": info.cpu.socket,
                    "l3_cache_size": info.cpu.l3_cache_size,
                },
                "gpus": gpus,
            }))
        }
        "power.active" => v(crate::optimization::get_active_power_plan().await),
        "power.plans" => {
            let builtin = crate::optimization::get_builtin_power_plans(app.clone()).await.unwrap_or_default();
            let system = crate::optimization::get_system_power_plans().await.unwrap_or_default();
            Ok(json!({ "builtin": builtin, "system": system }))
        }
        "gamemode.status" => v(crate::game_mode::game_mode_get_status(app.clone()).await),
        "filter.settings" => {
            let idx = get_usize(args, "displayIndex");
            v(crate::display_filter::get_filter_settings(idx).await)
        }
        // 准心状态：只回开关位，刻意不走 get_crosshair_status（它会枚举显示器，太重）
        "crosshair.status" => Ok(json!({ "enabled": crate::crosshair::is_active() })),
        // 悬浮框状态：复用已有命令（含 Win32 与竖排两种实现）
        "overlay.status" => v(crate::overlay_panel::get_overlay_panel_status()
            .await
            .map(|active| json!({ "active": active }))),
        "apps.list" => v(crate::app_manager::list_installed_apps().await),
        _ => Err(format!("unknown query: {key}")),
    }
}

/// 写操作。调用前 api 层已完成鉴权与 needs_confirm 拦截。
pub async fn exec_action(app: &tauri::AppHandle, key: &str, args: &Value) -> Result<Value, String> {
    match key {
        "mem.optimize" => v(crate::optimization::optimize_memory().await),
        "sys.clean_temp" => v(crate::optimization::clean_temp_files().await),
        "power.high_perf" => v(crate::optimization::set_high_performance_power_plan().await),
        "power.activate" => {
            let guid = get_str(args, "guid").ok_or_else(|| "缺少参数 guid".to_string())?;
            v(crate::optimization::activate_power_plan(guid).await)
        }
        "gamemode.set_preset" => {
            let preset = get_str(args, "preset").ok_or_else(|| "缺少参数 preset".to_string())?;
            v(crate::game_mode::game_mode_set_preset(app.clone(), preset).await.map(|_| json!({ "ok": true })))
        }
        "gamemode.set_auto" => {
            let enabled = get_bool(args, "enabled").ok_or_else(|| "缺少参数 enabled".to_string())?;
            v(crate::game_mode::game_mode_set_auto(app.clone(), enabled).await.map(|_| json!({ "ok": true })))
        }
        "filter.enable" => {
            let idx = get_usize(args, "displayIndex");
            v(crate::display_filter::enable_filter(idx).await)
        }
        "filter.disable" => {
            let idx = get_usize(args, "displayIndex");
            v(crate::display_filter::disable_filter(idx).await)
        }
        "overlay.toggle" => v(crate::overlay_panel::toggle_overlay_panel(app.clone()).await),
        // 准心切换：直接调已有命令（内部同步置位 CROSSHAIR_ACTIVE，回读即准确）
        "crosshair.toggle" => v(crate::crosshair::toggle_crosshair(app.clone()).await),
        _ => Err(format!("unknown action: {key}")),
    }
}
