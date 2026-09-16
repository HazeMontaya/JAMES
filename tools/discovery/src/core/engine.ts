import {
  PlatformAdapter,
  HardwareInfo,
  SoftwareInfo,
  NetworkInfo,
  JAMESInfo,
  Capability,
  Snapshot,
  ChangeReport,
  Change,
  ChangeSummary,
  ScanConfig,
  DiscoveryConfig,
  DiscoveryResult,
} from '../core/interfaces';
import { WindowsAdapter } from '../adapters/windows';
import { LinuxAdapter } from '../adapters/stubs';
import { MacOSAdapter } from '../adapters/stubs';
import { AndroidAdapter } from '../adapters/stubs';
import { iOSAdapter } from '../adapters/stubs';
import * as fs from 'fs';
import * as path from 'path';
import * as crypto from 'crypto';

/**
 * Read a JSON text file, tolerating a leading BOM. Inventory files on
 * Windows are regularly UTF-8-with-BOM (PowerShell/legacy writers), and
 * JSON.parse rejects the BOM character.
 */
export function readJsonText(filePath: string): string {
  const content = fs.readFileSync(filePath, 'utf-8');
  return content.charCodeAt(0) === 0xfeff ? content.slice(1) : content;
}

export class DiscoveryEngine {
  private config: DiscoveryConfig;
  private adapter: PlatformAdapter;
  private inventoryDir: string;
  private snapshotsDir: string;

  constructor(config: DiscoveryConfig) {
    this.config = config;
    this.adapter = this.createAdapter(config.adapters.platform);
    this.inventoryDir = path.resolve(config.output.inventory_dir);
    this.snapshotsDir = path.resolve(config.snapshot.directory);
    this.ensureDirectories();
  }

  private createAdapter(platform: string): PlatformAdapter {
    switch (platform) {
      case 'windows':
        return new WindowsAdapter();
      case 'linux':
        return new LinuxAdapter();
      case 'macos':
        return new MacOSAdapter();
      case 'android':
        return new AndroidAdapter();
      case 'ios':
        return new iOSAdapter();
      case 'auto':
      default:
        const detectedPlatform = process.platform;
        if (detectedPlatform === 'win32') return new WindowsAdapter();
        if (detectedPlatform === 'linux') return new LinuxAdapter();
        if (detectedPlatform === 'darwin') return new MacOSAdapter();
        return new WindowsAdapter();
    }
  }

  private ensureDirectories(): void {
    if (!fs.existsSync(this.inventoryDir)) {
      fs.mkdirSync(this.inventoryDir, { recursive: true });
    }
    if (!fs.existsSync(this.snapshotsDir)) {
      fs.mkdirSync(this.snapshotsDir, { recursive: true });
    }
  }

  async scan(config?: Partial<ScanConfig>): Promise<Snapshot> {
    const scanConfig: ScanConfig = {
      mode: config?.mode || this.config.scan.mode,
      timeout_seconds: config?.timeout_seconds || this.config.scan.timeout_seconds,
      parallel: config?.parallel ?? this.config.scan.parallel,
      include: config?.include || this.config.scan.include,
      exclude: config?.exclude || this.config.scan.exclude,
    };

    const startTime = Date.now();
    const timeout = scanConfig.timeout_seconds * 1000;

    const hardwarePromise = scanConfig.include.includes('hardware') ? this.adapter.detectHardware() : Promise.resolve(this.createEmptyHardware());
    const softwarePromise = scanConfig.include.includes('software') ? this.adapter.detectSoftware() : Promise.resolve(this.createEmptySoftware());
    const networkPromise = scanConfig.include.includes('network') ? this.adapter.detectNetwork() : Promise.resolve(this.createEmptyNetwork());
    const jamesPromise = scanConfig.include.includes('james') ? this.adapter.detectJAMES() : Promise.resolve(this.createEmptyJAMES());

    const [hardware, software, network, james] = await Promise.all([
      this.withTimeout(hardwarePromise, timeout, 'hardware'),
      this.withTimeout(softwarePromise, timeout, 'software'),
      this.withTimeout(networkPromise, timeout, 'network'),
      this.withTimeout(jamesPromise, timeout, 'james'),
    ]);

    let capabilities: Capability[] = [];
    if (scanConfig.include.includes('capabilities') && this.config.capability_detection.enabled) {
      capabilities = this.adapter.getCapabilities(hardware, software, network);
    }

    const snapshot: Snapshot = {
      id: crypto.randomUUID(),
      timestamp: new Date().toISOString(),
      scan_mode: scanConfig.mode,
      platform: this.adapter.platform,
      hardware,
      software,
      network,
      james,
      capabilities,
      checksum: '',
    };

    snapshot.checksum = this.computeChecksum(snapshot);
    await this.saveSnapshot(snapshot);
    await this.updateCurrentInventory(snapshot);

    return snapshot;
  }

