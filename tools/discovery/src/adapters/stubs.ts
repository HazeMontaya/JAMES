import {
  PlatformAdapter,
  HardwareInfo,
  SoftwareInfo,
  NetworkInfo,
  JAMESInfo,
  Capability,
  DiscoveryResult,
} from '../core/interfaces';
import { BaseAdapter } from './base';

export class LinuxAdapter extends BaseAdapter {
  readonly platform = 'linux' as const;

  async detectHardware(): Promise<HardwareInfo> {
    return this.createUnknownHardware('linux');
  }

  async detectSoftware(): Promise<SoftwareInfo> {
    return this.createUnknownSoftware('linux');
  }

  async detectNetwork(): Promise<NetworkInfo> {
    return this.createUnknownNetwork('linux');
  }

  async detectJAMES(): Promise<JAMESInfo> {
    return this.createUnknownJAMES('linux');
  }

  private createUnknownHardware(source: string): HardwareInfo {
    const unknown = this.createUnknownResult.bind(this);
    return {
      cpu: unknown('cpu'),
      ram: unknown('ram'),
      gpu: this.createResult([], source, 0.5),
      npu: this.createResult(null, source, 1.0),
      motherboard: unknown('motherboard'),
      bios: unknown('bios'),
      storage: this.createResult([], source, 0.5),
      partitions: this.createResult([], source, 0.5),
      monitors: this.createResult([], source, 0.5),
      audio: this.createResult([], source, 0.5),
      microphones: this.createResult([], source, 0.5),
      cameras: this.createResult([], source, 0.5),
      usb: this.createResult([], source, 0.5),
      bluetooth: this.createResult({ available: false, adapters: [], devices: [] }, source, 0.5),
      network_adapters: this.createResult([], source, 0.5),
    };
  }

  private createUnknownSoftware(source: string): SoftwareInfo {
    return {
      installed_programs: this.createResult([], source, 0.3),
      running_services: this.createResult([], source, 0.3),
      developer_tools: this.createResult([], source, 0.3),
      runtimes: this.createResult([], source, 0.3),
      browsers: this.createResult([], source, 0.3),
      containers: this.createResult([], source, 0.3),
      wsl: this.createResult({ installed: false, distributions: [] }, source, 0.5),
      ai_runtimes: this.createResult([], source, 0.3),
      local_models: this.createResult([], source, 0.3),
    };
  }

  private createUnknownNetwork(source: string): NetworkInfo {
    return {
      interfaces: this.createResult([], source, 0.3),
      ip_config: this.createResult([], source, 0.3),
      routes: this.createResult([], source, 0.3),
      dns: this.createResult([], source, 0.3),
      local_ips: this.createResult([], source, 0.3),
      gateway: this.createUnknownResult(source),
      mdns_services: this.createResult([], source, 0.3),
      local_devices: this.createResult([], source, 0.3),
    };
  }

  private createUnknownJAMES(source: string): JAMESInfo {
    return {
      modules: this.createResult([], source, 0.3),
      plugins: this.createResult([], source, 0.3),
      config: this.createResult({}, source, 0.3),
      version: this.createResult('unknown', source, 0.3),
      git_status: this.createResult({ repo_exists: false }, source, 0.3),
      services: this.createResult([], source, 0.3),
    };
  }
}

export class MacOSAdapter extends BaseAdapter {
  readonly platform = 'macos' as const;

  async detectHardware(): Promise<HardwareInfo> {
    return this.createUnknownHardware('macos');
  }

  async detectSoftware(): Promise<SoftwareInfo> {
    return this.createUnknownSoftware('macos');
  }

  async detectNetwork(): Promise<NetworkInfo> {
    return this.createUnknownNetwork('macos');
  }

  async detectJAMES(): Promise<JAMESInfo> {
    return this.createUnknownJAMES('macos');
  }

