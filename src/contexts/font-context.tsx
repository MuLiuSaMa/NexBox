"use client";

import { createContext, useContext, useState, ReactNode, useEffect, useCallback, useMemo, useRef } from "react";
import { store } from "@/lib/store";
import { registerFontFace } from "@/lib/fonts";

/** 可选字体项 */
export interface FontOption {
  /** 存储值（CSS font-family 名称） */
  value: string;
  /** 显示名称 */
  label: string;
  /** 是否为用户导入的自定义字体 */
  isCustom?: boolean;
}

/** 自定义字体记录（持久化） */
interface CustomFontRecord {
  /** CSS font-family 名称 */
  value: string;
  /** 显示名称 */
  label: string;
  /** base64 编码的字体数据 */
  data: string;
  /** mime 类型 */
  format: string;
}

/** 内置可选字体列表 */
export const BUILTIN_FONT_OPTIONS: FontOption[] = [
  { value: "MiSans", label: "MiSans" },
  { value: "Sinter-Regular", label: "Sinter" },
  { value: "AiDianFengYaHei", label: "AiDianFengYaHei" },
  { value: "Cubic", label: "Cubic" },
  { value: "NotoSerifSC-900", label: "思源宋体 900" },
];

/** 默认字体 */
export const DEFAULT_FONT = "MiSans";

interface FontContextType {
  /** 当前字体 */
  font: string;
  /** 设置字体 */
  setFont: (font: string) => void;
  /** 恢复默认 */
  resetToDefault: () => void;
  /** 所有可选字体（内置 + 自定义） */
  fontOptions: FontOption[];
  /** 导入自定义字体 */
  importCustomFont: (file: File) => Promise<void>;
  /** 删除自定义字体 */
  removeCustomFont: (value: string) => void;
  /** 正在导入 */
  importing: boolean;
}

const FontContext = createContext<FontContextType>({
  font: DEFAULT_FONT,
  setFont: () => {},
  resetToDefault: () => {},
  fontOptions: BUILTIN_FONT_OPTIONS,
  importCustomFont: async () => {},
  removeCustomFont: () => {},
  importing: false,
});

export function useFont() {
  return useContext(FontContext);
}

/** 从文件名提取字体显示名 */
function getFontLabel(fileName: string): string {
  const base = fileName.replace(/\.(ttf|otf|woff|woff2)$/i, "");
  // 去掉常见的版本后缀
  return base.replace(/[-_](\d+|Medium|Regular|Bold|Light|Thin|Black|SemiBold|ExtraBold)$/i, "").replace(/[-_]/g, " ");
}

/** 从文件名推断 font-format */
function getFontFormat(fileName: string): string {
  const ext = fileName.split(".").pop()?.toLowerCase();
  switch (ext) {
    case "ttf": return "truetype";
    case "otf": return "opentype";
    case "woff": return "woff";
    case "woff2": return "woff2";
    default: return "truetype";
  }
}

