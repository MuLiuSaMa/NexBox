/**
 * 竖排悬浮框独立窗口页面
 *
 * 特性：
 * - 所有硬件项竖向排列，完整显示项名
 * - 每项带 Lucide 图标
 * - 拖动模式 / 鼠标穿透切换
 * - 位置记忆
 * - 不透明度联动
 * - 数值颜色渐变（绿→黄→红）
 */

import { useEffect, useLayoutEffect, useRef, useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  Gauge,
  Thermometer,
  Cpu,
  Zap,
  HardDrive,
  MemoryStick,
  Wifi,
  Key,
  Fan,
  Activity,
  Battery,
  Monitor,
  Clock,
  Download,
  Upload,
  type LucideIcon,
} from "lucide-react";

// ===== 类型定义 =====

interface DisplayItemConfig {
  id: string;
  label: string;
  enabled: boolean;
}

interface CustomOverlayItem {
  id: string;
  text: string;
  color: string;
  enabled: boolean;
}

interface OverlaySettings {
  display_items: DisplayItemConfig[];
  custom_items: CustomOverlayItem[];
  opacity: number;
  style: string;
  font: string;
  font_size: number;
  item_width: number;
  font_color: string;
  position_x?: number | null;
  position_y?: number | null;
  vertical_position_x?: number | null;
  vertical_position_y?: number | null;
}

interface HardwareData {
  fps: number | null;
  fps_1low: number | null;
  fps_01low: number | null;
  cpu_usage: number | null;
  cpu_temp: number | null;
  cpu_clock: number | null;
  cpu_voltage: number | null;
  cpu_power: number | null;
  cpu_fan_speed: number | null;
  gpu_temp: number | null;
  gpu_usage: number | null;
  gpu_fan_speed: number | null;
  gpu_power: number | null;
  gpu_clock: number | null;
  gpu_voltage: number | null;
  gpu_memory_clock: number | null;
  memory_usage: number | null;
  ssd_temp: number | null;
  delta_password: string | null;
  game_ping: number | null;
  gpu_vram_used: number | null;
  gpu_vram_total: number | null;
  net_time_offset_ms: number | null;
  net_down_speed: number | null;
  net_up_speed: number | null;
}

// ===== 图标映射 =====

const ITEM_ICONS: Record<string, LucideIcon> = {
  time: Clock,
  fps: Gauge,
  fps_1low: Gauge,
  fps_01low: Gauge,
  cpu_temp: Thermometer,
  cpu_usage: Cpu,
  cpu_clock: Cpu,
  cpu_voltage: Zap,
  cpu_power: Activity,
  cpu_fan_speed: Fan,
  gpu_temp: Thermometer,
  gpu_usage: Monitor,
  gpu_fan_speed: Fan,
  gpu_power: Zap,
  gpu_clock: Monitor,
  gpu_voltage: Battery,
  gpu_vram: MemoryStick,
  gpu_memory_clock: Monitor,
  memory_usage: MemoryStick,
  ssd_temp: HardDrive,
  game_ping: Wifi,
  delta_password: Key,
  net_down: Download,
  net_up: Upload,
};

// ===== 默认值 =====

const DEFAULT_DISPLAY_ITEMS: DisplayItemConfig[] = [
  { id: "time", label: "时间", enabled: false },
  { id: "fps", label: "FPS", enabled: false },
  { id: "fps_1low", label: "1% Low", enabled: false },
  { id: "fps_01low", label: "0.1% Low", enabled: false },
  { id: "cpu_temp", label: "CPU温度", enabled: false },
  { id: "cpu_usage", label: "CPU占用", enabled: true },
  { id: "cpu_fan_speed", label: "CPU风扇转速", enabled: false },
  { id: "cpu_clock", label: "CPU频率", enabled: false },
  { id: "cpu_voltage", label: "CPU电压", enabled: false },
  { id: "cpu_power", label: "CPU功耗", enabled: false },
  { id: "gpu_temp", label: "GPU温度", enabled: true },
  { id: "gpu_usage", label: "GPU占用", enabled: true },
  { id: "gpu_fan_speed", label: "GPU风扇转速", enabled: false },
  { id: "gpu_power", label: "GPU功耗", enabled: false },
  { id: "gpu_clock", label: "GPU频率", enabled: false },
  { id: "gpu_voltage", label: "GPU电压", enabled: false },
  { id: "gpu_vram", label: "GPU显存占用", enabled: false },
  { id: "gpu_memory_clock", label: "GPU显存频率", enabled: false },
  { id: "memory_usage", label: "内存占用", enabled: true },
  { id: "ssd_temp", label: "硬盘温度", enabled: false },
  { id: "game_ping", label: "游戏延迟", enabled: false },
  { id: "delta_password", label: "三角洲密码", enabled: false },
  { id: "net_down", label: "下载速率", enabled: false },
  { id: "net_up", label: "上传速率", enabled: false },
];

