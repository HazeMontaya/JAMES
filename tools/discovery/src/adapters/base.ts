import {
  PlatformAdapter,
  HardwareInfo,
  SoftwareInfo,
  NetworkInfo,
  JAMESInfo,
  Capability,
  DiscoveryResult,
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

export abstract class BaseAdapter implements PlatformAdapter {
  abstract readonly platform: 'windows' | 'linux' | 'macos' | 'android' | 'ios';

  abstract detectHardware(): Promise<HardwareInfo>;
  abstract detectSoftware(): Promise<SoftwareInfo>;
  abstract detectNetwork(): Promise<NetworkInfo>;
  abstract detectJAMES(): Promise<JAMESInfo>;

  getCapabilities(hardware: HardwareInfo, software: SoftwareInfo, network: NetworkInfo): Capability[] {
    const capabilities: Capability[] = [];

    if (hardware.gpu.value.length > 0) {
      capabilities.push({
        id: 'ai.local.gpu_inference',
        name: 'GPU-Accelerated Inference',
        category: 'ai',
        detected: true,
        provider: hardware.gpu.value[0].name,
        version: hardware.gpu.value[0].driver_version,
        dependencies: ['cuda' in hardware.gpu.value[0] ? 'cuda' : 'directml'],
        risk_level: 'low',
        status: 'available',
        metadata: { vram_gb: hardware.gpu.value[0].adapter_ram_gb }
      });
    }

    if (hardware.cpu.value.cores >= 4 && hardware.ram.value.total_gb >= 8) {
      capabilities.push({
        id: 'ai.local.cpu_inference',
        name: 'CPU-Based Inference',
        category: 'ai',
        detected: true,
        provider: hardware.cpu.value.name,
        dependencies: [],
        risk_level: 'low',
        status: 'available',
        metadata: { cores: hardware.cpu.value.cores, ram_gb: hardware.ram.value.total_gb }
      });
    }

    if (hardware.npu.value) {
      capabilities.push({
        id: 'ai.local.npu_inference',
        name: 'NPU-Accelerated Inference',
        category: 'ai',
        detected: true,
        provider: hardware.npu.value.name,
        dependencies: [],
        risk_level: 'low',
        status: 'available',
        metadata: { performance_tops: hardware.npu.value.performance_tops }
      });
    }

    const hasMic = hardware.microphones.value.length > 0;
    if (hasMic) {
      capabilities.push({
        id: 'voice.input',
        name: 'Voice Input (Microphone)',
        category: 'voice',
        detected: true,
        provider: 'system',
        dependencies: [],
        risk_level: 'low',
        status: 'available'
      });
    }

    const hasSpeakers = hardware.audio.value.length > 0;
    if (hasSpeakers) {
      capabilities.push({
        id: 'voice.output',
        name: 'Voice Output (Speakers)',
        category: 'voice',
        detected: true,
        provider: 'system',
        dependencies: [],
        risk_level: 'low',
        status: 'available'
      });
    }

    const hasBrowser = software.browsers.value.length > 0;
    if (hasBrowser) {
      capabilities.push({
        id: 'browser.automation',
        name: 'Browser Automation',
        category: 'automation',
        detected: true,
        provider: software.browsers.value[0].name,
        version: software.browsers.value[0].version,
        dependencies: ['playwright', 'puppeteer'],
        risk_level: 'medium',
        status: 'available'
      });
    }

    const hasDocker = software.containers.value.some(c => c.name === 'docker' && c.running);
    if (hasDocker) {
      capabilities.push({
        id: 'container.runtime',
        name: 'Container Runtime (Docker)',
        category: 'container',
        detected: true,
        provider: 'docker',
        dependencies: [],
        risk_level: 'medium',
        status: 'available'
      });
    }

    const hasPodman = software.containers.value.some(c => c.name === 'podman' && c.running);
    if (hasPodman) {
      capabilities.push({
        id: 'container.runtime.podman',
        name: 'Container Runtime (Podman)',
        category: 'container',
        detected: true,
        provider: 'podman',
        dependencies: [],
        risk_level: 'medium',
        status: 'available'
      });
    }

    if (software.wsl.value.installed) {
      capabilities.push({
        id: 'wsl2.environment',
        name: 'WSL 2 Environment',
        category: 'system',
        detected: true,
        provider: 'microsoft',
        version: software.wsl.value.version,
        dependencies: [],
        risk_level: 'low',
        status: 'available',
        metadata: { distributions: software.wsl.value.distributions.length }
      });
    }

    if (hardware.bluetooth.value.available && hardware.bluetooth.value.adapters.length > 0) {
      capabilities.push({
        id: 'device.bluetooth',
        name: 'Bluetooth Device Discovery',
        category: 'device',
        detected: true,
        provider: 'system',
        dependencies: [],
        risk_level: 'low',
        status: 'available'
      });
    }

    const hasHA = software.ai_runtimes.value.some(r => r.name.toLowerCase().includes('home assistant'));
    if (hasHA) {
      capabilities.push({
        id: 'smart_home.home_assistant',
        name: 'Home Assistant Integration',
        category: 'smart_home',
        detected: true,
        provider: 'home_assistant',
        dependencies: ['mqtt', 'websocket'],
        risk_level: 'medium',
        status: 'available'
      });
    }

    const hasOllama = software.ai_runtimes.value.some(r => r.name.toLowerCase() === 'ollama' && r.running);
    if (hasOllama) {
      capabilities.push({
        id: 'ai.local.ollama',
        name: 'Local LLM via Ollama',
        category: 'ai',
        detected: true,
        provider: 'ollama',
        version: software.ai_runtimes.value.find(r => r.name.toLowerCase() === 'ollama')?.version,
        dependencies: [],
        risk_level: 'low',
        status: 'available',
        metadata: { models: software.ai_runtimes.value.find(r => r.name.toLowerCase() === 'ollama')?.models }
      });
    }

    const hasVSCode = software.developer_tools.value.some(t => t.name.toLowerCase().includes('vscode') || t.name.toLowerCase().includes('visual studio code'));
    if (hasVSCode) {
      capabilities.push({
        id: 'dev.vscode',
        name: 'VS Code Development Environment',
        category: 'development',
        detected: true,
        provider: 'microsoft',
        version: software.developer_tools.value.find(t => t.name.toLowerCase().includes('vscode') || t.name.toLowerCase().includes('visual studio code'))?.version,
        dependencies: [],
        risk_level: 'low',
        status: 'available'
      });
    }

    const hasGit = software.developer_tools.value.some(t => t.name.toLowerCase() === 'git');
    if (hasGit) {
      capabilities.push({
        id: 'dev.git',
        name: 'Git Version Control',
        category: 'development',
        detected: true,
        provider: 'git-scm',
        version: software.developer_tools.value.find(t => t.name.toLowerCase() === 'git')?.version,
        dependencies: [],
        risk_level: 'low',
        status: 'available'
      });
    }

    if (hardware.cameras.value.length > 0) {
      capabilities.push({
        id: 'device.camera',
        name: 'Camera Access',
        category: 'device',
        detected: true,
        provider: 'system',
        dependencies: [],
        risk_level: 'medium',
        status: 'available'
      });
    }

    if (network.interfaces.value.length > 0) {
      capabilities.push({
        id: 'network.local',
        name: 'Local Network Access',
        category: 'network',
        detected: true,
        provider: 'system',
        dependencies: [],
        risk_level: 'low',
        status: 'available',
        metadata: { interfaces: network.interfaces.value.map(i => i.name) }
      });
    }

    if (network.local_devices.value.length > 0) {
      capabilities.push({
        id: 'network.device_discovery',
        name: 'Local Device Discovery',
        category: 'network',
        detected: true,
        provider: 'mdns',
        dependencies: ['mdns'],
        risk_level: 'medium',
        status: 'available',
        metadata: { device_count: network.local_devices.value.length }
      });
    }

    return capabilities;
  }

  protected createResult<T>(value: T, source: string, confidence: number = 1.0, metadata?: Record<string, unknown>): DiscoveryResult<T> {
    return {
      value,
      source,
      detected_at: new Date().toISOString(),
      confidence,
      metadata
    };
  }

  protected createUnknownResult<T>(source: string, metadata?: Record<string, unknown>): DiscoveryResult<T> {
    return {
      value: 'unknown' as T,
      source,
      detected_at: new Date().toISOString(),
      confidence: 0,
      metadata: { ...metadata, reason: 'not_detected' }
    };
  }
}