export function FontProvider({ children }: { children: ReactNode }) {
  const [font, setFontState] = useState<string>(DEFAULT_FONT);
  const [customFonts, setCustomFonts] = useState<CustomFontRecord[]>([]);
  const [isLoaded, setIsLoaded] = useState(false);
  const [importing, setImporting] = useState(false);
  const fontRef = useRef(font);

  // 加载已保存的字体设置和自定义字体
  useEffect(() => {
    async function loadSettings() {
      try {
        // 加载当前选中字体
        const savedFont = await store.get<string>("app-font-family");
        if (savedFont) {
          setFontState(savedFont);
          fontRef.current = savedFont;
        }

        // 加载自定义字体列表
        const savedCustomFonts = await store.get<CustomFontRecord[]>("app-custom-fonts");
        if (savedCustomFonts && Array.isArray(savedCustomFonts) && savedCustomFonts.length > 0) {
          // 先注册所有自定义字体到 document.fonts
          for (const cf of savedCustomFonts) {
            await registerFontFace(cf.value, cf.data, cf.format);
          }
          setCustomFonts(savedCustomFonts);
        }

        setIsLoaded(true);
      } catch (error) {
        console.error("Failed to load font settings:", error);
        setIsLoaded(true);
      }
    }
    loadSettings();
  }, []);

  // 应用字体到 CSS 变量
  useEffect(() => {
    if (!isLoaded) return;
    document.documentElement.style.setProperty("--app-font-family", `"${font}"`);
    fontRef.current = font;
  }, [font, isLoaded]);

  // 持久化当前选中字体
  useEffect(() => {
    if (!isLoaded) return;
    async function saveFont() {
      try {
        await store.set("app-font-family", font);
        await store.save();
      } catch (error) {
        console.error("Failed to save font settings:", error);
      }
    }
    saveFont();
  }, [font, isLoaded]);

  // 持久化自定义字体列表
  useEffect(() => {
    if (!isLoaded) return;
    async function saveCustomFonts() {
      try {
        await store.set("app-custom-fonts", customFonts);
        await store.save();
      } catch (error) {
        console.error("Failed to save custom fonts:", error);
      }
    }
    saveCustomFonts();
  }, [customFonts, isLoaded]);

  const setFont = useCallback((newFont: string) => {
    setFontState(newFont);
  }, []);

  const resetToDefault = useCallback(() => {
    setFontState(DEFAULT_FONT);
  }, []);

  /** 导入自定义字体 */
  const importCustomFont = useCallback(async (file: File) => {
    setImporting(true);
    try {
      const fileName = file.name;
      const format = getFontFormat(fileName);
      const label = getFontLabel(fileName);
      // 用文件名（去扩展名）作为 CSS font-family 名，确保唯一性
      const value = fileName.replace(/\.(ttf|otf|woff|woff2)$/i, "").replace(/\s+/g, "-");

      // 检查是否已存在同名字体
      const exists = customFonts.some(cf => cf.value === value);
      if (exists) {
        throw new Error("DUPLICATE_FONT");
      }

      // 读取文件为 base64
      const arrayBuffer = await file.arrayBuffer();
      const bytes = new Uint8Array(arrayBuffer);
      let binary = "";
      for (let i = 0; i < bytes.length; i++) {
        binary += String.fromCharCode(bytes[i]);
      }
      const base64Data = btoa(binary);

      // 注册到 document.fonts
      await registerFontFace(value, base64Data, format);

      // 添加到自定义字体列表
      setCustomFonts(prev => [...prev, { value, label, data: base64Data, format }]);
    } finally {
      setImporting(false);
    }
  }, [customFonts]);

  /** 删除自定义字体 */
  const removeCustomFont = useCallback((value: string) => {
    // 从 document.fonts 中移除
    const fontFaces = document.fonts;
    fontFaces.forEach(ff => {
      if (ff.family === `"${value}"` || ff.family === value) {
        fontFaces.delete(ff);
      }
    });

    // 从列表中移除
    setCustomFonts(prev => prev.filter(cf => cf.value !== value));

    // 如果当前正在使用被删除的字体，恢复默认
    if (fontRef.current === value) {
      setFontState(DEFAULT_FONT);
    }
  }, []);

  /** 合并内置 + 自定义字体选项 */
  const fontOptions = useMemo<FontOption[]>(() => {
    return [
      ...BUILTIN_FONT_OPTIONS,
      ...customFonts.map(cf => ({ value: cf.value, label: cf.label, isCustom: true })),
    ];
  }, [customFonts]);

  const value = useMemo(() => ({
    font,
    setFont,
    resetToDefault,
    fontOptions,
    importCustomFont,
    removeCustomFont,
    importing,
  }), [font, setFont, resetToDefault, fontOptions, importCustomFont, removeCustomFont, importing]);

  return (
    <FontContext.Provider value={value}>
      {children}
    </FontContext.Provider>
  );
}
