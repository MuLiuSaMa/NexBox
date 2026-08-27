import {
  Box,
  Flex,
  Text,
  Heading,
  VStack,
  HStack,
  Input,
  Button,
  Tab,
  TabList,
  TabPanel,
  TabPanels,
  Tabs,
  useColorModeValue,
  Spinner,
  Table,
  Thead,
  Tbody,
  Tr,
  Th,
  Td,
  IconButton,
  Tooltip,
  Badge,
  Image,
} from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { motion } from "framer-motion";
import { useTransitionMode, getVariants, getTransitionConfig } from "@/components/ui/animated-page";
import { LiquidGlassButton } from "@/components/special/liquid-glass-button";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { useTranslation } from "react-i18next";
import { useState, useEffect, useCallback, useMemo } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Ban,
  RotateCcw,
  RefreshCw,
  ArrowLeft,
  HardDrive,
} from "lucide-react";
import { useBackground } from "@/contexts/background-context";
import { useThemeColor } from "@/contexts/theme-color-context";
import { useNavigate } from "react-router-dom";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";

interface ContextMenuItem {
  name: string;
  verb: string;
  command: string;
  hive: string;
  category: string;
  reg_path: string;
  icon: string;
  is_hidden: boolean;
}

interface ThisPcItem {
  name: string;
  clsid: string;
  hive: string;
  reg_path: string;
  is_hidden: boolean;
}

// 「此电脑」按 CLSID 去重合并后的行（跨 HKCU/HKLM 等多处注册只显示一条，
// 隐藏/恢复时一次性处理所有位置）
interface ThisPcRow {
  name: string;
  clsid: string;
  hives: string[];
  originals: ThisPcItem[]; // 该 CLSID 在注册表中的所有出现位置
  is_hidden: boolean; // 仅当所有位置都隐藏时为 true
}

interface DriveItem {
  letter: string;
  label: string;
  drive_type: string;
  is_hidden: boolean;
}

const driveTypeLabels: Record<string, string> = {
  fixed: "固定",
  removable: "可移动",
  cdrom: "光驱",
  remote: "网络",
  ram: "RAM",
  unknown: "未知",
};

const menuCategoryLabels: Record<string, string> = {
  file: "文件右键",
  folder: "文件夹",
  desktop: "桌面空白",
  drive: "磁盘",
  allFiles: "所有文件系统",
};

const menuCategoryOrder = ["all", "file", "folder", "desktop", "drive", "allFiles"] as const;

