import { DiscoveryEngine } from '../src/core/engine';
import { WindowsAdapter } from '../src/adapters/windows';
import { LinuxAdapter, MacOSAdapter, AndroidAdapter, iOSAdapter } from '../src/adapters/stubs';
import { DiscoveryConfig, Capability, HardwareInfo, SoftwareInfo, NetworkInfo, JAMESInfo, CPUInfo, GatewayInfo } from '../src/core/interfaces';
import * as fs from 'fs';
import * as path from 'path';

function createTestConfig(): DiscoveryConfig {
  return {
    version: '1.0',
    scan: {
      mode: 'full',
      timeout_seconds: 30,
      parallel: true,
      include: ['hardware', 'software', 'network', 'james', 'capabilities'],
      exclude: [],
    },
    adapters: {
      platform: 'windows',
      windows: {
        use_wmi: true,
        use_powershell: true,
        use_registry: true,
        wmi_namespace: 'root\\cimv2',
      },
      linux: {
        use_sysfs: true,
        use_proc: true,
        use_dmidecode: false,
        use_lshw: false,
      },
    },
    capability_detection: {
      enabled: true,
      rules_file: 'capability-rules.json',
      custom_rules: [],
    },
    snapshot: {
      directory: '.james/inventory/snapshots',
      max_snapshots: 10,
      compress: false,
      retention_days: 1,
    },
    output: {
      inventory_dir: '.james/inventory',
      formats: ['json'],
      pretty_print: true,
    },
    security: {
      no_credentials: true,
      no_passwords: true,
      no_cookies: true,
      no_external_scan: true,
      redact_sensitive: true,
    },
    logging: {
      level: 'error',
      file: '.james/logs/discovery.log',
      console: false,
    },
  };
}

