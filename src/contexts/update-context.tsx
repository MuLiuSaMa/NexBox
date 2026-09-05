"use client";

import {
  createContext,
  useContext,
  useState,
  useEffect,
  useCallback,
  useMemo,
  useRef,
  type ReactNode,
} from "react";
import { useNavigate } from "react-router-dom";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { useTranslation } from "react-i18next";
import { store } from "@/lib/store";
import { useAppStartup } from "@/contexts/app-startup-context";
import { fetchLatestRelease, compareVersions, type ReleaseInfo } from "@/lib/update-checker";

const CURRENT_VERSION = "v9.5.2";
const AUTO_UPDATE_KEY = "nexbox_auto_update";
/** 灵动岛更新下载岛的固定 id */
const UPDATE_ISLAND_ID = "update-download";

interface UpdateContextValue {
  latestRelease: ReleaseInfo | null;
  hasUpdate: boolean;
  isChecking: boolean;
  isDownloading: boolean;
  downloadProgress: number;
  isDownloadComplete: boolean;
  downloadedFilePath: string;
  isModalOpen: boolean;
  autoUpdateEnabled: boolean;
  setAutoUpdateEnabled: (value: boolean) => Promise<void>;
  openModal: () => void;
  closeModal: () => void;
  toggleModal: () => void;
  handleCheckUpdate: () => Promise<void>;
  manualDownload: () => Promise<void>;
  handleInstall: () => Promise<void>;
  handleSkip: () => Promise<void>;
}

const UpdateContext = createContext<UpdateContextValue | null>(null);

export function useUpdate() {
  const ctx = useContext(UpdateContext);
  if (!ctx) {
    throw new Error("useUpdate must be used within UpdateProvider");
  }
  return ctx;
}

