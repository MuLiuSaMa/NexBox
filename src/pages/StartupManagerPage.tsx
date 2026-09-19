import {
  Box,
  Flex,
  Text,
  Heading,
  VStack,
  HStack,
  Button,
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
  Tabs,
  TabList,
  TabPanels,
  TabPanel,
  Tab,
} from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { LiquidGlassCard } from "@/components/special/liquid-glass-card";
import { LiquidGlassButton } from "@/components/special/liquid-glass-button";
import { useTranslation } from "react-i18next";
import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Ban,
  RotateCcw,
  RefreshCw,
  ArrowLeft,
  FolderOpen,
  Search,
  FileCode,
} from "lucide-react";
import { useBackground } from "@/contexts/background-context";
import { useThemeColor } from "@/contexts/theme-color-context";
import { useNavigate } from "react-router-dom";
import { useAdaptiveTextColor } from "@/hooks/use-adaptive-text-color";

interface StartupItem {
  name: string;
  file_location: string;
  location_type: string;
  item_type: string;
  reg_key_path: string | null;
  reg_value_name: string | null;
  folder_path: string | null;
  raw_registry_value: string | null;
  is_disabled: boolean;
}

interface ServiceItem {
  name: string;
  display_name: string;
  description: string | null;
  is_disabled: boolean;
  is_running: boolean;
  binary_path: string | null;
}

