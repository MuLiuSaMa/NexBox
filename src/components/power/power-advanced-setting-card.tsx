import { Box, Flex, Text, HStack, VStack, Badge, Button, Menu, MenuButton, MenuList, MenuItem, Slider, SliderTrack, SliderFilledTrack, SliderThumb, Input, Portal } from "@chakra-ui/react";

import { memo, useState } from "react";
import { useTranslation } from "react-i18next";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { LiquidGlassButton } from "@/components/special/liquid-glass-button";
import { useThemeColor } from "@/contexts/theme-color-context";
import { hexToRgba } from "@/lib/color-utils";
import type { PowerAdvancedSettingDef } from "@/config/power-advanced-settings";
import { CheckCircle2, ChevronDown, MinusCircle, Sparkles } from "lucide-react";

import { useColorValue } from "@/hooks/use-color-value";
export interface PowerAdvancedSettingInfo {
  id: string;
  acValue: number | null;
  dcValue: number | null;
  hidden: boolean;
  supported: boolean;
  state: "recommended" | "default" | "custom";
  recommendedAc: number;
  recommendedDc: number;
  defaultAc: number;
  defaultDc: number;
}

interface SettingCardProps {
  def: PowerAdvancedSettingDef;
  info: PowerAdvancedSettingInfo;
  locked: boolean;
  hasBattery: boolean;
  ac: number;
  dc: number | null;
  applying: boolean;
  onChange: (id: string, ac: number, dc: number | null) => void;
  onApply: (id: string, ac: number, dc: number | null) => void;
  onApplyPreset: (id: string, ac: number, dc: number | null, kind: "recommended" | "default") => void;
}

/** 状态徽章：推荐=绿 / 默认=灰 / 自定义=橙 */
function StateBadge({ state }: { state: "recommended" | "default" | "custom" }) {
  const { t } = useTranslation();
  const palette = {
    recommended: { color: "#48BB78", bg: "rgba(72,187,120,0.12)", icon: <CheckCircle2 size={10} /> },
    default: { color: "#A0AEC0", bg: "rgba(160,174,192,0.12)", icon: <MinusCircle size={10} /> },
    custom: { color: "#ED8936", bg: "rgba(237,137,54,0.12)", icon: <Sparkles size={10} /> },
  } as const;
  const p = palette[state];
  return (
    <Badge borderRadius="full" data-corner="round" px={2.5} py={0.5} fontSize="xs" fontWeight="bold" color={p.color} bg={p.bg} border="1px solid" borderColor={hexToRgba(p.color, 0.25)}>
      <HStack spacing={1}>
        {p.icon}
        <Text>
          {state === "recommended"
            ? t("powerAdvanced.stateRecommended")
            : state === "default"
              ? t("powerAdvanced.stateDefault")
              : t("powerAdvanced.stateCustom")}
        </Text>
      </HStack>
    </Badge>
  );
}

/** 百分比控件：滑块 + 数字输入。拖动过程使用本地状态，松开时才提交，避免全局重渲染导致的卡顿。 */
function PercentControl({
  value,
  disabled,
  onChange,
}: {
  value: number;
  disabled: boolean;
  onChange: (v: number) => void;
}) {
  const cm = useColorValue();
  const { t } = useTranslation();
  const trackBg = cm("gray.200", "rgba(255,255,255,0.1)");
  const { getActiveColor, getBorderColor } = useThemeColor();
  const primaryColor = getActiveColor();
  const contrastText = cm("#1A202C", "#ffffff");
  const inputBg = cm("#ffffff", "rgba(255,255,255,0.06)");
  // 拖动中的临时值（仅本地更新，不触发外层重渲染）
  const [local, setLocal] = useState<number>(value);
  const [dragging, setDragging] = useState(false);
  const shown = dragging ? local : value;

  return (
    <HStack spacing={2.5} w="full">
      <Slider
        flex={1}
        min={0}
        max={100}
        step={1}
        value={shown}
        onChange={(v) => {
          setLocal(v);
          setDragging(true);
        }}
        onChangeEnd={(v) => {
          setLocal(v);
          setDragging(false);
          onChange(v);
        }}
        isDisabled={disabled}
        focusThumbOnChange={false}
      >
        <SliderTrack bg={trackBg}>
          <SliderFilledTrack bg={primaryColor} />
        </SliderTrack>
        <SliderThumb boxSize={4} bg={primaryColor} _focus={{ boxShadow: `0 0 0 3px ${hexToRgba(primaryColor, 0.4)}` }} />
      </Slider>
      <HStack spacing={1} flexShrink={0}>
        <Input
          type="text"
          inputMode="numeric"
          size="sm"
          w={16}
          borderRadius="md"
          textAlign="center"
          value={shown}
          onChange={(e) => {
            const digits = e.target.value.replace(/[^\d]/g, "");
            const v = Number(digits);
            const next = digits === "" || !Number.isFinite(v) ? 0 : Math.max(0, Math.min(100, v));
            setLocal(next);
            onChange(next);
          }}
          isDisabled={disabled}
          color={contrastText}
          bg={inputBg}
          borderColor={getBorderColor()}
          _hover={{ borderColor: getBorderColor() }}
          _focus={{ borderColor: primaryColor, boxShadow: `0 0 0 1px ${primaryColor}` }}
          px={2}
        />
        <Text fontSize="xs" color={cm("gray.500", "rgba(200,200,200,0.6)")} flexShrink={0}>
          {t("powerAdvanced.percentUnit")}
        </Text>
      </HStack>
    </HStack>
  );
}

