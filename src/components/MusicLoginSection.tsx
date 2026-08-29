import { useState } from "react";
import {
  HStack,
  VStack,
  Button,
  Text,
  IconButton,
  Tooltip,
  Avatar,
  Box,
  Badge,
  Modal,
  ModalOverlay,
  ModalContent,
  ModalHeader,
  ModalCloseButton,
  ModalBody,
  Menu,
  MenuButton,
  MenuList,
  MenuItem,
  useColorModeValue,
} from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import {
  ExternalLink,
  LogOut,
  Crown,
  Plus,
  ChevronDown,
  Check,
  Construction,
} from "lucide-react";
import { useShallow } from "zustand/react/shallow";
import { useMusicStore, coverProxyUrl } from "@/stores/music-store";
import { useThemeColor } from "@/contexts/theme-color-context";
import type { MusicProvider } from "@/types/music";

interface ProviderOption {
  id: MusicProvider;
  name: string;
  desc: string;
  color: string;
  logo: string;
  /** 开发中：仅展示，不响应交互 */
  disabled?: boolean;
}

const PROVIDERS: ProviderOption[] = [
  {
    id: "netease",
    name: "网易云音乐",
    desc: "海量曲库 · 歌单 · 云村社区",
    color: "#C20C0C",
    logo: "/music-providers/wyy.png",
  },
  {
    id: "kugou",
    name: "酷狗音乐",
    desc: "千万曲库 · 高品质音效",
    color: "#00A0E9",
    logo: "/music-providers/kugou.png",
  },
  {
    id: "qqmusic",
    name: "QQ 音乐",
    desc: "海量曲库 · Hi-Res · 精准推荐",
    color: "#FEC135",
    logo: "/music-providers/qqmusic.png",
  },
  {
    id: "migu",
    name: "咪咕音乐",
    desc: "华语曲库 · 臻品音质",
    color: "#FF69B4",
    logo: "/music-providers/migu.webp",
  },
];

/** 平台 Logo 圆角图片容器（用于选择 Modal 与下拉菜单） */
function ProviderLogo({ src, size = 40, rounded = "lg" }: { src: string; size?: number; rounded?: string }) {
  return (
    <Box
      w={`${size}px`}
      h={`${size}px`}
      borderRadius={rounded}
      overflow="hidden"
      flexShrink={0}
      bg="white"
      display="flex"
      alignItems="center"
      justifyContent="center"
    >
      <img
        src={src}
        alt=""
        style={{ width: "100%", height: "100%", objectFit: "cover", display: "block" }}
        draggable={false}
      />
    </Box>
  );
}