  private async withTimeout<T>(promise: Promise<T>, ms: number, label: string): Promise<T> {
    let timeoutId: NodeJS.Timeout;
    const timeoutPromise = new Promise<never>((_, reject) => {
      timeoutId = setTimeout(() => reject(new Error(`${label} detection timed out after ${ms}ms`)), ms);
    });
    try {
      return await Promise.race([promise, timeoutPromise]);
    } finally {
      clearTimeout(timeoutId!);
    }
  }

  private createEmptyHardware(): HardwareInfo {
    const unknown = (source: string) => ({
      value: 'unknown',
      source,
      detected_at: new Date().toISOString(),
      confidence: 0,
    });
    
    const unknownCPU = (source: string) => ({
      value: { name: 'unknown', manufacturer: 'unknown', cores: 0, logical_processors: 0, max_clock_speed_mhz: 0, architecture: 0 },
      source,
      detected_at: new Date().toISOString(),
      confidence: 0,
    });
    
    const unknownRAM = (source: string) => ({
      value: { total_gb: 0 },
      source,
      detected_at: new Date().toISOString(),
      confidence: 0,
    });
    
    const unknownMotherboard = (source: string) => ({
      value: { manufacturer: 'unknown', product: 'unknown', version: 'unknown' },
      source,
      detected_at: new Date().toISOString(),
      confidence: 0,
    });
    
    const unknownBIOS = (source: string) => ({
      value: { version: 'unknown', release_date: 'unknown', vendor: 'unknown' },
      source,
      detected_at: new Date().toISOString(),
      confidence: 0,
    });
    return {
      cpu: unknownCPU('skipped'),
      ram: unknownRAM('skipped'),
      gpu: { value: [], source: 'skipped', detected_at: new Date().toISOString(), confidence: 0 },
      npu: { value: null, source: 'skipped', detected_at: new Date().toISOString(), confidence: 1.0 },
      motherboard: unknownMotherboard('skipped'),
      bios: unknownBIOS('skipped'),
      storage: { value: [], source: 'skipped', detected_at: new Date().toISOString(), confidence: 0 },
      partitions: { value: [], source: 'skipped', detected_at: new Date().toISOString(), confidence: 0 },
      monitors: { value: [], source: 'skipped', detected_at: new Date().toISOString(), confidence: 0 },
      audio: { value: [], source: 'skipped', detected_at: new Date().toISOString(), confidence: 0 },
      microphones: { value: [], source: 'skipped', detected_at: new Date().toISOString(), confidence: 0 },
      cameras: { value: [], source: 'skipped', detected_at: new Date().toISOString(), confidence: 0 },
      usb: { value: [], source: 'skipped', detected_at: new Date().toISOString(), confidence: 0 },
      bluetooth: { value: { available: false, adapters: [], devices: [] }, source: 'skipped', detected_at: new Date().toISOString(), confidence: 0 },
      network_adapters: { value: [], source: 'skipped', detected_at: new Date().toISOString(), confidence: 0 },
    };
  }

