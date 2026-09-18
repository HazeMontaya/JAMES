export interface DiscoveryResult<T> {
  value: T;
  source: string;
  detected_at: string;
  confidence: number;
  metadata?: Record<string, unknown>;
}

export interface Capability {
  id: string;
  name: string;
  category: string;
  detected: boolean;
  provider: string;
  version?: string;
  dependencies: string[];
  risk_level: 'low' | 'medium' | 'high' | 'critical';
  status: 'available' | 'unavailable' | 'degraded' | 'unknown';
  metadata?: Record<string, unknown>;
}

export interface HardwareInfo {
  cpu: DiscoveryResult<CPUInfo>;
  ram: DiscoveryResult<RAMInfo>;
  gpu: DiscoveryResult<GPUInfo[]>;
  npu: DiscoveryResult<NPUInfo | null>;
  motherboard: DiscoveryResult<MotherboardInfo>;
  bios: DiscoveryResult<BIOSInfo>;
  storage: DiscoveryResult<StorageInfo[]>;
  partitions: DiscoveryResult<PartitionInfo[]>;
  monitors: DiscoveryResult<MonitorInfo[]>;
  audio: DiscoveryResult<AudioInfo[]>;
  microphones: DiscoveryResult<MicrophoneInfo[]>;
  cameras: DiscoveryResult<CameraInfo[]>;
  usb: DiscoveryResult<USBInfo[]>;
  bluetooth: DiscoveryResult<BluetoothInfo>;
  network_adapters: DiscoveryResult<NetworkAdapterInfo[]>;
}

export interface CPUInfo {
  name: string;
  manufacturer: string;
  cores: number;
  logical_processors: number;
  max_clock_speed_mhz: number;
  l2_cache_kb?: number;
  l3_cache_kb?: number;
  architecture: number;
}

export interface RAMInfo {
  total_gb: number;
  available_gb?: number;
  speed_mhz?: number;
  type?: string;
}

export interface GPUInfo {
  name: string;
  adapter_ram_gb: number;
  driver_version: string;
  video_processor: string;
  video_mode_description?: string;
}

export interface NPUInfo {
  name: string;
  vendor: string;
  performance_tops?: number;
}

export interface MotherboardInfo {
  manufacturer: string;
  product: string;
  version: string;
  serial_number?: string;
}

export interface BIOSInfo {
  version: string;
  release_date: string;
  vendor: string;
}

export interface StorageInfo {
  model: string;
  size_gb: number;
  media_type: string;
  interface_type: string;
  partitions: number;
}

export interface PartitionInfo {
  drive_letter: string;
  label: string | null;
  file_system: string | null;
  capacity_gb: number;
  free_space_gb: number;
}

export interface MonitorInfo {
  name: string;
  screen_width?: number;
  screen_height?: number;
  manufacturer?: string;
  monitor_type?: string;
}

export interface AudioInfo {
  name: string;
  manufacturer: string;
  device_id: string;
  status: string;
}

export interface MicrophoneInfo {
  name: string;
  device_id: string;
  status: string;
}

export interface CameraInfo {
  name: string;
  device_id: string;
  status: string;
}

export interface USBInfo {
  name: string;
  instance_id: string;
  status: string;
  class: string;
  manufacturer?: string;
}

export interface BluetoothInfo {
  available: boolean;
  adapters: BluetoothAdapterInfo[];
  devices: BluetoothDeviceInfo[];
}

export interface BluetoothAdapterInfo {
  name: string;
  address: string;
  status: string;
}

export interface BluetoothDeviceInfo {
  name: string;
  address: string;
  paired: boolean;
  connected: boolean;
}

export interface NetworkAdapterInfo {
  name: string;
  description: string;
  mac_address: string;
  link_speed: string;
  status: string;
  ipv4_addresses: string[];
  ipv6_addresses: string[];
  gateway?: string;
  dns_servers: string[];
}

export interface SoftwareInfo {
  installed_programs: DiscoveryResult<InstalledProgram[]>;
  running_services: DiscoveryResult<RunningService[]>;
  developer_tools: DiscoveryResult<DeveloperTool[]>;
  runtimes: DiscoveryResult<Runtime[]>;
  browsers: DiscoveryResult<Browser[]>;
  containers: DiscoveryResult<ContainerRuntime[]>;
  wsl: DiscoveryResult<WSLInfo>;
  ai_runtimes: DiscoveryResult<AIRuntime[]>;
  local_models: DiscoveryResult<LocalModel[]>;
}

export interface InstalledProgram {
  name: string;
  version: string;
  publisher?: string;
  install_date?: string;
  install_location?: string;
}

export interface RunningService {
  name: string;
  display_name: string;
  status: string;
  start_type: string;
  service_type?: string;
}

export interface DeveloperTool {
  name: string;
  version: string;
  path?: string;
}

export interface Runtime {
  name: string;
  version: string;
  path?: string;
}

export interface Browser {
  name: string;
  version: string;
  path: string;
}

export interface ContainerRuntime {
  name: string;
  version: string;
  running: boolean;
}

export interface WSLInfo {
  installed: boolean;
  version?: string;
  distributions: WSLDistribution[];
  default_distro?: string;
}

export interface WSLDistribution {
  name: string;
  version: string;
  state: string;
  is_default: boolean;
}

export interface AIRuntime {
  name: string;
  version: string;
  running: boolean;
  models: string[];
}

