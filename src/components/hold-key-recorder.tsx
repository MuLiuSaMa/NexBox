"use client";

import { Box, Text, useColorModeValue } from "@chakra-ui/react";
import { useState, useCallback, useRef, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { keyToHotkeyFormat } from "./hotkey-recorder";
import { useThemeColor } from "@/contexts/theme-color-context";

/// 鼠标按键映射：准星「按住打开」允许左键、右键、中键与侧键
function mouseButtonToken(button: number): string | null {
  switch (button) {
    case 0: return "MouseLeft";
    case 1: return "MouseMiddle";
    case 2: return "MouseRight";
    case 3: return "MouseX1";
    case 4: return "MouseX2";
    default: return null;
  }
}

const MODIFIER_LABELS = ["Ctrl", "Shift", "Alt", "Command"];

/// 鼠标键 token → i18n 展示名
const MOUSE_TOKEN_KEYS: Record<string, string> = {
  MouseLeft: "crosshair.mouseLeft",
  MouseRight: "crosshair.mouseRight",
  MouseMiddle: "crosshair.mouseMiddle",
  MouseX1: "crosshair.mouseX1",
  MouseX2: "crosshair.mouseX2",
};

export function HoldKeyRecorder({
  value,
  onChange,
}: {
  value: string;
  onChange: (val: string) => void;
}) {
  const { t } = useTranslation();
  const [isRecording, setIsRecording] = useState(false);
  const [displayText, setDisplayText] = useState("");
  const pendingRef = useRef<string[]>([]);
  const justCommittedRef = useRef(false);
  const textColor = useColorModeValue("gray.800", "#ffffff");
  const borderColor = useColorModeValue("gray.200", "#333333");
  const { getActiveColor, getHoverColor } = useThemeColor();
  const recordBg = getHoverColor();
  const recordBorder = getActiveColor();

  const commit = useCallback(
    (combo: string[]) => {
      if (combo.length > 0) {
        onChange(combo.join("+"));
      }
      setIsRecording(false);
      setDisplayText("");
      pendingRef.current = [];
      justCommittedRef.current = true;
    },
    [onChange]
  );

  const cancel = useCallback(() => {
    setIsRecording(false);
    setDisplayText("");
    pendingRef.current = [];
  }, []);

  useEffect(() => {
    if (!isRecording) return;

    const onMouseDown = (e: MouseEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const token = mouseButtonToken(e.button);
      if (!token) return;
      const parts: string[] = [];
      if (e.ctrlKey) parts.push("Ctrl");
      if (e.shiftKey) parts.push("Shift");
      if (e.altKey) parts.push("Alt");
      if (e.metaKey) parts.push("Command");
      parts.push(token);
      commit(parts);
    };

    const onKeyDown = (e: KeyboardEvent) => {
      e.preventDefault();
      e.stopPropagation();
      const parts: string[] = [];
      if (e.ctrlKey) parts.push("Ctrl");
      if (e.shiftKey) parts.push("Shift");
      if (e.altKey) parts.push("Alt");
      if (e.metaKey) parts.push("Command");
      const nonModifier = e.key;
      if (!["Control", "Shift", "Alt", "Meta"].includes(nonModifier)) {
        const mapped = keyToHotkeyFormat(nonModifier);
        if (mapped) parts.push(mapped);
      }
      if (parts.length > 0) {
        pendingRef.current = parts;
        setDisplayText(parts.join("+"));
      }
    };

    const onKeyUp = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        cancel();
        return;
      }
      const combo = pendingRef.current;
      if (combo.length > 0) {
        const lastPart = combo[combo.length - 1];
        const hasMainKey = !MODIFIER_LABELS.includes(lastPart);
        if (hasMainKey) {
          commit(combo);
        }
      }
    };

    window.addEventListener("mousedown", onMouseDown, true);
    window.addEventListener("keydown", onKeyDown, true);
    window.addEventListener("keyup", onKeyUp, true);
    return () => {
      window.removeEventListener("mousedown", onMouseDown, true);
      window.removeEventListener("keydown", onKeyDown, true);
      window.removeEventListener("keyup", onKeyUp, true);
    };
  }, [isRecording, commit, cancel]);

  const startRecording = useCallback(() => {
    // 若刚通过鼠标录制提交（该次点击的 click 事件），忽略避免立即重新进入录制
    if (justCommittedRef.current) {
      justCommittedRef.current = false;
      return;
    }
    setIsRecording(true);
    setDisplayText("");
    pendingRef.current = [];
  }, []);

  const displayValue = (v: string) => {
    if (!v) return t("crosshair.holdKeyNone") || "无";
    return v
      .split("+")
      .map((tok) => (MOUSE_TOKEN_KEYS[tok] ? t(MOUSE_TOKEN_KEYS[tok]) : tok))
      .join("+");
  };

  return (
    <Box
      role="button"
      cursor="pointer"
      onClick={startRecording}
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
          {displayText || t("crosshair.holdKeyRecording") || "按下按键..."}
        </Text>
      ) : (
        <Text color={textColor} fontSize="sm" fontWeight="medium">
          {displayValue(value)}
        </Text>
      )}
    </Box>
  );
}