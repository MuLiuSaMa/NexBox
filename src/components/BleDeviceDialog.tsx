import {
  Modal,
  ModalOverlay,
  ModalContent,
  ModalHeader,
  ModalBody,
  ModalCloseButton,
  VStack,
  HStack,
  Text,
  Button,
  Icon,
  Badge,
  Spinner,
  useColorModeValue,
  Divider,
} from "@chakra-ui/react";
import { useDynamicIsland } from "@/components/ui/dynamic-island";
import { useState, useEffect, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Bluetooth, Radio, Wifi, WifiOff } from "lucide-react";
import { useThemeColor } from "@/contexts/theme-color-context";

interface BleDeviceInfo {
  address: number;
  address_str: string;
  name: string;
}

type BleConnectionStatus = "Disconnected" | "Scanning" | "Connecting" | "Connected";

interface HeartRateData {
  heart_rate: number | null;
  device_name: string | null;
  connection_status: BleConnectionStatus;
}

interface BleDeviceDialogProps {
  isOpen: boolean;
  onClose: () => void;
}

export function BleDeviceDialog({ isOpen, onClose }: BleDeviceDialogProps) {
  const [devices, setDevices] = useState<BleDeviceInfo[]>([]);
  const [isScanning, setIsScanning] = useState(false);
  const [connectingAddress, setConnectingAddress] = useState<number | null>(null);
  const [heartRateData, setHeartRateData] = useState<HeartRateData>({
    heart_rate: null,
    device_name: null,
    connection_status: "Disconnected",
  });

  const textColor = useColorModeValue("gray.800", "#ffffff");
  const subTextColor = useColorModeValue("gray.500", "#ffffff");
  const borderColor = useColorModeValue("gray.200", "#333333");
  const cardBg = useColorModeValue("gray.50", "#1a1a1a");
  const toast = useDynamicIsland("bluetooth");
  const { getActiveColor } = useThemeColor();

  const refreshStatus = useCallback(async () => {
    try {
      const data = await invoke<HeartRateData>("get_heart_rate_data");
      setHeartRateData(data);
    } catch {
      // ignore
    }
  }, []);

  useEffect(() => {
    if (isOpen) {
      refreshStatus();
    }
  }, [isOpen, refreshStatus]);

  const handleScan = async () => {
    setIsScanning(true);
    setDevices([]);
    try {
      const result = await invoke<BleDeviceInfo[]>("scan_ble_devices");
      setDevices(result);
      if (result.length === 0) {
        toast({
          title: "未发现设备",
          description: "请确保手环/手表已开启并在系统蓝牙中已配对",
          status: "warning",
          duration: 3000,
          isClosable: true,
        });
      }
    } catch (err) {
      toast({
        title: "扫描失败",
        description: String(err),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    } finally {
      setIsScanning(false);
    }
  };

  const handleConnect = async (address: number, name: string) => {
    setConnectingAddress(address);
    try {
      // 小米设备使用纯广播监听（免配对、免连接）
      const isXiaomi = name.toLowerCase().includes("mi band")
        || name.toLowerCase().includes("miband")
        || name.toLowerCase().includes("xiaomi smart band")
        || name.toLowerCase().includes("mi smart band");

      if (isXiaomi) {
        await invoke("start_advert_hr_listen", { address });
      } else {
        await invoke("connect_ble_device", { address });
      }

      toast({
        title: "连接成功",
        status: "success",
        duration: 2000,
        isClosable: true,
      });
      await refreshStatus();
    } catch (err) {
      toast({
        title: "连接失败",
        description: String(err),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    } finally {
      setConnectingAddress(null);
    }
  };

  const handleDisconnect = async () => {
    try {
      await invoke("disconnect_ble_device");
      setHeartRateData({
        heart_rate: null,
        device_name: null,
        connection_status: "Disconnected",
      });
      toast({
        title: "已断开连接",
        status: "info",
        duration: 2000,
        isClosable: true,
      });
    } catch (err) {
      toast({
        title: "断开失败",
        description: String(err),
        status: "error",
        duration: 3000,
        isClosable: true,
      });
    }
  };

  const isConnected = heartRateData.connection_status === "Connected";

  return (
    <Modal isOpen={isOpen} onClose={onClose} size="md" isCentered>
      <ModalOverlay />
      <ModalContent bg={useColorModeValue("white", "#111111")} border="1px solid" borderColor={borderColor}>
        <ModalHeader color={textColor}>
          <HStack>
            <Icon as={Bluetooth} color={getActiveColor()} />
            <Text>BLE 心率设备</Text>
          </HStack>
        </ModalHeader>
        <ModalCloseButton color={subTextColor} />
        <ModalBody pb={6}>
          <VStack align="stretch" spacing={4}>
            {/* 连接状态 */}
            {isConnected ? (
              <VStack
                align="stretch"
                p={4}
                bg={cardBg}
                borderRadius="lg"
                border="1px solid"
                borderColor={getActiveColor()}
                spacing={3}
              >
                <HStack justify="space-between">
                  <HStack>
                    <Icon as={Wifi} color="green.400" />
                    <Text color={textColor} fontWeight="medium">
                      {heartRateData.device_name || "已连接设备"}
                    </Text>
                  </HStack>
                  <Badge colorScheme="green">已连接</Badge>
                </HStack>
                <Text color={getActiveColor()} fontSize="3xl" fontWeight="bold" textAlign="center">
                  {heartRateData.heart_rate !== null ? `${heartRateData.heart_rate} BPM` : "-- BPM"}
                </Text>
                <Button
                  size="sm"
                  variant="outline"
                  colorScheme="red"
                  leftIcon={<WifiOff size={14} />}
                  onClick={handleDisconnect}
                >
                  断开连接
                </Button>
              </VStack>
            ) : (
              <>
                <Button
                  leftIcon={isScanning ? <Spinner size="sm" /> : <Radio size={16} />}
                  colorScheme="blue"
                  onClick={handleScan}
                  isLoading={isScanning}
                  loadingText="扫描中..."
                >
                  扫描设备
                </Button>

                {isScanning && (
                  <Text fontSize="sm" color={subTextColor} textAlign="center">
                    正在扫描附近的 BLE 设备... (5秒)
                  </Text>
                )}

                {devices.length > 0 && (
                  <>
                    <Divider borderColor={borderColor} />
                    <Text fontSize="sm" fontWeight="medium" color={subTextColor}>
                      发现 {devices.length} 个设备
                    </Text>
                    <VStack align="stretch" spacing={2} maxH="200px" overflowY="auto">
                      {devices.map((device) => (
                        <HStack
                          key={device.address}
                          p={3}
                          bg={cardBg}
                          borderRadius="md"
                          border="1px solid"
                          borderColor={borderColor}
                          justify="space-between"
                        >
                          <VStack align="start" spacing={0}>
                            <Text color={textColor} fontSize="sm" fontWeight="medium">
                              {device.name}
                            </Text>
                            <Text color={subTextColor} fontSize="xs">
                              {device.address_str}
                            </Text>
                          </VStack>
                          <Button
                            size="xs"
                            data-corner="round"
                            colorScheme="blue"
                            isLoading={connectingAddress === device.address}
                            onClick={() => handleConnect(device.address, device.name)}
                          >
                            连接
                          </Button>
                        </HStack>
                      ))}
                    </VStack>
                  </>
                )}

                {!isScanning && devices.length === 0 && (
                  <Text fontSize="sm" color={subTextColor} textAlign="center">
                    点击"扫描设备"查找附近的 BLE 心率设备
                  </Text>
                )}
              </>
            )}
          </VStack>
        </ModalBody>
      </ModalContent>
    </Modal>
  );
}