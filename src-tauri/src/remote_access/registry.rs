//! 白名单元数据：驱动 `GET /api/capabilities`，让安卓端 UI 数据化渲染。
//!
//! 注意：真正的"可达集合"由 `actions.rs` 的 match 分支决定；此处仅是展示用的
//! 描述表。二者 key 必须一致——若某 key 只在此登记而 actions.rs 未实现，调用会返回未知错误。
//! **危险项（注册表批量、驱动装卸、卸载、删除、网络重置、改 DNS、退出应用等）不在此登记，永不暴露。**

use serde_json::{json, Value};

/// 参数描述。
fn param(name: &str, ty: &str, required: bool, enum_vals: Option<Vec<&str>>) -> Value {
    let mut o = json!({ "name": name, "type": ty, "required": required });
    if let Some(v) = enum_vals {
        o["enum"] = json!(v);
    }
    o
}

/// 能力清单：分组 → 查询（只读）+ 动作（写）。
pub fn capabilities() -> Value {
    json!({
        "groups": [
            {
                "id": "hardware",
                "title": "硬件状态",
                "queries": [
                    { "key": "stats.hw", "title": "实时硬件快照(FPS/CPU/GPU/内存/网络)" },
                    { "key": "disk.status", "title": "磁盘占用" },
                    { "key": "hw.info", "title": "硬件型号信息(CPU/GPU)" }
                ],
                "actions": []
            },
            {
                "id": "memory",
                "title": "内存与清理",
                "queries": [
                    { "key": "mem.status", "title": "内存占用" }
                ],
                "actions": [
                    { "key": "mem.optimize", "title": "一键内存优化", "needsConfirm": false, "params": [] },
                    { "key": "sys.clean_temp", "title": "清理临时文件", "needsConfirm": false, "params": [] }
                ]
            },
            {
                "id": "power",
                "title": "电源计划",
                "queries": [
                    { "key": "power.active", "title": "当前电源计划" },
                    { "key": "power.plans", "title": "可用电源计划" }
                ],
                "actions": [
                    { "key": "power.high_perf", "title": "切换高性能计划", "needsConfirm": false, "params": [] },
                    {
                        "key": "power.activate",
                        "title": "激活指定电源计划",
                        "needsConfirm": true,
                        "params": [ param("guid", "string", true, None) ]
                    }
                ]
            },
            {
                "id": "gamemode",
                "title": "游戏模式",
                "queries": [
                    { "key": "gamemode.status", "title": "游戏模式状态" }
                ],
                "actions": [
                    {
                        "key": "gamemode.set_preset",
                        "title": "设置模式档位",
                        "needsConfirm": false,
                        "params": [ param("preset", "string", true, Some(vec!["default", "regular", "competitive"])) ]
                    },
                    {
                        "key": "gamemode.set_auto",
                        "title": "自动切换开关",
                        "needsConfirm": false,
                        "params": [ param("enabled", "bool", true, None) ]
                    }
                ]
            },
            {
                "id": "filter",
                "title": "显示滤镜",
                "queries": [
                    { "key": "filter.settings", "title": "滤镜设置" }
                ],
                "actions": [
                    { "key": "filter.enable", "title": "开启滤镜", "needsConfirm": false, "params": [ param("displayIndex", "number", false, None) ] },
                    { "key": "filter.disable", "title": "关闭滤镜", "needsConfirm": false, "params": [ param("displayIndex", "number", false, None) ] }
                ]
            },
            {
                "id": "overlay",
                "title": "悬浮框",
                "queries": [
                    { "key": "overlay.status", "title": "悬浮框状态" }
                ],
                "actions": [
                    { "key": "overlay.toggle", "title": "切换悬浮框", "needsConfirm": false, "params": [] }
                ]
            },
            {
                "id": "crosshair",
                "title": "准心",
                "queries": [
                    { "key": "crosshair.status", "title": "准心状态" }
                ],
                "actions": [
                    { "key": "crosshair.toggle", "title": "切换准心", "needsConfirm": false, "params": [] }
                ]
            },
            {
                "id": "apps",
                "title": "应用",
                "queries": [
                    { "key": "apps.list", "title": "已安装应用列表" }
                ],
                "actions": []
            }
        ]
    })
}

/// 动作是否需要二次确认（供 api 层拦截）。
pub fn action_needs_confirm(key: &str) -> bool {
    matches!(key, "power.activate")
}

/// 动作 key 是否属于白名单（快速拒绝未知，避免误调用）。
pub fn is_known_action(key: &str) -> bool {
    matches!(
        key,
        "mem.optimize" | "sys.clean_temp" | "power.high_perf" | "power.activate"
            | "gamemode.set_preset" | "gamemode.set_auto" | "filter.enable" | "filter.disable" | "overlay.toggle"
            | "crosshair.toggle"
    )
}

/// 查询 key 是否属于白名单。
pub fn is_known_query(key: &str) -> bool {
    matches!(
        key,
        "stats.hw" | "mem.status" | "disk.status" | "hw.info" | "power.active" | "power.plans"
            | "gamemode.status" | "filter.settings" | "apps.list"
            | "crosshair.status" | "overlay.status"
    )
}
