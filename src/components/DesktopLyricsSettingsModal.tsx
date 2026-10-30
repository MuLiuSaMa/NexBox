/**
 * 桌面歌词设置弹窗
 *
 * 可调节：
 * - 字体大小 (24~60px)
 * - 高亮颜色 (已唱)
 * - 底色 (未唱)
 * - 显示行数 (单行/双行)
 * - 实时预览
 */

import { useState, useEffect, useMemo, memo } from "react";
import {
  Modal,
  ModalOverlay,
  ModalContent,
  ModalHeader,
  ModalCloseButton,
  ModalBody,
  ModalFooter,
  Button,
  Text,
  HStack,
  VStack,
  Box,
  Tooltip,
  useColorModeValue,
  ButtonGroup,
  Switch,
  useToast,
  Divider,
} from "@chakra-ui/react";
import { CustomColorPicker } from "@/components/special/custom-color-picker";
import { CustomSelect } from "@/components/special/custom-select";
import { useMusicStore } from "@/stores/music-store";
import { useAppStartup } from "@/contexts/app-startup-context";
import { useFont } from "@/contexts/font-context";
import { useThemeColor } from "@/contexts/theme-color-context";
import { hexToRgba } from "@/lib/color-utils";
import { HotkeyRecorder } from "@/components/hotkey-recorder";
import { Eraser, LocateFixed, Upload } from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import { centerLyricsWindow } from "@/lib/desktop-lyrics-window";

interface DesktopLyricsSettingsModalProps {
  isOpen: boolean;
  onClose: () => void;
}

// 复用桌面歌词的 range slider 样式
const rangeSliderSx = {
  "&": {
    appearance: "none",
    WebkitAppearance: "none",
    height: "6px",
    borderRadius: "3px",
    outline: "none",
    cursor: "pointer",
  },
  "&::-webkit-slider-runnable-track": {
    height: "6px",
    borderRadius: "3px",
  },
  "&::-webkit-slider-thumb": {
    WebkitAppearance: "none",
    width: "14px",
    height: "14px",
    borderRadius: "50%",
    marginTop: "-4px",
    border: "none",
  },
  "&::-moz-range-track": {
    height: "6px",
    borderRadius: "3px",
  },
  "&::-moz-range-thumb": {
    width: "14px",
    height: "14px",
    borderRadius: "50%",
    border: "none",
  },
};

