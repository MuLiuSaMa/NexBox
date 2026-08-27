import { createContext, useContext, useState, useCallback, useRef, useEffect, ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Store } from "@tauri-apps/plugin-store";

interface MusicFile {
  name: string;
  path: string;
}

type PlayMode = "list" | "shuffle" | "one";

interface MusicContextType {
  musicFiles: MusicFile[];
  currentIndex: number;
  isPlaying: boolean;
  currentTime: number;
  duration: number;
  volume: number;
  isMuted: boolean;
  playMode: PlayMode;
  currentFileName: string | null;
  togglePlay: () => void;
  stopPlayback: () => void;
  setVolume: (v: number) => void;
  toggleMute: () => void;
  nextTrack: () => void;
  prevTrack: () => void;
  seekTo: (time: number) => void;
  playTrack: (index: number) => void;
}

const MusicContext = createContext<MusicContextType | null>(null);

let storeInstance: Store | null = null;

const getStore = async (): Promise<Store> => {
  if (!storeInstance) {
    storeInstance = await Store.load("music-player-settings.json");
  }
  return storeInstance;
};

export function useMusicPlayer() {
  const context = useContext(MusicContext);
  if (!context) {
    throw new Error("useMusicPlayer must be used within MusicProvider");
  }
  return context;
}