/** 下拉控件（Chakra Menu 实现，无原生 option） */
function SelectControl({
  value,
  options,
  disabled,
  onChange,
}: {
  value: number;
  options: { value: number; labelKey: string }[];
  disabled: boolean;
  onChange: (v: number) => void;
}) {
  const cm = useColorValue();
  const { t } = useTranslation();
  const { getActiveColor, getBorderColor } = useThemeColor();
  const primaryColor = getActiveColor();
  const textColor = cm("#1A202C", "#ffffff");
  const current = options.find((o) => o.value === value);
  const menuBg = cm("#ffffff", "#1a1a1a");
  const menuBorder = cm("gray.200", "rgba(255,255,255,0.12)");
  const hoverBg = cm("gray.100", "rgba(255,255,255,0.08)");
  const buttonBg = cm("#ffffff", "rgba(255,255,255,0.06)");
  return (
    <Menu isLazy matchWidth>
      <MenuButton
        as={Button}
        size="sm"
        w="full"
        isDisabled={disabled}
        variant="outline"
        bg={buttonBg}
        borderColor={getBorderColor()}
        _hover={{ borderColor: primaryColor, bg: buttonBg }}
        _active={{ borderColor: primaryColor, bg: buttonBg }}
        textAlign="left"
        fontWeight="normal"
        color={textColor}
        rightIcon={<ChevronDown size={14} color={primaryColor} />}
        sx={{ borderRadius: "md" }}
      >
        {current ? t(current.labelKey) : `-`}
      </MenuButton>
      <Portal>
        <MenuList
          bg={menuBg}
          borderColor={menuBorder}
          zIndex={1600}
          boxShadow="0 8px 24px rgba(0,0,0,0.18)"
          sx={{
            "&::-webkit-scrollbar": { width: "4px" },
            "&::-webkit-scrollbar-track": { background: "transparent" },
            "&::-webkit-scrollbar-thumb": {
              background: `${primaryColor}66`,
              borderRadius: "2px",
            },
          }}
        >
          {options.map((o) => (
            <MenuItem
              key={o.value}
              onClick={() => onChange(o.value)}
              bg="transparent"
              color={textColor}
              _hover={{ bg: hoverBg, color: primaryColor }}
              _focus={{ bg: hoverBg, color: primaryColor }}
              icon={
                o.value === value ? (
                  <CheckCircle2 size={13} color={primaryColor} />
                ) : (
                  <Box w={3} />
                )
              }
            >
              <Text fontSize="sm">{t(o.labelKey)}</Text>
            </MenuItem>
          ))}
        </MenuList>
      </Portal>
    </Menu>
  );
}