  private createEmptySoftware(): SoftwareInfo {
    const empty = (source: string) => ({
      value: [],
      source,
      detected_at: new Date().toISOString(),
      confidence: 0,
    });
    return {
      installed_programs: empty('skipped'),
      running_services: empty('skipped'),
      developer_tools: empty('skipped'),
      runtimes: empty('skipped'),
      browsers: empty('skipped'),
      containers: empty('skipped'),
      wsl: { value: { installed: false, distributions: [] }, source: 'skipped', detected_at: new Date().toISOString(), confidence: 0 },
      ai_runtimes: empty('skipped'),
      local_models: empty('skipped'),
    };
  }

  private createEmptyNetwork(): NetworkInfo {
    const empty = (source: string) => ({
      value: [],
      source,
      detected_at: new Date().toISOString(),
      confidence: 0,
    });
    return {
      interfaces: empty('skipped'),
      ip_config: empty('skipped'),
      routes: empty('skipped'),
      dns: empty('skipped'),
      local_ips: empty('skipped'),
      gateway: { value: { next_hop: 'unknown', interface_alias: 'unknown' }, source: 'skipped', detected_at: new Date().toISOString(), confidence: 0 },
      mdns_services: empty('skipped'),
      local_devices: empty('skipped'),
    };
  }

  private createEmptyJAMES(): JAMESInfo {
    const empty = (source: string) => ({
      value: [],
      source,
      detected_at: new Date().toISOString(),
      confidence: 0,
    });
    return {
      modules: empty('skipped'),
      plugins: empty('skipped'),
      config: { value: {}, source: 'skipped', detected_at: new Date().toISOString(), confidence: 0 },
      version: { value: 'unknown', source: 'skipped', detected_at: new Date().toISOString(), confidence: 0 },
      git_status: { value: { repo_exists: false }, source: 'skipped', detected_at: new Date().toISOString(), confidence: 0 },
      services: empty('skipped'),
    };
  }

  private computeChecksum(snapshot: Snapshot): string {
    const hash = crypto.createHash('sha256');
    const data = JSON.stringify({
      hardware: snapshot.hardware,
      software: snapshot.software,
      network: snapshot.network,
      james: snapshot.james,
      capabilities: snapshot.capabilities.map(c => c.id),
    });
    hash.update(data);
    return hash.digest('hex').substring(0, 16);
  }

  private async saveSnapshot(snapshot: Snapshot): Promise<void> {
    const filename = `${snapshot.timestamp.replace(/[:.]/g, '-')}_${snapshot.id.substring(0, 8)}.json`;
    const filepath = path.join(this.snapshotsDir, filename);
    
    const output = this.config.output.pretty_print 
      ? JSON.stringify(snapshot, null, 2)
      : JSON.stringify(snapshot);
    
    fs.writeFileSync(filepath, output);
    
    await this.cleanupOldSnapshots();
  }

  private async cleanupOldSnapshots(): Promise<void> {
    const files = fs.readdirSync(this.snapshotsDir)
      .filter(f => f.endsWith('.json'))
      .map(f => ({
        name: f,
        time: fs.statSync(path.join(this.snapshotsDir, f)).mtime.getTime(),
      }))
      .sort((a, b) => b.time - a.time);

    if (files.length > this.config.snapshot.max_snapshots) {
      for (let i = this.config.snapshot.max_snapshots; i < files.length; i++) {
        fs.unlinkSync(path.join(this.snapshotsDir, files[i].name));
      }
    }

    const now = Date.now();
    const retentionMs = this.config.snapshot.retention_days * 24 * 60 * 60 * 1000;
    for (const file of files) {
      if (now - file.time > retentionMs) {
        fs.unlinkSync(path.join(this.snapshotsDir, file.name));
      }
    }
  }

  private async updateCurrentInventory(snapshot: Snapshot): Promise<void> {
    const current = this.snapshotToInventory(snapshot);
    const filepath = path.join(this.inventoryDir, 'current.json');
    const output = this.config.output.pretty_print 
      ? JSON.stringify(current, null, 2)
      : JSON.stringify(current);
    fs.writeFileSync(filepath, output);

    const capabilitiesPath = path.join(this.inventoryDir, 'capabilities.json');
    const capsOutput = this.config.output.pretty_print
      ? JSON.stringify(snapshot.capabilities, null, 2)
      : JSON.stringify(snapshot.capabilities);
    fs.writeFileSync(capabilitiesPath, capsOutput);
  }

