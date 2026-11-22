import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

export interface CommunityTool {
  id: string;
  name: string;
  version: string | null;
  description: string | null;
  category: string;
  tags: string[];
  author: string | null;
  publisher: string | null;
  homepage: string | null;
  submitted_at: string | null;
  launch_target: string | null;
  file: string | null;
  icon: string | null;
  download_url: string | null;
  download_filter: string | null;
  repo_path: string;
  install_status: "not_installed" | "installed" | "update_available";
  icon_url: string | null;
}

export interface GitCodeUser {
  login: string;
  avatar_url: string | null;
  avatar_data?: string | null;
  name: string | null;
}

export interface GitCodeLoginStatus {
  logged_in: boolean;
  user: GitCodeUser | null;
}

export interface SubmitCommunityToolParams {
  name: string;
  description: string;
  category: string;
  tags: string;
  zipPath?: string | null;
  launchTarget?: string | null;
  publisher?: string | null;
  homepage?: string | null;
  version?: string | null;
  iconPath?: string | null;
  downloadUrl?: string | null;
  downloadFilter?: string | null;
}



/** 打开外部链接：优先 tauri shell，失败回退 window.open */
async function openExternal(url: string) {
  try {
    const { open } = await import("@tauri-apps/plugin-shell");
    await open(url);
  } catch {
    window.open(url, "_blank");
  }
}

