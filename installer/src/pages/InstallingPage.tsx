import { useEffect, useRef, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { invoke } from "@tauri-apps/api/core";

interface InstallingPageProps {
  targetDir: string;
  createDesktopShortcut: boolean;
  onComplete: () => void;
  onError: (msg: string) => void;
}

interface Slide {
  src: string;
  title: string;
}

const SLIDES: Slide[] = [
  { src: "/showcase/听听音乐.png", title: "听听音乐" },
  { src: "/showcase/强大的功能.png", title: "强大的功能" },
  { src: "/showcase/管理你的Steam游戏库.png", title: "管理你的Steam游戏库" },
  { src: "/showcase/美观的UI.png", title: "美观的UI" },
];

/** 随机打乱数组（每轮安装起始顺序随机） */
function shuffled<T>(arr: T[]): T[] {
  const a = [...arr];
  for (let i = a.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [a[i], a[j]] = [a[j], a[i]];
  }
  return a;
}

export default function InstallingPage({
  targetDir,
  createDesktopShortcut,
  onComplete,
  onError,
}: InstallingPageProps) {
  const [progress, setProgress] = useState(0);
  const hasStarted = useRef(false);
  const [order] = useState(() => shuffled(SLIDES));
  const [imgIndex, setImgIndex] = useState(0);

  // 图片轮播
  useEffect(() => {
    const id = setInterval(() => {
      setImgIndex((i) => (i + 1) % order.length);
    }, 2500);
    return () => clearInterval(id);
  }, [order.length]);

  useEffect(() => {
    if (hasStarted.current) return;
    hasStarted.current = true;

    const doInstall = async () => {
      try {
        for (let i = 0; i <= 70; i += 5) {
          setProgress(i);
          await new Promise((r) => setTimeout(r, 30));
        }

        await invoke("install", {
          targetDir,
          createDesktopShortcut,
        });

        for (let i = 70; i <= 85; i += 3) {
          setProgress(i);
          await new Promise((r) => setTimeout(r, 20));
        }

        for (let i = 85; i < 100; i += 2) {
          setProgress(i);
          await new Promise((r) => setTimeout(r, 15));
        }

        setProgress(100);
        await new Promise((r) => setTimeout(r, 400));
        onComplete();
      } catch (err: any) {
        onError(String(err));
      }
    };

    doInstall();
  }, []);

  const current = order[imgIndex];

  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.3 }}
      style={{ display: "flex", flexDirection: "column", flex: 1, minHeight: 0 }}
    >
      {/* 顶部：当前展示图片的标题 */}
      <div style={{ textAlign: "center" }}>
        <AnimatePresence mode="wait">
          <motion.h2
            key={current.title}
            initial={{ opacity: 0, y: 5 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -5 }}
            transition={{ duration: 0.25 }}
            className="page-title"
          >
            {current.title}
          </motion.h2>
        </AnimatePresence>
      </div>

      {/* 图片轮播 */}
      <div className="install-carousel">
        <AnimatePresence mode="wait">
          <motion.img
            key={imgIndex}
            src={current.src}
            alt={current.title}
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.4 }}
            draggable={false}
          />
        </AnimatePresence>
      </div>

      {/* 底部进度条（贴窗口最底部） */}
      <div className="install-progress">
        <div className="progress-bar">
          <div
            className="progress-bar-fill"
            style={{ width: `${progress}%` }}
          />
        </div>
        <div className="install-progress-text">{progress}%</div>
      </div>
    </motion.div>
  );
}