export function UpdateProvider({ children }: { children: ReactNode }) {
  const { isStartupComplete } = useAppStartup();
  const navigate = useNavigate();
  const toast = useDynamicIsland("download");
  const { t } = useTranslation();

  const [autoUpdateEnabled, setAutoUpdateEnabledState] = useState(true);
  const [latestRelease, setLatestRelease] = useState<ReleaseInfo | null>(null);
  const [hasUpdate, setHasUpdate] = useState(false);
  const [isChecking, setIsChecking] = useState(false);
  const [isDownloading, setIsDownloading] = useState(false);
  const [autoUpdateLoaded, setAutoUpdateLoaded] = useState(false);
  const [downloadProgress, setDownloadProgress] = useState(0); // 目标进度
  const [displayProgress, setDisplayProgress] = useState(0); // 平滑显示的进度
  const [isDownloadComplete, setIsDownloadComplete] = useState(false);
  const [downloadedFilePath, setDownloadedFilePath] = useState<string>("");
  const [isModalOpen, setIsModalOpen] = useState(false);

  const downloadStartedRef = useRef(false);
  const autoUpdateLoadedRef = useRef(false);
  const autoUpdateEnabledRef = useRef(true);
  const downloadCancelledRef = useRef(false);
  const isDownloadingRef = useRef(false);
  const targetProgressRef = useRef(0);
  const displayProgressRef = useRef(0);
  // 下载完成后的安装包路径 ref：供灵动岛内 onClick 等闭包读取最新值，避免过期捕获
  const downloadedFilePathRef = useRef("");
  // 完成态标记：进度监听器据此阻止回写，避免延迟事件覆盖「点击重启」导致进度条回退/闪烁
  const isDownloadCompleteRef = useRef(false);
  // 岛内进度只涨不跌的保险
  const lastIslandProgressRef = useRef(0);

  // 平滑动画：显示值每 50ms 向目标值逼近，且强制单调递增、永不倒退
  useEffect(() => {
    const id = setInterval(() => {
      const target = targetProgressRef.current;
      const current = displayProgressRef.current;
      // 目标值 ≤ 当前显示值时保持不动（进度只涨不跌）；否则缓动逼近
      const next = Math.max(current, current + (target - current) * 0.4);
      if (next !== current) {
        displayProgressRef.current = next;
        setDisplayProgress(next);
      }
    }, 50);
    return () => clearInterval(id);
  }, []);

  // 读取静默更新设置（默认开启，强制布尔化防止字符串/数字等 truthy 假值误判）
  useEffect(() => {
    (async () => {
      try {
        const raw = await store.get<unknown>(AUTO_UPDATE_KEY);
        let v: boolean;
        if (raw === null || raw === undefined) {
          const ls = localStorage.getItem(AUTO_UPDATE_KEY);
          v = ls === null ? true : ls === "true";
        } else {
          v = raw === true || raw === "true" || raw === 1;
        }
        setAutoUpdateEnabledState(v);
        autoUpdateEnabledRef.current = v;
      } catch (error) {
        console.error("Failed to load auto update setting:", error);
      } finally {
        autoUpdateLoadedRef.current = true;
        setAutoUpdateLoaded(true);
      }
    })();
  }, []);

  // 监听静默更新设置变更广播（设置页等同步）
  useEffect(() => {
    const handler = (e: Event) => {
      const detail = (e as CustomEvent<boolean>).detail;
      if (typeof detail === "boolean") {
        setAutoUpdateEnabledState(detail);
        autoUpdateEnabledRef.current = detail;
      }
    };
    window.addEventListener("auto-update-setting-changed", handler as EventListener);
    return () => {
      window.removeEventListener("auto-update-setting-changed", handler as EventListener);
    };
  }, []);

  // 保持 ref 与 state 同步，供回调读取最新值
  useEffect(() => {
    autoUpdateEnabledRef.current = autoUpdateEnabled;
  }, [autoUpdateEnabled]);
  useEffect(() => {
    isDownloadingRef.current = isDownloading;
  }, [isDownloading]);
  useEffect(() => {
    downloadedFilePathRef.current = downloadedFilePath;
  }, [downloadedFilePath]);
  useEffect(() => {
    isDownloadCompleteRef.current = isDownloadComplete;
  }, [isDownloadComplete]);

  const setAutoUpdateEnabled = useCallback(async (value: boolean) => {
    setAutoUpdateEnabledState(value);
    autoUpdateEnabledRef.current = value;
    localStorage.setItem(AUTO_UPDATE_KEY, String(value));
    window.dispatchEvent(new CustomEvent("auto-update-setting-changed", { detail: value }));
    if (!value) {
      // 关闭静默更新：立即置取消标志（先于任何 await），确保下载 catch 分支静默处理
      downloadCancelledRef.current = true;
    }
    try {
      await store.set(AUTO_UPDATE_KEY, value);
      await store.save();
    } catch (error) {
      console.error("Failed to save auto update setting:", error);
    }
    if (!value) {
      // 关闭静默更新：立即取消进行中的下载、清除待安装标记，关闭软件不再自动更新
      try {
        await invoke("cancel_download");
      } catch (error) {
        console.error("Failed to cancel download:", error);
      }
      try {
        await invoke("clear_pending_install");
      } catch (error) {
        console.error("Failed to clear pending install:", error);
      }
      // 仅当下载已不在进行时才重置互斥锁；进行中的下载由其后端中止后的分支负责重置，
      // 避免用户重新开启后触发并发下载
      if (!isDownloadingRef.current) {
        downloadStartedRef.current = false;
      }
      setIsDownloading(false);
      setIsDownloadComplete(false);
      setDownloadedFilePath("");
      setDownloadProgress(0);
      setDisplayProgress(0);
      targetProgressRef.current = 0;
      displayProgressRef.current = 0;
      // 关闭更新弹窗与"有新版本"标记，彻底隐藏更新入口
      setIsModalOpen(false);
      setHasUpdate(false);
      // 关闭灵动岛更新下载岛
      toast.closePersistent(UPDATE_ISLAND_ID);
    }
  }, [toast]);

  // 下载进度事件：后端已按 200ms 节流，此处直接更新目标值，并静默同步灵动岛进度
  useEffect(() => {
    const unlisten = listen<{ progress: number; total: number }>("download-progress", (event) => {
      targetProgressRef.current = event.payload.progress;
      setDownloadProgress(event.payload.progress);
      // 完成态不再回写进度，避免延迟事件覆盖「点击重启」导致进度条回退/闪烁
      if (isDownloadCompleteRef.current) return;
      // 岛内进度只涨不跌（保险，杜绝任何回退）
      const next = Math.max(lastIslandProgressRef.current, event.payload.progress);
      lastIslandProgressRef.current = next;
      // 静默更新（animate=false），避免每 200ms 触发缩→扩动画
      toast.updatePersistent(UPDATE_ISLAND_ID, { progress: next }, false);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [toast]);

  // 重启安装：路径从 ref 读取，供灵动岛内 onClick 闭包获取最新值（不捕获过期 state）
  const handleInstall = useCallback(async () => {
    const filePath = downloadedFilePathRef.current;
    if (!filePath) return;
    try {
      await invoke("clear_pending_install");
      await invoke("install_update", {
        filePath,
      });
    } catch (error) {
      console.error("Failed to install:", error);
      toast({
        title: t("settings.aboutSettings.installFailed") || "安装失败",
        status: "error",
        duration: 2000,
        isClosable: true,
      });
    }
  }, [t, toast]);

  // 核心下载逻辑（静默与手动共用，互斥进行）
  // manual=true 表示用户手动点击下载：始终允许，不受静默开关限制
  const startDownload = useCallback(
    async (release: ReleaseInfo, openModalOnStart: boolean, manual = false) => {
      if (downloadStartedRef.current) return;
      // 硬门禁：自动下载（manual=false）在静默更新关闭时拒绝；手动下载始终允许
      if (!manual && !autoUpdateEnabledRef.current) {
        console.warn("[Update] 静默更新已关闭，拒绝自动下载");
        return;
      }
      downloadStartedRef.current = true;
      downloadCancelledRef.current = false;
      try {
        await invoke("reset_download_cancel");
      } catch (error) {
        console.error("Failed to reset download cancel flag:", error);
      }
      setLatestRelease(release);
      setHasUpdate(true);
      setIsDownloading(true);
      targetProgressRef.current = 0;
      displayProgressRef.current = 0;
      setDownloadProgress(0);
      setDisplayProgress(0);
      setIsDownloadComplete(false);
      if (openModalOnStart) setIsModalOpen(true);

      // 重置岛内进度/完成标记
      lastIslandProgressRef.current = 0;
      isDownloadCompleteRef.current = false;

      // 灵动岛显示更新下载基线（持久，不自动关闭）
      toast.showPersistent({
        id: UPDATE_ISLAND_ID,
        iconKey: "download",
        status: "blue",
        persistent: true,
        title: t("settings.aboutSettings.islandDownloading") || "新版本下载",
        description: release.tag_name,
        progress: 0,
      });

      try {
        const asset = release.assets.find(
          (a) => a.name.endsWith(".msi") || a.name.endsWith(".exe"),
        );
        if (asset) {
          // GitCode API 返回的资产 URL 落在 test.gitcode.net 等不可达 CDN，
          // 替换为主站域名后由其 302 重定向到 file-cdn.gitcode.com 正常下载
          const downloadUrl = asset.browser_download_url.replace(
            "https://test.gitcode.net/",
            "https://gitcode.com/",
          );
          const filePath = await invoke<string>("download_update", {
            url: downloadUrl,
            fileName: asset.name,
            silent: !manual,
          });
          // 若下载期间用户关闭了静默更新：删除文件并静默重置，绝不进入完成态
          if (downloadCancelledRef.current) {
            try {
              await invoke("delete_download_file", { filePath });
            } catch (error) {
              console.error("Failed to delete cancelled download:", error);
            }
            toast.closePersistent(UPDATE_ISLAND_ID);
            downloadStartedRef.current = false;
            setIsDownloading(false);
            setIsDownloadComplete(false);
            setDownloadedFilePath("");
            setDownloadProgress(0);
            setDisplayProgress(0);
            targetProgressRef.current = 0;
            displayProgressRef.current = 0;
            return;
          }
          setDownloadedFilePath(filePath);
          // 下载完成后延迟 1 秒再进入"点击重启"态：sync_all 之后给磁盘落盘
          // 与杀软首次扫描留出缓冲，避免文件尚未就绪就显示重启安装导致安装失败
          await new Promise((resolve) => setTimeout(resolve, 1000));
          targetProgressRef.current = 100;
          setDownloadProgress(100);
          // 同步置完成标记，杜绝延迟进度事件回写
          isDownloadCompleteRef.current = true;
          setIsDownloadComplete(true);
          setIsDownloading(false);
          // 灵动岛切换为「点击重启」，点击直接重启安装；常驻显示直至用户点击或跳过
          toast.updatePersistent(UPDATE_ISLAND_ID, {
            title: t("settings.aboutSettings.islandClickRestart") || "点击重启",
            status: "success",
            progress: undefined,
            onClick: handleInstall,
          });
          // 登记待安装标记：仅当静默更新仍开启时，关闭软件退出流程才自动启动安装向导
          if (autoUpdateEnabledRef.current) {
            try {
              await invoke("mark_pending_install", { filePath });
            } catch (error) {
              console.error("Failed to mark pending install:", error);
            }
          } else if (!manual) {
            // 自动下载但开关在下载期间被关闭（极端竞态）：删除文件，不进入完成态
            try {
              await invoke("delete_download_file", { filePath });
            } catch (error) {
              console.error("Failed to delete stale download:", error);
            }
            toast.closePersistent(UPDATE_ISLAND_ID);
            setIsDownloadComplete(false);
            setDownloadedFilePath("");
          }
          // 手动下载即使静默关闭也保留文件，显示"重启安装"供用户点击（但不登记自动安装）
        } else {
          toast.closePersistent(UPDATE_ISLAND_ID);
          downloadStartedRef.current = false;
          setIsDownloading(false);
          try {
            const { open } = await import("@tauri-apps/plugin-shell");
            await open(release.html_url);
          } catch (error) {
            console.error("Failed to open link:", error);
          }
        }
      } catch (error) {
        // 静默更新已关闭（用户主动取消 / 后端拒绝）：静默重置，绝不提示"下载失败"
        if (downloadCancelledRef.current || !autoUpdateEnabledRef.current) {
          toast.closePersistent(UPDATE_ISLAND_ID);
          downloadStartedRef.current = false;
          setIsDownloading(false);
          setDownloadProgress(0);
          setDisplayProgress(0);
          targetProgressRef.current = 0;
          displayProgressRef.current = 0;
          return;
        }
        console.error("Failed to download:", error);
        toast({
          title: t("settings.aboutSettings.downloadFailed") || "下载失败",
          status: "error",
          duration: 2000,
          isClosable: true,
        });
        toast.closePersistent(UPDATE_ISLAND_ID);
        setIsDownloading(false);
        downloadStartedRef.current = false;
      }
    },
    [t, toast, handleInstall],
  );

  // 启动完成后的自动检查：开启静默 → 后台下载不弹窗；关闭 → 弹窗但不下载
  useEffect(() => {
    if (!isStartupComplete) return;
    if (!autoUpdateLoaded) return;
    if (downloadStartedRef.current) return;

    (async () => {
      try {
        const release = await fetchLatestRelease();
        if (release) {
          const found = compareVersions(CURRENT_VERSION, release.tag_name);
          if (found) {
            setLatestRelease(release);
            setHasUpdate(true);
            if (autoUpdateEnabledRef.current) {
              await startDownload(release, false);
            } else {
              // 静默更新关闭：仅弹窗通知，不下载
              setIsModalOpen(true);
            }
          }
        }
      } catch (error) {
        console.error("[Update] 启动更新检查失败:", error);
      }
    })();
  }, [isStartupComplete, autoUpdateLoaded, startDownload]);

  // 托盘"检查更新"事件
  useEffect(() => {
    const unlisten = listen("check-update", async () => {
      // 仅主窗口跳转关于页；托盘菜单窗口也挂载了本 Provider，
      // 若不拦截会导致托盘菜单被导航到设置页（下次打开变成主窗口内容）。
      if (getCurrentWindow().label !== "main") return;

      navigate("/settings?section=about");

      setTimeout(async () => {
        // 等待静默更新设置加载完成，避免竞态误用默认值（true）
        while (!autoUpdateLoadedRef.current) {
          await new Promise((resolve) => setTimeout(resolve, 50));
        }
        // 静默更新已关闭：托盘检查不再下载、不再弹窗，仅提示已关闭
        if (!autoUpdateEnabledRef.current) {
          toast({
            title: t("settings.generalSettings.autoUpdate") || "自动静默更新",
            description: t("settings.aboutSettings.autoUpdateDisabledHint") || "已关闭，请开启后自动更新",
            status: "info",
            duration: 2000,
            isClosable: true,
          });
          return;
        }
        if (downloadStartedRef.current) {
          setIsModalOpen(true);
          return;
        }
        try {
          const release = await fetchLatestRelease();
          if (release) {
            const found = compareVersions(CURRENT_VERSION, release.tag_name);
            if (found) {
              setLatestRelease(release);
              setHasUpdate(true);
              if (autoUpdateEnabledRef.current) {
                await startDownload(release, false);
                setIsModalOpen(true);
              } else {
                setIsModalOpen(true);
              }
            } else {
              toast({
                title: t("settings.aboutSettings.noUpdate") || "已是最新版本",
                status: "success",
                duration: 2000,
                isClosable: true,
              });
            }
          }
        } catch (error) {
          console.error("Failed to check for updates:", error);
          toast({
            title: t("settings.aboutSettings.checkFailed") || "检查失败",
            status: "error",
            duration: 2000,
            isClosable: true,
          });
        }
      }, 500);
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, [navigate, toast, t, startDownload]);

  // 手动检查（设置页按钮）：开启静默且有新版 → 自动下载并打开弹窗显示进度
  const handleCheckUpdate = useCallback(async () => {
    if (isDownloading || isDownloadComplete) {
      setIsModalOpen(true);
      return;
    }
    setIsChecking(true);
    try {
      const release = await fetchLatestRelease();
      if (release) {
        const found = compareVersions(CURRENT_VERSION, release.tag_name);
        if (found) {
          if (autoUpdateEnabledRef.current) {
            await startDownload(release, true);
          } else {
            setLatestRelease(release);
            setHasUpdate(true);
            setIsModalOpen(true);
          }
        } else {
          toast({
            title: t("settings.aboutSettings.noUpdate") || "已是最新版本",
            status: "success",
            duration: 2000,
            isClosable: true,
          });
        }
      } else {
        toast({
          title: t("settings.aboutSettings.checkFailed") || "检查失败",
          status: "error",
          duration: 2000,
          isClosable: true,
        });
      }
    } catch (error) {
      console.error("Failed to check for updates:", error);
      toast({
        title: t("settings.aboutSettings.checkFailed") || "检查失败",
        status: "error",
        duration: 2000,
        isClosable: true,
      });
    } finally {
      setIsChecking(false);
    }
  }, [isDownloading, isDownloadComplete, startDownload, t, toast]);

  // 弹窗"下载"按钮：用户手动点击，无论静默开关是否开启都允许下载
  const manualDownload = useCallback(async () => {
    if (isDownloading || isDownloadComplete) return;
    let release = latestRelease;
    if (!release) {
      release = await fetchLatestRelease();
    }
    if (!release) {
      toast({
        title: t("settings.aboutSettings.checkFailed") || "检查失败",
        status: "error",
        duration: 2000,
        isClosable: true,
      });
      return;
    }
    const found = compareVersions(CURRENT_VERSION, release.tag_name);
    if (!found) {
      toast({
        title: t("settings.aboutSettings.noUpdate") || "已是最新版本",
        status: "success",
        duration: 2000,
        isClosable: true,
      });
      return;
    }
    await startDownload(release, false, true);
  }, [isDownloading, isDownloadComplete, latestRelease, startDownload, t, toast]);

  const handleSkip = useCallback(async () => {
    // 关闭灵动岛更新下载岛
    toast.closePersistent(UPDATE_ISLAND_ID);
    if (downloadedFilePath) {
      try {
        await invoke("delete_download_file", {
          filePath: downloadedFilePath,
        });
      } catch (error) {
        console.error("Failed to delete file:", error);
      }
    }
    try {
      await invoke("clear_pending_install");
    } catch (error) {
      console.error("Failed to clear pending install:", error);
    }
    downloadStartedRef.current = false;
    setIsModalOpen(false);
    setIsDownloadComplete(false);
    setDownloadedFilePath("");
    setIsDownloading(false);
    setLatestRelease(null);
    setHasUpdate(false);
  }, [downloadedFilePath, toast]);

  const openModal = useCallback(() => setIsModalOpen(true), []);
  const closeModal = useCallback(() => setIsModalOpen(false), []);
  const toggleModal = useCallback(() => setIsModalOpen((v) => !v), []);

  const value = useMemo<UpdateContextValue>(
    () => ({
      latestRelease,
      hasUpdate,
      isChecking,
      isDownloading,
      downloadProgress: displayProgress,
      isDownloadComplete,
      downloadedFilePath,
      isModalOpen,
      autoUpdateEnabled,
      setAutoUpdateEnabled,
      openModal,
      closeModal,
      toggleModal,
      handleCheckUpdate,
      manualDownload,
      handleInstall,
      handleSkip,
    }),
    [
      latestRelease,
      hasUpdate,
      isChecking,
      isDownloading,
      displayProgress,
      isDownloadComplete,
      downloadedFilePath,
      isModalOpen,
      autoUpdateEnabled,
      setAutoUpdateEnabled,
      openModal,
      closeModal,
      toggleModal,
      handleCheckUpdate,
      manualDownload,
      handleInstall,
      handleSkip,
    ],
  );

  return <UpdateContext.Provider value={value}>{children}</UpdateContext.Provider>;
}
