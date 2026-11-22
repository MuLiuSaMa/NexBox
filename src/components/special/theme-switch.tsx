"use client";

import { Switch, useColorModeValue } from "@chakra-ui/react";
import { useThemeColor } from "@/contexts/theme-color-context";

interface ThemeSwitchProps {
  size?: "sm" | "md" | "lg";
  isChecked?: boolean;
  onChange?: (event: React.ChangeEvent<HTMLInputElement>) => void;
  isDisabled?: boolean;
  [key: string]: unknown;
}

export function ThemeSwitch({ size = "md", isChecked, onChange, isDisabled, ...props }: ThemeSwitchProps) {
  const { getActiveColor } = useThemeColor();
  
  const defaultTrackColor = useColorModeValue("gray.200", "gray.600");
  const primaryColor = getActiveColor();

  return (
    <Switch
      size={size}
      isChecked={isChecked}
      onChange={onChange}
      isDisabled={isDisabled}
      sx={{
        "& > span": {
          bg: isChecked ? primaryColor : defaultTrackColor,
        },
        "& > span > span": {
          bg: "white",
        },
        "&:hover > span": {
          bg: isChecked ? primaryColor : defaultTrackColor,
        },
      }}
      {...props}
    />
  );
}