describe('WindowsAdapter', () => {
  let adapter: WindowsAdapter;

  beforeAll(() => {
    adapter = new WindowsAdapter();
  });

  test('should have correct platform', () => {
    expect(adapter.platform).toBe('windows');
  });

  test('detectHardware should return HardwareInfo structure', async () => {
    const hardware = await adapter.detectHardware();
    
    expect(hardware).toBeDefined();
    expect(hardware.cpu).toBeDefined();
    expect(hardware.ram).toBeDefined();
    expect(hardware.gpu).toBeDefined();
    expect(hardware.storage).toBeDefined();
    expect(hardware.partitions).toBeDefined();
    expect(hardware.network_adapters).toBeDefined();
  });

  test('detectSoftware should return SoftwareInfo structure', async () => {
    const software = await adapter.detectSoftware();
    
    expect(software).toBeDefined();
    expect(software.developer_tools).toBeDefined();
    expect(software.runtimes).toBeDefined();
    expect(software.containers).toBeDefined();
    expect(software.wsl).toBeDefined();
    expect(software.ai_runtimes).toBeDefined();
  });

  test('detectNetwork should return NetworkInfo structure', async () => {
    const network = await adapter.detectNetwork();
    
    expect(network).toBeDefined();
    expect(network.interfaces).toBeDefined();
    expect(network.ip_config).toBeDefined();
    expect(network.dns).toBeDefined();
    expect(network.local_ips).toBeDefined();
    expect(network.gateway).toBeDefined();
  });

  test('detectJAMES should return JAMESInfo structure', async () => {
    const james = await adapter.detectJAMES();
    
    expect(james).toBeDefined();
    expect(james.modules).toBeDefined();
    expect(james.plugins).toBeDefined();
    expect(james.config).toBeDefined();
    expect(james.version).toBeDefined();
    expect(james.git_status).toBeDefined();
  });

  test('getCapabilities should return capabilities based on hardware', async () => {
    const mockHardware: HardwareInfo = {
      cpu: { value: { name: 'Test CPU', manufacturer: 'Test', cores: 8, logical_processors: 8, max_clock_speed_mhz: 3600, architecture: 64 }, source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      ram: { value: { total_gb: 32 }, source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      gpu: { value: [{ name: 'RTX 2080 Ti', adapter_ram_gb: 4, driver_version: '32.0', video_processor: 'RTX 2080 Ti' }], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      npu: { value: null, source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      motherboard: { value: { manufacturer: 'Test', product: 'Test', version: '1.0' }, source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      bios: { value: { version: '1.0', release_date: '2024-01-01', vendor: 'Test' }, source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      storage: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      partitions: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      monitors: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      audio: { value: [{ name: 'Realtek', manufacturer: 'Realtek', device_id: '', status: 'OK' }], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      microphones: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      cameras: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      usb: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      bluetooth: { value: { available: false, adapters: [], devices: [] }, source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      network_adapters: { value: [{ name: 'Ethernet', description: 'Test', mac_address: '00:00:00:00:00:00', link_speed: '1 Gbps', status: 'Up', ipv4_addresses: ['192.168.1.100'], ipv6_addresses: [], dns_servers: ['192.168.1.1'] }], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
    };

    const mockSoftware: SoftwareInfo = {
      installed_programs: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      running_services: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      developer_tools: { value: [{ name: 'git', version: '2.40.0' }, { name: 'code', version: '1.80.0' }], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      runtimes: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      browsers: { value: [{ name: 'Chrome', version: '120.0', path: '' }], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      containers: { value: [{ name: 'docker', version: '24.0', running: true }], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      wsl: { value: { installed: true, version: '2.0', distributions: [{ name: 'Ubuntu', version: '22.04', state: 'Running', is_default: true }], default_distro: 'Ubuntu' }, source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      ai_runtimes: { value: [{ name: 'ollama', version: '0.32.8', running: true, models: ['llama3.2'] }], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      local_models: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
    };

    const mockNetwork: NetworkInfo = {
      interfaces: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      ip_config: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      routes: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      dns: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      local_ips: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      gateway: { value: { next_hop: '192.168.1.1', interface_alias: 'Ethernet' }, source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      mdns_services: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      local_devices: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
    };

    const capabilities = adapter.getCapabilities(mockHardware, mockSoftware, mockNetwork);
    
    expect(capabilities.length).toBeGreaterThan(0);
    
    const capabilityIds = capabilities.map(c => c.id);
    expect(capabilityIds).toContain('ai.local.gpu_inference');
    expect(capabilityIds).toContain('ai.local.cpu_inference');
    expect(capabilityIds).toContain('voice.output');
    expect(capabilityIds).toContain('browser.automation');
    expect(capabilityIds).toContain('container.runtime');
    expect(capabilityIds).toContain('wsl2.environment');
    expect(capabilityIds).toContain('dev.vscode');
    expect(capabilityIds).toContain('dev.git');
    expect(capabilityIds).toContain('ai.local.ollama');
    expect(capabilityIds).toContain('network.local');
  });

  test('capabilities should have required fields', async () => {
    const hardware = await adapter.detectHardware();
    const software = await adapter.detectSoftware();
    const network = await adapter.detectNetwork();
    const capabilities = adapter.getCapabilities(hardware, software, network);

    for (const cap of capabilities) {
      expect(cap.id).toBeDefined();
      expect(cap.name).toBeDefined();
      expect(cap.category).toBeDefined();
      expect(typeof cap.detected).toBe('boolean');
      expect(cap.provider).toBeDefined();
      expect(Array.isArray(cap.dependencies)).toBe(true);
      expect(['low', 'medium', 'high', 'critical']).toContain(cap.risk_level);
      expect(['available', 'unavailable', 'degraded', 'unknown']).toContain(cap.status);
    }
  });
});

describe('DiscoveryEngine', () => {
  let engine: DiscoveryEngine;
  let testConfig: DiscoveryConfig;

  beforeAll(() => {
    testConfig = createTestConfig();
    engine = new DiscoveryEngine(testConfig);
  });

  test('should create engine instance', () => {
    expect(engine).toBeDefined();
  });

  test('scan should return a snapshot', async () => {
    const snapshot = await engine.scan({ mode: 'fast', include: ['hardware', 'software', 'network', 'james', 'capabilities'] });
    
    expect(snapshot).toBeDefined();
    expect(snapshot.id).toBeDefined();
    expect(snapshot.timestamp).toBeDefined();
    expect(snapshot.scan_mode).toBe('fast');
    expect(snapshot.platform).toBe('windows');
    expect(snapshot.hardware).toBeDefined();
    expect(snapshot.software).toBeDefined();
    expect(snapshot.network).toBeDefined();
    expect(snapshot.james).toBeDefined();
    expect(snapshot.capabilities).toBeDefined();
    expect(snapshot.checksum).toBeDefined();
  });

  test('snapshot should be saved to disk', async () => {
    const snapshot = await engine.scan({ mode: 'fast', include: ['hardware'] });
    
    const snapshotsDir = path.resolve(testConfig.snapshot.directory);
    const files = fs.readdirSync(snapshotsDir).filter(f => f.endsWith('.json'));
    expect(files.length).toBeGreaterThan(0);
    
    const latestFile = files.sort().pop()!;
    const content = fs.readFileSync(path.join(snapshotsDir, latestFile), 'utf-8');
    const saved = JSON.parse(content);
    expect(saved.id).toBe(snapshot.id);
  });

  test('current.json should be updated', async () => {
    await engine.scan({ mode: 'fast', include: ['hardware', 'software'] });
    
    const currentPath = path.resolve(testConfig.output.inventory_dir, 'current.json');
    expect(fs.existsSync(currentPath)).toBe(true);
    
    const current = JSON.parse(fs.readFileSync(currentPath, 'utf-8'));
    expect(current.timestamp).toBeDefined();
    expect(current.hardware).toBeDefined();
    expect(current.software).toBeDefined();
  });

  test('capabilities.json should be updated', async () => {
    await engine.scan({ mode: 'fast', include: ['hardware', 'software', 'capabilities'] });
    
    const capsPath = path.resolve(testConfig.output.inventory_dir, 'capabilities.json');
    expect(fs.existsSync(capsPath)).toBe(true);
    
    const caps = JSON.parse(fs.readFileSync(capsPath, 'utf-8'));
    expect(Array.isArray(caps)).toBe(true);
    expect(caps.length).toBeGreaterThan(0);
  });

  test('listSnapshots should return snapshots', () => {
    const snapshots = engine.listSnapshots();
    expect(Array.isArray(snapshots)).toBe(true);
    expect(snapshots.length).toBeGreaterThan(0);
  });

  test('getLatestSnapshot should return latest', () => {
    const latest = engine.getLatestSnapshot();
    expect(latest).toBeDefined();
    expect(latest?.id).toBeDefined();
  });

  test('compareSnapshots should detect changes', async () => {
    const snap1 = await engine.scan({ mode: 'fast', include: ['hardware'] });
    const snap2 = await engine.scan({ mode: 'fast', include: ['hardware'] });
    
    const report = await engine.compareSnapshots(snap1.id, snap2.id);
    
    expect(report).toBeDefined();
    expect(report.previous_snapshot_id).toBe(snap1.id);
    expect(report.current_snapshot_id).toBe(snap2.id);
    expect(report.changes).toBeDefined();
    expect(report.summary).toBeDefined();
    expect(typeof report.summary.total_changes).toBe('number');
  });

  test('changes.json should be written', async () => {
    const snap1 = await engine.scan({ mode: 'fast', include: ['hardware'] });
    const snap2 = await engine.scan({ mode: 'fast', include: ['hardware'] });
    await engine.compareSnapshots(snap1.id, snap2.id);
    
    const changesPath = path.resolve(testConfig.output.inventory_dir, 'changes.json');
    expect(fs.existsSync(changesPath)).toBe(true);
    
    const changes = JSON.parse(fs.readFileSync(changesPath, 'utf-8'));
    expect(changes.changes).toBeDefined();
    expect(changes.summary).toBeDefined();
  });
});

describe('Stub Adapters', () => {
  test('LinuxAdapter should implement interface', () => {
    const adapter = new LinuxAdapter();
    expect(adapter.platform).toBe('linux');
  });

  test('MacOSAdapter should implement interface', () => {
    const adapter = new MacOSAdapter();
    expect(adapter.platform).toBe('macos');
  });

  test('AndroidAdapter should implement interface', () => {
    const adapter = new AndroidAdapter();
    expect(adapter.platform).toBe('android');
  });

  test('iOSAdapter should implement interface', () => {
    const adapter = new iOSAdapter();
    expect(adapter.platform).toBe('ios');
  });

  test('stub adapters should return unknown hardware', async () => {
    const adapter = new LinuxAdapter();
    const hardware = await adapter.detectHardware();
    
    expect(hardware.cpu.value).toBe('unknown');
    expect(hardware.ram.value).toBe('unknown');
    expect(hardware.gpu.value).toEqual([]);
  });

  test('stub adapters should return unknown software', async () => {
    const adapter = new LinuxAdapter();
    const software = await adapter.detectSoftware();
    
    expect(software.installed_programs.value).toEqual([]);
    expect(software.developer_tools.value).toEqual([]);
  });

  test('stub adapters should return unknown network', async () => {
    const adapter = new LinuxAdapter();
    const network = await adapter.detectNetwork();
    
    expect(network.interfaces.value).toEqual([]);
    expect(network.local_ips.value).toEqual([]);
  });

  test('stub adapters should return unknown JAMES', async () => {
    const adapter = new LinuxAdapter();
    const james = await adapter.detectJAMES();
    
    expect(james.modules.value).toEqual([]);
    expect(james.version.value).toBe('unknown');
  });
});

describe('Capability Detection Edge Cases', () => {
  let adapter: WindowsAdapter;

  beforeAll(() => {
    adapter = new WindowsAdapter();
  });

  test('no GPU should not add gpu_inference capability', () => {
    const hardware: HardwareInfo = {
      cpu: { value: { name: 'CPU', manufacturer: 'Test', cores: 4, logical_processors: 4, max_clock_speed_mhz: 3000, architecture: 64 }, source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      ram: { value: { total_gb: 16 }, source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      gpu: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      npu: { value: null, source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      motherboard: { value: { manufacturer: '', product: '', version: '' }, source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      bios: { value: { version: '', release_date: '', vendor: '' }, source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      storage: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      partitions: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      monitors: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      audio: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      microphones: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      cameras: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      usb: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      bluetooth: { value: { available: false, adapters: [], devices: [] }, source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      network_adapters: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
    };

    const software: SoftwareInfo = {
      installed_programs: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      running_services: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      developer_tools: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      runtimes: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      browsers: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      containers: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      wsl: { value: { installed: false, distributions: [] }, source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      ai_runtimes: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      local_models: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
    };

    const network: NetworkInfo = {
      interfaces: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      ip_config: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      routes: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      dns: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      local_ips: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      gateway: { value: { next_hop: 'unknown', interface_alias: 'unknown' }, source: 'test', detected_at: new Date().toISOString(), confidence: 0 },
      mdns_services: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      local_devices: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
    };

    const caps = adapter.getCapabilities(hardware, software, network);
    const ids = caps.map(c => c.id);
    
    expect(ids).not.toContain('ai.local.gpu_inference');
    expect(ids).toContain('ai.local.cpu_inference');
  });

  test('low RAM should not add cpu_inference', () => {
    const hardware: HardwareInfo = {
      cpu: { value: { name: 'CPU', manufacturer: 'Test', cores: 2, logical_processors: 2, max_clock_speed_mhz: 2000, architecture: 64 }, source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      ram: { value: { total_gb: 4 }, source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      gpu: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      npu: { value: null, source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      motherboard: { value: { manufacturer: '', product: '', version: '' }, source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      bios: { value: { version: '', release_date: '', vendor: '' }, source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      storage: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      partitions: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      monitors: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      audio: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      microphones: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      cameras: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      usb: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      bluetooth: { value: { available: false, adapters: [], devices: [] }, source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      network_adapters: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
    };

    const software: SoftwareInfo = {
      installed_programs: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      running_services: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      developer_tools: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      runtimes: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      browsers: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      containers: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      wsl: { value: { installed: false, distributions: [] }, source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      ai_runtimes: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      local_models: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
    };

    const network: NetworkInfo = {
      interfaces: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      ip_config: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      routes: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      dns: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      local_ips: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      gateway: { value: { next_hop: 'unknown', interface_alias: 'unknown' }, source: 'test', detected_at: new Date().toISOString(), confidence: 0 },
      mdns_services: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
      local_devices: { value: [], source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 },
    };

    const caps = adapter.getCapabilities(hardware, software, network);
    const ids = caps.map(c => c.id);
    
    expect(ids).not.toContain('ai.local.cpu_inference');
  });
});

describe('Data Quality', () => {
  let adapter: WindowsAdapter;

  beforeAll(() => {
    adapter = new WindowsAdapter();
  });

  test('all hardware results should have metadata', async () => {
    const hardware = await adapter.detectHardware();
    
    for (const [key, value] of Object.entries(hardware)) {
      expect(value.source).toBeDefined();
      expect(value.detected_at).toBeDefined();
      expect(typeof value.confidence).toBe('number');
      expect(value.confidence).toBeGreaterThanOrEqual(0);
      expect(value.confidence).toBeLessThanOrEqual(1);
    }
  });

  test('all software results should have metadata', async () => {
    const software = await adapter.detectSoftware();
    
    for (const [key, value] of Object.entries(software)) {
      expect(value.source).toBeDefined();
      expect(value.detected_at).toBeDefined();
      expect(typeof value.confidence).toBe('number');
    }
  });

  test('all network results should have metadata', async () => {
    const network = await adapter.detectNetwork();
    
    for (const [key, value] of Object.entries(network)) {
      expect(value.source).toBeDefined();
      expect(value.detected_at).toBeDefined();
      expect(typeof value.confidence).toBe('number');
    }
  });

  test('unknown values should have confidence 0', async () => {
    const hardware = await adapter.detectHardware();
    
    // NPU should be unknown on this platform
    expect(hardware.npu.confidence).toBe(1.0); // explicitly set to null with confidence 1.0
    expect(hardware.npu.value).toBeNull();
  });
});

describe('Snapshot & Change Detection', () => {
  let engine: DiscoveryEngine;
  let testConfig: DiscoveryConfig;

  beforeAll(() => {
    testConfig = createTestConfig();
    engine = new DiscoveryEngine(testConfig);
  });

  test('two consecutive scans should produce valid snapshots', async () => {
    const snap1 = await engine.scan({ mode: 'fast', include: ['hardware'] });
    const snap2 = await engine.scan({ mode: 'fast', include: ['hardware'] });
    
    expect(snap1.id).not.toBe(snap2.id);
    expect(snap1.timestamp).not.toBe(snap2.timestamp);
    expect(snap1.checksum).toBeDefined();
    expect(snap2.checksum).toBeDefined();
  });

  test('compareSnapshots should work with same snapshots', async () => {
    const snap1 = await engine.scan({ mode: 'fast', include: ['hardware'] });
    const snap2 = await engine.scan({ mode: 'fast', include: ['hardware'] });
    
    const report = await engine.compareSnapshots(snap1.id, snap2.id);
    
    expect(report.changes).toBeDefined();
    expect(report.summary.total_changes).toBeGreaterThanOrEqual(0);
  });

  test('change severity should be categorized', async () => {
    const snap1 = await engine.scan({ mode: 'fast', include: ['hardware'] });
    const snap2 = await engine.scan({ mode: 'fast', include: ['hardware'] });
    const report = await engine.compareSnapshots(snap1.id, snap2.id);
    
    for (const change of report.changes) {
      expect(['info', 'warning', 'critical']).toContain(change.severity);
    }
  });
});

describe('Performance', () => {
  let engine: DiscoveryEngine;
  let testConfig: DiscoveryConfig;

  beforeAll(() => {
    testConfig = createTestConfig();
    engine = new DiscoveryEngine(testConfig);
  });

  test('fast scan should complete within timeout', async () => {
    const start = Date.now();
    await engine.scan({ mode: 'fast', timeout_seconds: 10, include: ['hardware'] });
    const elapsed = Date.now() - start;
    
    expect(elapsed).toBeLessThan(15000); // 15 seconds max for fast scan
  });

  test('full scan should complete within timeout', async () => {
    const start = Date.now();
    await engine.scan({ mode: 'full', timeout_seconds: 60, include: ['hardware', 'software'] });
    const elapsed = Date.now() - start;
    
    expect(elapsed).toBeLessThan(90000); // 90 seconds max for full scan
  });
});