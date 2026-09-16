import {
  PlatformAdapter,
  HardwareInfo,
  SoftwareInfo,
  NetworkInfo,
  JAMESInfo,
  CPUInfo,
  RAMInfo,
  GPUInfo,
  NPUInfo,
  MotherboardInfo,
  BIOSInfo,
  StorageInfo,
  PartitionInfo,
  MonitorInfo,
  AudioInfo,
  MicrophoneInfo,
  CameraInfo,
  USBInfo,
  BluetoothInfo,
  BluetoothAdapterInfo,
  BluetoothDeviceInfo,
  NetworkAdapterInfo,
  InstalledProgram,
  RunningService,
  DeveloperTool,
  Runtime,
  Browser,
  ContainerRuntime,
  WSLInfo,
  WSLDistribution,
  AIRuntime,
  LocalModel,
  NetworkInterfaceInfo,
  IPConfigInfo,
  RouteInfo,
  DNSInfo,
  LocalIPInfo,
  GatewayInfo,
  MDNSService,
  LocalDevice,
  GitStatus,
  JAMESService,
} from '../core/interfaces';
import { BaseAdapter } from './base';
import * as child_process from 'child_process';
import { promisify } from 'util';
import * as fs from 'fs';
import * as path from 'path';

const rawExec = promisify(child_process.exec);
/**
 * Central command runner. Adds `-NoProfile -NonInteractive` to every
 * powershell call: machine PS profiles intermittently prepend banner text
 * to stdout ("Build Acceleration Profile Loaded..."), which breaks
 * JSON.parse downstream. Belt & suspenders with psJson() below.
 */
const exec = (cmd: string) =>
  rawExec(
    cmd.startsWith('powershell ')
      ? cmd.replace('powershell ', 'powershell -NoProfile -NonInteractive ')
      : cmd,
    { timeout: 60000 }
  );

/**
 * Parse PowerShell ConvertTo-Json output robustly: slice off any banner
 * prefix (first { or [) and always return an array (PS collapses
 * single-element results to a bare object, silently dropping data
 * in Array.isArray-guarded callers).
 */
