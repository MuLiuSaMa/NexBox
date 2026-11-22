import type { ComponentType } from "react";
import { Network, Zap } from "lucide-react";

export interface DnsPreset {
  id: string;
  name: string;
  primary: string;
  secondary: string;
  iconColor: string;
}

export const dnsPresets: DnsPreset[] = [
  { id: "alidns",   name: "阿里 DNS",    primary: "223.5.5.5",     secondary: "223.6.6.6",     iconColor: "#FF6A00" },
  { id: "dnspod",   name: "DNSPod",     primary: "119.29.29.29",  secondary: "119.28.28.28",   iconColor: "#007FFF" },
  { id: "114dns",   name: "114 DNS",    primary: "114.114.114.114", secondary: "114.114.115.115", iconColor: "#3182CE" },
  { id: "baidu",    name: "百度 DNS",    primary: "180.76.76.76",  secondary: "",               iconColor: "#DE2910" },
  { id: "google",   name: "Google DNS", primary: "8.8.8.8",      secondary: "8.8.4.4",        iconColor: "#4285F4" },
  { id: "cloudflare", name: "Cloudflare", primary: "1.1.1.1",     secondary: "1.0.0.1",        iconColor: "#F6821F" },
];

export interface NetworkOptimizerItem {
  id: string;
  stateKey: string;
  icon: ComponentType<{ size?: number; strokeWidth?: number }>;
  color: string;
  enableCmd: string;
  disableCmd: string;
  titleKey: string;
  descKey: string;
  requiresReboot: boolean;
}

export const networkOptimizerItems: NetworkOptimizerItem[] = [
  {
    id: "tcp-congestion",
    stateKey: "tcp_congestion_optimized",
    icon: Network,
    color: "#38A169",
    enableCmd: "set_tcp_congestion",
    disableCmd: "restore_tcp_congestion",
    titleKey: "networkOptimize.tcpCongestion.title",
    descKey: "networkOptimize.tcpCongestion.description",
    requiresReboot: false,
  },
  {
    id: "chimney-offload",
    stateKey: "chimney_offload",
    icon: Network,
    color: "#DD6B20",
    enableCmd: "set_tcp_chimney_off",
    disableCmd: "restore_tcp_chimney",
    titleKey: "networkOptimize.chimney.title",
    descKey: "networkOptimize.chimney.description",
    requiresReboot: false,
  },
  {
    id: "nagle-algorithm",
    stateKey: "nagle_optimized",
    icon: Network,
    color: "#805AD5",
    enableCmd: "set_nagle_optimization",
    disableCmd: "restore_nagle_optimization",
    titleKey: "networkOptimize.nagle.title",
    descKey: "networkOptimize.nagle.description",
    requiresReboot: false,
  },
  {
    id: "adapter-power",
    stateKey: "adapter_power_saving_off",
    icon: Zap,
    color: "#FF6B9D",
    enableCmd: "set_adapter_power_saving_off",
    disableCmd: "restore_adapter_power_saving",
    titleKey: "networkOptimize.adapterPower.title",
    descKey: "networkOptimize.adapterPower.description",
    requiresReboot: false,
  },
];
