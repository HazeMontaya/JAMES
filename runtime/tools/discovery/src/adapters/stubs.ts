import {
  HardwareInfo,
  SoftwareInfo,
  NetworkInfo,
  JAMESInfo,
} from '../core/interfaces';
import { BaseAdapter } from './base';

export type StubPlatform = 'linux' | 'macos' | 'android' | 'ios';

/**
 * Single shared stub implementation. Previously four ~80-line classes with
 * identical bodies differing only in the platform string (F1-09 dedup).
 * Platform adapters stay thin subclasses so existing imports keep working.
 */
export class StubAdapter extends BaseAdapter {
  constructor(readonly platform: StubPlatform) {
    super();
  }

  async detectHardware(): Promise<HardwareInfo> {
    const unknown = this.createUnknownResult.bind(this);
    const source = this.platform;
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

  async detectSoftware(): Promise<SoftwareInfo> {
    const source = this.platform;
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

  async detectNetwork(): Promise<NetworkInfo> {
    const source = this.platform;
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

  async detectJAMES(): Promise<JAMESInfo> {
    const source = this.platform;
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

export class LinuxAdapter extends StubAdapter {
  constructor() {
    super('linux');
  }
}

export class MacOSAdapter extends StubAdapter {
  constructor() {
    super('macos');
  }
}

export class AndroidAdapter extends StubAdapter {
  constructor() {
    super('android');
  }
}

export class iOSAdapter extends StubAdapter {
  constructor() {
    super('ios');
  }
}