  private createUnknownHardware(source: string): HardwareInfo {
    const unknown = this.createUnknownResult.bind(this);
    return {
      cpu: unknown('cpu'),
      ram: unknown('ram'),
      gpu: this.createResult([], source, 0.5),
      npu: this.createResult(null, source, 1.0),
      motherboard: unknown('motherboard'),
      bios: unknown('bios'),
      storage: this.createResult([], source, 0.5),
      partitions: this.createResult([], source, 0.5),
      monitors: this.createResult([], source, 0.5),
      audio: this.createResult([], source, 0.5),
      microphones: this.createResult([], source, 0.5),
      cameras: this.createResult([], source, 0.5),
      usb: this.createResult([], source, 0.5),
      bluetooth: this.createResult({ available: false, adapters: [], devices: [] }, source, 0.5),
      network_adapters: this.createResult([], source, 0.5),
    };
  }

  private createUnknownSoftware(source: string): SoftwareInfo {
    return {
      installed_programs: this.createResult([], source, 0.3),
      running_services: this.createResult([], source, 0.3),
      developer_tools: this.createResult([], source, 0.3),
      runtimes: this.createResult([], source, 0.3),
      browsers: this.createResult([], source, 0.3),
      containers: this.createResult([], source, 0.3),
      wsl: this.createResult({ installed: false, distributions: [] }, source, 0.5),
      ai_runtimes: this.createResult([], source, 0.3),
      local_models: this.createResult([], source, 0.3),
    };
  }

  private createUnknownNetwork(source: string): NetworkInfo {
    return {
      interfaces: this.createResult([], source, 0.3),
      ip_config: this.createResult([], source, 0.3),
      routes: this.createResult([], source, 0.3),
      dns: this.createResult([], source, 0.3),
      local_ips: this.createResult([], source, 0.3),
      gateway: this.createUnknownResult(source),
      mdns_services: this.createResult([], source, 0.3),
      local_devices: this.createResult([], source, 0.3),
    };
  }

  private createUnknownJAMES(source: string): JAMESInfo {
    return {
      modules: this.createResult([], source, 0.3),
      plugins: this.createResult([], source, 0.3),
      config: this.createResult({}, source, 0.3),
      version: this.createResult('unknown', source, 0.3),
      git_status: this.createResult({ repo_exists: false }, source, 0.3),
      services: this.createResult([], source, 0.3),
    };
  }
}

export class AndroidAdapter extends BaseAdapter {
  readonly platform = 'android' as const;

  async detectHardware(): Promise<HardwareInfo> {
    return this.createUnknownHardware('android');
  }

  async detectSoftware(): Promise<SoftwareInfo> {
    return this.createUnknownSoftware('android');
  }

  async detectNetwork(): Promise<NetworkInfo> {
    return this.createUnknownNetwork('android');
  }

  async detectJAMES(): Promise<JAMESInfo> {
    return this.createUnknownJAMES('android');
  }

  private createUnknownHardware(source: string): HardwareInfo {
    const unknown = this.createUnknownResult.bind(this);
    return {
      cpu: unknown('cpu'),
      ram: unknown('ram'),
      gpu: this.createResult([], source, 0.5),
      npu: this.createResult(null, source, 1.0),
      motherboard: unknown('motherboard'),
      bios: unknown('bios'),
      storage: this.createResult([], source, 0.5),
      partitions: this.createResult([], source, 0.5),
      monitors: this.createResult([], source, 0.5),
      audio: this.createResult([], source, 0.5),
      microphones: this.createResult([], source, 0.5),
      cameras: this.createResult([], source, 0.5),
      usb: this.createResult([], source, 0.5),
      bluetooth: this.createResult({ available: false, adapters: [], devices: [] }, source, 0.5),
      network_adapters: this.createResult([], source, 0.5),
    };
  }

  private createUnknownSoftware(source: string): SoftwareInfo {
    return {
      installed_programs: this.createResult([], source, 0.3),
      running_services: this.createResult([], source, 0.3),
      developer_tools: this.createResult([], source, 0.3),
      runtimes: this.createResult([], source, 0.3),
      browsers: this.createResult([], source, 0.3),
      containers: this.createResult([], source, 0.3),
      wsl: this.createResult({ installed: false, distributions: [] }, source, 0.5),
      ai_runtimes: this.createResult([], source, 0.3),
      local_models: this.createResult([], source, 0.3),
    };
  }