const PowerAdvancedSettingCard = memo(function PowerAdvancedSettingCard({
  def,
  info,
  locked,
  hasBattery,
  ac,
  dc,
  applying,
  onChange,
  onApply,
  onApplyPreset,
}: SettingCardProps) {
  const cm = useColorValue();
  const { t } = useTranslation();
  const { getActiveColor } = useThemeColor();
  const primaryColor = getActiveColor();

  const headingColor = cm("gray.800", "#ffffff");
  const descColor = cm("gray.500", "rgba(200,200,200,0.85)");
  const labelColor = cm("gray.500", "rgba(200,200,200,0.6)");
  const schemeLabel = cm("gray.400", "rgba(200,200,200,0.5)");
  const disabled = locked || !info.supported || applying;
  const Icon = def.icon;

  // 是否有未应用的修改
  const dirty = info.acValue !== null && ac !== info.acValue;

  return (
    <LiquidGlassCard w="full" cursor="default" position="relative" overflow="visible" opacity={info.supported ? 1 : 0.6}>
      <VStack align="stretch" spacing={3} p={4}>
        {/* 头部：图标 + 标题 + 状态徽章 */}
        <Flex justify="space-between" align="center" gap={2}>
          <HStack spacing={3} flex={1} minW={0}>
            <Box
              w={9}
              h={9}
              borderRadius="lg"
              bg={hexToRgba(def.color, 0.15)}
              display="flex"
              alignItems="center"
              justifyContent="center"
              color={def.color}
              flexShrink={0}
            >
              <Icon size={18} />
            </Box>
            <VStack align="start" spacing={0} minW={0}>
              <Text fontSize="sm" fontWeight="bold" color={headingColor} noOfLines={1}>
                {t(def.titleKey)}
              </Text>
              {info.hidden && (
                <Text fontSize="xs" color={labelColor}>
                  {t("powerAdvanced.hiddenHint")}
                </Text>
              )}
            </VStack>
          </HStack>
          <StateBadge state={info.state} />
        </Flex>

        {/* 介绍 */}
        <Text fontSize="xs" color={descColor} lineHeight="tall">
          {t(def.descKey)}
        </Text>

        {!info.supported ? (
          <Text fontSize="xs" color={cm("orange.600", "orange.300")}>
            {t("powerAdvanced.notSupported")}
          </Text>
        ) : (
          <>
            {/* AC / DC 控件 */}
            <VStack align="stretch" spacing={2.5}>
              <Box>
                <HStack justify="space-between" mb={1}>
                  <Text fontSize="xs" fontWeight="bold" color={primaryColor}>
                    {t("powerAdvanced.acLabel")}
                  </Text>
                  {def.type === "select" ? (
                    <Text fontSize="xs" color={schemeLabel}>
                      {ac}
                    </Text>
                  ) : (
                    <Text fontSize="xs" color={schemeLabel}>
                      {ac}
                      {t("powerAdvanced.percentUnit")}
                    </Text>
                  )}
                </HStack>
                {def.type === "percent" ? (
                  <PercentControl value={ac} disabled={disabled} onChange={(v) => onChange(def.id, v, dc)} />
                ) : (
                  <SelectControl
                    value={ac}
                    options={def.options || []}
                    disabled={disabled}
                    onChange={(v) => onChange(def.id, v, dc)}
                  />
                )}
              </Box>

              {hasBattery && (
                <Box>
                  <HStack justify="space-between" mb={1}>
                    <Text fontSize="xs" fontWeight="bold" color={primaryColor}>
                      {t("powerAdvanced.dcLabel")}
                    </Text>
                    {def.type === "select" ? (
                      <Text fontSize="xs" color={schemeLabel}>
                        {dc ?? "-"}
                      </Text>
                    ) : (
                      <Text fontSize="xs" color={schemeLabel}>
                        {dc ?? "-"}
                        {t("powerAdvanced.percentUnit")}
                      </Text>
                    )}
                  </HStack>
                  {def.type === "percent" ? (
                    <PercentControl
                      value={dc ?? ac}
                      disabled={disabled || dc === null}
                      onChange={(v) => onChange(def.id, ac, v)}
                    />
                  ) : (
                    <SelectControl
                      value={dc ?? ac}
                      options={def.options || []}
                      disabled={disabled || dc === null}
                      onChange={(v) => onChange(def.id, ac, v)}
                    />
                  )}
                </Box>
              )}
            </VStack>

            {/* 操作区 */}
            <HStack spacing={2} pt={1} wrap="wrap">
              <LiquidGlassButton
                size="xs"
                leftIcon={<Sparkles size={12} />}
                onClick={() => onApplyPreset(def.id, info.recommendedAc, info.recommendedDc, "recommended")}
                isLoading={applying}
                isDisabled={disabled || info.state === "recommended"}
                colorScheme="green"
              >
                {t("powerAdvanced.setRecommended")}
              </LiquidGlassButton>
              <LiquidGlassButton
                size="xs"
                variant="outline"
                onClick={() => onApplyPreset(def.id, info.defaultAc, info.defaultDc, "default")}
                isLoading={applying}
                isDisabled={disabled || info.state === "default"}
                colorScheme="gray"
              >
                {t("powerAdvanced.setDefault")}
              </LiquidGlassButton>
              {dirty && !locked && (
                <LiquidGlassButton
                  size="xs"
                  onClick={() => onApply(def.id, ac, dc)}
                  isLoading={applying}
                  colorScheme="orange"
                >
                  {t("powerAdvanced.applyChanges")}
                </LiquidGlassButton>
              )}
            </HStack>
          </>
        )}
      </VStack>
    </LiquidGlassCard>
  );
});

export default PowerAdvancedSettingCard;