export interface LocalModel {
  name: string;
  type: string;
  size_gb?: number;
  path?: string;
  format: string;
}

export interface NetworkInfo {
  interfaces: DiscoveryResult<NetworkInterfaceInfo[]>;
  ip_config: DiscoveryResult<IPConfigInfo[]>;
  routes: DiscoveryResult<RouteInfo[]>;
  dns: DiscoveryResult<DNSInfo[]>;
  local_ips: DiscoveryResult<LocalIPInfo[]>;
  gateway: DiscoveryResult<GatewayInfo>;
  mdns_services: DiscoveryResult<MDNSService[]>;
  local_devices: DiscoveryResult<LocalDevice[]>;
}

export interface NetworkInterfaceInfo {
  name: string;
  description: string;
  mac_address: string;
  link_speed: string;
  status: string;
  index: number;
}

export interface IPConfigInfo {
  interface_alias: string;
  ipv4_addresses: string[];
  ipv6_addresses: string[];
  dns_servers: string[];
  gateway?: string;
  prefix_lengths: number[];
}

export interface RouteInfo {
  destination: string;
  next_hop: string;
  interface_alias: string;
  metric: number;
}

export interface DNSInfo {
  interface_alias: string;
  server_addresses: string[];
}

export interface LocalIPInfo {
  ip_address: string;
  interface_alias: string;
  prefix_length: number;
}

export interface GatewayInfo {
  next_hop: string;
  interface_alias: string;
}

export interface MDNSService {
  name: string;
  type: string;
  host: string;
  port: number;
  txt?: Record<string, string>;
}

export interface LocalDevice {
  ip: string;
  hostname?: string;
  mac?: string;
  vendor?: string;
  open_ports?: number[];
  services?: string[];
}

export interface JAMESInfo {
  modules: DiscoveryResult<string[]>;
  plugins: DiscoveryResult<string[]>;
  config: DiscoveryResult<Record<string, unknown>>;
  version: DiscoveryResult<string>;
  git_status: DiscoveryResult<GitStatus>;
  services: DiscoveryResult<JAMESService[]>;
}

export interface GitStatus {
  repo_exists: boolean;
  branch?: string;
  commit?: string;
  clean?: boolean;
  untracked_files?: string[];
  modified_files?: string[];
}

export interface JAMESService {
  name: string;
  status: string;
  port?: number;
  pid?: number;
}

export interface PlatformAdapter {
  readonly platform: 'windows' | 'linux' | 'macos' | 'android' | 'ios';
  detectHardware(): Promise<HardwareInfo>;
  detectSoftware(): Promise<SoftwareInfo>;
  detectNetwork(): Promise<NetworkInfo>;
  detectJAMES(): Promise<JAMESInfo>;
  getCapabilities(hardware: HardwareInfo, software: SoftwareInfo, network: NetworkInfo): Capability[];
}

export interface Snapshot {
  id: string;
  timestamp: string;
  scan_mode: 'fast' | 'full';
  platform: string;
  hardware: HardwareInfo;
  software: SoftwareInfo;
  network: NetworkInfo;
  james: JAMESInfo;
  capabilities: Capability[];
  checksum: string;
}

export interface ChangeReport {
  timestamp: string;
  previous_snapshot_id: string;
  current_snapshot_id: string;
  changes: Change[];
  summary: ChangeSummary;
}

export interface Change {
  type: 'ADDED' | 'REMOVED' | 'MODIFIED';
  category: 'hardware' | 'software' | 'network' | 'james' | 'capability' | 'device';
  path: string;
  previous_value?: unknown;
  current_value?: unknown;
  severity: 'info' | 'warning' | 'critical';
}

export interface ChangeSummary {
  total_changes: number;
  added: number;
  removed: number;
  modified: number;
  by_category: Record<string, number>;
}

export interface ScanConfig {
  mode: 'fast' | 'full';
  timeout_seconds: number;
  parallel: boolean;
  include: string[];
  exclude: string[];
}

export interface DiscoveryConfig {
  version: string;
  scan: ScanConfig;
  adapters: {
    platform: 'auto' | 'windows' | 'linux' | 'macos' | 'android' | 'ios';
    windows: WindowsAdapterConfig;
    linux: LinuxAdapterConfig;
  };
  capability_detection: {
    enabled: boolean;
    rules_file: string;
    custom_rules: CustomCapabilityRule[];
  };
  snapshot: {
    directory: string;
    max_snapshots: number;
    compress: boolean;
    retention_days: number;
  };
  output: {
    inventory_dir: string;
    formats: ('json' | 'yaml' | 'markdown')[];
    pretty_print: boolean;
  };
  security: {
    no_credentials: boolean;
    no_passwords: boolean;
    no_cookies: boolean;
    no_external_scan: boolean;
    redact_sensitive: boolean;
  };
  logging: {
    level: 'debug' | 'info' | 'warn' | 'error';
    file: string;
    console: boolean;
  };
}

export interface WindowsAdapterConfig {
  use_wmi: boolean;
  use_powershell: boolean;
  use_registry: boolean;
  wmi_namespace: string;
}

export interface LinuxAdapterConfig {
  use_sysfs: boolean;
  use_proc: boolean;
  use_dmidecode: boolean;
  use_lshw: boolean;
}

export interface CustomCapabilityRule {
  id: string;
  name: string;
  category: string;
  condition: string;
  dependencies: string[];
  risk_level: 'low' | 'medium' | 'high' | 'critical';
  provider: string;
}