export default function StartupManagerPage() {
  const { t } = useTranslation();
  const toast = useDynamicIsland("rocket");
  const { liquidGlassEnabled } = useBackground();
  const { config: themeConfig, getContrastTextColor } = useThemeColor();
  const navigate = useNavigate();

  const adaptiveTitle = useAdaptiveTextColor();
  const headingColor = useColorModeValue("gray.900", "#ffffff");
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
  const enableColor = useColorModeValue("teal.600", "teal.300");
  const tabTextColor = getContrastTextColor();

  const [items, setItems] = useState<StartupItem[]>([]);
  const [isScanning, setIsScanning] = useState(false);
  const [disablingItems, setDisablingItems] = useState<Set<string>>(new Set());
  // 启动项软件图标（file_location -> data URI），空字符串表示未取到
  const [icons, setIcons] = useState<Record<string, string>>({});
  // Windows 系统服务（自启动 + 已禁用）
  const [services, setServices] = useState<ServiceItem[]>([]);
  const [servicesScanning, setServicesScanning] = useState(false);
  const [togglingServices, setTogglingServices] = useState<Set<string>>(new Set());
  const [isAppAdmin, setIsAppAdmin] = useState(false);

  // 扫描完成后异步获取每个启动项的软件图标（不阻塞列表渲染）
  useEffect(() => {
    let cancelled = false;
    const fetchIcons = async () => {
      const next: Record<string, string> = {};
      for (const item of items) {
        const path = item.file_location;
        if (!path || next[path]) continue;
        try {
          const dataUri = await invoke<string>("get_startup_item_icon", { fileLocation: path });
          if (!cancelled && dataUri) next[path] = dataUri;
        } catch {
          // 单条图标获取失败不影响其他项
        }
      }
      if (!cancelled && Object.keys(next).length > 0) {
        setIcons((prev) => ({ ...prev, ...next }));
      }
    };
    fetchIcons();
    return () => {
      cancelled = true;
    };
  }, [items]);

  const loadServices = useCallback(
    async (showSpinner = true) => {
      if (showSpinner) setServicesScanning(true);
      try {
        const [svcResult, adminResult] = await Promise.all([
          invoke<ServiceItem[]>("scan_services"),
          invoke<boolean>("is_app_admin"),
        ]);
        setServices(svcResult);
        setIsAppAdmin(adminResult);
      } catch (error) {
        console.error("Failed to scan services:", error);
        toast({
          title: t("optimization.startupManager.scanServicesError"),
          description: String(error),
          status: "error",
          duration: 3000,
          isClosable: true,
        });
      }
      if (showSpinner) setServicesScanning(false);
    },
    [t, toast]
  );

  const handleToggleService = async (svc: ServiceItem, enable: boolean) => {
    setTogglingServices((prev) => new Set(prev).add(svc.name));
    try {
      await invoke("set_service_start_type", { name: svc.name, enable });
      toast({
        title: enable
          ? t("optimization.startupManager.enableServiceSuccess", { name: svc.name })
          : t("optimization.startupManager.disableServiceSuccess", { name: svc.name }),
        status: "success",
        duration: 2000,
        isClosable: true,
      });
      await loadServices(false);
    } catch (error) {
      console.error("Failed to change service start type:", error);
      toast({
        title: t("optimization.startupManager.serviceActionError"),
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
    setTogglingServices((prev) => {
      const next = new Set(prev);
      next.delete(svc.name);
      return next;
    });
  };

  const doScan = useCallback(
    async (showSpinner = true) => {
      if (showSpinner) setIsScanning(true);
      try {
        const result = await invoke<StartupItem[]>("scan_startup_items");
        setItems(result);
      } catch (error) {
        console.error("Failed to scan startup items:", error);
        toast({
          title: t("optimization.startupManager.scanError"),
          description: String(error),
          status: "error",
          duration: 3000,
          isClosable: true,
        });
      }
      if (showSpinner) setIsScanning(false);
      await loadServices(false);
    },
    [t, toast, loadServices]
  );

  useEffect(() => {
    doScan();
  }, [doScan]);

  const handleDisable = async (item: StartupItem, index: number) => {
    const key = `${item.name}-${index}`;
    setDisablingItems((prev) => new Set(prev).add(key));
    try {
      await invoke("disable_startup_item", { item });
      toast({
        title: t("optimization.startupManager.disableSuccess", { name: item.name }),
        status: "success",
        duration: 2000,
        isClosable: true,
      });
      await doScan(false);
    } catch (error) {
      console.error("Failed to disable startup item:", error);
      toast({
        title: t("optimization.startupManager.disableError"),
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
    setDisablingItems((prev) => {
      const next = new Set(prev);
      next.delete(key);
      return next;
    });
  };

  const handleEnable = async (item: StartupItem, index: number) => {
    const key = `${item.name}-${index}`;
    setDisablingItems((prev) => new Set(prev).add(key));
    try {
      await invoke("enable_startup_item", { item });
      toast({
        title: t("optimization.startupManager.enableSuccess", { name: item.name }),
        status: "success",
        duration: 2000,
        isClosable: true,
      });
      await doScan(false);
    } catch (error) {
      console.error("Failed to enable startup item:", error);
      toast({
        title: t("optimization.startupManager.enableError"),
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
    setDisablingItems((prev) => {
      const next = new Set(prev);
      next.delete(key);
      return next;
    });
  };

  const handleLocateFile = async (fileLocation: string, itemType: string, rawRegistryValue: string | null) => {
    try {
      await invoke("locate_startup_file", { fileLocation, itemType, rawRegistryValue });
    } catch (error) {
      console.error("Failed to locate file:", error);
      toast({
        title: t("optimization.startupManager.locateError"),
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
  };

  const handleFindInRegistry = async (regKeyPath: string, locationType: string) => {
    try {
      await invoke("find_startup_key_in_registry", { regKeyPath, locationType });
    } catch (error) {
      console.error("Failed to open registry:", error);
      toast({
        title: t("optimization.startupManager.registryError"),
        description: String(error),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
  };


  const content = (
    <VStack align="stretch" spacing={6} pt={8}>
      <HStack justifyContent="space-between" alignItems="center" w="full">
        <Button
          variant="ghost"
          leftIcon={<ArrowLeft size={18} />}
          onClick={() => navigate("/optimization")}
          color={headingColor}
        >
                        返回
        </Button>
        <Heading size="lg" color={adaptiveTitle.text} textShadow={adaptiveTitle.shadow} fontWeight="700">
          {t("optimization.startupManager.title")}
        </Heading>
        <Box w="100px" />
      </HStack>

      <Tabs variant="soft-rounded" isLazy>
        <TabList>
          <Tab
            fontSize="sm"
            _selected={{
              bg: themeConfig.primaryColor,
              color: tabTextColor,
              boxShadow: `0 2px 14px -3px ${themeConfig.primaryColor}66`,
            }}
          >
            {t("optimization.startupManager.tabStartupItems")}
          </Tab>
          <Tab
            fontSize="sm"
            _selected={{
              bg: themeConfig.primaryColor,
              color: tabTextColor,
              boxShadow: `0 2px 14px -3px ${themeConfig.primaryColor}66`,
            }}
          >
            {t("optimization.startupManager.tabServices")}
          </Tab>
        </TabList>

        <TabPanels pt={4}>
          <TabPanel px={0}>
            <HStack spacing={3} justify="space-between" mb={3}>
              <Text color={subTextColor} fontSize="sm">
                {t("optimization.startupManager.totalItems", { count: items.length })}
              </Text>
              <LiquidGlassButton
                leftIcon={<RefreshCw size={16} />}
                onClick={doScan}
                isLoading={isScanning}
                size="sm"
                variant="outline"
                colorScheme="gray"
              >
                {t("optimization.startupManager.refresh")}
              </LiquidGlassButton>
            </HStack>

      {isScanning ? (
        <VStack py={12}>
          <Spinner size="lg" color="teal.500" />
          <Text color={subTextColor}>{t("optimization.startupManager.scanning")}</Text>
        </VStack>
      ) : items.length === 0 ? (
        <VStack py={12}>
          <Text color={subTextColor}>{t("optimization.startupManager.noItems")}</Text>
        </VStack>
      ) : (
        <LiquidGlassCard
          overflow="hidden"
        >
          <Table variant="unstyled" size="sm" layout="fixed">
            <Thead borderBottom="1px solid" borderColor={tableBorder}>
              <Tr>
                <Th px={4} py={3} color={subTextColor} fontSize="xs" textTransform="uppercase" letterSpacing="wider" w="30%">
                  {t("optimization.startupManager.columnName")}
                </Th>
                <Th px={4} py={3} color={subTextColor} fontSize="xs" textTransform="uppercase" letterSpacing="wider" w="45%">
                  {t("optimization.startupManager.columnPath")}
                </Th>
                <Th px={4} py={3} color={subTextColor} fontSize="xs" textTransform="uppercase" letterSpacing="wider" w="140px">
                  {t("optimization.startupManager.columnActions")}
                </Th>
              </Tr>
            </Thead>
            <Tbody>
              {items.map((item, index) => {
                const key = `${item.name}-${index}`;
                const isDisabling = disablingItems.has(key);
                return (
                  <Tr
                    key={key}
                    _hover={{ bg: hoverBg }}
                    transition="background 0.15s"
                    opacity={isDisabling ? 0.5 : item.is_disabled ? 0.6 : 1}
                  >
                    <Td px={4} py={3}>
                      <Flex align="center" gap={2}>
                        <Box
                          w={8}
                          h={8}
                          borderRadius="md"
                          bg={icons[item.file_location] ? "transparent" : `${themeConfig.primaryColor}15`}
                          display="flex"
                          alignItems="center"
                          justifyContent="center"
                          color={themeConfig.primaryColor}
                          flexShrink={0}
                          overflow="hidden"
                        >
                          {icons[item.file_location] ? (
                            <img
                              src={icons[item.file_location]}
                              alt=""
                              style={{ width: "100%", height: "100%", objectFit: "contain" }}
                              loading="lazy"
                            />
                          ) : item.item_type === "Registry" ? (
                            <Search size={14} />
                          ) : (
                            <FileCode size={14} />
                          )}
                        </Box>
                        <Text
                          color={item.is_disabled ? pathColor : headingColor}
                          fontWeight="medium"
                          fontSize="sm"
                          noOfLines={1}
                          textDecoration={item.is_disabled ? "line-through" : "none"}
                        >
                          {item.name}
                        </Text>
                        {item.is_disabled && (
                          <Badge
                            colorScheme="gray"
                            variant="subtle"
                            borderRadius="full"
                            px={2}
                            fontSize="xs"
                            flexShrink={0}
                          >
                            {t("optimization.startupManager.disabled")}
                          </Badge>
                        )}
                      </Flex>
                    </Td>
                    <Td px={4} py={3}>
                      <Tooltip label={item.file_location} placement="top">
                        <Text
                          color={pathColor}
                          fontSize="xs"
                          noOfLines={1}
                          fontFamily="mono"
                        >
                          {item.file_location || "-"}
                        </Text>
                      </Tooltip>
                    </Td>
                    <Td px={4} py={3}>
                      <HStack spacing={1}>
                        <Tooltip label={t("optimization.startupManager.locateFile")} placement="top">
                          <IconButton
                            aria-label={t("optimization.startupManager.locateFile")}
                            icon={<FolderOpen size={14} />}
                            size="sm"
                            variant="ghost"
                            onClick={() => handleLocateFile(item.file_location, item.item_type, item.raw_registry_value)}
                            isDisabled={!item.file_location}
                          />
                        </Tooltip>
                        {item.item_type === "Registry" && item.reg_key_path && (
                          <Tooltip label={t("optimization.startupManager.findInRegistry")} placement="top">
                            <IconButton
                              aria-label={t("optimization.startupManager.findInRegistry")}
                              icon={<Search size={14} />}
                              size="sm"
                              variant="ghost"
                              onClick={() =>
                                handleFindInRegistry(item.reg_key_path!, item.location_type)
                              }
                            />
                          </Tooltip>
                        )}
                        {item.is_disabled ? (
                          <Tooltip label={t("optimization.startupManager.enable")} placement="top">
                            <IconButton
                              aria-label={t("optimization.startupManager.enable")}
                              icon={<RotateCcw size={14} />}
                              size="sm"
                              variant="ghost"
                              colorScheme="teal"
                              color={enableColor}
                              onClick={() => handleEnable(item, index)}
                              isLoading={isDisabling}
                            />
                          </Tooltip>
                        ) : (
                          <Tooltip label={t("optimization.startupManager.disable")} placement="top">
                            <IconButton
                              aria-label={t("optimization.startupManager.disable")}
                              icon={<Ban size={14} />}
                              size="sm"
                              variant="ghost"
                              colorScheme="red"
                              color={deleteColor}
                              onClick={() => handleDisable(item, index)}
                              isLoading={isDisabling}
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

      </TabPanel>
          <TabPanel px={0}>
            <HStack spacing={2} mb={3}>
              <Text color={subTextColor} fontSize="sm">
                {t("optimization.startupManager.servicesCount", { count: services.length })}
              </Text>
            </HStack>

            {!isAppAdmin && (
              <Text color={useColorModeValue("orange.600", "orange.300")} fontSize="xs">
                {t("optimization.startupManager.serviceAdminRequired")}
              </Text>
            )}

      {services.length === 0 ? (
        <VStack py={6}>
          <Text color={subTextColor}>{t("optimization.startupManager.noServices")}</Text>
        </VStack>
      ) : (
        <LiquidGlassCard overflow="hidden">
          <Table variant="unstyled" size="sm" layout="fixed">
            <Thead borderBottom="1px solid" borderColor={tableBorder}>
              <Tr>
                <Th px={4} py={3} color={subTextColor} fontSize="xs" textTransform="uppercase" letterSpacing="wider" w="20%">
                  {t("optimization.startupManager.columnServiceName")}
                </Th>
                <Th px={4} py={3} color={subTextColor} fontSize="xs" textTransform="uppercase" letterSpacing="wider" w="20%">
                  {t("optimization.startupManager.columnDisplayName")}
                </Th>
                <Th px={4} py={3} color={subTextColor} fontSize="xs" textTransform="uppercase" letterSpacing="wider" w="38%">
                  {t("optimization.startupManager.columnDescription")}
                </Th>
                <Th px={4} py={3} color={subTextColor} fontSize="xs" textTransform="uppercase" letterSpacing="wider" w="12%">
                  {t("optimization.startupManager.columnState")}
                </Th>
                <Th px={4} py={3} color={subTextColor} fontSize="xs" textTransform="uppercase" letterSpacing="wider" w="110px">
                  {t("optimization.startupManager.columnActions")}
                </Th>
              </Tr>
            </Thead>
            <Tbody>
              {services.map((svc, index) => {
                const toggling = togglingServices.has(svc.name);
                return (
                  <Tr
                    key={`${svc.name}-${index}`}
                    _hover={{ bg: hoverBg }}
                    transition="background 0.15s"
                    opacity={toggling ? 0.5 : svc.is_disabled ? 0.6 : 1}
                  >
                    <Td px={4} py={3}>
                      <Text
                        color={svc.is_disabled ? pathColor : headingColor}
                        fontWeight="medium"
                        fontSize="sm"
                        noOfLines={1}
                        textDecoration={svc.is_disabled ? "line-through" : "none"}
                      >
                        {svc.name}
                      </Text>
                    </Td>
                    <Td px={4} py={3}>
                      <Text color={pathColor} fontSize="xs" noOfLines={1}>
                        {svc.display_name || "-"}
                      </Text>
                    </Td>
                    <Td px={4} py={3}>
                      {svc.description ? (
                        <Tooltip label={svc.description} placement="top">
                          <Text color={pathColor} fontSize="xs" noOfLines={1}>
                            {svc.description}
                          </Text>
                        </Tooltip>
                      ) : (
                        <Text color={pathColor} fontSize="xs">-</Text>
                      )}
                    </Td>
                    <Td px={4} py={3}>
                      {svc.is_running ? (
                        <Badge colorScheme="teal" variant="subtle" borderRadius="full" px={2} fontSize="xs">
                          {t("optimization.startupManager.stateRunning")}
                        </Badge>
                      ) : (
                        <Badge colorScheme="gray" variant="subtle" borderRadius="full" px={2} fontSize="xs">
                          {t("optimization.startupManager.stateStopped")}
                        </Badge>
                      )}
                    </Td>
                    <Td px={4} py={3}>
                      <HStack spacing={1}>
                        {svc.is_disabled ? (
                          <Tooltip label={t("optimization.startupManager.enable")} placement="top">
                            <IconButton
                              aria-label={t("optimization.startupManager.enable")}
                              icon={<RotateCcw size={14} />}
                              size="sm"
                              variant="ghost"
                              colorScheme="teal"
                              color={enableColor}
                              onClick={() => handleToggleService(svc, true)}
                              isLoading={toggling}
                            />
                          </Tooltip>
                        ) : (
                          <Tooltip label={t("optimization.startupManager.disable")} placement="top">
                            <IconButton
                              aria-label={t("optimization.startupManager.disable")}
                              icon={<Ban size={14} />}
                              size="sm"
                              variant="ghost"
                              colorScheme="red"
                              color={deleteColor}
                              onClick={() => handleToggleService(svc, false)}
                              isLoading={toggling}
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
          </TabPanel>
        </TabPanels>
      </Tabs>
    </VStack>
  );

  return content;
}