function DesktopLyricsSettingsModalInner({
  isOpen,
  onClose,
}: DesktopLyricsSettingsModalProps) {
  const desktopLyricsFontSize = useMusicStore((s) => s.desktopLyricsFontSize);
  const desktopLyricsFontFamily = useMusicStore((s) => s.desktopLyricsFontFamily);
  const desktopLyricsHighlightColor = useMusicStore((s) => s.desktopLyricsHighlightColor);
  const desktopLyricsBaseColor = useMusicStore((s) => s.desktopLyricsBaseColor);
  const desktopLyricsLineCount = useMusicStore((s) => s.desktopLyricsLineCount);
  const desktopLyricsShowTranslation = useMusicStore((s) => s.desktopLyricsShowTranslation);
  const desktopLyricsHideUnlockBtn = useMusicStore((s) => s.desktopLyricsHideUnlockBtn);
  const { lyricsBtnHotkey, saveLyricsBtnHotkey } = useAppStartup();
  const { fontOptions, importCustomFont, importing } = useFont();
  const toast = useToast();
  // 系统中文字体列表（打开弹窗时拉取）
  const [systemFonts, setSystemFonts] = useState<
    { name: string; supports_chinese: boolean }[]
  >([]);

  useEffect(() => {
    if (!isOpen) return;
    let cancelled = false;
    (async () => {
      try {
        const res = await invoke<{ name: string; supports_chinese: boolean }[]>(
          "get_system_fonts"
        );
        if (!cancelled) setSystemFonts(Array.isArray(res) ? res : []);
      } catch (err) {
        console.error("[DesktopLyrics] list system fonts failed:", err);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [isOpen]);

  // 组装字体下拉选项：跟随全局（留空） + 全局字体(内置/自定义) + 系统中文字体，按值去重
  const fontSelectOptions = useMemo(() => {
    const seen = new Set<string>([""]);
    const opts: { value: string; label: string; badge?: string }[] = [
      { value: "", label: "跟随全局字体（默认）" },
    ];
    for (const f of fontOptions) {
      if (seen.has(f.value)) continue;
      seen.add(f.value);
      opts.push({ value: f.value, label: f.label, badge: f.isCustom ? "自定义" : "全局" });
    }
    for (const f of systemFonts) {
      if (!f.supports_chinese) continue;
      const v = f.name.trim();
      if (!v || seen.has(v)) continue;
      seen.add(v);
      opts.push({ value: v, label: v, badge: "系统" });
    }
    return opts;
  }, [fontOptions, systemFonts]);

  const labelColor = useColorModeValue("gray.700", "#ffffff");
  const subLabelColor = useColorModeValue("gray.500", "#ffffff");
  const modalBg = useColorModeValue("white", "#111111");
  const modalBorderColor = useColorModeValue("gray.200", "#333333");
  const sliderTrackBg = useColorModeValue("rgba(0,0,0,0.1)", "rgba(255,255,255,0.9)");

  const { getActiveColor, getHoverColor } = useThemeColor();
  const themeColor = getActiveColor();
  const themeHover = getHoverColor();

  // 滚动条适配主题色
  const modalBodySx = {
    "&::-webkit-scrollbar": {
      width: "6px",
    },
    "&::-webkit-scrollbar-track": {
      background: "transparent",
    },
    "&::-webkit-scrollbar-thumb": {
      background: getActiveColor(),
      borderRadius: "3px",
    },
    "&::-webkit-scrollbar-thumb:hover": {
      background: getHoverColor(),
    },
  };

  return (
    <Modal isOpen={isOpen} onClose={onClose} isCentered scrollBehavior="inside" size="4xl">
      <ModalOverlay />
      <ModalContent
        bg={modalBg}
        borderColor={modalBorderColor}
        borderRadius="3xl"
        maxWidth="860px"
        w="100%"
        maxHeight="70vh"
        display="flex"
        flexDirection="column"
        sx={modalBodySx}
      >
        <ModalHeader color={labelColor} fontSize="lg" fontWeight="bold">
          桌面歌词设置
        </ModalHeader>
        <ModalCloseButton />

        <ModalBody overflowY="auto" flex="1" minH={0} pr={4} sx={modalBodySx}>
          <HStack align="flex-start" spacing={6}>
            <VStack spacing={3} align="stretch" flex={1} minW={0}>
            {/* 字体选择 */}
            <Box>
              <Text color={labelColor} fontSize="sm" fontWeight="medium" mb={1}>
                字体
              </Text>
              <Text color={subLabelColor} fontSize="xs" mb={2}>
                选择歌词显示字体，跟随全局字体时留空
              </Text>
              <HStack spacing={2} align="flex-start">
                <Box flex={1} minW={0}>
                  <CustomSelect
                    value={desktopLyricsFontFamily}
                    onChange={(val) =>
                      useMusicStore.getState().setDesktopLyricsFontFamily(val)
                    }
                    options={fontSelectOptions}
                    placeholder="跟随全局字体"
                    width="100%"
                  />
                </Box>
                <Button
                  size="sm"
                  variant="outline"
                  leftIcon={<Upload size={14} />}
                  isLoading={importing}
                  flexShrink={0}
                  onClick={() =>
                    document
                      .getElementById("desktop-lyrics-font-upload")
                      ?.click()
                  }
                  borderColor={themeColor}
                  color={themeColor}
                  _hover={{
                    bg: hexToRgba(themeColor, 0.12),
                  }}
                >
                  导入字体
                </Button>
              </HStack>
            </Box>

            {/* 字体大小 */}
            <Box>
              <HStack justify="space-between" mb={2}>
                <Text color={labelColor} fontSize="sm" fontWeight="medium">
                  字体大小
                </Text>
                <Text color={subLabelColor} fontSize="xs">
                  {desktopLyricsFontSize}px
                </Text>
              </HStack>
              <HStack spacing={2} align="center">
                <Text fontSize="xs" color={subLabelColor}>A</Text>
                <Box
                  as="input"
                  type="range"
                  min={24}
                  max={60}
                  step={2}
                  value={desktopLyricsFontSize}
                  onChange={(e) =>
                    useMusicStore.getState().setDesktopLyricsFontSize(
                      parseInt((e.target as HTMLInputElement).value)
                    )
                  }
                  w="100%"
                  sx={{
                    ...rangeSliderSx,
                    background: `linear-gradient(to right, ${themeColor} 0%, ${themeColor} ${((desktopLyricsFontSize - 24) / 36) * 100}%, ${sliderTrackBg} ${((desktopLyricsFontSize - 24) / 36) * 100}%, ${sliderTrackBg} 100%)`,
                    "&::-webkit-slider-thumb": {
                      ...rangeSliderSx["&::-webkit-slider-thumb"],
                      background: themeColor,
                    },
                    "&::-moz-range-thumb": {
                      ...rangeSliderSx["&::-moz-range-thumb"],
                      background: themeColor,
                    },
                  }}
                />
                <Text fontSize="md" color={subLabelColor} fontWeight="bold">A</Text>
              </HStack>
            </Box>

            {/* 颜色选择 */}
            <HStack spacing={6} align="start">
              <Box flex={1}>
                <Text color={labelColor} fontSize="sm" fontWeight="medium" mb={2}>
                  高亮颜色（已唱）
                </Text>
                <CustomColorPicker
                  color={desktopLyricsHighlightColor}
                  onChange={(c) => useMusicStore.getState().setDesktopLyricsHighlightColor(c)}
                />
              </Box>
              <Box flex={1}>
                <Text color={labelColor} fontSize="sm" fontWeight="medium" mb={2}>
                  底色（未唱）
                </Text>
                <CustomColorPicker
                  color={desktopLyricsBaseColor}
                  onChange={(c) => useMusicStore.getState().setDesktopLyricsBaseColor(c)}
                />
              </Box>
            </HStack>

            {/* 显示行数 */}
            <Box>
              <Text color={labelColor} fontSize="sm" fontWeight="medium" mb={2}>
                显示行数
              </Text>
              <ButtonGroup size="sm" variant="outline" isAttached>
                <Button
                  onClick={() => useMusicStore.getState().setDesktopLyricsLineCount(1)}
                  bg={desktopLyricsLineCount === 1 ? hexToRgba(themeColor, 0.15) : "transparent"}
                  borderColor={desktopLyricsLineCount === 1 ? themeColor : modalBorderColor}
                  color={desktopLyricsLineCount === 1 ? themeColor : subLabelColor}
                  fontWeight={desktopLyricsLineCount === 1 ? "bold" : "normal"}
                >
                  单行（仅当前句）
                </Button>
                <Button
                  onClick={() => useMusicStore.getState().setDesktopLyricsLineCount(2)}
                  bg={desktopLyricsLineCount === 2 ? hexToRgba(themeColor, 0.15) : "transparent"}
                  borderColor={desktopLyricsLineCount === 2 ? themeColor : modalBorderColor}
                  color={desktopLyricsLineCount === 2 ? themeColor : subLabelColor}
                  fontWeight={desktopLyricsLineCount === 2 ? "bold" : "normal"}
                >
                  双行（当前+下一句）
                </Button>
              </ButtonGroup>
            </Box>
            </VStack>

            <Divider orientation="vertical" />

            <VStack spacing={3} align="stretch" flex={1} minW={0}>
            {/* 显示翻译 */}
            <Box>
              <HStack justify="space-between" align="center">
                <Box>
                  <Text color={labelColor} fontSize="sm" fontWeight="medium">
                    显示翻译
                  </Text>
                  <Text color={subLabelColor} fontSize="xs">
                    在歌词下方显示翻译内容
                  </Text>
                </Box>
                <Switch
                  size="lg"
                  isChecked={desktopLyricsShowTranslation}
                  onChange={(e) =>
                    useMusicStore.getState().setDesktopLyricsShowTranslation(
                      (e.target as HTMLInputElement).checked
                    )
                  }
                  sx={{
                    "& .chakra-switch__track": {
                      bg: desktopLyricsShowTranslation ? themeColor : undefined,
                    },
                  }}
                />
              </HStack>
            </Box>

            {/* 隐藏解锁按钮 */}
            <Box>
              <HStack justify="space-between" align="center">
                <Box>
                  <Text color={labelColor} fontSize="sm" fontWeight="medium">
                    隐藏解锁按钮
                  </Text>
                  <Text color={subLabelColor} fontSize="xs">
                    锁定歌词后不显示解锁按钮
                  </Text>
                </Box>
                <Switch
                  size="lg"
                  isChecked={desktopLyricsHideUnlockBtn}
                  onChange={(e) =>
                    useMusicStore.getState().setDesktopLyricsHideUnlockBtn(
                      (e.target as HTMLInputElement).checked
                    )
                  }
                  sx={{
                    "& .chakra-switch__track": {
                      bg: desktopLyricsHideUnlockBtn ? themeColor : undefined,
                    },
                  }}
                />
              </HStack>
            </Box>

            {/* 显示/隐藏解锁按钮热键 */}
            <Box>
              <Text color={labelColor} fontSize="sm" fontWeight="medium" mb={1}>
                解锁按钮热键
              </Text>
              <Text color={subLabelColor} fontSize="xs" mb={2}>
                按下可切换解锁按钮的显示/隐藏，可留空
              </Text>
              <HStack spacing={2}>
                <HotkeyRecorder
                  value={lyricsBtnHotkey}
                  onChange={async (val) => {
                    await saveLyricsBtnHotkey(val);
                  }}
                />
                {lyricsBtnHotkey && (
                  <Button
                    size="sm"
                    variant="ghost"
                    leftIcon={<Eraser size={14} />}
                    onClick={() => saveLyricsBtnHotkey("")}
                    color={subLabelColor}
                    _hover={{ color: themeColor }}
                  >
                    清除
                  </Button>
                )}
              </HStack>
            </Box>

            {/* 复位到屏幕中央 */}
            <Box>
              <Text color={labelColor} fontSize="sm" fontWeight="medium" mb={2}>
                窗口位置
              </Text>
              <Button
                size="sm"
                variant="outline"
                w="100%"
                leftIcon={<LocateFixed size={14} />}
                onClick={() => centerLyricsWindow()}
                borderColor={themeColor}
                color={themeColor}
                _hover={{
                  bg: hexToRgba(themeColor, 0.12),
                }}
              >
                复位到屏幕中央
              </Button>
            </Box>
          </VStack>
          </HStack>
        </ModalBody>

        <ModalFooter>
          <Button
            variant="ghost"
            onClick={onClose}
            color={themeColor}
            _hover={{ bg: hexToRgba(themeColor, 0.1) }}
          >
            完成
          </Button>
        </ModalFooter>
        <input
          id="desktop-lyrics-font-upload"
          type="file"
          accept=".ttf,.otf,.woff,.woff2"
          style={{ display: "none" }}
          onChange={async (e) => {
            const file = e.target.files?.[0];
            if (file) {
              const family = file.name
                .replace(/\.(ttf|otf|woff|woff2)$/i, "")
                .replace(/\s+/g, "-");
              try {
                await importCustomFont(file);
                await useMusicStore.getState().setDesktopLyricsFontFamily(family);
              } catch (err) {
                if (err instanceof Error && err.message === "DUPLICATE_FONT") {
                  toast({
                    title: "已存在同名自定义字体",
                    status: "warning",
                    duration: 2000,
                    isClosable: true,
                  });
                } else {
                  toast({
                    title: "字体导入失败",
                    status: "error",
                    duration: 2000,
                    isClosable: true,
                  });
                }
              }
            }
            e.target.value = "";
          }}
        />
      </ModalContent>
    </Modal>
  );
}

export const DesktopLyricsSettingsModal = memo(DesktopLyricsSettingsModalInner);
