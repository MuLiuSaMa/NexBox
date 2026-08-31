import { useCallback, useEffect, useRef, useState } from "react";
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
  IconButton,
  Spinner,
  Center,
  useColorModeValue,
} from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { useTranslation } from "react-i18next";
import { FaRotate, FaDownload, FaImage } from "react-icons/fa6";
import { CustomSelect } from "@/components/special/custom-select";
import { useThemeColor } from "@/contexts/theme-color-context";
import { save } from "@tauri-apps/plugin-dialog";
import {
  fetchRandomImageRaw,
  saveRandomImage,
  RANDOM_IMAGE_CATEGORIES,
  RANDOM_IMAGE_CATEGORY_LABELS,
  RANDOM_IMAGE_TYPE_MAP,
  RANDOM_IMAGE_TYPE_LABELS,
  RANDOM_IMAGE_TYPELESS_CATEGORIES,
} from "@/lib/uapi";

interface RandomImageModalProps {
  isOpen: boolean;
  onClose: () => void;
}

export default function RandomImageModal({ isOpen, onClose }: RandomImageModalProps) {
  const { t } = useTranslation();
  const toast = useDynamicIsland("image");
  const { getActiveColor, getContrastTextColor, getHoverColor } = useThemeColor();

  const [category, setCategory] = useState("");
  const [imageType, setImageType] = useState("");
  const [imageUrl, setImageUrl] = useState<string | null>(null);
  const [base64, setBase64] = useState("");
  const [loading, setLoading] = useState(false);
  const [downloading, setDownloading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const objectUrlRef = useRef<string | null>(null);

  const labelColor = useColorModeValue("gray.700", "#ffffff");
  // react-icons 的 color 不经 Chakra 解析，必须用真实色值（token 在浅色下会退化成白色）
  const subLabelColor = useColorModeValue("#718096", "#ffffff");
  const modalBg = useColorModeValue("white", "#111111");
  const modalBorderColor = useColorModeValue("gray.200", "#333333");
  const previewBg = useColorModeValue("gray.100", "#1e1e1e");
  const previewBorder = useColorModeValue("gray.200", "#333333");

  // 该主类别是否支持子类别 type
  const typeless =
    category === "" ||
    RANDOM_IMAGE_TYPELESS_CATEGORIES.includes(category) ||
    !(category in RANDOM_IMAGE_TYPE_MAP);

  // 主类别下拉选项（空值 = 全局随机），显示为「英文值（中文标注）」
  const categoryOptions = [
    { value: "", label: t("home.randomImage.anyCategory") || "全局随机" },
    ...RANDOM_IMAGE_CATEGORIES.map((c) => ({
      value: c,
      label: `${c}（${RANDOM_IMAGE_CATEGORY_LABELS[c] ?? c}）`,
    })),
  ];

  // 子类别下拉选项（根据主类别动态生成），显示为「英文值（中文标注）」
  const typeOptions = typeless
    ? []
    : [
        { value: "", label: t("home.randomImage.anyType") || "不指定" },
        ...RANDOM_IMAGE_TYPE_MAP[category].map((tp) => ({
          value: tp,
          label: `${tp}（${RANDOM_IMAGE_TYPE_LABELS[tp] ?? tp}）`,
        })),
      ];

  const releaseObjectUrl = useCallback(() => {
    if (objectUrlRef.current) {
      URL.revokeObjectURL(objectUrlRef.current);
      objectUrlRef.current = null;
    }
  }, []);

  const loadImage = useCallback(
    async (cat: string, tp: string) => {
      setLoading(true);
      setError(null);
      try {
        const raw = await fetchRandomImageRaw(
          cat ? cat : undefined,
          tp && !(cat === "" || RANDOM_IMAGE_TYPELESS_CATEGORIES.includes(cat)) ? tp : undefined
        );
        releaseObjectUrl();
        objectUrlRef.current = URL.createObjectURL(base64ToBlob(raw));
        setImageUrl(objectUrlRef.current);
        setBase64(raw);
      } catch (e) {
        setImageUrl(null);
        setBase64("");
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setLoading(false);
      }
    },
    [releaseObjectUrl]
  );

  // 打开弹窗时自动生成一张默认（全局随机）图片
  useEffect(() => {
    if (isOpen) {
      setCategory("");
      setImageType("");
      loadImage("", "");
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [isOpen]);

  // 关闭时释放资源
  useEffect(() => {
    return () => releaseObjectUrl();
  }, [releaseObjectUrl]);

  // 切换主类别时，重置不兼容的子类别
  const handleCategoryChange = (value: string) => {
    setCategory(value);
    if (value !== category) {
      setImageType("");
    }
  };

  const handleGenerate = () => {
    loadImage(category, imageType);
  };

  const handleDownload = async () => {
    if (!base64) {
      toast({
        title: t("home.randomImage.downloadFailed") || "图片保存失败",
        description: t("home.randomImage.noImageYet") || "还没有可下载的图片",
        status: "warning",
        duration: 3000,
        isClosable: true,
      });
      return;
    }
    setDownloading(true);
    try {
      const stamp = new Date().toISOString().slice(0, 19).replace(/[:T]/g, "-");
      const path = await save({
        filters: [{ name: "JPEG 图片", extensions: ["jpg", "jpeg"] }],
        title: t("home.randomImage.downloadTitle") || "保存随机图片",
        defaultPath: `nexbox-random-image-${stamp}.jpg`,
      });
      if (!path) return; // 用户取消
      await saveRandomImage(base64, path);
      toast({
        title: t("home.randomImage.downloadSuccess") || "图片已保存",
        description: path,
        status: "success",
        duration: 3000,
        isClosable: true,
      });
    } catch (e) {
      toast({
        title: t("home.randomImage.downloadFailed") || "图片保存失败",
        description: e instanceof Error ? e.message : String(e),
        status: "error",
        duration: 4000,
        isClosable: true,
      });
    } finally {
      setDownloading(false);
    }
  };

  return (
    <Modal isOpen={isOpen} onClose={onClose} isCentered size="lg">
      <ModalOverlay />
      <ModalContent bg={modalBg} borderColor={modalBorderColor} borderRadius="xl">
        <ModalHeader color={labelColor} fontSize="lg" fontWeight="bold">
          {t("home.randomImage.modalTitle") || "随机图片"}
        </ModalHeader>
        <ModalCloseButton />

        <ModalBody>
          <VStack spacing={4} align="stretch">
            {/* 类别选择 */}
            <HStack spacing={4} align="center">
              <Text fontSize="sm" color={labelColor} fontWeight="medium" w="56px" flexShrink={0}>
                {t("home.randomImage.categoryLabel") || "类别"}
              </Text>
              <CustomSelect value={category} onChange={handleCategoryChange} options={categoryOptions} width="240px" />
            </HStack>

            {/* 子类别选择 */}
            <HStack spacing={4} align="center">
              <Text fontSize="sm" color={labelColor} fontWeight="medium" w="56px" flexShrink={0}>
                {t("home.randomImage.typeLabel") || "子类别"}
              </Text>
              {typeless ? (
                <Text fontSize="sm" color={subLabelColor}>
                  {t("home.randomImage.typeNotSupported") || "当前类别不支持子类别"}
                </Text>
              ) : (
                <CustomSelect value={imageType} onChange={setImageType} options={typeOptions} width="240px" />
              )}
            </HStack>

            {/* 图片预览 */}
            <Box
              position="relative"
              borderRadius="lg"
              overflow="hidden"
              h="320px"
              bg={previewBg}
              border="1px solid"
              borderColor={previewBorder}
            >
              {loading ? (
                <Center h="100%">
                  <Spinner size="lg" />
                </Center>
              ) : imageUrl ? (
                <img
                  src={imageUrl}
                  alt={t("home.randomImage.previewAlt") || "随机图片预览"}
                  style={{ width: "100%", height: "100%", objectFit: "contain", display: "block" }}
                />
              ) : (
                <Center h="100%" px={4} textAlign="center">
                  <VStack spacing={2} align="center">
                    <FaImage size={28} color={subLabelColor} />
                    <Text fontSize="sm" color={subLabelColor} noOfLines={3}>
                      {error || (t("home.randomImage.error") || "图片加载失败")}
                    </Text>
                    {error && (
                      <IconButton
                        size="sm"
                        aria-label={t("home.randomImage.retry") || "重试"}
                        icon={<FaRotate />}
                        onClick={handleGenerate}
                      />
                    )}
                  </VStack>
                </Center>
              )}
            </Box>
          </VStack>
        </ModalBody>

        <ModalFooter>
          <HStack spacing={3} w="100%" justify="flex-end">
            <Button
              leftIcon={<FaDownload />}
              size="sm"
              variant="outline"
              borderColor={modalBorderColor}
              color={labelColor}
              onClick={handleDownload}
              isLoading={downloading}
              isDisabled={!imageUrl}
              _hover={{ borderColor: getActiveColor(), color: getActiveColor() }}
            >
              {t("home.randomImage.download") || "下载"}
            </Button>
            <Button
              leftIcon={<FaRotate />}
              size="sm"
              bg={getActiveColor()}
              color={getContrastTextColor()}
              border="1px solid"
              borderColor={getHoverColor()}
              onClick={handleGenerate}
              isLoading={loading}
              _hover={{ bg: getHoverColor() }}
              _active={{ bg: getHoverColor() }}
            >
              {t("home.randomImage.generate") || "生成"}
            </Button>
          </HStack>
        </ModalFooter>
      </ModalContent>
    </Modal>
  );
}

/** 将 base64 字符串解码为 Blob（默认按 JPEG 处理） */
function base64ToBlob(base64: string, mimeType = "image/jpeg"): Blob {
  const binary = atob(base64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return new Blob([bytes], { type: mimeType });
}