export function MusicLoginSection() {
  const {
    loginInfo,
    loginInfos,
    playbackSource,
    proxyPort,
    logout,
    openLoginWindow,
    switchPlaybackSource,
  } = useMusicStore(
    useShallow((s) => ({
      loginInfo: s.loginInfo,
      loginInfos: s.loginInfos,
      playbackSource: s.playbackSource,
      proxyPort: s.proxyPort,
      logout: s.logout,
      openLoginWindow: s.openLoginWindow,
      switchPlaybackSource: s.switchPlaybackSource,
    }))
  );
  const { getActiveColor, getContrastTextColor } = useThemeColor();
  const toast = useDynamicIsland("music");
  const [isSelecting, setIsSelecting] = useState(false);

  const activeColor = getActiveColor();
  const contrastText = getContrastTextColor();

  const textColor = useColorModeValue("gray.800", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const modalBg = useColorModeValue("white", "#1a1a1a");
  const modalBorder = useColorModeValue("gray.200", "#333333");
  const menuBg = useColorModeValue("white", "#1a1a1a");
  const menuBorder = useColorModeValue("gray.200", "#333333");
  const menuHoverBg = useColorModeValue("gray.50", "#252525");

  // 已登录的平台数量
  const loggedInProviders = PROVIDERS.filter((p) => !p.disabled && loginInfos[p.id]?.logged_in);
  const hasMultipleLoggedIn = loggedInProviders.length > 1;

  // 处理平台选择点击（开发中平台仅提示，不响应）
  const handleProviderClick = (p: ProviderOption) => {
    if (p.disabled) {
      toast({
        title: `${p.name} 正在开发中`,
        description: "该平台暂未上线，敬请期待",
        status: "info",
        duration: 2500,
        isClosable: true,
      });
      return;
    }
    setIsSelecting(false);
    if (loginInfos[p.id]?.logged_in) {
      switchPlaybackSource(p.id);
    } else {
      openLoginWindow(p.id);
    }
  };

  // 平台选择 Modal (已登录/未登录共用)
  const platformModal = (
    <Modal isOpen={isSelecting} onClose={() => setIsSelecting(false)} isCentered>
      <ModalOverlay />
      <ModalContent bg={modalBg} border="1px solid" borderColor={modalBorder} maxW="400px" mx={4}>
        <ModalHeader fontSize="md" pb={2}>
          选择登录平台
        </ModalHeader>
        <ModalCloseButton />
        <ModalBody pb={5}>
          <VStack spacing={3} align="stretch">
            {PROVIDERS.map((p) => {
              const loggedIn = !p.disabled && loginInfos[p.id]?.logged_in;
              return (
                <Button
                  key={p.id}
                  variant="outline"
                  h="auto"
                  justifyContent="flex-start"
                  px={4}
                  py={3}
                  borderColor={modalBorder}
                  opacity={p.disabled ? 0.65 : 1}
                  cursor={p.disabled ? "not-allowed" : "pointer"}
                  _hover={
                    p.disabled
                      ? { borderColor: modalBorder }
                      : { bg: `${p.color}12`, borderColor: p.color }
                  }
                  _disabled={{ opacity: 0.65, cursor: "not-allowed" }}
                  isDisabled={p.disabled}
                  onClick={() => handleProviderClick(p)}
                >
                  <HStack spacing={3} w="100%">
                    <ProviderLogo src={p.logo} size={40} />
                    <VStack spacing={0} align="start" flex={1}>
                      <HStack spacing={1.5}>
                        <Text fontSize="sm" fontWeight="semibold" color={textColor}>
                          {p.name}
                        </Text>
                        {p.disabled && (
                          <Badge
                            variant="subtle"
                            colorScheme="orange"
                            fontSize="9px"
                            display="inline-flex"
                            alignItems="center"
                            gap={0.5}
                            px={1.5}
                          >
                            <Construction size={9} />
                            开发中
                          </Badge>
                        )}
                      </HStack>
                      <Text fontSize="xs" color={subTextColor}>
                        {p.desc}
                      </Text>
                    </VStack>
                    {!p.disabled && (
                      <Badge variant="subtle" colorScheme={loggedIn ? "green" : "gray"} fontSize="10px">
                        {loggedIn ? "已登录" : "未登录"}
                      </Badge>
                    )}
                  </HStack>
                </Button>
              );
            })}
          </VStack>
        </ModalBody>
      </ModalContent>
    </Modal>
  );

  // 未登录状态：点击登录后弹出平台选择
  if (!loginInfo?.logged_in) {
    return (
      <>
        <Button
          size="sm"
          leftIcon={<ExternalLink size={14} />}
          onClick={() => setIsSelecting(true)}
          sx={{
            bg: activeColor,
            color: contrastText,
            _hover: { bg: activeColor, filter: "brightness(0.9)" },
            _active: { bg: activeColor, filter: "brightness(0.8)" },
          }}
        >
          登录
        </Button>
        {platformModal}
      </>
    );
  }

  // 已登录状态
  return (
    <>
      <HStack spacing={2} justify="space-between">
        <HStack spacing={2}>
          {/* 平台切换 Menu */}
          {hasMultipleLoggedIn ? (
            <Menu placement="bottom-start">
              <MenuButton
                as={Box}
                cursor="pointer"
                borderRadius="md"
                _hover={{ bg: useColorModeValue("gray.100", "#252525") }}
                p={1}
              >
                <HStack spacing={2}>
                  <Avatar size="sm" src={coverProxyUrl(loginInfo.avatar, proxyPort)} />
                  <VStack spacing={0} align="start" maxW="120px">
                    <Text color={textColor} fontSize="sm" fontWeight="medium" noOfLines={1}>
                      {loginInfo.nickname}
                    </Text>
                    <HStack spacing={1}>
                      {loginInfo.is_svip ? (
                        <Box
                          as="span"
                          display="inline-flex"
                          alignItems="center"
                          gap={1}
                          px={1.5}
                          py={0.5}
                          borderRadius="sm"
                          fontSize="10px"
                          fontWeight="bold"
                          bg="linear-gradient(135deg, #f6d365 0%, #fda085 50%, #f6d365 100%)"
                          color="#5a3000"
                        >
                          <Crown size={10} strokeWidth={2.5} />
                          SVIP
                        </Box>
                      ) : loginInfo.is_vip ? (
                        <Text color={subTextColor} fontSize="xs">VIP</Text>
                      ) : (
                        <Text color={subTextColor} fontSize="xs">普通用户</Text>
                      )}
                    </HStack>
                  </VStack>
                  <ChevronDown size={14} color={subTextColor} />
                </HStack>
              </MenuButton>
              <MenuList bg={menuBg} border="1px solid" borderColor={menuBorder} minW="200px">
                {loggedInProviders.map((p) => {
                  const info = loginInfos[p.id]!;
                  return (
                    <MenuItem
                      key={p.id}
                      bg={menuBg}
                      _hover={{ bg: menuHoverBg }}
                      onClick={() => switchPlaybackSource(p.id)}
                    >
                      <HStack spacing={3} w="100%">
                        <Avatar size="xs" src={coverProxyUrl(info.avatar, proxyPort)} />
                        <VStack spacing={0} align="start" flex={1}>
                          <Text fontSize="sm" color={textColor} noOfLines={1}>
                            {info.nickname}
                          </Text>
                          <Text fontSize="xs" color={p.color}>
                            {p.name}
                          </Text>
                        </VStack>
                        {playbackSource === p.id && (
                          <Check size={16} color={p.color} />
                        )}
                      </HStack>
                    </MenuItem>
                  );
                })}
                {/* 分隔线 + 添加平台 / 开发中平台 */}
                {PROVIDERS.filter((p) => p.disabled || !loginInfos[p.id]?.logged_in).length > 0 && (
                  <Box my={1} borderTop="1px solid" borderColor={menuBorder} />
                )}
                {PROVIDERS.filter((p) => p.disabled || !loginInfos[p.id]?.logged_in).map((p) => (
                  <MenuItem
                    key={p.id}
                    bg={menuBg}
                    opacity={p.disabled ? 0.6 : 1}
                    cursor={p.disabled ? "not-allowed" : "pointer"}
                    _hover={p.disabled ? { bg: menuBg } : { bg: menuHoverBg }}
                    onClick={() => handleProviderClick(p)}
                  >
                    <HStack spacing={3} w="100%">
                      <ProviderLogo src={p.logo} size={28} rounded="full" />
                      <Text fontSize="sm" color={textColor} flex={1}>
                        {p.disabled ? p.name : `添加${p.name}`}
                      </Text>
                      {p.disabled ? (
                        <Badge
                          variant="subtle"
                          colorScheme="orange"
                          fontSize="9px"
                          display="inline-flex"
                          alignItems="center"
                          gap={0.5}
                        >
                          <Construction size={9} />
                          开发中
                        </Badge>
                      ) : (
                        <Plus size={14} color={subTextColor} />
                      )}
                    </HStack>
                  </MenuItem>
                ))}
              </MenuList>
            </Menu>
          ) : (
            <HStack spacing={2}>
              <Avatar size="sm" src={coverProxyUrl(loginInfo.avatar, proxyPort)} />
              <VStack spacing={0} align="start" maxW="120px">
                <Text color={textColor} fontSize="sm" fontWeight="medium" noOfLines={1}>
                  {loginInfo.nickname}
                </Text>
                {loginInfo.is_svip ? (
                  <Box
                    as="span"
                    display="inline-flex"
                    alignItems="center"
                    gap={1}
                    px={1.5}
                    py={0.5}
                    borderRadius="sm"
                    fontSize="10px"
                    fontWeight="bold"
                    bg="linear-gradient(135deg, #f6d365 0%, #fda085 50%, #f6d365 100%)"
                    color="#5a3000"
                  >
                    <Crown size={10} strokeWidth={2.5} />
                    SVIP
                  </Box>
                ) : loginInfo.is_vip ? (
                  <Text color={subTextColor} fontSize="xs">VIP</Text>
                ) : (
                  <Text color={subTextColor} fontSize="xs">普通用户</Text>
                )}
              </VStack>
            </HStack>
          )}

          {/* 添加平台按钮（只在存在非开发中且未登录平台时显示） */}
          {!hasMultipleLoggedIn &&
            PROVIDERS.some((p) => !p.disabled && !loginInfos[p.id]?.logged_in) && (
              <Tooltip label="添加其他平台账号">
                <IconButton
                  aria-label="Add platform"
                  icon={<Plus size={16} />}
                  size="sm"
                  variant="ghost"
                  onClick={() => setIsSelecting(true)}
                />
              </Tooltip>
            )}
        </HStack>

        <Tooltip label="退出登录">
          <IconButton
            aria-label="Logout"
            icon={<LogOut size={16} />}
            size="sm"
            variant="ghost"
            onClick={() => logout()}
          />
        </Tooltip>
      </HStack>
      {platformModal}
    </>
  );
}
