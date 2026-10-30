import { useState, useEffect } from "react";
import {
  Modal,
  ModalOverlay,
  ModalContent,
  ModalBody,
  ModalFooter,
  Button,
  useColorModeValue,
  Text,
  VStack,
  Image,
} from "@chakra-ui/react";
import { store } from "@/lib/store";
import { useThemeColor } from "@/contexts/theme-color-context";

const REPO_URL = "https://github.com/MuLiuSaMa/NexBox";
const STORE_KEY = "nexbox_star_prompt";
/** 「算惹吧...」的冷却期：7 天后再问一次 */
const LATER_COOLDOWN_MS = 7 * 24 * 60 * 60 * 1000;

interface StarPromptRecord {
  action: "done" | "later";
  at: number;
}

/**
 * 开屏求 Star 弹窗：
 *   「好滴~」→ 打开 GitHub 仓库并永久不再弹；
 *   「算惹吧...」→ 7 天冷却后再弹一次。
 * 记忆与 ImportantAnnouncementModal 同一套模式：tauri store 为主、localStorage 兜底。
 */
export function StarPromptModal() {
  const [isOpen, setIsOpen] = useState(false);
  const { config } = useThemeColor();
  const primaryColor = config.primaryColor;

  const modalBg = useColorModeValue("white", "#1a1a1a");
  const modalBorderColor = useColorModeValue("gray.200", "#333333");
  const titleColor = useColorModeValue("gray.800", "#ffffff");
  const subLabelColor = useColorModeValue("gray.500", "whiteAlpha.700");

  useEffect(() => {
    let disposed = false;
    const check = async () => {
      try {
        let raw = await store.get<string>(STORE_KEY);
        if (!raw) raw = localStorage.getItem(STORE_KEY);
        if (raw) {
          const rec = JSON.parse(raw) as StarPromptRecord;
          if (rec.action === "done") return;
          if (
            rec.action === "later" &&
            typeof rec.at === "number" &&
            Date.now() - rec.at < LATER_COOLDOWN_MS
          ) {
            return;
          }
        }
        if (!disposed) setIsOpen(true);
      } catch {
        /* 记录损坏按「从未点过」处理，正常弹出 */
        if (!disposed) setIsOpen(true);
      }
    };
    // 进主页立即弹，不设延迟
    void check();
    return () => {
      disposed = true;
    };
  }, []);

  const mark = async (action: "done" | "later") => {
    try {
      const payload = JSON.stringify({ action, at: Date.now() } satisfies StarPromptRecord);
      localStorage.setItem(STORE_KEY, payload);
      await store.set(STORE_KEY, payload);
      await store.save();
    } catch {
      /* 存储失败仅影响下次是否再弹，不打断关闭流程 */
    }
  };

  const handleOk = async () => {
    setIsOpen(false);
    void mark("done");
    try {
      const { openUrl } = await import("@tauri-apps/plugin-opener");
      await openUrl(REPO_URL);
    } catch {
      window.open(REPO_URL, "_blank");
    }
  };

  const handleLater = () => {
    setIsOpen(false);
    void mark("later");
  };

  return (
    <Modal isOpen={isOpen} onClose={handleLater} isCentered>
      <ModalOverlay />
      <ModalContent
        bg={modalBg}
        borderColor={modalBorderColor}
        borderRadius="3xl"
        maxW="sm"
        overflow="hidden"
      >
        <ModalBody p={0}>
          <Image src="/Star.png" alt="Starred" w="100%" display="block" />
          <VStack spacing={1} px={5} pt={4} pb={2}>
            <Text fontWeight="bold" fontSize="lg" color={titleColor}>
              为我们的开源项目点一个Star叭 qwq
            </Text>
            <Text fontSize="sm" color={subLabelColor} textAlign="center">
              新境盒持续免费开源，这一个星标对我滴帮助很大。
            </Text>
          </VStack>
        </ModalBody>
        <ModalFooter gap={3}>
          <Button variant="ghost" onClick={handleLater}>
            算惹吧...
          </Button>
          <Button
            bg={primaryColor}
            color="white"
            _hover={{ filter: "brightness(0.88)" }}
            _active={{ filter: "brightness(0.8)" }}
            onClick={handleOk}
          >
            好滴~
          </Button>
        </ModalFooter>
      </ModalContent>
    </Modal>
  );
}
