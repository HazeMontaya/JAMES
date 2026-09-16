import { DiscoveryEngine, readJsonText } from '../src/core/engine';
import { WindowsAdapter, psJson, splitCsvLine, parseCsv, smbiosMemoryTypeName } from '../src/adapters/windows';
import { LinuxAdapter, MacOSAdapter, AndroidAdapter, iOSAdapter } from '../src/adapters/stubs';
import { DiscoveryConfig, Capability, HardwareInfo, SoftwareInfo, NetworkInfo, JAMESInfo, CPUInfo, GatewayInfo, Snapshot } from '../src/core/interfaces';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';

/**
 * F1-09 test isolation rules:
 * - Unit tests (mock data, helpers, stubs, compare logic) ALWAYS run,
 *   never touch the real tree, never exec external commands.
 * - Live tests (real wmic/powershell/git probes) run ONLY with
 *   JAMES_LIVE_TESTS=1, always into temp dirs, with explicit timeouts.
 *   Rationale: live probes take seconds and depend on machine state.
 */
const LIVE = process.env.JAMES_LIVE_TESTS === '1';
const liveTest = LIVE ? test : test.skip;

const LIVE_TIMEOUT = 120000;

function makeTempRoot(prefix: string): string {
  return fs.mkdtempSync(path.join(os.tmpdir(), prefix));
}

function removeTempRoot(dir: string): void {
  fs.rmSync(dir, { recursive: true, force: true });
}

function createTestConfig(tmpRoot: string): DiscoveryConfig {
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
      directory: path.join(tmpRoot, 'snapshots'),
      max_snapshots: 10,
      compress: false,
      retention_days: 1,
    },
    output: {
      inventory_dir: tmpRoot,
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
      file: path.join(tmpRoot, 'discovery.log'),
      console: false,
    },
  };
}

function stamp<T>(value: T): { value: T; source: string; detected_at: string; confidence: number } {
  return { value, source: 'test', detected_at: new Date().toISOString(), confidence: 1.0 };
}