const DEFAULT_SETTINGS: OverlaySettings = {
  display_items: DEFAULT_DISPLAY_ITEMS,
  custom_items: [],
  opacity: 200,
  style: "vertical_panel",
  font: "Microsoft YaHei",
  font_size: 13,
  item_width: 220,
  font_color: "#ffffff",
};

// ===== 工具函数 =====

/** 根据 item.id 和 hardwareData 获取显示值 */
function getItemValue(item: DisplayItemConfig, data: HardwareData | null): string {
  // 时间项：优先网络时间
  if (item.id === "time") return formatNow(data);
  if (!data) return "--";
  switch (item.id) {
    case "fps":
      return data.fps != null ? `${data.fps}` : "--";
    case "fps_1low":
      return data.fps_1low != null ? `${data.fps_1low}` : "--";
    case "fps_01low":
      return data.fps_01low != null ? `${data.fps_01low}` : "--";
    case "cpu_temp":
      return data.cpu_temp != null ? `${data.cpu_temp.toFixed(0)}°C` : "--";
    case "cpu_usage":
      return data.cpu_usage != null ? `${data.cpu_usage}%` : "--";
    case "cpu_clock":
      return data.cpu_clock != null ? `${data.cpu_clock}MHz` : "--";
    case "cpu_voltage":
      return data.cpu_voltage != null ? `${data.cpu_voltage.toFixed(2)}V` : "--";
    case "cpu_power":
      return data.cpu_power != null ? `${data.cpu_power.toFixed(1)}W` : "--";
    case "cpu_fan_speed":
      return data.cpu_fan_speed != null ? `${data.cpu_fan_speed}RPM` : "--";
    case "gpu_temp":
      return data.gpu_temp != null ? `${data.gpu_temp.toFixed(0)}°C` : "--";
    case "gpu_usage":
      return data.gpu_usage != null ? `${data.gpu_usage}%` : "--";
    case "gpu_fan_speed":
      return data.gpu_fan_speed != null ? `${data.gpu_fan_speed}RPM` : "--";
    case "gpu_power":
      return data.gpu_power != null ? `${data.gpu_power}W` : "--";
    case "gpu_clock":
      return data.gpu_clock != null ? `${data.gpu_clock}MHz` : "--";
    case "gpu_voltage":
      return data.gpu_voltage != null ? `${data.gpu_voltage.toFixed(2)}V` : "--";
    case "gpu_vram":
      if (data.gpu_vram_used != null && data.gpu_vram_total != null) {
        return `${data.gpu_vram_used}/${data.gpu_vram_total}MB`;
      }
      return "--";
    case "gpu_memory_clock":
      return data.gpu_memory_clock != null ? `${data.gpu_memory_clock}MHz` : "--";
    case "memory_usage":
      return data.memory_usage != null ? `${data.memory_usage.toFixed(1)}%` : "--";
    case "ssd_temp":
      return data.ssd_temp != null ? `${data.ssd_temp.toFixed(0)}°C` : "--";
    case "game_ping":
      return data.game_ping != null ? `${data.game_ping}ms` : "--";
    case "delta_password":
      return data.delta_password ?? "--";
    case "net_down":
      return data.net_down_speed != null ? formatNetSpeed(data.net_down_speed) : "--";
    case "net_up":
      return data.net_up_speed != null ? formatNetSpeed(data.net_up_speed) : "--";
    default:
      return "--";
  }
}