export default function ContextMenuManagerPage() {
  const { t } = useTranslation();
  const toast = useDynamicIsland("rocket");
  const { liquidGlassEnabled } = useBackground();
  const { config: themeConfig, getContrastTextColor } = useThemeColor();
  const tabTextColor = getContrastTextColor();
  const navigate = useNavigate();

  // 将系统报错翻译为面向用户的可读信息；权限不足时给出管理员提示
  const localErr = useCallback(
    (raw: string) => {
      const s = raw.toLowerCase();
      if (s.includes("os error 5") || s.includes("access is denied") || s.includes("拒绝访问")) {
        return t("contextMenu.errorDenied");
      }
      return raw;
    },
    [t]
  );
  // 隐藏/恢复成功后局部更新该项状态，保持其在列表中的位置（不对整表重新扫描，
  // 否则列表会因按可见/隐藏分组的扫描顺序而发生位置跳变）
  const markMenuHidden = useCallback((item: ContextMenuItem, hidden: boolean) => {
    setMenuItems((prev) =>
      prev.map((it) =>
        it.reg_path === item.reg_path && it.verb === item.verb ? { ...it, is_hidden: hidden } : it
      )
    );
  }, []);
  const markPcHidden = useCallback((name: string, hidden: boolean) => {
    setPcItems((prev) =>
      prev.map((it) => ((it.name || it.clsid) === name ? { ...it, is_hidden: hidden } : it))
    );
  }, []);

  const headingColor = useColorModeValue("gray.900", "#ffffff");
  const adaptiveTitle = useAdaptiveTextColor();
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const tableBg = liquidGlassEnabled
    ? "rgba(255,255,255,0.7)"
    : useColorModeValue("#ffffff", "#1a1a1a");
  const tableBorder = liquidGlassEnabled
    ? "rgba(255,255,255,0.3)"
    : useColorModeValue("gray.200", "#333333");
  const pathColor = useColorModeValue("gray.500", "#888888");
  const hoverBg = `${themeConfig.primaryColor}1F`;
  const deleteColor = useColorModeValue("red.500", "red.400");
  const restoreColor = themeConfig.primaryColor;
  const inputGlassBg = liquidGlassEnabled
    ? useColorModeValue("rgba(255,255,255,0.26)", "rgba(255,255,255,0.1)")
    : useColorModeValue("#ffffff", "#1a1a1a");
  const inputGlassProps = {
    bg: inputGlassBg,
    borderColor: `${themeConfig.primaryColor}59`,
    color: headingColor,
    _placeholder: { color: subTextColor },
    _hover: { borderColor: themeConfig.primaryColor },
    _focus: {
      borderColor: themeConfig.primaryColor,
      boxShadow: `0 0 0 2px ${themeConfig.primaryColor}33`,
    },
  };

  const [menuItems, setMenuItems] = useState<ContextMenuItem[]>([]);
  const [pcItems, setPcItems] = useState<ThisPcItem[]>([]);
  const [drives, setDrives] = useState<DriveItem[]>([]);
  const [isScanning, setIsScanning] = useState(false);
  const [operatingKeys, setOperatingKeys] = useState<Set<string>>(new Set());
  const [menuQuery, setMenuQuery] = useState("");
  const [pcQuery, setPcQuery] = useState("");
  const [menuCat, setMenuCat] = useState<string>("all");

  const scanMenu = useCallback(
    async (spinner = true) => {
      if (spinner) setIsScanning(true);
      try {
        const result = await invoke<ContextMenuItem[]>("scan_context_menu_items");
        setMenuItems(result);
      } catch (error) {
        console.error("Failed to scan context menu items:", error);
        toast({
          title: t("contextMenu.scanError"),
          description: localErr(String(error)),
          status: "error",
          duration: 3000,
          isClosable: true,
        });
      }
      if (spinner) setIsScanning(false);
    },
    [t, toast]
  );

  const scanPc = useCallback(
    async (spinner = true) => {
      if (spinner) setIsScanning(true);
      try {
        const result = await invoke<ThisPcItem[]>("scan_this_pc_items");
        setPcItems(result);
      } catch (error) {
        console.error("Failed to scan this-pc items:", error);
        toast({
          title: t("contextMenu.scanError"),
          description: localErr(String(error)),
          status: "error",
          duration: 3000,
          isClosable: true,
        });
      }
      if (spinner) setIsScanning(false);
    },
    [t, toast]
  );

  const loadDrives = useCallback(async () => {
    try {
      const result = await invoke<DriveItem[]>("scan_drives");
      setDrives(result);
    } catch (error) {
      console.error("Failed to scan drives:", error);
    }
  }, []);

  useEffect(() => {
    scanMenu();
    scanPc();
    loadDrives();
  }, [scanMenu, scanPc, loadDrives]);

  const handleHideMenu = async (item: ContextMenuItem) => {
    const key = `${item.reg_path}/${item.verb}`;
    setOperatingKeys((prev) => new Set(prev).add(key));
    try {
      await invoke("hide_context_menu_item", { item });
      toast({
        title: t("contextMenu.hideSuccess", { name: item.name }),
        status: "success",
        duration: 2000,
        isClosable: true,
      });
      markMenuHidden(item, true);
    } catch (error) {
      console.error("Failed to hide context menu item:", error);
      toast({
        title: t("contextMenu.hideError"),
        description: localErr(String(error)),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
    setOperatingKeys((prev) => {
      const next = new Set(prev);
      next.delete(key);
      return next;
    });
  };

  const handleRestoreMenu = async (item: ContextMenuItem) => {
    const key = `${item.reg_path}/${item.verb}`;
    setOperatingKeys((prev) => new Set(prev).add(key));
    try {
      await invoke("restore_context_menu_item", { item });
      toast({
        title: t("contextMenu.restoreSuccess", { name: item.name }),
        status: "success",
        duration: 2000,
        isClosable: true,
      });
      markMenuHidden(item, false);
    } catch (error) {
      console.error("Failed to restore context menu item:", error);
      toast({
        title: t("contextMenu.restoreError"),
        description: localErr(String(error)),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
    setOperatingKeys((prev) => {
      const next = new Set(prev);
      next.delete(key);
      return next;
    });
  };

  const handleHidePc = async (row: ThisPcRow) => {
    const key = `pc/${row.name}`;
    setOperatingKeys((prev) => new Set(prev).add(key));
    try {
      // 对该行对应名称的所有 CLSID/位置一次性隐藏
      for (const item of row.originals) {
        await invoke("hide_this_pc_item", { item });
      }
      toast({
        title: t("contextMenu.hideSuccess", { name: row.name }),
        status: "success",
        duration: 2000,
        isClosable: true,
      });
      markPcHidden(row.name, true);
    } catch (error) {
      console.error("Failed to hide this-pc item:", error);
      toast({
        title: t("contextMenu.hideError"),
        description: localErr(String(error)),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
    setOperatingKeys((prev) => {
      const next = new Set(prev);
      next.delete(key);
      return next;
    });
  };

  const handleRestorePc = async (row: ThisPcRow) => {
    const key = `pc/${row.name}`;
    setOperatingKeys((prev) => new Set(prev).add(key));
    try {
      for (const item of row.originals) {
        await invoke("restore_this_pc_item", { item });
      }
      toast({
        title: t("contextMenu.restoreSuccess", { name: row.name }),
        status: "success",
        duration: 2000,
        isClosable: true,
      });
      markPcHidden(row.name, false);
    } catch (error) {
      console.error("Failed to restore this-pc item:", error);
      toast({
        title: t("contextMenu.restoreError"),
        description: localErr(String(error)),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
    setOperatingKeys((prev) => {
      const next = new Set(prev);
      next.delete(key);
      return next;
    });
  };

  const filteredMenu = useMemo(() => {
    const q = menuQuery.trim().toLowerCase();
    const base = menuCat === "all" ? menuItems : menuItems.filter((it) => it.category === menuCat);
    if (!q) return base;
    return base.filter(
      (it) =>
        it.name.toLowerCase().includes(q) ||
        it.command.toLowerCase().includes(q) ||
        it.verb.toLowerCase().includes(q) ||
        menuCategoryLabels[it.category]?.toLowerCase().includes(q)
    );
  }, [menuItems, menuQuery, menuCat]);

  // 按显示名（中文名）去重合并「此电脑」项：My/Local 两套同逻辑文件夹（多个 CLSID）
  // 以及跨 HKCU/HKLM 的多处注册，都合并成一行；隐藏/恢复时一次性处理所有 CLSID/位置
  const pcRows = useMemo<ThisPcRow[]>(() => {
    const map = new Map<string, ThisPcRow>();
    for (const it of pcItems) {
      const keyName = it.name || it.clsid;
      const row = map.get(keyName);
      if (row) {
        if (!row.hives.includes(it.hive)) row.hives.push(it.hive);
        row.originals.push(it);
        if (!it.is_hidden) row.is_hidden = false;
      } else {
        map.set(keyName, {
          name: keyName,
          clsid: it.clsid,
          hives: [it.hive],
          originals: [it],
          is_hidden: it.is_hidden,
        });
      }
    }
    return [...map.values()];
  }, [pcItems]);

  const filteredPc = useMemo(() => {
    const q = pcQuery.trim().toLowerCase();
    if (!q) return pcRows;
    return pcRows.filter(
      (r) => r.name.toLowerCase().includes(q) || r.clsid.toLowerCase().includes(q)
    );
  }, [pcRows, pcQuery]);

  const transitionMode = useTransitionMode();

  const content = (
    <VStack align="stretch" spacing={6} pt={8}>
      <HStack justifyContent="space-between" alignItems="center" w="full">
        <Button
          variant="ghost"
          leftIcon={<ArrowLeft size={18} />}
          onClick={() => navigate("/builtin-tools")}
          color={headingColor}
        >
          {t("builtinTools.back")}
        </Button>
        <Heading size="lg" color={adaptiveTitle.text} textShadow={adaptiveTitle.shadow} fontWeight="700">
          {t("contextMenu.title")}
        </Heading>
        <LiquidGlassButton
          leftIcon={<RefreshCw size={16} />}
          onClick={() => {
            scanMenu();
            scanPc();
            loadDrives();
          }}
          isLoading={isScanning}
          size="sm"
          variant="outline"
          colorScheme="gray"
        >
          {t("contextMenu.refresh")}
        </LiquidGlassButton>
      </HStack>

      <Tabs variant="soft-rounded" isLazy>
        <TabList>
          <Tab
            _selected={{ bg: themeConfig.primaryColor, color: tabTextColor }}
          >
            {t("contextMenu.menuTab")}
          </Tab>
          <Tab
            _selected={{ bg: themeConfig.primaryColor, color: tabTextColor }}
          >
            {t("contextMenu.thisPcTab")}
          </Tab>
        </TabList>
        <TabPanels>
          <TabPanel px={0}>
            <VStack align="stretch" spacing={3}>
              <Tabs
                size="sm"
                variant="soft-rounded"
                index={menuCategoryOrder.indexOf(menuCat as (typeof menuCategoryOrder)[number])}
                onChange={(i) => setMenuCat(menuCategoryOrder[i])}
              >
                <TabList>
                  {menuCategoryOrder.map((c) => (
                    <Tab
                      key={c}
                      _selected={{ bg: themeConfig.primaryColor, color: tabTextColor }}
                    >
                      {c === "all" ? t("contextMenu.all") : menuCategoryLabels[c]}
                    </Tab>
                  ))}
                </TabList>
              </Tabs>

              <HStack spacing={3} justify="space-between">
                <HStack spacing={2}>
                  <Text color={subTextColor} fontSize="sm">
                    {t("contextMenu.totalMenu", { count: filteredMenu.length })}
                  </Text>
                </HStack>
                <Box w="260px">
                  <Input
                    size="sm"
                    placeholder={t("contextMenu.searchPlaceholder")}
                    value={menuQuery}
                    onChange={(e) => setMenuQuery(e.target.value)}
                    {...inputGlassProps}
                  />
                </Box>
              </HStack>

              {isScanning ? (
                <VStack py={12}>
                  <Spinner size="lg" color={themeConfig.primaryColor} />
                  <Text color={subTextColor}>{t("contextMenu.scanning")}</Text>
                </VStack>
              ) : filteredMenu.length === 0 ? (
                <VStack py={12}>
                  <Text color={subTextColor}>{t("contextMenu.noMenu")}</Text>
                </VStack>
              ) : (
                <LiquidGlassCard
                  overflow="hidden"
                >
                  <Table variant="unstyled" size="sm" layout="fixed">
                    <Thead borderBottom="1px solid" borderColor={tableBorder}>
                      <Tr>
                        <Th px={4} py={3} color={subTextColor} fontSize="xs" textTransform="uppercase" letterSpacing="wider" w="28%">
                          {t("contextMenu.colName")}
                        </Th>
                        <Th px={4} py={3} color={subTextColor} fontSize="xs" textTransform="uppercase" letterSpacing="wider" w="38%">
                          {t("contextMenu.colCommand")}
                        </Th>
                        <Th px={4} py={3} color={subTextColor} fontSize="xs" textTransform="uppercase" letterSpacing="wider" w="18%">
                          {t("contextMenu.colSource")}
                        </Th>
                        <Th px={4} py={3} color={subTextColor} fontSize="xs" textTransform="uppercase" letterSpacing="wider" w="16%">
                          {t("contextMenu.colActions")}
                        </Th>
                      </Tr>
                    </Thead>
                    <Tbody>
                      {filteredMenu.map((item) => {
                        const key = `${item.reg_path}/${item.verb}`;
                        const isOp = operatingKeys.has(key);
                        return (
                          <Tr
                            key={key}
                            _hover={{ bg: hoverBg }}
                            transition="background 0.15s"
                            opacity={isOp ? 0.5 : item.is_hidden ? 0.6 : 1}
                          >
                            <Td px={4} py={3}>
                              <Flex align="center" gap={2}>
                                {item.icon && (
                                  <Image
                                    src={item.icon}
                                    alt=""
                                    boxSize="20px"
                                    objectFit="contain"
                                    flexShrink={0}
                                    draggable={false}
                                  />
                                )}
                                <Text
                                  color={item.is_hidden ? pathColor : headingColor}
                                  fontWeight="medium"
                                  fontSize="sm"
                                  noOfLines={1}
                                  textDecoration={item.is_hidden ? "line-through" : "none"}
                                >
                                  {item.name}
                                </Text>
                                {item.is_hidden && (
                                  <Badge colorScheme="gray" variant="subtle" borderRadius="full" px={2} fontSize="xs" flexShrink={0}>
                                    {t("contextMenu.hidden")}
                                  </Badge>
                                )}
                              </Flex>
                            </Td>
                            <Td px={4} py={3}>
                              <Tooltip label={item.command || item.verb} placement="top">
                                <Text
                                  color={pathColor}
                                  fontSize="xs"
                                  noOfLines={1}
                                  fontFamily="mono"
                                >
                                  {item.command || item.verb}
                                </Text>
                              </Tooltip>
                            </Td>
                            <Td px={4} py={3}>
                              <Badge
                                colorScheme={item.category === "file" ? "purple" : item.category === "desktop" ? "green" : "blue"}
                                variant="subtle"
                                borderRadius="full"
                                px={2}
                                fontSize="xs"
                                flexShrink={0}
                              >
                                {menuCategoryLabels[item.category] || item.category}
                              </Badge>
                            </Td>
                            <Td px={4} py={3}>
                              <HStack spacing={1}>
                                {item.is_hidden ? (
                                  <Tooltip label={t("contextMenu.restore")} placement="top">
                                    <IconButton
                                      aria-label={t("contextMenu.restore")}
                                      icon={<RotateCcw size={14} />}
                                      size="sm"
                                      variant="ghost"
                                      color={restoreColor}
                                      onClick={() => handleRestoreMenu(item)}
                                      isLoading={isOp}
                                    />
                                  </Tooltip>
                                ) : (
                                  <Tooltip label={t("contextMenu.hide")} placement="top">
                                    <IconButton
                                      aria-label={t("contextMenu.hide")}
                                      icon={<Ban size={14} />}
                                      size="sm"
                                      variant="ghost"
                                      color={deleteColor}
                                      onClick={() => handleHideMenu(item)}
                                      isLoading={isOp}
                                    />
                                  </Tooltip>
                                )}
                              </HStack>
                            </Td>
                          </Tr>
                        );
                      })}
                    </Tbody>
                  </Table>
                </LiquidGlassCard>
              )}
            </VStack>
          </TabPanel>

          <TabPanel px={0}>
            <VStack align="stretch" spacing={3}>
              {drives.length > 0 && (
                <Box>
                  <Text fontWeight="semibold" mb={2} color={headingColor} fontSize="sm">
                    {t("contextMenu.drives")}
                  </Text>
                  <Flex wrap="wrap" gap={2}>
                    {drives.map((d) => (
                      <LiquidGlassCard
                        key={d.letter}
                        p={2}
                        borderRadius="lg"
                        boxShadow="sm"
                        _hover={{ borderColor: themeConfig.primaryColor }}
                      >
                        <Flex align="center" gap={2}>
                          <HardDrive size={15} color={themeConfig.primaryColor} />
                          <Text fontWeight="semibold" fontSize="sm" color={headingColor}>
                            {d.letter}
                          </Text>
                          {d.label && (
                            <Text color={pathColor} fontSize="xs" maxW="120px" isTruncated>
                              {d.label}
                            </Text>
                          )}
                          <Badge colorScheme="gray" variant="subtle" fontSize="xs">
                            {driveTypeLabels[d.drive_type] || d.drive_type}
                          </Badge>
                        </Flex>
                      </LiquidGlassCard>
                    ))}
                  </Flex>
                </Box>
              )}
              <HStack spacing={3} justify="space-between">
                <HStack spacing={2}>
                  <Text color={subTextColor} fontSize="sm">
                    {t("contextMenu.totalPc", { count: filteredPc.length })}
                  </Text>
                </HStack>
                <Box w="260px">
                  <Input
                    size="sm"
                    placeholder={t("contextMenu.searchPlaceholder")}
                    value={pcQuery}
                    onChange={(e) => setPcQuery(e.target.value)}
                    {...inputGlassProps}
                  />
                </Box>
              </HStack>

              {isScanning ? (
                <VStack py={12}>
                  <Spinner size="lg" color={themeConfig.primaryColor} />
                  <Text color={subTextColor}>{t("contextMenu.scanning")}</Text>
                </VStack>
              ) : filteredPc.length === 0 ? (
                <VStack py={12}>
                  <Text color={subTextColor}>{t("contextMenu.noPc")}</Text>
                </VStack>
              ) : (
                <LiquidGlassCard
                  overflow="hidden"
                >
                  <Table variant="unstyled" size="sm" layout="fixed">
                    <Thead borderBottom="1px solid" borderColor={tableBorder}>
                      <Tr>
                        <Th px={4} py={3} color={subTextColor} fontSize="xs" textTransform="uppercase" letterSpacing="wider" w="30%">
                          {t("contextMenu.colName")}
                        </Th>
                        <Th px={4} py={3} color={subTextColor} fontSize="xs" textTransform="uppercase" letterSpacing="wider" w="42%">
                          CLSID
                        </Th>
                        <Th px={4} py={3} color={subTextColor} fontSize="xs" textTransform="uppercase" letterSpacing="wider" w="12%">
                          {t("contextMenu.colSource")}
                        </Th>
                        <Th px={4} py={3} color={subTextColor} fontSize="xs" textTransform="uppercase" letterSpacing="wider" w="16%">
                          {t("contextMenu.colActions")}
                        </Th>
                      </Tr>
                    </Thead>
                    <Tbody>
                      {filteredPc.map((row) => {
                        const rowKey = `pc/${row.name}`;
                        const isOp = operatingKeys.has(rowKey);
                        return (
                          <Tr
                            key={rowKey}
                            _hover={{ bg: hoverBg }}
                            transition="background 0.15s"
                            opacity={isOp ? 0.5 : row.is_hidden ? 0.6 : 1}
                          >
                            <Td px={4} py={3}>
                              <Flex align="center" gap={2}>
                                <Text
                                  color={row.is_hidden ? pathColor : headingColor}
                                  fontWeight="medium"
                                  fontSize="sm"
                                  noOfLines={1}
                                  textDecoration={row.is_hidden ? "line-through" : "none"}
                                >
                                  {row.name}
                                </Text>
                                {row.is_hidden && (
                                  <Badge colorScheme="gray" variant="subtle" borderRadius="full" px={2} fontSize="xs" flexShrink={0}>
                                    {t("contextMenu.hidden")}
                                  </Badge>
                                )}
                              </Flex>
                            </Td>
                            <Td px={4} py={3}>
                              <Tooltip label={row.originals.map((o) => o.clsid).join("\n")} placement="top">
                                <Text color={pathColor} fontSize="xs" noOfLines={1} fontFamily="mono">
                                  {row.originals.map((o) => o.clsid).join(", ")}
                                </Text>
                              </Tooltip>
                            </Td>
                            <Td px={4} py={3}>
                              <Badge
                                colorScheme={row.hives.some((h) => h === "HKCU") ? "green" : "orange"}
                                variant="subtle"
                                borderRadius="full"
                                px={2}
                                fontSize="xs"
                                flexShrink={0}
                              >
                                {row.hives.join(", ")}
                              </Badge>
                            </Td>
                            <Td px={4} py={3}>
                              <HStack spacing={1}>
                                {row.is_hidden ? (
                                  <Tooltip label={t("contextMenu.restore")} placement="top">
                                    <IconButton
                                      aria-label={t("contextMenu.restore")}
                                      icon={<RotateCcw size={14} />}
                                      size="sm"
                                      variant="ghost"
                                      color={restoreColor}
                                      onClick={() => handleRestorePc(row)}
                                      isLoading={isOp}
                                    />
                                  </Tooltip>
                                ) : (
                                  <Tooltip label={t("contextMenu.hide")} placement="top">
                                    <IconButton
                                      aria-label={t("contextMenu.hide")}
                                      icon={<Ban size={14} />}
                                      size="sm"
                                      variant="ghost"
                                      color={deleteColor}
                                      onClick={() => handleHidePc(row)}
                                      isLoading={isOp}
                                    />
                                  </Tooltip>
                                )}
                              </HStack>
                            </Td>
                          </Tr>
                        );
                      })}
                    </Tbody>
                  </Table>
                </LiquidGlassCard>
              )}
            </VStack>
          </TabPanel>
        </TabPanels>
      </Tabs>
    </VStack>
  );

  return transitionMode !== "off" ? (
    <motion.div
      initial="initial"
      animate="enter"
      exit="exit"
      variants={getVariants(transitionMode)}
      transition={getTransitionConfig(transitionMode)}
    >
      {content}
    </motion.div>
  ) : (
    <div>{content}</div>
  );
}