function psJson(stdout: string): any[] {
  const start = stdout.search(/[{[]/);
  const text = start > 0 ? stdout.slice(start) : stdout;
  const parsed = JSON.parse(text);
  if (Array.isArray(parsed)) return parsed;
  return parsed === null || parsed === undefined ? [] : [parsed];
}

/** Quote-aware CSV line splitter (wmic /format:csv fields contain commas). */
function splitCsvLine(line: string): string[] {
  const out: string[] = [];
  let cur = '';
  let quoted = false;
  for (let i = 0; i < line.length; i++) {
    const c = line[i];
    if (c === '"') {
      if (quoted && line[i + 1] === '"') {
        cur += '"';
        i++;
      } else {
        quoted = !quoted;
      }
    } else if (c === ',' && !quoted) {
      out.push(cur);
      cur = '';
    } else {
      cur += c;
    }
  }
  out.push(cur);
  return out;
}

/** Parse wmic /format:csv output into header-keyed rows. */
function parseCsv(stdout: string): Array<Record<string, string>> {
  const lines = stdout.split(/\r?\n/).map(l => l.trim()).filter(l => l.length > 0);
  if (lines.length < 2) return [];
  const headers = splitCsvLine(lines[0]);
  return lines.slice(1).map(line => {
    const values = splitCsvLine(line);
    const row: Record<string, string> = {};
    headers.forEach((h, j) => {
      row[h.trim()] = (values[j] ?? '').trim();
    });
    return row;
  });
}

/** SMBIOS memory-device type -> human name (common values only). */
function smbiosMemoryTypeName(t: number): string | undefined {
  switch (t) {
    case 18:
      return 'DDR2';
    case 19:
      return 'DDR2 FB-DIMM';
    case 24:
      return 'DDR3';
    case 26:
      return 'DDR4';
    case 34:
      return 'DDR5';
    default:
      return undefined;
  }
}

export class WindowsAdapter extends BaseAdapter {
  readonly platform = 'windows' as const;

  /**
   * Locate the JAMES installation root: $JAMES_HOME first, then an upward
   * marker search (core/Cargo.toml) from the working directory, then the
   * legacy S:\JAMES default with low confidence. Never hard-fail: callers
   * degrade to empty results with an honest source tag.
   */
  private jamesRoot(): { path: string; confidence: number; source: string } {
    const fromEnv = process.env.JAMES_HOME;
    if (fromEnv && fs.existsSync(fromEnv)) {
      return { path: fromEnv, confidence: 1.0, source: 'env:JAMES_HOME' };
    }
    let dir = path.resolve(process.cwd());
    for (let i = 0; i < 8; i++) {
      if (fs.existsSync(path.join(dir, 'core', 'Cargo.toml'))) {
        return { path: dir, confidence: 0.9, source: 'marker-search' };
      }
      const parent = path.dirname(dir);
      if (parent === dir) break;
      dir = parent;
    }
    const legacy = 'S:\\JAMES';
    if (fs.existsSync(legacy)) {
      return { path: legacy, confidence: 0.3, source: 'legacy-default' };
    }
    return { path: process.cwd(), confidence: 0.2, source: 'cwd-fallback' };
  }

  async detectHardware(): Promise<HardwareInfo> {
    const [cpu, ram, gpu, npu, motherboard, bios, storage, partitions, monitors, audio, microphones, cameras, usb, bluetooth, network_adapters] = await Promise.all([
      this.detectCPU(),
      this.detectRAM(),
      this.detectGPU(),
      this.detectNPU(),
      this.detectMotherboard(),
      this.detectBIOS(),
      this.detectStorage(),
      this.detectPartitions(),
      this.detectMonitors(),
      this.detectAudio(),
      this.detectMicrophones(),
      this.detectCameras(),
      this.detectUSB(),
      this.detectBluetooth(),
      this.detectNetworkAdapters(),
    ]);

    return {
      cpu,
      ram,
      gpu,
      npu,
      motherboard,
      bios,
      storage,
      partitions,
      monitors,
      audio,
      microphones,
      cameras,
      usb,
      bluetooth,
      network_adapters,
    };
  }

  async detectSoftware(): Promise<SoftwareInfo> {
    const [installed_programs, running_services, developer_tools, runtimes, browsers, containers, wsl, ai_runtimes, local_models] = await Promise.all([
      this.detectInstalledPrograms(),
      this.detectRunningServices(),
      this.detectDeveloperTools(),
      this.detectRuntimes(),
      this.detectBrowsers(),
      this.detectContainers(),
      this.detectWSL(),
      this.detectAIRuntimes(),
      this.detectLocalModels(),
    ]);

    return {
      installed_programs,
      running_services,
      developer_tools,
      runtimes,
      browsers,
      containers,
      wsl,
      ai_runtimes,
      local_models,
    };
  }

  async detectNetwork(): Promise<NetworkInfo> {
    const [interfaces, ip_config, routes, dns, local_ips, gateway, mdns_services, local_devices] = await Promise.all([
      this.detectNetworkInterfaces(),
      this.detectIPConfig(),
      this.detectRoutes(),
      this.detectDNS(),
      this.detectLocalIPs(),
      this.detectGateway(),
      this.detectMDNSServices(),
      this.detectLocalDevices(),
    ]);

    return {
      interfaces,
      ip_config,
      routes,
      dns,
      local_ips,
      gateway,
      mdns_services,
      local_devices,
    };
  }

  async detectJAMES(): Promise<JAMESInfo> {
    const [modules, plugins, config, version, git_status, services] = await Promise.all([
      this.detectJAMESModules(),
      this.detectJAMESPlugins(),
      this.detectJAMESConfig(),
      this.detectJAMESVersion(),
      this.detectGitStatus(),
      this.detectJAMESServices(),
    ]);

    return {
      modules,
      plugins,
      config,
      version,
      git_status,
      services,
    };
  }

  private async detectCPU(): Promise<ReturnType<typeof this.createResult<CPUInfo>>> {
    try {
      const { stdout } = await exec('powershell "Get-CimInstance Win32_Processor | Select-Object Name,NumberOfCores,NumberOfLogicalProcessors,MaxClockSpeed,Manufacturer,AddressWidth,L2CacheSize,L3CacheSize | ConvertTo-Json"');
      const cpus = psJson(stdout);
      if (cpus.length > 0) {
        // Multi-socket: report the first processor (documented limitation).
        const data = cpus[0];
        return this.createResult({
          name: data.Name || 'Unknown',
          manufacturer: data.Manufacturer || 'Unknown',
          cores: parseInt(data.NumberOfCores || '0'),
          logical_processors: parseInt(data.NumberOfLogicalProcessors || '0'),
          max_clock_speed_mhz: parseInt(data.MaxClockSpeed || '0'),
          l2_cache_kb: parseInt(data.L2CacheSize || '0'),
          l3_cache_kb: parseInt(data.L3CacheSize || '0'),
          architecture: parseInt(data.AddressWidth || '64'),
        }, 'cim', 0.95);
      }
    } catch (e) {}
    return this.createUnknownResult<CPUInfo>('cim');
  }

  private async detectRAM(): Promise<ReturnType<typeof this.createResult<RAMInfo>>> {
    try {
      const { stdout } = await exec('powershell "Get-CimInstance Win32_PhysicalMemory | Select-Object Capacity,Speed,SMBIOSMemoryType | ConvertTo-Json"');
      const sticks = psJson(stdout);
      if (sticks.length > 0) {
        const totalBytes = sticks.reduce((sum: number, s: any) => sum + (parseInt(s.Capacity || '0') || 0), 0);
        const totalGB = Math.round(totalBytes / (1024**3) * 100) / 100;
        const speeds = sticks.map((s: any) => parseInt(s.Speed || '0') || 0).filter((v: number) => v > 0);
        const typeName = smbiosMemoryTypeName(parseInt(sticks[0].SMBIOSMemoryType || '0'));
        return this.createResult({
          total_gb: totalGB,
          speed_mhz: speeds.length > 0 ? Math.max(...speeds) : undefined,
          type: typeName,
        }, 'cim', 0.9);
      }
      // Fallback: total only, no DIMM detail.
      const { stdout: sysOut } = await exec('powershell "Get-CimInstance Win32_ComputerSystem | Select-Object TotalPhysicalMemory | ConvertTo-Json"');
      const sys = psJson(sysOut);
      if (sys.length > 0 && sys[0].TotalPhysicalMemory) {
        const totalGB = Math.round(parseInt(sys[0].TotalPhysicalMemory) / (1024**3) * 100) / 100;
        return this.createResult({ total_gb: totalGB }, 'cim', 0.7);
      }
    } catch (e) {}
    return this.createUnknownResult<RAMInfo>('cim');
  }

  private async detectGPU(): Promise<ReturnType<typeof this.createResult<GPUInfo[]>>> {
    try {
      const { stdout } = await exec('powershell "Get-CimInstance Win32_VideoController | Select-Object Name,AdapterRAM,DriverVersion,VideoProcessor,VideoModeDescription | ConvertTo-Json"');
      const controllers = psJson(stdout);
      const gpus: GPUInfo[] = [];
      for (const data of controllers) {
        if (data.Name) {
          gpus.push({
            name: data.Name,
            // NOTE: AdapterRAM is a 32-bit value and wraps on >4GB cards
            // (verified: 2080 Ti 11GB reports ~4GB). Corrected below via
            // nvidia-smi when available.
            adapter_ram_gb: data.AdapterRAM ? Math.round(parseInt(data.AdapterRAM) / (1024**3) * 100) / 100 : 0,
            driver_version: data.DriverVersion || 'Unknown',
            video_processor: data.VideoProcessor || data.Name,
            video_mode_description: data.VideoModeDescription,
          });
        }
      }
      // Prefer nvidia-smi VRAM over the wrapping WMI counter.
      try {
        const { stdout: smi } = await exec('nvidia-smi --query-gpu=name,memory.total --format=csv,noheader,nounits');
        for (const line of smi.split(/\r?\n/)) {
          const parts = line.split(',');
          if (parts.length >= 2) {
            const smiName = parts[0].trim();
            const smiMemMb = parseInt(parts[1].trim());
            const match = gpus.find(g => g.name === smiName)
              ?? gpus.find(g => smiName.includes(g.name) || g.name.includes(smiName));
            if (match && smiMemMb > 0) {
              match.adapter_ram_gb = Math.round(smiMemMb / 1024 * 100) / 100;
            }
          }
        }
      } catch {}
      if (gpus.length > 0) {
        return this.createResult(gpus, 'cim+nvidia-smi', 0.95);
      }
    } catch (e) {}
    return this.createResult([], 'cim', 0.5);
  }

  private async detectNPU(): Promise<ReturnType<typeof this.createResult<NPUInfo | null>>> {
    return this.createResult(null, 'wmic', 1.0, { reason: 'No NPU detected on this platform' });
  }

  private async detectMotherboard(): Promise<ReturnType<typeof this.createResult<MotherboardInfo>>> {
    try {
      const { stdout } = await exec('powershell "Get-CimInstance Win32_BaseBoard | Select-Object Manufacturer,Product,Version,SerialNumber | ConvertTo-Json"');
      const boards = psJson(stdout);
      if (boards.length > 0) {
        const data = boards[0];
        return this.createResult({
          manufacturer: data.Manufacturer || 'Unknown',
          product: data.Product || 'Unknown',
          version: data.Version || 'Unknown',
          serial_number: data.SerialNumber || undefined,
        }, 'cim', 0.9);
      }
    } catch (e) {}
    return this.createUnknownResult<MotherboardInfo>('cim');
  }

  private async detectBIOS(): Promise<ReturnType<typeof this.createResult<BIOSInfo>>> {
    try {
      const { stdout } = await exec('powershell "Get-CimInstance Win32_BIOS | Select-Object SMBIOSBIOSVersion,ReleaseDate,Manufacturer | ConvertTo-Json"');
      const entries = psJson(stdout);
      if (entries.length > 0) {
        const data = entries[0];
        return this.createResult({
          version: data.SMBIOSBIOSVersion || 'Unknown',
          release_date: data.ReleaseDate || 'Unknown',
          vendor: data.Manufacturer || 'Unknown',
        }, 'cim', 0.95);
      }
    } catch (e) {}
    return this.createUnknownResult<BIOSInfo>('cim');
  }

  private async detectStorage(): Promise<ReturnType<typeof this.createResult<StorageInfo[]>>> {
    try {
      const { stdout } = await exec('powershell "Get-CimInstance Win32_DiskDrive | Select-Object Model,Size,MediaType,InterfaceType,Partitions | ConvertTo-Json"');
      const drives = psJson(stdout);
      const storage: StorageInfo[] = [];
      for (const data of drives) {
        if (data.Model && data.Size) {
          storage.push({
            model: data.Model,
            size_gb: Math.round(parseInt(data.Size) / (1024**3) * 100) / 100,
            media_type: data.MediaType || 'Unknown',
            interface_type: data.InterfaceType || 'Unknown',
            partitions: parseInt(data.Partitions || '0'),
          });
        }
      }
      if (storage.length > 0) {
        return this.createResult(storage, 'cim', 0.9);
      }
    } catch (e) {}
    return this.createResult([], 'cim', 0.5);
  }

  private async detectPartitions(): Promise<ReturnType<typeof this.createResult<PartitionInfo[]>>> {
    try {
      const { stdout } = await exec('powershell "Get-Volume | Select-Object DriveLetter,FileSystemLabel,FileSystem,Size,SizeRemaining | ConvertTo-Json"');
      const volumes = psJson(stdout);
      const partitions: PartitionInfo[] = [];
      for (const data of volumes) {
        // Skip unmounted/system entries without a drive letter.
        if (data.DriveLetter) {
          partitions.push({
            drive_letter: data.DriveLetter,
            label: data.FileSystemLabel || null,
            file_system: data.FileSystem || null,
            capacity_gb: data.Size ? Math.round(parseInt(data.Size) / (1024**3) * 100) / 100 : 0,
            free_space_gb: data.SizeRemaining ? Math.round(parseInt(data.SizeRemaining) / (1024**3) * 100) / 100 : 0,
          });
        }
      }
      if (partitions.length > 0) {
        return this.createResult(partitions, 'powershell', 0.9);
      }
    } catch (e) {}
    return this.createResult([], 'powershell', 0.5);
  }

  private async detectMonitors(): Promise<ReturnType<typeof this.createResult<MonitorInfo[]>>> {
    try {
      // wmic kept deliberately: the CIM equivalent (WmiMonitorID) returns
      // encodedEDID blobs needing manual decoding.
      const { stdout } = await exec('wmic desktopmonitor get Name,ScreenWidth,ScreenHeight,MonitorManufacturer,MonitorType /format:csv');
      const rows = parseCsv(stdout);
      const monitors: MonitorInfo[] = [];
      for (const data of rows) {
        if (data.Name) {
          monitors.push({
            name: data.Name,
            screen_width: data.ScreenWidth ? parseInt(data.ScreenWidth) : undefined,
            screen_height: data.ScreenHeight ? parseInt(data.ScreenHeight) : undefined,
            manufacturer: data.MonitorManufacturer || undefined,
            monitor_type: data.MonitorType || undefined,
          });
        }
      }
      if (monitors.length > 0) {
        return this.createResult(monitors, 'wmic', 0.8);
      }
    } catch (e) {}
    return this.createResult([], 'wmic', 0.5);
  }

  private async detectAudio(): Promise<ReturnType<typeof this.createResult<AudioInfo[]>>> {
    try {
      const { stdout } = await exec('powershell "Get-CimInstance Win32_SoundDevice | Select-Object Name,Manufacturer,DeviceID,Status | ConvertTo-Json"');
      const devices = psJson(stdout);
      const audio: AudioInfo[] = [];
      for (const data of devices) {
        if (data.Name) {
          audio.push({
            name: data.Name,
            manufacturer: data.Manufacturer || 'Unknown',
            device_id: data.DeviceID || '',
            status: data.Status || 'Unknown',
          });
        }
      }
      if (audio.length > 0) {
        return this.createResult(audio, 'cim', 0.9);
      }
    } catch (e) {}
    return this.createResult([], 'cim', 0.5);
  }

  private async detectMicrophones(): Promise<ReturnType<typeof this.createResult<MicrophoneInfo[]>>> {
    try {
      const { stdout } = await exec('powershell "Get-PnpDevice -Class AudioEndpoint -Status OK | Where-Object {$_.FriendlyName -like \'*microphone*\' -or $_.FriendlyName -like \'*Mic*\' -or $_.FriendlyName -like \'*Headset*\' } | Select-Object FriendlyName,InstanceId,Status | ConvertTo-Json"');
      const devices = psJson(stdout);
      const mics: MicrophoneInfo[] = Array.isArray(devices) ? devices.map((d: any) => ({
        name: d.FriendlyName || 'Unknown',
        device_id: d.InstanceId || '',
        status: d.Status || 'Unknown',
      })) : [];
      return this.createResult(mics, 'powershell', 0.8);
    } catch (e) {}
    return this.createResult([], 'powershell', 0.5);
  }

  private async detectCameras(): Promise<ReturnType<typeof this.createResult<CameraInfo[]>>> {
    try {
      const { stdout } = await exec('powershell "Get-PnpDevice -Class Camera -Status OK | Select-Object FriendlyName,InstanceId,Status | ConvertTo-Json"');
      const devices = psJson(stdout);
      const cameras: CameraInfo[] = Array.isArray(devices) ? devices.map((d: any) => ({
        name: d.FriendlyName || 'Unknown',
        device_id: d.InstanceId || '',
        status: d.Status || 'Unknown',
      })) : [];
      return this.createResult(cameras, 'powershell', 0.8);
    } catch (e) {}
    return this.createResult([], 'powershell', 0.5);
  }

  private async detectUSB(): Promise<ReturnType<typeof this.createResult<USBInfo[]>>> {
    try {
      const { stdout } = await exec('powershell "Get-PnpDevice -Class USB | Select-Object FriendlyName,InstanceId,Status,Class,Manufacturer | ConvertTo-Json"');
      const devices = psJson(stdout);
      const usb: USBInfo[] = Array.isArray(devices) ? devices.map((d: any) => ({
        name: d.FriendlyName || 'Unknown',
        instance_id: d.InstanceId || '',
        status: d.Status || 'Unknown',
        class: d.Class || 'USB',
        manufacturer: d.Manufacturer,
      })) : [];
      return this.createResult(usb, 'powershell', 0.85);
    } catch (e) {}
    return this.createResult([], 'powershell', 0.5);
  }

  private async detectBluetooth(): Promise<ReturnType<typeof this.createResult<BluetoothInfo>>> {
    try {
      const { stdout } = await exec('powershell "Get-PnpDevice -Class Bluetooth | Select-Object FriendlyName,InstanceId,Status | ConvertTo-Json"');
      const rawDevices = psJson(stdout);
      const adapters: BluetoothAdapterInfo[] = Array.isArray(rawDevices) ? rawDevices
        .filter((d: any) => d.FriendlyName?.toLowerCase().includes('adapter') || d.FriendlyName?.toLowerCase().includes('radio'))
        .map((d: any) => ({
          name: d.FriendlyName || 'Bluetooth Adapter',
          address: '',
          status: d.Status || 'Unknown',
        })) : [];

      const { stdout: radioOut } = await exec('powershell "Get-PnpDevice -Class Radio | Select-Object FriendlyName,InstanceId,Status | ConvertTo-Json"');
      const radioDevices = psJson(radioOut);
      const btRadio = Array.isArray(radioDevices) ? radioDevices.filter((d: any) => d.FriendlyName?.toLowerCase().includes('bluetooth')) : [];
      btRadio.forEach((d: any) => {
        adapters.push({
          name: d.FriendlyName || 'Bluetooth Radio',
          address: '',
          status: d.Status || 'Unknown',
        });
      });

      const pairedOut = await exec('powershell "Get-PnpDevice -Class Bluetooth | Where-Object {$_.Status -eq \'OK\'} | Select-Object FriendlyName,InstanceId | ConvertTo-Json"');
      const pairedDevices = psJson(pairedOut.stdout);
      const devices: BluetoothDeviceInfo[] = Array.isArray(pairedDevices) ? pairedDevices.map((d: any) => ({
        name: d.FriendlyName || 'Unknown',
        address: '',
        paired: true,
        connected: d.Status === 'OK',
      })) : [];

      return this.createResult({
        available: adapters.length > 0,
        adapters,
        devices,
      }, 'powershell', 0.7, {
        // Get-PnpDevice exposes no MAC addresses; real BLE enumeration
        // needs WinRT APIs (documented future work, not silent data).
        address_source: 'not-available-via-pnp',
      });
    } catch (e) {}
    return this.createResult({ available: false, adapters: [], devices: [] }, 'powershell', 0.5);
  }

  private async detectNetworkAdapters(): Promise<ReturnType<typeof this.createResult<NetworkAdapterInfo[]>>> {
    try {
      const { stdout } = await exec('powershell "Get-NetAdapter | Where-Object {$_.Status -ne \'Disconnected\'} | Select-Object Name,InterfaceDescription,MacAddress,LinkSpeed,Status,IfIndex | ConvertTo-Json"');
      const adapters = psJson(stdout);
      const nets: NetworkAdapterInfo[] = Array.isArray(adapters) ? adapters.map((a: any) => ({
        name: a.Name,
        description: a.InterfaceDescription,
        mac_address: a.MacAddress,
        link_speed: a.LinkSpeed,
        status: a.Status,
        ipv4_addresses: [],
        ipv6_addresses: [],
        dns_servers: [],
      })) : [];
      return this.createResult(nets, 'powershell', 0.9);
    } catch (e) {}
    return this.createResult([], 'powershell', 0.5);
  }

  private async detectInstalledPrograms(): Promise<ReturnType<typeof this.createResult<InstalledProgram[]>>> {
    try {
      const { stdout } = await exec('powershell "Get-ItemProperty HKLM:\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\*, HKLM:\\Software\\Wow6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\* | Where-Object {$_.DisplayName -and $_.DisplayVersion} | Select-Object DisplayName,DisplayVersion,Publisher,InstallDate,InstallLocation | Sort-Object DisplayName | ConvertTo-Json"');
      const programs = psJson(stdout);
      const installed: InstalledProgram[] = Array.isArray(programs) ? programs.map((p: any) => ({
        name: p.DisplayName,
        version: p.DisplayVersion,
        publisher: p.Publisher,
        install_date: p.InstallDate,
        install_location: p.InstallLocation,
      })) : [];
      return this.createResult(installed, 'registry', 0.7);
    } catch (e) {}
    return this.createResult([], 'registry', 0.3);
  }

  private async detectRunningServices(): Promise<ReturnType<typeof this.createResult<RunningService[]>>> {
    try {
      const { stdout } = await exec('powershell "Get-Service | Where-Object {$_.Status -eq \'Running\'} | Select-Object Name,DisplayName,Status,StartType | ConvertTo-Json"');
      const services = psJson(stdout);
      const running: RunningService[] = Array.isArray(services) ? services.map((s: any) => ({
        name: s.Name,
        display_name: s.DisplayName,
        status: s.Status,
        start_type: s.StartType,
      })) : [];
      return this.createResult(running, 'powershell', 0.9);
    } catch (e) {}
    return this.createResult([], 'powershell', 0.5);
  }

  private async detectDeveloperTools(): Promise<ReturnType<typeof this.createResult<DeveloperTool[]>>> {
    const tools: DeveloperTool[] = [];
    const checks = [
      { name: 'git', cmd: 'git --version', parse: (out: string) => out.replace('git version ', '').trim() },
      { name: 'python', cmd: 'python --version', parse: (out: string) => out.replace('Python ', '').trim() },
      { name: 'node', cmd: 'node --version', parse: (out: string) => out.replace('v', '').trim() },
      { name: 'npm', cmd: 'npm --version', parse: (out: string) => out.trim() },
      { name: 'rust', cmd: 'rustc --version', parse: (out: string) => out.replace('rustc ', '').split(' ')[0] },
      { name: 'cargo', cmd: 'cargo --version', parse: (out: string) => out.replace('cargo ', '').split(' ')[0] },
      { name: 'docker', cmd: 'docker --version', parse: (out: string) => out.replace('Docker version ', '').split(',')[0] },
      { name: 'vscode', cmd: 'code --version', parse: (out: string) => out.split('\n')[0].trim() },
      { name: 'wt', cmd: 'wt --version', parse: (out: string) => out.trim() },
    ];

    for (const check of checks) {
      try {
        const { stdout } = await exec(check.cmd);
        tools.push({
          name: check.name,
          version: check.parse(stdout),
        });
      } catch (e) {}
    }

    return this.createResult(tools, 'cli', 0.9);
  }

  private async detectRuntimes(): Promise<ReturnType<typeof this.createResult<Runtime[]>>> {
    const runtimes: Runtime[] = [];
    const checks = [
      { name: '.NET', cmd: 'dotnet --version' },
      { name: 'Java', cmd: 'java -version 2>&1' },
      { name: 'Python', cmd: 'python --version' },
      { name: 'Node.js', cmd: 'node --version' },
    ];

    for (const check of checks) {
      try {
        const { stdout } = await exec(check.cmd);
        runtimes.push({
          name: check.name,
          version: stdout.trim().split('\n')[0],
        });
      } catch (e) {}
    }

    return this.createResult(runtimes, 'cli', 0.8);
  }

  private async detectBrowsers(): Promise<ReturnType<typeof this.createResult<Browser[]>>> {
    const browsers: Browser[] = [];
    const paths = [
      { name: 'Chrome', path: 'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe' },
      { name: 'Chrome', path: 'C:\\Program Files (x86)\\Google\\Chrome\\Application\\chrome.exe' },
      { name: 'Firefox', path: 'C:\\Program Files\\Mozilla Firefox\\firefox.exe' },
      { name: 'Edge', path: 'C:\\Program Files (x86)\\Microsoft\\Edge\\Application\\msedge.exe' },
      { name: 'Brave', path: 'C:\\Program Files\\BraveSoftware\\Brave-Browser\\Application\\brave.exe' },
      { name: 'Opera', path: 'C:\\Program Files\\Opera\\launcher.exe' },
    ];

    for (const browser of paths) {
      try {
        const { stdout } = await exec(`powershell "(Get-Item '${browser.path}').VersionInfo.FileVersion"`);
        if (stdout.trim()) {
          browsers.push({
            name: browser.name,
            version: stdout.trim(),
            path: browser.path,
          });
        }
      } catch (e) {}
    }

    return this.createResult(browsers, 'filesystem', 0.8);
  }

  private async detectContainers(): Promise<ReturnType<typeof this.createResult<ContainerRuntime[]>>> {
    const containers: ContainerRuntime[] = [];

    try {
      const { stdout } = await exec('docker version --format "{{.Server.Version}}"');
      if (stdout.trim()) {
        containers.push({
          name: 'docker',
          version: stdout.trim(),
          running: true,
        });
      }
    } catch (e) {
      // Absent binaries produce no entry (previous phantom
      // running:false entries polluted the inventory).
    }

    try {
      const { stdout } = await exec('podman version --format "{{.Version}}"');
      if (stdout.trim()) {
        containers.push({
          name: 'podman',
          version: stdout.trim(),
          running: true,
        });
      }
    } catch (e) {}

    return this.createResult(containers, 'cli', 0.9);
  }

  private async detectWSL(): Promise<ReturnType<typeof this.createResult<WSLInfo>>> {
    try {
      const { stdout: versionOut } = await exec('wsl --version');
      const versionMatch = versionOut.match(/WSL version: ([\d.]+)/);
      const version = versionMatch ? versionMatch[1] : 'unknown';

      const { stdout: listOut } = await exec('wsl --list --verbose');
      const lines = listOut.trim().split('\n').slice(1);
      const distributions: WSLDistribution[] = [];
      let defaultDistro: string | undefined;

      // Actual column order of `wsl --list --verbose`: NAME, STATE, VERSION
      // (previously parsed as NAME, VERSION, STATE — swapped values).
      for (const line of lines) {
        const parts = line.trim().split(/\s+/);
        if (parts.length >= 3) {
          const name = parts[0].replace('*', '');
          const isDefault = line.includes('*');
          if (isDefault) defaultDistro = name;
          distributions.push({
            name,
            version: parts[2] || 'unknown',
            state: parts[1] || 'unknown',
            is_default: isDefault,
          });
        }
      }

      return this.createResult({
        installed: true,
        version,
        distributions,
        default_distro: defaultDistro,
      }, 'wsl', 0.9);
    } catch (e) {
      return this.createResult({
        installed: false,
        distributions: [],
      }, 'wsl', 0.9);
    }
  }

  private async detectAIRuntimes(): Promise<ReturnType<typeof this.createResult<AIRuntime[]>>> {
    const runtimes: AIRuntime[] = [];

    try {
      const { stdout } = await exec('ollama --version');
      const version = stdout.replace('Warning: could not connect to a running Ollama instance\n', '').replace('Warning: client version is ', '').trim();
      runtimes.push({
        name: 'ollama',
        version,
        running: false,
        models: [],
      });
    } catch (e) {}

    try {
      const { stdout } = await exec('ollama list');
      const ollamaRuntime = runtimes.find(r => r.name === 'ollama');
      if (ollamaRuntime) {
        const lines = stdout.trim().split('\n').slice(1);
        ollamaRuntime.models = lines.map(l => l.split(/\s+/)[0]).filter(Boolean);
        ollamaRuntime.running = true;
      }
    } catch (e) {}

    return this.createResult(runtimes, 'cli', 0.8);
  }

  private async detectLocalModels(): Promise<ReturnType<typeof this.createResult<LocalModel[]>>> {
    const models: LocalModel[] = [];
    // Ollama keeps blobs content-addressed; loose .gguf/.bin files are an
    // additional signal, not the full picture (`ollama list` covers the
    // daemon case in detectAIRuntimes).
    const roots = [
      process.env.USERPROFILE ? path.join(process.env.USERPROFILE, '.ollama', 'models') : '',
      process.env.LOCALAPPDATA ? path.join(process.env.LOCALAPPDATA, 'Ollama', 'models') : '',
      process.env.PROGRAMDATA ? path.join(process.env.PROGRAMDATA, 'Ollama', 'models') : '',
    ].filter(p => p.length > 0);

    try {
      for (const modelsPath of roots) {
        if (!fs.existsSync(modelsPath)) continue;
        const files = fs.readdirSync(modelsPath);
        for (const file of files) {
          if (file.endsWith('.gguf') || file.endsWith('.bin')) {
            const full = path.join(modelsPath, file);
            const stats = fs.statSync(full);
            models.push({
              name: file.replace(/\.(gguf|bin)$/, ''),
              type: 'llm',
              size_gb: Math.round(stats.size / (1024**3) * 100) / 100,
              path: full,
              format: file.endsWith('.gguf') ? 'gguf' : 'bin',
            });
          }
        }
      }
    } catch (e) {}

    return this.createResult(models, 'filesystem', 0.7);
  }

  private async detectNetworkInterfaces(): Promise<ReturnType<typeof this.createResult<NetworkInterfaceInfo[]>>> {
    try {
      const { stdout } = await exec('powershell "Get-NetAdapter | Where-Object {$_.Status -ne \'Disconnected\'} | Select-Object Name,InterfaceDescription,MacAddress,LinkSpeed,Status,IfIndex | ConvertTo-Json"');
      const adapters = psJson(stdout);
      const interfaces: NetworkInterfaceInfo[] = Array.isArray(adapters) ? adapters.map((a: any) => ({
        name: a.Name,
        description: a.InterfaceDescription,
        mac_address: a.MacAddress,
        link_speed: a.LinkSpeed,
        status: a.Status,
        // PowerShell serializes IfIndex as lowercase `ifIndex`.
        index: a.IfIndex ?? a.ifIndex,
      })) : [];
      return this.createResult(interfaces, 'powershell', 0.9);
    } catch (e) {}
    return this.createResult([], 'powershell', 0.5);
  }

  private async detectIPConfig(): Promise<ReturnType<typeof this.createResult<IPConfigInfo[]>>> {
    try {
      const { stdout } = await exec('powershell "Get-NetIPConfiguration | Select-Object InterfaceAlias,IPv4Address,IPv6Address,DNSServer,Ipv4DefaultGateway | ConvertTo-Json"');
      const configs = psJson(stdout);
      const ipConfigs: IPConfigInfo[] = Array.isArray(configs) ? configs.map((c: any) => ({
        interface_alias: c.InterfaceAlias,
        ipv4_addresses: c.IPv4Address ? (Array.isArray(c.IPv4Address) ? c.IPv4Address.map((a: any) => a.IPAddress) : [c.IPv4Address.IPAddress]) : [],
        ipv6_addresses: c.IPv6Address ? (Array.isArray(c.IPv6Address) ? c.IPv6Address.map((a: any) => a.IPAddress) : [c.IPv6Address.IPAddress]) : [],
        dns_servers: c.DNSServer ? (Array.isArray(c.DNSServer) ? c.DNSServer.map((d: any) => d.ServerAddresses).flat() : c.DNSServer.ServerAddresses) : [],
        gateway: c.Ipv4DefaultGateway?.NextHop,
        prefix_lengths: c.IPv4Address ? (Array.isArray(c.IPv4Address) ? c.IPv4Address.map((a: any) => a.PrefixLength) : [c.IPv4Address.PrefixLength]) : [],
      })) : [];
      return this.createResult(ipConfigs, 'powershell', 0.9);
    } catch (e) {}
    return this.createResult([], 'powershell', 0.5);
  }

  private async detectRoutes(): Promise<ReturnType<typeof this.createResult<RouteInfo[]>>> {
    try {
      const { stdout } = await exec('powershell "Get-NetRoute -AddressFamily IPv4 | Select-Object DestinationPrefix,NextHop,InterfaceAlias,RouteMetric | ConvertTo-Json"');
      const routes = psJson(stdout);
      const routeInfo: RouteInfo[] = Array.isArray(routes) ? routes.map((r: any) => ({
        destination: r.DestinationPrefix,
        next_hop: r.NextHop,
        interface_alias: r.InterfaceAlias,
        metric: r.RouteMetric,
      })) : [];
      return this.createResult(routeInfo, 'powershell', 0.8);
    } catch (e) {}
    return this.createResult([], 'powershell', 0.5);
  }

  private async detectDNS(): Promise<ReturnType<typeof this.createResult<DNSInfo[]>>> {
    try {
      const { stdout } = await exec('powershell "Get-DnsClientServerAddress -AddressFamily IPv4 | Where-Object {$_.ServerAddresses.Count -gt 0} | Select-Object InterfaceAlias,ServerAddresses | ConvertTo-Json"');
      const dns = psJson(stdout);
      const dnsInfo: DNSInfo[] = Array.isArray(dns) ? dns.map((d: any) => ({
        interface_alias: d.InterfaceAlias,
        server_addresses: d.ServerAddresses,
      })) : [];
      return this.createResult(dnsInfo, 'powershell', 0.9);
    } catch (e) {}
    return this.createResult([], 'powershell', 0.5);
  }

  private async detectLocalIPs(): Promise<ReturnType<typeof this.createResult<LocalIPInfo[]>>> {
    try {
      const { stdout } = await exec('powershell "Get-NetIPAddress -AddressFamily IPv4 | Where-Object {$_.IPAddress -notmatch \'^127\\.|^169\\.254\\.\'} | Select-Object IPAddress,InterfaceAlias,PrefixLength | ConvertTo-Json"');
      const ips = psJson(stdout);
      const localIPs: LocalIPInfo[] = Array.isArray(ips) ? ips.map((i: any) => ({
        ip_address: i.IPAddress,
        interface_alias: i.InterfaceAlias,
        prefix_length: i.PrefixLength,
      })) : [];
      return this.createResult(localIPs, 'powershell', 0.9);
    } catch (e) {}
    return this.createResult([], 'powershell', 0.5);
  }

  private async detectGateway(): Promise<ReturnType<typeof this.createResult<GatewayInfo>>> {
    try {
      const { stdout } = await exec('powershell "Get-NetRoute -AddressFamily IPv4 | Where-Object {$_.DestinationPrefix -eq \'0.0.0.0/0\'} | Select-Object NextHop,InterfaceAlias | ConvertTo-Json"');
      const routes = psJson(stdout);
      const route = Array.isArray(routes) ? routes[0] : routes;
      if (route?.NextHop) {
        return this.createResult({
          next_hop: route.NextHop,
          interface_alias: route.InterfaceAlias,
        }, 'powershell', 0.95);
      }
    } catch (e) {}
    return this.createUnknownResult<GatewayInfo>('powershell');
  }

  private async detectMDNSServices(): Promise<ReturnType<typeof this.createResult<MDNSService[]>>> {
    return this.createResult([], 'mdns', 0.3, { reason: 'mDNS scanning not implemented' });
  }

  private async detectLocalDevices(): Promise<ReturnType<typeof this.createResult<LocalDevice[]>>> {
    return this.createResult([], 'network', 0.3, { reason: 'Local device scanning not implemented' });
  }

  private async detectJAMESModules(): Promise<ReturnType<typeof this.createResult<string[]>>> {
    try {
      const root = this.jamesRoot();
      const dirs = fs.readdirSync(root.path, { withFileTypes: true })
        .filter(d => d.isDirectory() && !d.name.startsWith('.'))
        .map(d => d.name);
      return this.createResult(dirs, `filesystem:${root.source}`, root.confidence);
    } catch (e) {}
    return this.createResult([], 'filesystem', 0.5);
  }

  private async detectJAMESPlugins(): Promise<ReturnType<typeof this.createResult<string[]>>> {
    try {
      const root = this.jamesRoot();
      const pluginsPath = path.join(root.path, 'plugins');
      if (fs.existsSync(pluginsPath)) {
        const dirs = fs.readdirSync(pluginsPath, { withFileTypes: true })
          .filter(d => d.isDirectory())
          .map(d => d.name);
        return this.createResult(dirs, `filesystem:${root.source}`, root.confidence);
      }
    } catch (e) {}
    return this.createResult([], 'filesystem', 0.5);
  }

  private async detectJAMESConfig(): Promise<ReturnType<typeof this.createResult<Record<string, unknown>>>> {
    try {
      const root = this.jamesRoot();
      const configPath = path.join(root.path, '.james', 'config');
      if (fs.existsSync(configPath)) {
        const files = fs.readdirSync(configPath);
        const config: Record<string, unknown> = {};
        for (const file of files) {
          if (file.endsWith('.json')) {
            const raw = fs.readFileSync(path.join(configPath, file), 'utf-8');
            const text = raw.charCodeAt(0) === 0xfeff ? raw.slice(1) : raw;
            config[file.replace('.json', '')] = JSON.parse(text);
          }
        }
        return this.createResult(config, `filesystem:${root.source}`, root.confidence);
      }
    } catch (e) {}
    return this.createResult({}, 'filesystem', 0.5);
  }

  private async detectJAMESVersion(): Promise<ReturnType<typeof this.createResult<string>>> {
    try {
      const root = this.jamesRoot();
      const versionPath = path.join(root.path, '.james', 'version');
      if (fs.existsSync(versionPath)) {
        const version = fs.readFileSync(versionPath, 'utf-8').trim();
        return this.createResult(version, 'filesystem', 1.0);
      }
      const pkgPath = path.join(root.path, 'package.json');
      if (fs.existsSync(pkgPath)) {
        const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf-8'));
        return this.createResult(pkg.version || '0.0.0', 'package.json', 0.9);
      }
    } catch (e) {}
    return this.createResult('0.0.0-dev', 'default', 0.5);
  }

  private async detectGitStatus(): Promise<ReturnType<typeof this.createResult<GitStatus>>> {
    try {
      const root = this.jamesRoot();
      const { stdout: branchOut } = await exec(`git -C "${root.path}" rev-parse --abbrev-ref HEAD`);
      const branch = branchOut.trim();

      const { stdout: commitOut } = await exec(`git -C "${root.path}" rev-parse HEAD`);
      const commit = commitOut.trim();

      const { stdout: statusOut } = await exec(`git -C "${root.path}" status --porcelain`);
      const statusLines = statusOut.trim().split('\n').filter(l => l.trim());
      const untracked_files: string[] = [];
      const modified_files: string[] = [];

      for (const line of statusLines) {
        const file = line.substring(3).trim();
        if (line.startsWith('??')) untracked_files.push(file);
        else if (line.startsWith(' M') || line.startsWith('M ')) modified_files.push(file);
      }

      return this.createResult({
        repo_exists: true,
        branch,
        commit,
        clean: statusLines.length === 0,
        untracked_files,
        modified_files,
      }, 'git', 0.95);
    } catch (e) {
      return this.createResult({
        repo_exists: false,
      }, 'git', 0.5);
    }
  }

  private async detectJAMESServices(): Promise<ReturnType<typeof this.createResult<JAMESService[]>>> {
    return this.createResult([], 'system', 0.5, { reason: 'No JAMES services running yet' });
  }
}