/** 格式化速率：>=1MB/s 显示 MB/s，否则 KB/s（与 Rust 端 format_net_speed 一致） */
function formatNetSpeed(kbs: number): string {
  if (kbs >= 1024) return `${(kbs / 1024).toFixed(1)}MB/s`;
  return `${Math.round(kbs)}KB/s`;
}

/** 格式化毫秒时间戳为 HH:MM:SS（北京时间 UTC+8） */
function formatBeijingTime(ts: number): string {
  const d = new Date(ts + 8 * 3600 * 1000);
  const pad = (n: number) => String(n).padStart(2, "0");
  // 已偏移到东八区，用 UTC 方法读取
  return `${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}:${pad(d.getUTCSeconds())}`;
}

/**
 * 格式化当前时间为 HH:MM:SS。
 * 优先使用网络标准时间（data.net_time_offset_ms 校正），否则回退到北京时间。
 */
function formatNow(data: HardwareData | null): string {
  if (data?.net_time_offset_ms != null) {
    return formatBeijingTime(Date.now() + data.net_time_offset_ms);
  }
  return formatBeijingTime(Date.now());
}

/** 获取数值颜色（绿→黄→红），仅温度/占用/FPS 使用动态色，其余用 fallback */
function getValueColor(value: string, fallback: string = "#ffffff"): string {
  if (value === "--" || value === "") return fallback;
  const num = parseFloat(value);
  if (isNaN(num)) return fallback;

  // 温度类
  if (value.includes("°C")) {
    if (num < 60) return "#00ff88";
    if (num < 80) return "#ffcc00";
    return "#ff4444";
  }
  // 占用百分比类
  if (value.includes("%")) {
    if (num < 50) return "#00ff88";
    if (num < 80) return "#ffcc00";
    return "#ff4444";
  }
  // FPS（纯数字）
  if (/^\d+$/.test(value)) {
    if (num >= 120) return "#00ff88";
    if (num >= 60) return "#ffcc00";
    return "#ff4444";
  }
  // 瓦数、转速、频率、电压、延迟等 → 字体颜色
  return fallback;
}

// ===== 主组件 =====