  private createUnknownNetwork(source: string): NetworkInfo {
    return {
      interfaces: this.createResult([], source, 0.3),
      ip_config: this.createResult([], source, 0.3),
      routes: this.createResult([], source, 0.3),
      dns: this.createResult([], source, 0.3),
      local_ips: this.createResult([], source, 0.3),
      gateway: this.createUnknownResult(source),
      mdns_services: this.createResult([], source, 0.3),
      local_devices: this.createResult([], source, 0.3),
    };
  }

  private createUnknownJAMES(source: string): JAMESInfo {
    return {
      modules: this.createResult([], source, 0.3),
      plugins: this.createResult([], source, 0.3),
      config: this.createResult({}, source, 0.3),
      version: this.createResult('unknown', source, 0.3),
      git_status: this.createResult({ repo_exists: false }, source, 0.3),
      services: this.createResult([], source, 0.3),
    };
  }
}

export class iOSAdapter extends BaseAdapter {
  readonly platform = 'ios' as const;

  async detectHardware(): Promise<HardwareInfo> {
    return this.createUnknownHardware('ios');
  }

  async detectSoftware(): Promise<SoftwareInfo> {
    return this.createUnknownSoftware('ios');
  }

  async detectNetwork(): Promise<NetworkInfo> {
    return this.createUnknownNetwork('ios');
  }

  async detectJAMES(): Promise<JAMESInfo> {
    return this.createUnknownJAMES('ios');
  }

  private createUnknownHardware(source: string): HardwareInfo {
    const unknown = this.createUnknownResult.bind(this);
    return {
      cpu: unknown('cpu'),
      ram: unknown('ram'),
      gpu: this.createResult([], source, 0.5),
      npu: this.createResult(null, source, 1.0),
      motherboard: unknown('motherboard'),
      bios: unknown('bios'),
      storage: this.createResult([], source, 0.5),
      partitions: this.createResult([], source, 0.5),
      monitors: this.createResult([], source, 0.5),
      audio: this.createResult([], source, 0.5),
      microphones: this.createResult([], source, 0.5),
      cameras: this.createResult([], source, 0.5),
      usb: this.createResult([], source, 0.5),
      bluetooth: this.createResult({ available: false, adapters: [], devices: [] }, source, 0.5),
      network_adapters: this.createResult([], source, 0.5),
    };
  }

  private createUnknownSoftware(source: string): SoftwareInfo {
    return {
      installed_programs: this.createResult([], source, 0.3),
      running_services: this.createResult([], source, 0.3),
      developer_tools: this.createResult([], source, 0.3),
      runtimes: this.createResult([], source, 0.3),
      browsers: this.createResult([], source, 0.3),
      containers: this.createResult([], source, 0.3),
      wsl: this.createResult({ installed: false, distributions: [] }, source, 0.5),
      ai_runtimes: this.createResult([], source, 0.3),
      local_models: this.createResult([], source, 0.3),
    };
  }

  private createUnknownNetwork(source: string): NetworkInfo {
    return {
      interfaces: this.createResult([], source, 0.3),
      ip_config: this.createResult([], source, 0.3),
      routes: this.createResult([], source, 0.3),
      dns: this.createResult([], source, 0.3),
      local_ips: this.createResult([], source, 0.3),
      gateway: this.createUnknownResult(source),
      mdns_services: this.createResult([], source, 0.3),
      local_devices: this.createResult([], source, 0.3),
    };
  }

  private createUnknownJAMES(source: string): JAMESInfo {
    return {
      modules: this.createResult([], source, 0.3),
      plugins: this.createResult([], source, 0.3),
      config: this.createResult({}, source, 0.3),
      version: this.createResult('unknown', source, 0.3),
      git_status: this.createResult({ repo_exists: false }, source, 0.3),
      services: this.createResult([], source, 0.3),
    };
  }
}