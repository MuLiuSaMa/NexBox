"use client";

import { Box, Text, useColorModeValue } from "@chakra-ui/react";
import { useState, useCallback, useRef } from "react";
import { useThemeColor } from "@/contexts/theme-color-context";

export function keyToHotkeyFormat(key: string): string | null {
  if (key.startsWith("F") && key.length >= 2 && key.length <= 3) {
    const num = parseInt(key.slice(1));
    if (num >= 1 && num <= 24) return key;
  }
  if (key === "ArrowUp") return "Up";
  if (key === "ArrowDown") return "Down";
  if (key === "ArrowLeft") return "Left";
  if (key === "ArrowRight") return "Right";
  if (key === " ") return "Space";
  if (key === "Escape") return "Escape";
  if (key === "Tab") return "Tab";
  if (key === "Enter") return "Enter";
  if (key === "Backspace") return "Backspace";
  if (key === "Delete") return "Delete";
  if (key === "Home") return "Home";
  if (key === "End") return "End";
  if (key === "PageUp") return "PageUp";
  if (key === "PageDown") return "PageDown";
  if (key === "Insert") return "Insert";
  if (key === "Pause") return "Pause";
  if (key === "ScrollLock") return "ScrollLock";
  if (key === "CapsLock") return "CapsLock";
  if (key === "NumLock") return "NumLock";
  if (key === "PrintScreen") return "PrintScreen";
  if (/^[0-9]$/.test(key)) return key;
  if (/^[a-zA-Z]$/.test(key)) return key.toUpperCase();
  // 标点符号键：把物理键的基准/上档两种字符都映射为后端可识别的 token
  const symbol = SYMBOL_KEYS[key];
  if (symbol) return symbol;
  return null;
}

/// 标点符号 → 后端 global-hotkey 可识别的 KeyCode token（大小写不敏感）
/// 上档形式（如对 !@#$% 等）对应的仍是同一物理键，因此映射到基准键
const SYMBOL_KEYS: Record<string, string> = {
  "-": "Minus",
  "_": "Minus",
  "=": "Equal",
  "+": "Equal",
  "[": "[",
  "]": "]",
  "{": "[",
  "}": "]",
  "\\": "Backslash",
  "|": "Backslash",
  ";": "Semicolon",
  ":": "Semicolon",
  "'": "Quote",
  '"': "Quote",
  ",": "Comma",
  "<": "Comma",
  ".": "Period",
  ">": "Period",
  "/": "Slash",
  "?": "Slash",
  "`": "Backquote",
  "~": "Backquote",
  "!": "1",
  "@": "2",
  "#": "3",
  "$": "4",
  "%": "5",
  "^": "6",
  "&": "7",
  "*": "8",
  "(": "9",
  ")": "0",
};

/// 后端 KeyCode token → 界面显示用的友好符号
/// 与 SYMBOL_KEYS 互为逆映射（后者把界面按键映射为后端 token）
const TOKEN_DISPLAY: Record<string, string> = {
  Minus: "-",
  Equal: "=",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Comma: ",",
  Period: ".",
  Slash: "/",
  Backquote: "`",
  Space: "空格",
  Escape: "Esc",
  Backspace: "退格",
  Delete: "Del",
  PageUp: "PgUp",
  PageDown: "PgDn",
  ScrollLock: "ScrLk",
  PrintScreen: "PrtSc",
  CapsLock: "Caps",
  NumLock: "NumLk",
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
};

/// 把后端热键 token 格式化为用户可读的显示文本
/// - 组合键（Ctrl+Shift+A）逐段转换后用 + 连接
/// - 单键（Minus / Escape / F1）转换为符号或中文名
/// - 未收录的 token 原样返回
export function formatHotkeyDisplay(value: string): string {
  if (!value) return "";
  return value
    .split("+")
    .map((part) => TOKEN_DISPLAY[part] ?? part)
    .join("+");
}

function buildComboFromEvent(e: React.KeyboardEvent): string[] {
  const parts: string[] = [];
  if (e.ctrlKey) parts.push("Ctrl");
  if (e.shiftKey) parts.push("Shift");
  if (e.altKey) parts.push("Alt");
  if (e.metaKey) parts.push("Command");
  const nonModifier = e.key;
  if (!["Control", "Shift", "Alt", "Meta"].includes(nonModifier)) {
    const mapped = keyToHotkeyFormat(nonModifier);
    if (mapped) {
      parts.push(mapped);
    }
  }
  return parts;
}

const MODIFIER_LABELS = ["Ctrl", "Shift", "Alt", "Command"];

export function HotkeyRecorder({
  value,
  onChange,
}: {
  value: string;
  onChange: (val: string) => void;
}) {
  const [isRecording, setIsRecording] = useState(false);
  const [displayText, setDisplayText] = useState("");
  const pendingRef = useRef<string[]>([]);
  const textColor = useColorModeValue("gray.800", "#ffffff");
  const borderColor = useColorModeValue("gray.200", "#333333");
  const { getActiveColor, getHoverColor } = useThemeColor();
  const recordBg = getHoverColor();
  const recordBorder = getActiveColor();

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (!isRecording) return;
      e.preventDefault();
      e.stopPropagation();
      const parts = buildComboFromEvent(e);
      pendingRef.current = parts;
      setDisplayText(parts.join("+"));
    },
    [isRecording]
  );

  const handleKeyUp = useCallback(
    (e: React.KeyboardEvent) => {
      if (!isRecording) return;
      e.preventDefault();
      e.stopPropagation();

      if (e.key === "Escape") {
        setIsRecording(false);
        setDisplayText("");
        pendingRef.current = [];
        return;
      }

      const combo = pendingRef.current;
      if (combo.length > 0) {
        const lastPart = combo[combo.length - 1];
        const hasMainKey = !MODIFIER_LABELS.includes(lastPart);
        if (hasMainKey) {
          onChange(combo.join("+"));
          setIsRecording(false);
          setDisplayText("");
          pendingRef.current = [];
          return;
        }
      }

      const remainingParts = buildComboFromEvent(e);
      pendingRef.current = remainingParts;
      setDisplayText(remainingParts.join("+"));
    },
    [isRecording, onChange]
  );

  const startRecording = useCallback(() => {
    setIsRecording(true);
    setDisplayText("");
    pendingRef.current = [];
  }, []);

  const stopRecording = useCallback(() => {
    if (isRecording) {
      setIsRecording(false);
      setDisplayText("");
      pendingRef.current = [];
    }
  }, [isRecording]);

  return (
    <Box
      tabIndex={0}
      role="button"
      cursor="pointer"
      onKeyDown={handleKeyDown}
      onKeyUp={handleKeyUp}
      onClick={startRecording}
      onBlur={stopRecording}
      px={3}
      py={2}
      borderRadius="lg"
      border="2px solid"
      borderColor={isRecording ? recordBorder : borderColor}
      bg={isRecording ? recordBg : "transparent"}
      transition="all 0.2s"
      _hover={{ borderColor: recordBorder }}
      outline="none"
      minW="180px"
      textAlign="center"
      userSelect="none"
    >
      {isRecording ? (
        <Text color={recordBorder} fontSize="sm" fontWeight="medium">
          {displayText ? formatHotkeyDisplay(displayText) : "按下快捷键..."}
        </Text>
      ) : (
        <Text color={textColor} fontSize="sm" fontWeight="medium">
          {value ? formatHotkeyDisplay(value) : "无"}
        </Text>
      )}
    </Box>
  );
}