export function MusicProvider({ children }: { children: ReactNode }) {
  const [musicFiles, setMusicFiles] = useState<MusicFile[]>([]);
  const [currentIndex, setCurrentIndex] = useState(0);
  const [isPlaying, setIsPlaying] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const [volume, setVolumeState] = useState(0.7);
  const [isMuted, setIsMuted] = useState(false);
  const [playMode, setPlayMode] = useState<PlayMode>("list");

  const audioRef = useRef<HTMLAudioElement | null>(null);

  useEffect(() => {
    loadSettings();
    loadMusicFiles();
  }, []);

  useEffect(() => {
    if (!audioRef.current && typeof document !== "undefined") {
      const audio = document.createElement("audio");
      // CORS 模式加载音频：createMediaElementSource 频谱（音域回响）需要 CORS 通过的媒体，
      // 本地代理(127.0.0.1:port/audio)已返回 Access-Control-Allow-Origin/CORP 头。
      // preload=metadata：只加载元数据，禁止 Chromium 预缓冲整首歌（内存平）
      audio.crossOrigin = "anonymous";
      audio.preload = "metadata";
      audio.style.display = "none";
      document.body.appendChild(audio);
      audioRef.current = audio;

      audio.addEventListener("timeupdate", () => {
        if (audioRef.current) {
          setCurrentTime(audioRef.current.currentTime);
        }
      });

      audio.addEventListener("loadedmetadata", () => {
        if (audioRef.current) {
          setDuration(audioRef.current.duration);
        }
      });

      audio.addEventListener("ended", handleEnded);
    }

    return () => {
      if (audioRef.current && audioRef.current.parentNode) {
        audioRef.current.pause();
        audioRef.current.src = "";
        audioRef.current.parentNode.removeChild(audioRef.current);
        audioRef.current = null;
      }
    };
  }, []);

  const loadSettings = async () => {
    try {
      const s = await getStore();
      const savedVolume = await s.get<number>("volume");
      if (savedVolume !== undefined && savedVolume !== null) {
        setVolumeState(savedVolume);
      }
      const savedMode = await s.get<PlayMode>("playMode");
      if (savedMode) {
        setPlayMode(savedMode);
      }
    } catch (error) {
      console.error("Failed to load music settings:", error);
    }
  };

  const saveSetting = async (key: string, value: unknown) => {
    try {
      const s = await getStore();
      await s.set(key, value);
      await s.save();
    } catch (error) {
      console.error("Failed to save setting:", error);
    }
  };

  const loadMusicFiles = async () => {
    try {
      const files = await invoke<MusicFile[]>("get_music_files");
      setMusicFiles(files);
    } catch (error) {
      console.error("Failed to load music files:", error);
    }
  };

  const switchToIndex = useCallback((newIndex: number) => {
    if (!audioRef.current || musicFiles.length === 0 || newIndex < 0 || newIndex >= musicFiles.length) return;

    const file = musicFiles[newIndex];
    const newSrc = `/${file.path}`;

    const audio = audioRef.current;
    audio.src = newSrc;
    audio.volume = isMuted ? 0 : volume;
    audio.load();
    audio.play().catch(() => {});

    setCurrentTime(0);
    setDuration(0);
    setCurrentIndex(newIndex);
    setIsPlaying(true);
  }, [musicFiles, volume, isMuted]);

  const handleEnded = useCallback(() => {
    if (playMode === "one") {
      if (audioRef.current) {
        audioRef.current.currentTime = 0;
        audioRef.current.play().catch(() => {});
      }
    } else if (currentIndex < musicFiles.length - 1 || playMode === "list") {
      let nextIndex = currentIndex + 1;
      if (nextIndex >= musicFiles.length) {
        nextIndex = 0;
      }
      switchToIndex(nextIndex);
    } else {
      setIsPlaying(false);
    }
  }, [playMode, currentIndex, musicFiles.length, switchToIndex]);

  const togglePlay = useCallback(async () => {
    if (!audioRef.current || musicFiles.length === 0) return;

    const currentFile = musicFiles[currentIndex];

    if (!currentFile) {
      switchToIndex(0);
      return;
    }

    const audio = audioRef.current;
    if (isPlaying) {
      audio.pause();
      setIsPlaying(false);
    } else {
      if (audio.src === "" || !audio.src.includes(currentFile.path)) {
        audio.src = `/${currentFile.path}`;
        audio.volume = isMuted ? 0 : volume;
        audio.load();
      }
      try {
        await audio.play();
        setIsPlaying(true);
      } catch {
        setIsPlaying(false);
      }
    }
  }, [isPlaying, currentIndex, musicFiles, volume, isMuted, switchToIndex]);

  const stopPlayback = useCallback(() => {
    if (audioRef.current) {
      audioRef.current.pause();
      audioRef.current.currentTime = 0;
      setIsPlaying(false);
      setCurrentTime(0);
    }
  }, []);

  const setVolume = useCallback((v: number) => {
    setVolumeState(v);
    saveSetting("volume", v);
    if (audioRef.current) {
      audioRef.current.volume = v;
      if (v > 0 && isMuted) {
        setIsMuted(false);
      }
    }
  }, [isMuted]);

  const toggleMute = useCallback(() => {
    if (audioRef.current) {
      audioRef.current.muted = !isMuted;
      setIsMuted(!isMuted);
    }
  }, [isMuted]);

  const nextTrack = useCallback(() => {
    if (musicFiles.length === 0) return;
    let nextIndex = currentIndex + 1;
    if (nextIndex >= musicFiles.length) {
      nextIndex = 0;
    }
    switchToIndex(nextIndex);
  }, [musicFiles, currentIndex, switchToIndex]);

  const prevTrack = useCallback(() => {
    if (musicFiles.length === 0) return;
    let prevIndex = currentIndex - 1;
    if (prevIndex < 0) {
      prevIndex = musicFiles.length - 1;
    }
    switchToIndex(prevIndex);
  }, [musicFiles, currentIndex, switchToIndex]);

  const seekTo = useCallback((time: number) => {
    if (audioRef.current) {
      audioRef.current.currentTime = time;
      setCurrentTime(time);
    }
  }, []);

  const playTrack = useCallback((index: number) => {
    if (index >= 0 && index < musicFiles.length) {
      switchToIndex(index);
    }
  }, [musicFiles, switchToIndex]);

  const currentFileName = musicFiles[currentIndex]?.name ?? null;

  const value: MusicContextType = {
    musicFiles,
    currentIndex,
    isPlaying,
    currentTime,
    duration,
    volume,
    isMuted,
    playMode,
    currentFileName,
    togglePlay,
    stopPlayback,
    setVolume,
    toggleMute,
    nextTrack,
    prevTrack,
    seekTo,
    playTrack,
  };

  return (
    <MusicContext.Provider value={value}>
      {children}
    </MusicContext.Provider>
  );
}