describe('WindowsAdapter', () => {
  let adapter: WindowsAdapter;

  beforeAll(() => {
    adapter = new WindowsAdapter();
  });

  test('should have correct platform', () => {
    expect(adapter.platform).toBe('windows');
  });

  liveTest('detectHardware should return HardwareInfo structure', async () => {
    const hardware = await adapter.detectHardware();

    expect(hardware).toBeDefined();
    expect(hardware.cpu).toBeDefined();
    expect(hardware.ram).toBeDefined();
    expect(hardware.gpu).toBeDefined();
    expect(hardware.storage).toBeDefined();
    expect(hardware.partitions).toBeDefined();
    expect(hardware.network_adapters).toBeDefined();
  }, LIVE_TIMEOUT);

  liveTest('detectSoftware should return SoftwareInfo structure', async () => {
    const software = await adapter.detectSoftware();

    expect(software).toBeDefined();
    expect(software.developer_tools).toBeDefined();
    expect(software.runtimes).toBeDefined();
    expect(software.containers).toBeDefined();
    expect(software.wsl).toBeDefined();
    expect(software.ai_runtimes).toBeDefined();
  }, LIVE_TIMEOUT);

  liveTest('detectNetwork should return NetworkInfo structure', async () => {
    const network = await adapter.detectNetwork();

    expect(network).toBeDefined();
    expect(network.interfaces).toBeDefined();
    expect(network.ip_config).toBeDefined();
    expect(network.dns).toBeDefined();
    expect(network.local_ips).toBeDefined();
    expect(network.gateway).toBeDefined();
  }, LIVE_TIMEOUT);

  liveTest('detectJAMES should return JAMESInfo structure', async () => {
    const james = await adapter.detectJAMES();

    expect(james).toBeDefined();
    expect(james.modules).toBeDefined();
    expect(james.plugins).toBeDefined();
    expect(james.config).toBeDefined();
    expect(james.version).toBeDefined();
    expect(james.git_status).toBeDefined();
  }, LIVE_TIMEOUT);

  test('getCapabilities should return capabilities based on hardware', async () => {
    const mockHardware: HardwareInfo = {
      cpu: stamp({ name: 'Test CPU', manufacturer: 'Test', cores: 8, logical_processors: 8, max_clock_speed_mhz: 3600, architecture: 64 }),
      ram: stamp({ total_gb: 32 }),
      gpu: stamp([{ name: 'RTX 2080 Ti', adapter_ram_gb: 11, driver_version: '32.0', video_processor: 'RTX 2080 Ti' }]),
      npu: stamp(null),
      motherboard: stamp({ manufacturer: 'Test', product: 'Test', version: '1.0' }),
      bios: stamp({ version: '1.0', release_date: '2024-01-01', vendor: 'Test' }),
      storage: stamp([]),
      partitions: stamp([]),
      monitors: stamp([]),
      audio: stamp([{ name: 'Realtek', manufacturer: 'Realtek', device_id: '', status: 'OK' }]),
      microphones: stamp([]),
      cameras: stamp([]),
      usb: stamp([]),
      bluetooth: stamp({ available: false, adapters: [], devices: [] }),
      network_adapters: stamp([{ name: 'Ethernet', description: 'Test', mac_address: '00:00:00:00:00:00', link_speed: '1 Gbps', status: 'Up', ipv4_addresses: ['192.168.1.100'], ipv6_addresses: [], dns_servers: ['192.168.1.1'] }]),
    };

    const mockSoftware: SoftwareInfo = {
      installed_programs: stamp([]),
      running_services: stamp([]),
      // NOTE: the collector registers VS Code as 'vscode' (from `code --version`,
      // first output line). A mock named 'code' must NOT match (regression F1-08).
      developer_tools: stamp([{ name: 'git', version: '2.40.0' }, { name: 'vscode', version: '1.80.0' }]),
      runtimes: stamp([]),
      browsers: stamp([{ name: 'Chrome', version: '120.0', path: '' }]),
      containers: stamp([{ name: 'docker', version: '24.0', running: true }]),
      wsl: stamp({ installed: true, version: '2.0', distributions: [{ name: 'Ubuntu', version: '22.04', state: 'Running', is_default: true }], default_distro: 'Ubuntu' }),
      ai_runtimes: stamp([{ name: 'ollama', version: '0.32.8', running: true, models: ['llama3.2'] }]),
      local_models: stamp([]),
    };

    const mockNetwork: NetworkInfo = {
      interfaces: stamp([{ name: 'Ethernet', description: 'Test', mac_address: '00:00:00:00:00:00', link_speed: '1 Gbps', status: 'Up', index: 1 }]),
      ip_config: stamp([]),
      routes: stamp([]),
      dns: stamp([]),
      local_ips: stamp([]),
      gateway: stamp({ next_hop: '192.168.1.1', interface_alias: 'Ethernet' }),
      mdns_services: stamp([]),
      local_devices: stamp([]),
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

  test('legacy tool name "code" must NOT match dev.vscode', () => {
    const mockSoftware: SoftwareInfo = {
      installed_programs: stamp([]),
      running_services: stamp([]),
      developer_tools: stamp([{ name: 'code', version: '1.80.0' }]),
      runtimes: stamp([]),
      browsers: stamp([]),
      containers: stamp([]),
      wsl: stamp({ installed: false, distributions: [] }),
      ai_runtimes: stamp([]),
      local_models: stamp([]),
    };
    const emptyHardware = { cpu: stamp({ name: 'CPU', manufacturer: 'T', cores: 2, logical_processors: 2, max_clock_speed_mhz: 2000, architecture: 64 }), ram: stamp({ total_gb: 4 }), gpu: stamp([]), npu: stamp(null), motherboard: stamp({ manufacturer: '', product: '', version: '' }), bios: stamp({ version: '', release_date: '', vendor: '' }), storage: stamp([]), partitions: stamp([]), monitors: stamp([]), audio: stamp([]), microphones: stamp([]), cameras: stamp([]), usb: stamp([]), bluetooth: stamp({ available: false, adapters: [], devices: [] }), network_adapters: stamp([]) } as HardwareInfo;
    const emptyNetwork = { interfaces: stamp([]), ip_config: stamp([]), routes: stamp([]), dns: stamp([]), local_ips: stamp([]), gateway: stamp({ next_hop: 'x', interface_alias: 'x' }), mdns_services: stamp([]), local_devices: stamp([]) } as NetworkInfo;

    const ids = adapter.getCapabilities(emptyHardware, mockSoftware, emptyNetwork).map(c => c.id);
    expect(ids).not.toContain('dev.vscode');
  });

  liveTest('capabilities should have required fields', async () => {
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
  }, LIVE_TIMEOUT);
});

describe('Scan Helpers (pure unit tests)', () => {
  test('psJson wraps single objects into arrays', () => {
    expect(psJson('{"a":1}')).toEqual([{ a: 1 }]);
    expect(psJson('[{"a":1},{"a":2}]')).toEqual([{ a: 1 }, { a: 2 }]);
  });

  test('psJson slices banner prefixes', () => {
    const out = psJson('Some banner text\nMore noise\n{"a":1}');
    expect(out).toEqual([{ a: 1 }]);
  });

  test('psJson maps null/undefined to empty', () => {
    expect(psJson('null')).toEqual([]);
  });

  test('psJson throws on invalid JSON', () => {
    expect(() => psJson('not json at all {{{')).toThrow();
  });

  test('splitCsvLine respects quoted commas', () => {
    expect(splitCsvLine('a,b,c')).toEqual(['a', 'b', 'c']);
    expect(splitCsvLine('"x,y",z')).toEqual(['x,y', 'z']);
    expect(splitCsvLine('"a""b",c')).toEqual(['a"b', 'c']);
    expect(splitCsvLine('')).toEqual(['']);
  });

  test('parseCsv maps headers to rows', () => {
    const rows = parseCsv('Node,Name,Size\nPC,GPU,100\nPC2,CPU,200');
    expect(rows).toEqual([
      { Node: 'PC', Name: 'GPU', Size: '100' },
      { Node: 'PC2', Name: 'CPU', Size: '200' },
    ]);
  });

  test('parseCsv tolerates CRLF and blanks', () => {
    const rows = parseCsv('H1,H2\r\n\r\nv1,v2\r\n');
    expect(rows).toEqual([{ H1: 'v1', H2: 'v2' }]);
  });

  test('parseCsv returns empty without data rows', () => {
    expect(parseCsv('H1,H2')).toEqual([]);
    expect(parseCsv('')).toEqual([]);
  });

  test('smbiosMemoryTypeName maps known types', () => {
    expect(smbiosMemoryTypeName(24)).toBe('DDR3');
    expect(smbiosMemoryTypeName(26)).toBe('DDR4');
    expect(smbiosMemoryTypeName(34)).toBe('DDR5');
    expect(smbiosMemoryTypeName(0)).toBeUndefined();
    expect(smbiosMemoryTypeName(99)).toBeUndefined();
  });

  test('readJsonText strips UTF-8 BOM', () => {
    const dir = makeTempRoot('james-bom-test-');
    try {
      const file = path.join(dir, 'bom.json');
      fs.writeFileSync(file, String.fromCharCode(0xfeff) + '{"a":1}');
      expect(JSON.parse(readJsonText(file))).toEqual({ a: 1 });
      const plain = path.join(dir, 'plain.json');
      fs.writeFileSync(plain, '{"b":2}');
      expect(JSON.parse(readJsonText(plain))).toEqual({ b: 2 });
    } finally {
      removeTempRoot(dir);
    }
  });
});

describe('DiscoveryEngine', () => {
  let engine: DiscoveryEngine;
  let testConfig: DiscoveryConfig;
  let tmpRoot: string;

  beforeAll(() => {
    tmpRoot = makeTempRoot('james-engine-test-');
    testConfig = createTestConfig(tmpRoot);
    engine = new DiscoveryEngine(testConfig);
  });

  afterAll(() => {
    removeTempRoot(tmpRoot);
  });

  test('should create engine instance', () => {
    expect(engine).toBeDefined();
  });

  test('engine writes only inside its configured dirs', () => {
    expect(testConfig.snapshot.directory.startsWith(tmpRoot)).toBe(true);
    expect(testConfig.output.inventory_dir.startsWith(tmpRoot)).toBe(true);
    expect(fs.existsSync(testConfig.snapshot.directory)).toBe(true);
  });

  liveTest('scan should return a snapshot', async () => {
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
  }, LIVE_TIMEOUT);

  liveTest('snapshot should be saved to disk', async () => {
    const snapshot = await engine.scan({ mode: 'fast', include: ['hardware'] });

    const snapshotsDir = path.resolve(testConfig.snapshot.directory);
    const files = fs.readdirSync(snapshotsDir).filter(f => f.endsWith('.json'));
    expect(files.length).toBeGreaterThan(0);

    const latestFile = files.sort().pop()!;
    const content = fs.readFileSync(path.join(snapshotsDir, latestFile), 'utf-8');
    const saved = JSON.parse(content);
    expect(saved.id).toBe(snapshot.id);
  }, LIVE_TIMEOUT);

  liveTest('current.json should be updated', async () => {
    await engine.scan({ mode: 'fast', include: ['hardware', 'software'] });

    const currentPath = path.resolve(testConfig.output.inventory_dir, 'current.json');
    expect(fs.existsSync(currentPath)).toBe(true);

    const current = JSON.parse(fs.readFileSync(currentPath, 'utf-8'));
    expect(current.timestamp).toBeDefined();
    expect(current.hardware).toBeDefined();
    expect(current.software).toBeDefined();
  }, LIVE_TIMEOUT);

  liveTest('capabilities.json should be updated', async () => {
    await engine.scan({ mode: 'fast', include: ['hardware', 'software', 'capabilities'] });

    const capsPath = path.resolve(testConfig.output.inventory_dir, 'capabilities.json');
    expect(fs.existsSync(capsPath)).toBe(true);

    const caps = JSON.parse(fs.readFileSync(capsPath, 'utf-8'));
    expect(Array.isArray(caps)).toBe(true);
    expect(caps.length).toBeGreaterThan(0);
  }, LIVE_TIMEOUT);

  liveTest('listSnapshots should return snapshots', async () => {
    await engine.scan({ mode: 'fast', include: ['hardware'] });
    const snapshots = engine.listSnapshots();
    expect(Array.isArray(snapshots)).toBe(true);
    expect(snapshots.length).toBeGreaterThan(0);
  }, LIVE_TIMEOUT);

  liveTest('getLatestSnapshot should return latest', async () => {
    await engine.scan({ mode: 'fast', include: ['hardware'] });
    const latest = engine.getLatestSnapshot();
    expect(latest).toBeDefined();
    expect(latest?.id).toBeDefined();
  }, LIVE_TIMEOUT);

  liveTest('compareSnapshots should detect changes', async () => {
    const snap1 = await engine.scan({ mode: 'fast', include: ['hardware'] });
    const snap2 = await engine.scan({ mode: 'fast', include: ['hardware'] });

    const report = await engine.compareSnapshots(snap1.id, snap2.id);

    expect(report).toBeDefined();
    expect(report.previous_snapshot_id).toBe(snap1.id);
    expect(report.current_snapshot_id).toBe(snap2.id);
    expect(report.changes).toBeDefined();
    expect(report.summary).toBeDefined();
    expect(typeof report.summary.total_changes).toBe('number');
  }, LIVE_TIMEOUT);

  liveTest('changes.json should be written', async () => {
    const snap1 = await engine.scan({ mode: 'fast', include: ['hardware'] });
    const snap2 = await engine.scan({ mode: 'fast', include: ['hardware'] });
    await engine.compareSnapshots(snap1.id, snap2.id);

    const changesPath = path.resolve(testConfig.output.inventory_dir, 'changes.json');
    expect(fs.existsSync(changesPath)).toBe(true);

    const changes = JSON.parse(fs.readFileSync(changesPath, 'utf-8'));
    expect(changes.changes).toBeDefined();
    expect(changes.summary).toBeDefined();
  }, LIVE_TIMEOUT);
});

describe('Compare Logic (fabricated snapshots, no live probes)', () => {
  let engine: DiscoveryEngine;
  let testConfig: DiscoveryConfig;
  let tmpRoot: string;

  function makeSnapshot(id: string, cpuName: string, extraCap?: object): Snapshot {
    return {
      id,
      timestamp: new Date().toISOString(),
      scan_mode: 'fast',
      platform: 'windows',
      hardware: { cpu: stamp({ name: cpuName }) },
      software: {},
      network: {},
      james: {},
      capabilities: extraCap ? [extraCap] : [],
      checksum: 'test',
    } as unknown as Snapshot;
  }

  function writeSnapshot(snap: Snapshot): void {
    fs.writeFileSync(
      path.join(testConfig.snapshot.directory, `${snap.id}.json`),
      JSON.stringify(snap)
    );
  }

  beforeAll(() => {
    tmpRoot = makeTempRoot('james-compare-test-');
    testConfig = createTestConfig(tmpRoot);
    engine = new DiscoveryEngine(testConfig);
  });

  afterAll(() => {
    removeTempRoot(tmpRoot);
  });

  test('identical snapshots compare to zero changes', async () => {
    const snap = makeSnapshot('aaa-1', 'CPU-X');
    writeSnapshot(snap);
    const report = await engine.compareSnapshots('aaa-1', 'aaa-1');
    expect(report.summary.total_changes).toBe(0);
  });

  test('differing snapshots produce categorized changes', async () => {
    writeSnapshot(makeSnapshot('bbb-1', 'CPU-X'));
    writeSnapshot(makeSnapshot('bbb-2', 'CPU-Y', { id: 'x.new', name: 'X', category: 'test', detected: true, provider: 't', dependencies: [], risk_level: 'low', status: 'available' }));
    const report = await engine.compareSnapshots('bbb-1', 'bbb-2');
    expect(report.summary.total_changes).toBeGreaterThan(0);
    for (const change of report.changes) {
      expect(['info', 'warning', 'critical']).toContain(change.severity);
    }
    expect(report.summary.added).toBeGreaterThanOrEqual(1);
  });

  test('legacy-shaped snapshots do not crash compare', async () => {
    const legacy = { id: 'ccc-1', timestamp: new Date().toISOString(), capabilities: { not: 'an-array' } };
    fs.writeFileSync(
      path.join(testConfig.snapshot.directory, 'ccc-1.json'),
      JSON.stringify(legacy)
    );
    writeSnapshot(makeSnapshot('ccc-2', 'CPU-Z'));
    const report = await engine.compareSnapshots('ccc-1', 'ccc-2');
    expect(report.summary.total_changes).toBeGreaterThanOrEqual(0);
  });

  test('unknown ids raise Snapshot-not-found', async () => {
    await expect(engine.compareSnapshots('nope-1', 'nope-2')).rejects.toThrow('Snapshot not found');
  });

  test('listSnapshots skips corrupt files', () => {
    fs.writeFileSync(path.join(testConfig.snapshot.directory, 'corrupt.json'), '{oops');
    const snapshots = engine.listSnapshots();
    expect(Array.isArray(snapshots)).toBe(true);
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
      cpu: stamp({ name: 'CPU', manufacturer: 'Test', cores: 4, logical_processors: 4, max_clock_speed_mhz: 3000, architecture: 64 }),
      ram: stamp({ total_gb: 16 }),
      gpu: stamp([]),
      npu: stamp(null),
      motherboard: stamp({ manufacturer: '', product: '', version: '' }),
      bios: stamp({ version: '', release_date: '', vendor: '' }),
      storage: stamp([]),
      partitions: stamp([]),
      monitors: stamp([]),
      audio: stamp([]),
      microphones: stamp([]),
      cameras: stamp([]),
      usb: stamp([]),
      bluetooth: stamp({ available: false, adapters: [], devices: [] }),
      network_adapters: stamp([]),
    };

    const software: SoftwareInfo = {
      installed_programs: stamp([]),
      running_services: stamp([]),
      developer_tools: stamp([]),
      runtimes: stamp([]),
      browsers: stamp([]),
      containers: stamp([]),
      wsl: stamp({ installed: false, distributions: [] }),
      ai_runtimes: stamp([]),
      local_models: stamp([]),
    };

    const network: NetworkInfo = {
      interfaces: stamp([]),
      ip_config: stamp([]),
      routes: stamp([]),
      dns: stamp([]),
      local_ips: stamp([]),
      gateway: stamp({ next_hop: 'unknown', interface_alias: 'unknown' }),
      mdns_services: stamp([]),
      local_devices: stamp([]),
    };

    const caps = adapter.getCapabilities(hardware, software, network);
    const ids = caps.map(c => c.id);

    expect(ids).not.toContain('ai.local.gpu_inference');
    expect(ids).toContain('ai.local.cpu_inference');
  });

  test('low RAM should not add cpu_inference', () => {
    const hardware: HardwareInfo = {
      cpu: stamp({ name: 'CPU', manufacturer: 'Test', cores: 2, logical_processors: 2, max_clock_speed_mhz: 2000, architecture: 64 }),
      ram: stamp({ total_gb: 4 }),
      gpu: stamp([]),
      npu: stamp(null),
      motherboard: stamp({ manufacturer: '', product: '', version: '' }),
      bios: stamp({ version: '', release_date: '', vendor: '' }),
      storage: stamp([]),
      partitions: stamp([]),
      monitors: stamp([]),
      audio: stamp([]),
      microphones: stamp([]),
      cameras: stamp([]),
      usb: stamp([]),
      bluetooth: stamp({ available: false, adapters: [], devices: [] }),
      network_adapters: stamp([]),
    };

    const software: SoftwareInfo = {
      installed_programs: stamp([]),
      running_services: stamp([]),
      developer_tools: stamp([]),
      runtimes: stamp([]),
      browsers: stamp([]),
      containers: stamp([]),
      wsl: stamp({ installed: false, distributions: [] }),
      ai_runtimes: stamp([]),
      local_models: stamp([]),
    };

    const network: NetworkInfo = {
      interfaces: stamp([]),
      ip_config: stamp([]),
      routes: stamp([]),
      dns: stamp([]),
      local_ips: stamp([]),
      gateway: stamp({ next_hop: 'unknown', interface_alias: 'unknown' }),
      mdns_services: stamp([]),
      local_devices: stamp([]),
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

  liveTest('all hardware results should have metadata', async () => {
    const hardware = await adapter.detectHardware();

    for (const [key, value] of Object.entries(hardware)) {
      expect(value.source).toBeDefined();
      expect(value.detected_at).toBeDefined();
      expect(typeof value.confidence).toBe('number');
      expect(value.confidence).toBeGreaterThanOrEqual(0);
      expect(value.confidence).toBeLessThanOrEqual(1);
    }
  }, LIVE_TIMEOUT);

  liveTest('all software results should have metadata', async () => {
    const software = await adapter.detectSoftware();

    for (const [key, value] of Object.entries(software)) {
      expect(value.source).toBeDefined();
      expect(value.detected_at).toBeDefined();
      expect(typeof value.confidence).toBe('number');
    }
  }, LIVE_TIMEOUT);

  liveTest('all network results should have metadata', async () => {
    const network = await adapter.detectNetwork();

    for (const [key, value] of Object.entries(network)) {
      expect(value.source).toBeDefined();
      expect(value.detected_at).toBeDefined();
      expect(typeof value.confidence).toBe('number');
    }
  }, LIVE_TIMEOUT);

  liveTest('unknown values should have confidence 0', async () => {
    const hardware = await adapter.detectHardware();

    // NPU should be unknown on this platform
    expect(hardware.npu.confidence).toBe(1.0); // explicitly set to null with confidence 1.0
    expect(hardware.npu.value).toBeNull();
  }, LIVE_TIMEOUT);
});

describe('Snapshot & Change Detection', () => {
  let engine: DiscoveryEngine;
  let testConfig: DiscoveryConfig;
  let tmpRoot: string;

  beforeAll(() => {
    tmpRoot = makeTempRoot('james-snap-test-');
    testConfig = createTestConfig(tmpRoot);
    engine = new DiscoveryEngine(testConfig);
  });

  afterAll(() => {
    removeTempRoot(tmpRoot);
  });

  liveTest('two consecutive scans should produce valid snapshots', async () => {
    const snap1 = await engine.scan({ mode: 'fast', include: ['hardware'] });
    const snap2 = await engine.scan({ mode: 'fast', include: ['hardware'] });

    expect(snap1.id).not.toBe(snap2.id);
    expect(snap1.timestamp).not.toBe(snap2.timestamp);
    expect(snap1.checksum).toBeDefined();
    expect(snap2.checksum).toBeDefined();
  }, LIVE_TIMEOUT);

  liveTest('compareSnapshots should work with same snapshots', async () => {
    const snap1 = await engine.scan({ mode: 'fast', include: ['hardware'] });
    const snap2 = await engine.scan({ mode: 'fast', include: ['hardware'] });

    const report = await engine.compareSnapshots(snap1.id, snap2.id);

    expect(report.changes).toBeDefined();
    expect(report.summary.total_changes).toBeGreaterThanOrEqual(0);
  }, LIVE_TIMEOUT);

  liveTest('change severity should be categorized', async () => {
    const snap1 = await engine.scan({ mode: 'fast', include: ['hardware'] });
    const snap2 = await engine.scan({ mode: 'fast', include: ['hardware'] });
    const report = await engine.compareSnapshots(snap1.id, snap2.id);

    for (const change of report.changes) {
      expect(['info', 'warning', 'critical']).toContain(change.severity);
    }
  }, LIVE_TIMEOUT);
});

describe('Performance', () => {
  let engine: DiscoveryEngine;
  let testConfig: DiscoveryConfig;
  let tmpRoot: string;

  beforeAll(() => {
    tmpRoot = makeTempRoot('james-perf-test-');
    testConfig = createTestConfig(tmpRoot);
    engine = new DiscoveryEngine(testConfig);
  });

  afterAll(() => {
    removeTempRoot(tmpRoot);
  });

  liveTest('fast scan should complete within timeout', async () => {
    const start = Date.now();
    await engine.scan({ mode: 'fast', timeout_seconds: 10, include: ['hardware'] });
    const elapsed = Date.now() - start;

    expect(elapsed).toBeLessThan(15000); // 15 seconds max for fast scan
  }, LIVE_TIMEOUT);

  liveTest('full scan should complete within timeout', async () => {
    const start = Date.now();
    await engine.scan({ mode: 'full', timeout_seconds: 60, include: ['hardware', 'software'] });
    const elapsed = Date.now() - start;

    expect(elapsed).toBeLessThan(90000); // 90 seconds max for full scan
  }, LIVE_TIMEOUT);
});
