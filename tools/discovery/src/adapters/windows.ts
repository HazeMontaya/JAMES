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

const exec = promisify(child_process.exec);
const execSync = child_process.execSync;

export class WindowsAdapter extends BaseAdapter {
  readonly platform = 'windows' as const;

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
      const { stdout } = await exec('wmic cpu get Name,NumberOfCores,NumberOfLogicalProcessors,MaxClockSpeed,L2CacheSize,L3CacheSize,AddressWidth /format:csv');
      const lines = stdout.trim().split('\n').filter(l => l.trim());
      if (lines.length >= 2) {
        const headers = lines[0].split(',');
        const values = lines[1].split(',');
        const data: Record<string, string> = {};
        headers.forEach((h, i) => data[h.trim()] = values[i]?.trim() || '');
        
        return this.createResult({
          name: data.Name || 'Unknown',
          manufacturer: 'GenuineIntel',
          cores: parseInt(data.NumberOfCores || '0'),
          logical_processors: parseInt(data.NumberOfLogicalProcessors || '0'),
          max_clock_speed_mhz: parseInt(data.MaxClockSpeed || '0'),
          l2_cache_kb: parseInt(data.L2CacheSize || '0'),
          l3_cache_kb: parseInt(data.L3CacheSize || '0'),
          architecture: parseInt(data.AddressWidth || '64'),
        }, 'wmic', 0.95);
      }
    } catch (e) {}
    return this.createUnknownResult<CPUInfo>('wmic');
  }

  private async detectRAM(): Promise<ReturnType<typeof this.createResult<RAMInfo>>> {
    try {
      const { stdout } = await exec('wmic computersystem get TotalPhysicalMemory /format:csv');
      const lines = stdout.trim().split('\n').filter(l => l.trim());
      if (lines.length >= 2) {
        const totalBytes = parseInt(lines[1].split(',')[1] || '0');
        const totalGB = Math.round(totalBytes / (1024**3) * 100) / 100;
        
        let speed = 'unknown';
        try {
          const { stdout: speedOut } = await exec('wmic memorychip get Speed /format:csv');
          const speedLines = speedOut.trim().split('\n').filter(l => l.trim());
          if (speedLines.length >= 2) {
            speed = speedLines[1].split(',')[1]?.trim() || 'unknown';
          }
        } catch {}

        return this.createResult({
          total_gb: totalGB,
          speed_mhz: speed === 'unknown' ? undefined : parseInt(speed),
          type: 'DDR4',
        }, 'wmic', 0.9);
      }
    } catch (e) {}
    return this.createUnknownResult<RAMInfo>('wmic');
  }

  private async detectGPU(): Promise<ReturnType<typeof this.createResult<GPUInfo[]>>> {
    try {
      const { stdout } = await exec('wmic path win32_VideoController get Name,AdapterRAM,DriverVersion,VideoProcessor,VideoModeDescription /format:csv');
      const lines = stdout.trim().split('\n').filter(l => l.trim());
      if (lines.length >= 2) {
        const headers = lines[0].split(',');
        const gpus: GPUInfo[] = [];
        for (let i = 1; i < lines.length; i++) {
          const values = lines[i].split(',');
          const data: Record<string, string> = {};
          headers.forEach((h, j) => data[h.trim()] = values[j]?.trim() || '');
          
          if (data.Name && data.AdapterRAM) {
            gpus.push({
              name: data.Name,
              adapter_ram_gb: Math.round(parseInt(data.AdapterRAM) / (1024**3) * 100) / 100,
              driver_version: data.DriverVersion || 'Unknown',
              video_processor: data.VideoProcessor || data.Name,
              video_mode_description: data.VideoModeDescription,
            });
          }
        }
        return this.createResult(gpus, 'wmic', 0.95);
      }
    } catch (e) {}
    return this.createResult([], 'wmic', 0.5);
  }

  private async detectNPU(): Promise<ReturnType<typeof this.createResult<NPUInfo | null>>> {
    return this.createResult(null, 'wmic', 1.0, { reason: 'No NPU detected on this platform' });
  }

  private async detectMotherboard(): Promise<ReturnType<typeof this.createResult<MotherboardInfo>>> {
    try {
      const { stdout } = await exec('wmic baseboard get Manufacturer,Product,Version,SerialNumber /format:csv');
      const lines = stdout.trim().split('\n').filter(l => l.trim());
      if (lines.length >= 2) {
        const headers = lines[0].split(',');
        const values = lines[1].split(',');
        const data: Record<string, string> = {};
        headers.forEach((h, i) => data[h.trim()] = values[i]?.trim() || '');
        
        return this.createResult({
          manufacturer: data.Manufacturer || 'Unknown',
          product: data.Product || 'Unknown',
          version: data.Version || 'Unknown',
          serial_number: data.SerialNumber || undefined,
        }, 'wmic', 0.9);
      }
    } catch (e) {}
    return this.createUnknownResult<MotherboardInfo>('wmic');
  }

  private async detectBIOS(): Promise<ReturnType<typeof this.createResult<BIOSInfo>>> {
    try {
      const { stdout } = await exec('wmic bios get SMBIOSBIOSVersion,ReleaseDate,Manufacturer /format:csv');
      const lines = stdout.trim().split('\n').filter(l => l.trim());
      if (lines.length >= 2) {
        const headers = lines[0].split(',');
        const values = lines[1].split(',');
        const data: Record<string, string> = {};
        headers.forEach((h, i) => data[h.trim()] = values[i]?.trim() || '');
        
        return this.createResult({
          version: data.SMBIOSBIOSVersion || 'Unknown',
          release_date: data.ReleaseDate || 'Unknown',
          vendor: data.Manufacturer || 'Unknown',
        }, 'wmic', 0.95);
      }
    } catch (e) {}
    return this.createUnknownResult<BIOSInfo>('wmic');
  }

  private async detectStorage(): Promise<ReturnType<typeof this.createResult<StorageInfo[]>>> {
    try {
      const { stdout } = await exec('wmic diskdrive get Model,Size,MediaType,InterfaceType,Partitions /format:csv');
      const lines = stdout.trim().split('\n').filter(l => l.trim());
      if (lines.length >= 2) {
        const headers = lines[0].split(',');
        const storage: StorageInfo[] = [];
        for (let i = 1; i < lines.length; i++) {
          const values = lines[i].split(',');
          const data: Record<string, string> = {};
          headers.forEach((h, j) => data[h.trim()] = values[j]?.trim() || '');
          
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
        return this.createResult(storage, 'wmic', 0.9);
      }
    } catch (e) {}
    return this.createResult([], 'wmic', 0.5);
  }

  private async detectPartitions(): Promise<ReturnType<typeof this.createResult<PartitionInfo[]>>> {
    try {
      const { stdout } = await exec('wmic volume get DriveLetter,Label,FileSystem,Capacity,FreeSpace /format:csv');
      const lines = stdout.trim().split('\n').filter(l => l.trim());
      if (lines.length >= 2) {
        const headers = lines[0].split(',');
        const partitions: PartitionInfo[] = [];
        for (let i = 1; i < lines.length; i++) {
          const values = lines[i].split(',');
          const data: Record<string, string> = {};
          headers.forEach((h, j) => data[h.trim()] = values[j]?.trim() || '');
          
          if (data.DriveLetter) {
            partitions.push({
              drive_letter: data.DriveLetter,
              label: data.Label || null,
              file_system: data.FileSystem || null,
              capacity_gb: data.Capacity ? Math.round(parseInt(data.Capacity) / (1024**3) * 100) / 100 : 0,
              free_space_gb: data.FreeSpace ? Math.round(parseInt(data.FreeSpace) / (1024**3) * 100) / 100 : 0,
            });
          }
        }
        return this.createResult(partitions, 'wmic', 0.9);
      }
    } catch (e) {}
    return this.createResult([], 'wmic', 0.5);
  }

  private async detectMonitors(): Promise<ReturnType<typeof this.createResult<MonitorInfo[]>>> {
    try {
      const { stdout } = await exec('wmic desktopmonitor get Name,ScreenWidth,ScreenHeight,MonitorManufacturer,MonitorType /format:csv');
      const lines = stdout.trim().split('\n').filter(l => l.trim());
      if (lines.length >= 2) {
        const headers = lines[0].split(',');
        const monitors: MonitorInfo[] = [];
        for (let i = 1; i < lines.length; i++) {
          const values = lines[i].split(',');
          const data: Record<string, string> = {};
          headers.forEach((h, j) => data[h.trim()] = values[j]?.trim() || '');
          
          if (data.Name) {
            monitors.push({
              name: data.Name,
              screen_width: data.ScreenWidth ? parseInt(data.ScreenWidth) : undefined,
              screen_height: data.ScreenHeight ? parseInt(data.ScreenHeight) : undefined,
              monitor_manufacturer: data.MonitorManufacturer || undefined,
              monitor_type: data.MonitorType || undefined,
            });
          }
        }
        return this.createResult(monitors, 'wmic', 0.8);
      }
    } catch (e) {}
    return this.createResult([], 'wmic', 0.5);
  }

  private async detectAudio(): Promise<ReturnType<typeof this.createResult<AudioInfo[]>>> {
    try {
      const { stdout } = await exec('wmic path win32_SoundDevice get Name,Manufacturer,DeviceID,Status /format:csv');
      const lines = stdout.trim().split('\n').filter(l => l.trim());
      if (lines.length >= 2) {
        const headers = lines[0].split(',');
        const audio: AudioInfo[] = [];
        for (let i = 1; i < lines.length; i++) {
          const values = lines[i].split(',');
          const data: Record<string, string> = {};
          headers.forEach((h, j) => data[h.trim()] = values[j]?.trim() || '');
          
          if (data.Name) {
            audio.push({
              name: data.Name,
              manufacturer: data.Manufacturer || 'Unknown',
              device_id: data.DeviceID || '',
              status: data.Status || 'Unknown',
            });
          }
        }
        return this.createResult(audio, 'wmic', 0.9);
      }
    } catch (e) {}
    return this.createResult([], 'wmic', 0.5);
  }

  private async detectMicrophones(): Promise<ReturnType<typeof this.createResult<MicrophoneInfo[]>>> {
    try {
      const { stdout } = await exec('powershell "Get-PnpDevice -Class AudioEndpoint -Status OK | Where-Object {$_.FriendlyName -like \'*microphone*\' -or $_.FriendlyName -like \'*Mic*\' -or $_.FriendlyName -like \'*Headset*\' } | Select-Object FriendlyName,InstanceId,Status | ConvertTo-Json"');
      const devices = JSON.parse(stdout);
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
      const devices = JSON.parse(stdout);
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
      const devices = JSON.parse(stdout);
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
      const devices = JSON.parse(stdout);
      const adapters: BluetoothAdapterInfo[] = Array.isArray(devices) ? devices
        .filter((d: any) => d.FriendlyName?.toLowerCase().includes('adapter') || d.FriendlyName?.toLowerCase().includes('radio'))
        .map((d: any) => ({
          name: d.FriendlyName || 'Bluetooth Adapter',
          address: '',
          status: d.Status || 'Unknown',
        })) : [];

      const { stdout: radioOut } = await exec('powershell "Get-PnpDevice -Class Radio | Select-Object FriendlyName,InstanceId,Status | ConvertTo-Json"');
      const radioDevices = JSON.parse(radioOut);
      const btRadio = Array.isArray(radioDevices) ? radioDevices.filter((d: any) => d.FriendlyName?.toLowerCase().includes('bluetooth')) : [];
      btRadio.forEach((d: any) => {
        adapters.push({
          name: d.FriendlyName || 'Bluetooth Radio',
          address: '',
          status: d.Status || 'Unknown',
        });
      });

      const pairedOut = await exec('powershell "Get-PnpDevice -Class Bluetooth | Where-Object {$_.Status -eq \'OK\'} | Select-Object FriendlyName,InstanceId | ConvertTo-Json"');
      const pairedDevices = JSON.parse(pairedOut.stdout);
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
      }, 'powershell', 0.7);
    } catch (e) {}
    return this.createResult({ available: false, adapters: [], devices: [] }, 'powershell', 0.5);
  }

  private async detectNetworkAdapters(): Promise<ReturnType<typeof this.createResult<NetworkAdapterInfo[]>>> {
    try {
      const { stdout } = await exec('powershell "Get-NetAdapter | Where-Object {$_.Status -ne \'Disconnected\'} | Select-Object Name,InterfaceDescription,MacAddress,LinkSpeed,Status,IfIndex | ConvertTo-Json"');
      const adapters = JSON.parse(stdout);
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
      const programs = JSON.parse(stdout);
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
      const services = JSON.parse(stdout);
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
      { name: 'code', cmd: 'code --version', parse: (out: string) => out.split('\n')[0] },
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
      containers.push({ name: 'docker', version: '', running: false });
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
    } catch (e) {
      containers.push({ name: 'podman', version: '', running: false });
    }

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

      for (const line of lines) {
        const parts = line.trim().split(/\s+/);
        if (parts.length >= 3) {
          const name = parts[0].replace('*', '');
          const isDefault = line.includes('*');
          if (isDefault) defaultDistro = name;
          distributions.push({
            name,
            version: parts[1] || 'unknown',
            state: parts[2] || 'unknown',
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
    
    try {
      const ollamaModelsPath = `${process.env.USERPROFILE}\\.ollama\\models`;
      const fs = await import('fs');
      if (fs.existsSync(ollamaModelsPath)) {
        const files = fs.readdirSync(ollamaModelsPath);
        for (const file of files) {
          if (file.endsWith('.gguf') || file.endsWith('.bin')) {
            const stats = fs.statSync(`${ollamaModelsPath}\\${file}`);
            models.push({
              name: file.replace(/\.(gguf|bin)$/, ''),
              type: 'llm',
              size_gb: Math.round(stats.size / (1024**3) * 100) / 100,
              path: `${ollamaModelsPath}\\${file}`,
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
      const adapters = JSON.parse(stdout);
      const interfaces: NetworkInterfaceInfo[] = Array.isArray(adapters) ? adapters.map((a: any) => ({
        name: a.Name,
        description: a.InterfaceDescription,
        mac_address: a.MacAddress,
        link_speed: a.LinkSpeed,
        status: a.Status,
        index: a.IfIndex,
      })) : [];
      return this.createResult(interfaces, 'powershell', 0.9);
    } catch (e) {}
    return this.createResult([], 'powershell', 0.5);
  }

  private async detectIPConfig(): Promise<ReturnType<typeof this.createResult<IPConfigInfo[]>>> {
    try {
      const { stdout } = await exec('powershell "Get-NetIPConfiguration | Select-Object InterfaceAlias,IPv4Address,IPv6Address,DNSServer,Ipv4DefaultGateway | ConvertTo-Json"');
      const configs = JSON.parse(stdout);
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
      const routes = JSON.parse(stdout);
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
      const dns = JSON.parse(stdout);
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
      const { stdout } = await exec('powershell "Get-NetIPAddress -AddressFamily IPv4 | Where-Object {$_.IPAddress -notmatch \'^127\\.|^169\\.254\.'} | Select-Object IPAddress,InterfaceAlias,PrefixLength | ConvertTo-Json"');
      const ips = JSON.parse(stdout);
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
      const routes = JSON.parse(stdout);
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
      const fs = await import('fs');
      const modulesPath = 'S:\\JAMES';
      const dirs = fs.readdirSync(modulesPath, { withFileTypes: true })
        .filter(d => d.isDirectory() && !d.name.startsWith('.'))
        .map(d => d.name);
      return this.createResult(dirs, 'filesystem', 0.9);
    } catch (e) {}
    return this.createResult([], 'filesystem', 0.5);
  }

  private async detectJAMESPlugins(): Promise<ReturnType<typeof this.createResult<string[]>>> {
    try {
      const fs = await import('fs');
      const pluginsPath = 'S:\\JAMES\\plugins';
      if (fs.existsSync(pluginsPath)) {
        const dirs = fs.readdirSync(pluginsPath, { withFileTypes: true })
          .filter(d => d.isDirectory())
          .map(d => d.name);
        return this.createResult(dirs, 'filesystem', 0.9);
      }
    } catch (e) {}
    return this.createResult([], 'filesystem', 0.5);
  }

  private async detectJAMESConfig(): Promise<ReturnType<typeof this.createResult<Record<string, unknown>>>> {
    try {
      const fs = await import('fs');
      const configPath = 'S:\\JAMES\\.james\\config';
      if (fs.existsSync(configPath)) {
        const files = fs.readdirSync(configPath);
        const config: Record<string, unknown> = {};
        for (const file of files) {
          if (file.endsWith('.json')) {
            const content = fs.readFileSync(`${configPath}\\${file}`, 'utf-8');
            config[file.replace('.json', '')] = JSON.parse(content);
          }
        }
        return this.createResult(config, 'filesystem', 0.9);
      }
    } catch (e) {}
    return this.createResult({}, 'filesystem', 0.5);
  }

  private async detectJAMESVersion(): Promise<ReturnType<typeof this.createResult<string>>> {
    try {
      const fs = await import('fs');
      const versionPath = 'S:\\JAMES\\.james\\version';
      if (fs.existsSync(versionPath)) {
        const version = fs.readFileSync(versionPath, 'utf-8').trim();
        return this.createResult(version, 'filesystem', 1.0);
      }
      const pkgPath = 'S:\\JAMES\\package.json';
      if (fs.existsSync(pkgPath)) {
        const pkg = JSON.parse(fs.readFileSync(pkgPath, 'utf-8'));
        return this.createResult(pkg.version || '0.0.0', 'package.json', 0.9);
      }
    } catch (e) {}
    return this.createResult('0.0.0-dev', 'default', 0.5);
  }

  private async detectGitStatus(): Promise<ReturnType<typeof this.createResult<GitStatus>>> {
    try {
      const { stdout: branchOut } = await exec('git -C S:\\JAMES rev-parse --abbrev-ref HEAD');
      const branch = branchOut.trim();
      
      const { stdout: commitOut } = await exec('git -C S:\\JAMES rev-parse HEAD');
      const commit = commitOut.trim();
      
      const { stdout: statusOut } = await exec('git -C S:\\JAMES status --porcelain');
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