  private snapshotToInventory(snapshot: Snapshot): Record<string, unknown> {
    return {
      timestamp: snapshot.timestamp,
      scan_mode: snapshot.scan_mode,
      platform: snapshot.platform,
      hardware: this.extractValues(snapshot.hardware as unknown as Record<string, DiscoveryResult<unknown>>),
      software: this.extractValues(snapshot.software as unknown as Record<string, DiscoveryResult<unknown>>),
      network: this.extractValues(snapshot.network as unknown as Record<string, DiscoveryResult<unknown>>),
      james: this.extractValues(snapshot.james as unknown as Record<string, DiscoveryResult<unknown>>),
      capabilities: snapshot.capabilities,
    };
  }

  private extractValues(obj: Record<string, DiscoveryResult<unknown>>): Record<string, unknown> {
    const result: Record<string, unknown> = {};
    for (const [key, value] of Object.entries(obj)) {
      result[key] = value.value;
      result[`${key}_meta`] = {
        source: value.source,
        detected_at: value.detected_at,
        confidence: value.confidence,
      };
    }
    return result;
  }

  async compareSnapshots(previousId: string, currentId: string): Promise<ChangeReport> {
    const previous = this.loadSnapshot(previousId);
    const current = this.loadSnapshot(currentId);

    if (!previous || !current) {
      throw new Error('Snapshot not found');
    }

    // Legacy snapshots may store sections/capabilities in older shapes
    // (e.g. capabilities as object instead of array). Coerce defensively:
    // a diff tool must report differences, never crash on old data.
    const asSection = (v: unknown): Record<string, DiscoveryResult<unknown>> =>
      typeof v === 'object' && v !== null && !Array.isArray(v)
        ? (v as Record<string, DiscoveryResult<unknown>>)
        : {};
    const asCapabilities = (v: unknown): Capability[] =>
      Array.isArray(v) ? (v as Capability[]) : [];

    const changes: Change[] = [];

    changes.push(...this.compareObjects('hardware', asSection(previous.hardware), asSection(current.hardware)));
    changes.push(...this.compareObjects('software', asSection(previous.software), asSection(current.software)));
    changes.push(...this.compareObjects('network', asSection(previous.network), asSection(current.network)));
    changes.push(...this.compareObjects('james', asSection(previous.james), asSection(current.james)));
    changes.push(...this.compareCapabilities(asCapabilities(previous.capabilities), asCapabilities(current.capabilities)));

    const summary: ChangeSummary = {
      total_changes: changes.length,
      added: changes.filter(c => c.type === 'ADDED').length,
      removed: changes.filter(c => c.type === 'REMOVED').length,
      modified: changes.filter(c => c.type === 'MODIFIED').length,
      by_category: {} as Record<string, number>,
    };

    for (const change of changes) {
      summary.by_category[change.category] = (summary.by_category[change.category] || 0) + 1;
    }

    const report: ChangeReport = {
      timestamp: new Date().toISOString(),
      previous_snapshot_id: previousId,
      current_snapshot_id: currentId,
      changes,
      summary,
    };

    const reportPath = path.join(this.inventoryDir, 'changes.json');
    const output = this.config.output.pretty_print 
      ? JSON.stringify(report, null, 2)
      : JSON.stringify(report);
    fs.writeFileSync(reportPath, output);

    return report;
  }

  private loadSnapshot(id: string): Snapshot | null {
    // Filenames do not reliably contain the snapshot id, so resolve by
    // parsing: exact match first, then unambiguous id-prefix match.
    // (Previous bug: filename substring match never hit real files.)
    const files = fs.readdirSync(this.snapshotsDir).filter(f => f.endsWith('.json'));
    const exact: string[] = [];
    const prefix: string[] = [];
    for (const f of files) {
      try {
        const snapshot = JSON.parse(readJsonText(path.join(this.snapshotsDir, f))) as Snapshot;
        if (snapshot.id === id) {
          exact.push(f);
        } else if (typeof snapshot.id === 'string' && snapshot.id.startsWith(id)) {
          prefix.push(f);
        }
      } catch (e) {
        // Skip corrupt snapshot files instead of failing the lookup.
      }
    }
    const match = exact.length === 1 ? exact[0] : prefix.length === 1 ? prefix[0] : null;
    if (match === null) return null;
    return JSON.parse(readJsonText(path.join(this.snapshotsDir, match))) as Snapshot;
  }

