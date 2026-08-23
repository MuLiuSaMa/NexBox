import { useEffect, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { useTranslation } from "react-i18next";
import { useNavigate } from "react-router-dom";

const ISLAND_ID = "accel-download";

interface AccelProgressPayload {
  taskId: string;
  fileName: string;
  downloadedBytes: number;
  totalBytes: number;
  speedBps: number;
  status: number;
}

interface TaskSnap {
  downloaded: number;
  total: number;
  speed: number;
  status: number;
}

const STATUS_DOWNLOADING = 1;
const STATUS_COMPLETED = 3;
const STATUS_ERROR = 4;

function formatSpeed(bps: number): string {
  if (!bps || bps <= 0) return "0 KB/s";
  const units = ["B", "KB", "MB", "GB"];
  let v = bps;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i++;
  }
  return `${v >= 100 || i === 0 ? Math.round(v) : v.toFixed(1)} ${units[i]}/s`;
}

/**
 * 下载加速器 → 灵动岛桥接。
 *
 * 与自动更新（update-context）同一模式：持久岛基线 + 静默进度同步。
 * 全局监听 accel-progress 事件，任一任务进入下载态时显示聚合进度岛
 * （Σ已下/Σ总量 + 总速度 + 活跃数），全部离开下载态后以成功/失败收尾
 * 并自动关闭。点击岛跳转下载加速器页面。
 */
export function AccelIslandBridge() {
  const toast = useDynamicIsland("download");
  const { t } = useTranslation();
  const navigate = useNavigate();

  const tasksRef = useRef<Map<string, TaskSnap>>(new Map());
  const visibleRef = useRef(false);
  const settleTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    const clearSettle = () => {
      if (settleTimerRef.current) {
        clearTimeout(settleTimerRef.current);
        settleTimerRef.current = null;
      }
    };

    const unlisten = listen<AccelProgressPayload>("accel-progress", (event) => {
      const p = event.payload;
      tasksRef.current.set(p.taskId, {
        downloaded: p.downloadedBytes,
        total: p.totalBytes,
        speed: p.speedBps,
        status: p.status,
      });

      const list = [...tasksRef.current.values()];
      const downloading = list.filter(
        (x) => x.status === STATUS_DOWNLOADING || x.status === 5
      );
      const failed = list.filter((x) => x.status === STATUS_ERROR);

      if (downloading.length > 0) {
        clearSettle();
        const dSum = downloading.reduce((a, x) => a + x.downloaded, 0);
        const tSum = downloading.reduce((a, x) => a + x.total, 0);
        const spd = downloading.reduce((a, x) => a + x.speed, 0);
        // 未知总大小的单流任务不计入分母；全未知时保持上次进度
        let pct = 0;
        if (tSum > 0) pct = Math.min(100, (dSum / tSum) * 100);

        const opts = {
          iconKey: "download" as const,
          status: "blue" as const,
          title:
            downloading.length > 1
              ? `${t("downloadAccel.islandDownloading")} × ${downloading.length}`
              : t("downloadAccel.islandDownloading"),
          description: `${formatSpeed(spd)}${downloading.length > 1 ? "" : ` · ${pct.toFixed(0)}%`}`,
          progress: pct,
          onClick: () => navigate("/download-accelerator"),
        };
        if (!visibleRef.current) {
          visibleRef.current = true;
          toast.showPersistent({
            id: ISLAND_ID,
            persistent: true,
            ...opts,
          });
        } else {
          // 静默更新：不触发缩→扩动画
          toast.updatePersistent(ISLAND_ID, opts, false);
        }
        return;
      }

      // 无下载中任务：若岛还挂着则以终态收尾
      if (visibleRef.current) {
        visibleRef.current = false;
        const hasFail = failed.length > 0;
        const doneCount = list.filter((x) => x.status === STATUS_COMPLETED).length;
        toast.updatePersistent(
          ISLAND_ID,
          hasFail
            ? {
                status: "error",
                title: t("downloadAccel.islandFailed"),
                description: `${failed.length}`,
              }
            : {
                status: "success",
                title: t("downloadAccel.islandComplete"),
                description: doneCount > 0 ? `${doneCount}` : "",
              },
          true
        );
        settleTimerRef.current = setTimeout(() => {
          toast.closePersistent(ISLAND_ID);
          tasksRef.current.clear();
        }, 4000);
      }
    });
    return () => {
      unlisten.then((fn) => fn());
      clearSettle();
    };
  }, [toast, t, navigate]);

  return null;
}