export function useCommunityTools() {
  const [tools, setTools] = useState<CommunityTool[]>([]);
  const [categories, setCategories] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [loginStatus, setLoginStatus] = useState<GitCodeLoginStatus>({
    logged_in: false,
    user: null,
  });
  const [submitting, setSubmitting] = useState(false);
  const [submitProgress, setSubmitProgress] = useState<string | null>(null);
  const [removing, setRemoving] = useState(false);
  const [removeProgress, setRemoveProgress] = useState<string | null>(null);
  const [installing, setInstalling] = useState(false);
  const [installPercent, setInstallPercent] = useState<number | null>(null);
  const [installMessage, setInstallMessage] = useState<string | null>(null);
  const [downloadDir, setDownloadDirState] = useState<string>("");
  const pollTimer = useRef<ReturnType<typeof setInterval> | null>(null);

  useEffect(() => {
    invoke<string>("get_community_download_dir")
      .then((dir) => setDownloadDirState(dir))
      .catch(() => {});
  }, []);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      await invoke("invalidate_community_cache");
      const data = await invoke<CommunityTool[]>("get_community_tools");
      setTools(data);
      setCategories([...new Set(data.map((t) => t.category))].sort());
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  /** 后端抓头像转 data URI（WebView 直接加载 GitCode CDN 图不可靠） */
  const loadAvatarData = useCallback(async (login: string) => {
    try {
      const data = await invoke<string | null>("get_gitcode_avatar_data");
      if (data) {
        setLoginStatus((prev) =>
          prev.user?.login === login ? { ...prev, user: { ...prev.user, avatar_data: data } } : prev
        );
      }
    } catch {
      /* ignore */
    }
  }, []);

  const refreshLogin = useCallback(async () => {
    try {
      const status = await invoke<GitCodeLoginStatus>("get_gitcode_login_status");
      setLoginStatus(status);
      if (status.logged_in && status.user) {
        await loadAvatarData(status.user.login);
      }
    } catch {
      /* ignore */
    }
  }, [loadAvatarData]);

  useEffect(() => {
    refresh();
    refreshLogin();
    return () => {
      if (pollTimer.current) clearInterval(pollTimer.current);
    };
  }, [refresh, refreshLogin]);

  /** 发起授权码登录：打开浏览器授权并轮询登录状态，成功返回 true */
  const login = useCallback(async (): Promise<boolean> => {
    try {
      const authorizeUrl = await invoke<string>("gitcode_login_start");
      await openExternal(authorizeUrl);
    } catch (e) {
      setError(String(e));
      return false;
    }
    return new Promise((resolve) => {
      if (pollTimer.current) clearInterval(pollTimer.current);
      let tries = 0;
      const timer = setInterval(async () => {
        tries += 1;
        try {
          const status = await invoke<GitCodeLoginStatus>("get_gitcode_login_status");
          setLoginStatus(status);
          if (status.logged_in) {
            clearInterval(timer);
            pollTimer.current = null;
            if (status.user) await loadAvatarData(status.user.login);
            resolve(true);
            return;
          }
        } catch {
          /* retry */
        }
        if (tries >= 200) {
          clearInterval(timer);
          pollTimer.current = null;
          resolve(false);
        }
      }, 1500);
      pollTimer.current = timer;
    });
  }, [loadAvatarData]);

  const logout = useCallback(async () => {
    try {
      await invoke("gitcode_logout");
    } catch {
      /* ignore */
    }
    setLoginStatus({ logged_in: false, user: null });
  }, []);

  const isAuthor = useCallback(
    (tool: CommunityTool) => {
      if (!loginStatus.logged_in || !tool.author || !loginStatus.user) return false;
      return tool.author.toLowerCase() === loginStatus.user.login.toLowerCase();
    },
    [loginStatus]
  );

  const openZip = useCallback(
    async (tool: CommunityTool) => {
      await invoke("open_community_zip", {
        category: tool.category,
        id: tool.id,
        file: tool.file,
      });
    },
    []
  );

  const setDownloadDir = useCallback(async (dir: string) => {
    await invoke<string>("set_community_download_dir", { dir });
    setDownloadDirState(dir);
  }, []);

  const pickDownloadDir = useCallback(async () => {
    const dir = await invoke<string | null>("pick_community_download_dir");
    if (dir) await setDownloadDir(dir);
  }, [setDownloadDir]);

  const install = useCallback(async (tool: CommunityTool) => {
    setInstalling(true);
    setInstallPercent(null);
    setInstallMessage("准备下载...");
    setTools((prev) => prev.map((x) => (x.id === tool.id && x.category === tool.category ? { ...x, install_status: "installed" } : x)));
    try {
      const unlisten = await import("@tauri-apps/api/event").then(({ listen }) =>
        listen<{ message: string; percent?: number | null }>("community-install-progress", (e) => {
          setInstallMessage(e.payload.message);
          const p = e.payload.percent ?? null;
          if (typeof p === "number") setInstallPercent(p);
        })
      );
      await invoke("install_community_tool", {
        category: tool.category,
        id: tool.id,
        file: tool.file,
        downloadUrl: tool.download_url,
      });
      unlisten();
    } finally {
      setInstalling(false);
      setInstallPercent(null);
      setInstallMessage(null);
    }
  }, []);

  const submit = useCallback(
    async (params: SubmitCommunityToolParams): Promise<string> => {
      setSubmitting(true);
      setSubmitProgress("正在准备提交...");
      try {
        const unlisten = await import("@tauri-apps/api/event").then(({ listen }) =>
          listen<{ message: string }>("community-submit-progress", (e) => {
            setSubmitProgress(e.payload.message);
          })
        );
        const prUrl = await invoke<string>("submit_community_tool", {
          name: params.name,
          description: params.description,
          category: params.category,
          tags: params.tags,
          zipPath: params.zipPath ?? null,
          launchTarget: params.launchTarget ?? null,
          publisher: params.publisher ?? null,
          homepage: params.homepage ?? null,
          version: params.version ?? null,
          iconPath: params.iconPath ?? null,
          downloadUrl: params.downloadUrl ?? null,
          downloadFilter: params.downloadFilter ?? null,
        });
        unlisten();
        await refresh();
        return prUrl;
      } finally {
        setSubmitting(false);
        setSubmitProgress(null);
      }
    },
    [refresh]
  );

  const remove = useCallback(
    async (tool: CommunityTool) => {
      setRemoving(true);
      setRemoveProgress("正在准备删除...");
      try {
        const unlisten = await import("@tauri-apps/api/event").then(({ listen }) =>
          listen<{ message: string }>("community-submit-progress", (e) => {
            setRemoveProgress(e.payload.message);
          })
        );
        const prUrl = await invoke<string>("delete_community_tool", {
          id: tool.id,
          name: tool.name,
          category: tool.category,
          author: tool.author,
          repoPath: tool.repo_path,
        });
        unlisten();
        await refresh();
        return prUrl;
      } finally {
        setRemoving(false);
        setRemoveProgress(null);
      }
    },
    [refresh]
  );

  return {
    tools,
    categories,
    loading,
    error,
    loginStatus,
    submitting,
    submitProgress,
    removing,
    removeProgress,
    installing,
    installPercent,
    installMessage,
    refresh,
    refreshLogin,
    login,
    logout,
    isAuthor,
    install,
    openZip,
    downloadDir,
    setDownloadDir,
    pickDownloadDir,
    submit,
    remove,
  };
}