  private compareObjects(category: string, prev: Record<string, DiscoveryResult<unknown>>, curr: Record<string, DiscoveryResult<unknown>>): Change[] {
    const changes: Change[] = [];
    const allKeys = new Set([...Object.keys(prev), ...Object.keys(curr)]);

    for (const key of allKeys) {
      const prevVal = prev[key]?.value;
      const currVal = curr[key]?.value;

      if (prevVal === undefined && currVal !== undefined) {
        changes.push({
          type: 'ADDED',
          category: category as Change['category'],
          path: key,
          current_value: currVal,
          severity: this.getSeverity(category, key, 'ADDED'),
        });
      } else if (prevVal !== undefined && currVal === undefined) {
        changes.push({
          type: 'REMOVED',
          category: category as Change['category'],
          path: key,
          previous_value: prevVal,
          severity: this.getSeverity(category, key, 'REMOVED'),
        });
      } else if (JSON.stringify(prevVal) !== JSON.stringify(currVal)) {
        changes.push({
          type: 'MODIFIED',
          category: category as Change['category'],
          path: key,
          previous_value: prevVal,
          current_value: currVal,
          severity: this.getSeverity(category, key, 'MODIFIED'),
        });
      }
    }

    return changes;
  }

  private compareCapabilities(prev: Capability[], curr: Capability[]): Change[] {
    const changes: Change[] = [];
    const prevMap = new Map(prev.map(c => [c.id, c]));
    const currMap = new Map(curr.map(c => [c.id, c]));
    const allIds = new Set([...prevMap.keys(), ...currMap.keys()]);

    for (const id of allIds) {
      const prevCap = prevMap.get(id);
      const currCap = currMap.get(id);

      if (!prevCap && currCap) {
        changes.push({
          type: 'ADDED',
          category: 'capability',
          path: id,
          current_value: currCap,
          severity: currCap.risk_level === 'critical' ? 'critical' : 'info',
        });
      } else if (prevCap && !currCap) {
        changes.push({
          type: 'REMOVED',
          category: 'capability',
          path: id,
          previous_value: prevCap,
          severity: prevCap.risk_level === 'critical' ? 'critical' : 'warning',
        });
      } else if (prevCap && currCap && prevCap.status !== currCap.status) {
        changes.push({
          type: 'MODIFIED',
          category: 'capability',
          path: id,
          previous_value: prevCap.status,
          current_value: currCap.status,
          severity: currCap.risk_level === 'critical' ? 'critical' : 'info',
        });
      }
    }

    return changes;
  }

  private getSeverity(category: string, key: string, type: 'ADDED' | 'REMOVED' | 'MODIFIED'): 'info' | 'warning' | 'critical' {
    const criticalKeys = ['cpu', 'gpu', 'ram', 'storage'];
    const warningKeys = ['network_adapters', 'audio', 'containers', 'wsl'];

    if (criticalKeys.includes(key)) return type === 'REMOVED' ? 'critical' : 'warning';
    if (warningKeys.includes(key)) return 'warning';
    return 'info';
  }

  listSnapshots(): Snapshot[] {
    const files = fs.readdirSync(this.snapshotsDir).filter(f => f.endsWith('.json'));
    const out: Snapshot[] = [];
    for (const f of files) {
      try {
        out.push(JSON.parse(readJsonText(path.join(this.snapshotsDir, f))) as Snapshot);
      } catch (e) {
        // Skip corrupt snapshot files (same policy as loadSnapshot).
      }
    }
    return out.sort((a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime());
  }

  getLatestSnapshot(): Snapshot | null {
    const snapshots = this.listSnapshots();
    return snapshots[0] || null;
  }
}