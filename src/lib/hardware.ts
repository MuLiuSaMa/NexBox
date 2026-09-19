
import { invoke } from "@tauri-apps/api/core";

export enum GpuVendor {
  NVIDIA = "NVIDIA",
  AMD = "AMD",
  Intel = "Intel",
  Unknown = "Unknown",
}

export interface CpuInfo {
  name: string;
  manufacturer: string;
  cores: number;
  threads: number;
  max_clock_speed: number;
  l2_cache_size: number;
  l3_cache_size: number;
  load_percentage: number | null;
  architecture: string;
  socket: string;
  l2_cache_speed: number | null;
  l3_cache_speed: number | null;
  current_clock_speed: number | null;
  ext_clock: number | null;
  processor_id: string;
  family: number;
  stepping: string;
  revision: string;
  enabled_cores: number | null;
  voltage_caps: string | null;
}

export interface GpuInfo {
  name: string;
  vendor: GpuVendor;
  /** 显存总量（GB）；获取不到时为 null（如核显），前端不显示显存项 */
  memory_gb: number | null;
  driver_version: string;
  temperature: number | null;
  usage: number | null;
  video_processor: string;
  adapter_compatibility: string;
  driver_date: string;
  installed_drivers: string;
  video_mode: string;
  resolution_width: number | null;
  resolution_height: number | null;
  refresh_rate: number | null;
  device_id: string;
  pnp_device_id: string;
  status: string;
  inf_filename: string;
  video_architecture: string | null;
  video_memory_type: string | null;
}

export interface MemoryInfo {
  manufacturer: string;
  part_number: string;
  capacity_gb: number;
  speed_mhz: number;
  bank_label: string;
  form_factor: string;
  memory_type: string;
  configured_clock_speed: number | null;
  configured_voltage: number | null;
  data_width: number | null;
  total_width: number | null;
  serial_number: string;
  type_detail: string;
}

export interface SoundCardInfo {
  name: string;
  manufacturer: string;
  status: string;
  device_id: string;
  pnp_device_id: string;
}

export interface NetworkCardInfo {
  name: string;
  manufacturer: string;
  adapter_type: string;
  mac_address: string;
  speed_mbps: number;
  connection_name: string;
  service_name: string;
  index: number;
  max_speed: number | null;
  guid: string;
}

export interface MotherboardInfo {
  product: string;
  manufacturer: string;
  serial_number: string;
  version: string;
  bios_vendor: string;
  bios_version: string;
  bios_release_date: string;
  system_manufacturer: string;
  system_model: string;
  system_type: string;
  chassis_type: string;
}

export interface DiskDetailInfo {
  model: string;
  size_gb: number;
  interface_type: string;
  serial_number: string;
  firmware_revision: string;
  media_type: string;
  bytes_per_sector: number | null;
  partitions: number;
  status: string;
  is_ssd: boolean;
}

export interface MonitorInfo {
  name: string;
  manufacturer: string;
  screen_width: number | null;
  screen_height: number | null;
  refresh_rate: number | null;
  pnp_device_id: string;
  status: string;
  availability: number | null;
  diagonal_inches?: number | null;
  is_primary?: boolean | null;
}

export interface HardwareInfo {
  cpu: CpuInfo;
  gpu: GpuInfo[];
  memory: MemoryInfo[];
  motherboard: MotherboardInfo;
  disk: DiskDetailInfo[];
  sound_card: SoundCardInfo[];
  network_card: NetworkCardInfo[];
  monitor: MonitorInfo[];
}

export async function getHardwareInfo(): Promise<HardwareInfo> {
  return await invoke<HardwareInfo>("get_hardware");
}