export default function VerticalOverlayPage() {
  const [hardwareData, setHardwareData] = useState<HardwareData | null>(null);
  const [settings, setSettings] = useState<OverlaySettings>(DEFAULT_SETTINGS);
  const [settingsLoaded, setSettingsLoaded] = useState(false);
  // 每秒 tick：驱动时间显示刷新
  const [, setTick] = useState(0);
  const containerRef = useRef<HTMLDivElement>(null);
  const win = getCurrentWindow();
  // 记录拖动过程中的最新窗口位置，避免 startDragging 返回后读取到旧坐标
  const movedPosRef = useRef<{ x: number; y: number } | null>(null);
  const lastSaveRef = useRef(0);
  // 移动停止后的兜底保存定时器（拖动结束 → 立即关闭时防止最后一次位置丢失）
  const flushTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // 保存竖排悬浮框位置：Rust 端负责更新内存并写入 settings.json（仅更新位置字段、保留其他键），
  // 主应用通过 overlay-position-saved 事件同步共享 store。
  // 这里不再直接写 LazyStore：独立窗口的 store 缓存可能过期，整体写回会覆盖主应用的其他设置。
  const persistPosition = useCallback(async (x: number, y: number) => {
    try {
      await invoke("save_vertical_overlay_position", { x, y });
    } catch (e) {
      console.error("Failed to save vertical overlay position:", e);
    }
  }, []);

  // 监听窗口移动，拖动过程中实时保存位置（节流），保证不依赖 startDragging 结束后的捕获
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    (async () => {
      unlisten = await win.onMoved(({ payload }) => {
        movedPosRef.current = { x: payload.x, y: payload.y };
        const now = Date.now();
        if (now - lastSaveRef.current > 300) {
          lastSaveRef.current = now;
          persistPosition(payload.x, payload.y);
        }
        // 移动停止约 200ms 后兜底保存最终位置（配合 Rust 端关闭时保存，双保险）
        if (flushTimerRef.current) clearTimeout(flushTimerRef.current);
        flushTimerRef.current = setTimeout(() => {
          flushTimerRef.current = null;
          const pos = movedPosRef.current;
          if (pos) persistPosition(pos.x, pos.y);
        }, 200);
      });
    })();
    return () => {
      if (flushTimerRef.current) clearTimeout(flushTimerRef.current);
      unlisten?.();
    };
  }, [win, persistPosition]);

  // 页面渲染完成后通知 Rust 端 show 窗口（避免加载时的白屏闪烁）
  useEffect(() => {
    invoke("vertical_overlay_ready");
  }, []);

  // 每秒刷新（驱动时间显示）
  useEffect(() => {
    const timer = setInterval(() => setTick((t) => t + 1), 1000);
    return () => clearInterval(timer);
  }, []);

  // 强制背景透明（与桌面歌词窗口相同处理）
  useEffect(() => {
    const html = document.documentElement;
    const body = document.body;
    const root = document.getElementById("root");
    const prevHtmlBg = html.style.background;
    const prevBodyBg = body.style.background;
    const prevRootBg = root?.style.background;

    html.style.background = "transparent";
    body.style.background = "transparent";
    if (root) root.style.background = "transparent";

    return () => {
      html.style.background = prevHtmlBg;
      body.style.background = prevBodyBg;
      if (root) root.style.background = prevRootBg || "";
    };
  }, []);

  // 获取设置
  useEffect(() => {
    (async () => {
      try {
        const stored = await invoke<OverlaySettings>("get_overlay_current_settings");
        setSettings(stored);
      } catch {
        // 使用默认设置
      }
      setSettingsLoaded(true);
    })();
  }, []);

  // 监听硬件数据
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    (async () => {
      unlisten = await listen<HardwareData>("vertical-overlay-data", (event) => {
        setHardwareData(event.payload);
      });
    })();
    return () => { unlisten?.(); };
  }, []);

  // 监听设置更新
  useEffect(() => {
    let unlisten: UnlistenFn | undefined;
    (async () => {
      unlisten = await listen<OverlaySettings>("vertical-overlay-settings", (event) => {
        setSettings(event.payload);
      });
    })();
    return () => { unlisten?.(); };
  }, []);

  // 默认开启鼠标穿透
  useEffect(() => {
    invoke("set_vertical_overlay_click_through", { enabled: true }).catch(() => {});
  }, []);

  // 窗口大小自适应：每次渲染后同步调整，确保绘制前窗口尺寸正确
  useLayoutEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    // 用 requestAnimationFrame 确保 DOM 布局已稳定
    requestAnimationFrame(() => {
      const height = el.scrollHeight;
      if (height > 0) {
        invoke("resize_vertical_overlay", { height }).catch(() => {});
      }
    });
  });

  // 计算启用的项
  const enabledItems = settings.display_items.filter((i) => i.enabled);
  const enabledCustomItems = settings.custom_items.filter((i) => i.enabled);

  // 计算背景不透明度
  const bgOpacity = settings.opacity / 255;

  return (
    <div
      ref={containerRef}
      style={{
        width: `${settings.item_width || 220}px`,
        background: `rgba(17, 17, 17, ${bgOpacity})`,
        borderRadius: "12px",
        padding: "10px 12px",
        display: "flex",
        flexDirection: "column",
        gap: "2px",
        fontFamily: settings.font || "Microsoft YaHei",
        fontSize: `${settings.font_size || 13}px`,
        color: "#ffffff",
        userSelect: "none",
        WebkitUserSelect: "none",
        border: "none",
        overflow: "hidden",
      }}
      onMouseDown={(e) => {
        e.preventDefault();
        // 发起 OS 级拖动（不依赖其返回时机，避免读到拖动前位置）
        win.startDragging().catch(() => {});
        // 拖动结束后（mouseup）保存最终位置
        const handleUp = async () => {
          window.removeEventListener("mouseup", handleUp);
          try {
            const pos = movedPosRef.current ?? (await win.outerPosition());
            if (pos) {
              await persistPosition(pos.x, pos.y);
            }
          } catch (err) {
            console.error("Failed to save vertical overlay position:", err);
          } finally {
            // 无论是否出错都恢复鼠标穿透，避免面板卡在可拖动状态
            invoke("set_vertical_overlay_click_through", { enabled: true }).catch(() => {});
          }
        };
        window.addEventListener("mouseup", handleUp, { once: true });
      }}
    >
      {!settingsLoaded ? (
        <div style={{ padding: "1px 0" }} />
      ) : (
        <>
      {/* 内置项列表 */}
      {enabledItems.map((item) => {
        const rawValue = getItemValue(item, hardwareData);
        const IconComp = ITEM_ICONS[item.id] ?? Gauge;
        const valueColor = getValueColor(rawValue, settings.font_color);

        // 三角洲密码单独处理：每张地图竖着排列
        if (item.id === "delta_password" && rawValue !== "--") {
          const entries = rawValue.split(/\s{2,}/).filter(Boolean);
          return (
            <div key={item.id} style={{ padding: "3px 0", borderBottom: "1px solid rgba(255,255,255,0.05)" }}>
              <div style={{ display: "flex", alignItems: "center", gap: "8px", marginBottom: entries.length > 1 ? "4px" : 0 }}>
                <IconComp size={14} style={{ flexShrink: 0, color: settings.font_color }} />
                <span style={{ flex: 1, whiteSpace: "nowrap", color: settings.font_color, fontWeight: 500 }}>
                  {item.label}
                </span>
              </div>
              {entries.length > 0 ? (
                <div style={{ paddingLeft: "22px", display: "flex", flexDirection: "column", gap: "2px" }}>
                  {entries.map((entry, idx) => {
                    const colonIdx = entry.indexOf("：");
                    const mapName = colonIdx > 0 ? entry.slice(0, colonIdx) : entry;
                    const password = colonIdx > 0 ? entry.slice(colonIdx + 1) : "";
                    return (
                      <div key={idx} style={{ display: "flex", alignItems: "center", gap: "6px", fontSize: "12px" }}>
                        <span style={{ color: "rgba(255,255,255,0.5)", flex: 1, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis" }}>
                          {mapName}
                        </span>
                        <span style={{ fontWeight: 600, whiteSpace: "nowrap", color: "#ffcc00" }}>
                          {password}
                        </span>
                      </div>
                    );
                  })}
                </div>
              ) : (
                <span style={{ paddingLeft: "22px", textAlign: "right", fontWeight: 600, whiteSpace: "nowrap", color: valueColor }}>
                  {rawValue}
                </span>
              )}
            </div>
          );
        }

        return (
          <div
            key={item.id}
            style={{
              display: "flex",
              alignItems: "center",
              gap: "8px",
              padding: "3px 0",
              borderBottom: "1px solid rgba(255, 255, 255, 0.05)",
            }}
          >
            <IconComp size={14} style={{ flexShrink: 0, color: settings.font_color }} />
            <span style={{ flex: 1, whiteSpace: "nowrap", color: settings.font_color }}>
              {item.label}
            </span>
            <span style={{ textAlign: "right", fontWeight: 600, whiteSpace: "nowrap", color: valueColor }}>
              {rawValue}
            </span>
          </div>
        );
      })}

      {/* 自定义项列表 */}
      {enabledCustomItems.map((item) => (
        <div
          key={item.id}
          style={{
            display: "flex",
            alignItems: "center",
            gap: "8px",
            padding: "3px 0",
            borderBottom: "1px solid rgba(255, 255, 255, 0.05)",
          }}
        >
          <span style={{ flexShrink: 0, width: 14, textAlign: "center", color: settings.font_color }}>•</span>
          <span style={{ flex: 1, whiteSpace: "nowrap", color: settings.font_color }}>
            {item.text}
          </span>
          <span style={{ textAlign: "right", fontWeight: 600, whiteSpace: "nowrap", color: item.color }}>
            ●
          </span>
        </div>
      ))}

      {/* 空状态 */}
      {enabledItems.length === 0 && enabledCustomItems.length === 0 && (
        <div style={{ textAlign: "center", padding: "12px 0", color: settings.font_color, opacity: 0.5 }}>
          请在设置中启用显示项
        </div>
      )}
        </>
      )}
    </div>
  );
}
