import { DiscoveryEngine } from '../core/engine';
import { DiscoveryConfig } from '../core/interfaces';
import * as fs from 'fs';
import * as path from 'path';

function loadConfig(): DiscoveryConfig {
  const configPath = path.resolve('tools/discovery/config.schema.json');
  const schema = JSON.parse(fs.readFileSync(configPath, 'utf-8'));
  
  const defaultConfig: DiscoveryConfig = {
    version: '1.0',
    scan: {
      mode: 'full',
      timeout_seconds: 120,
      parallel: true,
      include: ['hardware', 'software', 'network', 'james', 'devices', 'capabilities'],
      exclude: [],
    },
    adapters: {
      platform: 'auto',
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
      max_snapshots: 50,
      compress: false,
      retention_days: 30,
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
      level: 'info',
      file: '.james/logs/discovery.log',
      console: true,
    },
  };

  return defaultConfig;
}

async function main() {
  const args = process.argv.slice(2);
  const command = args[0] || 'help';
  const config = loadConfig();
  const engine = new DiscoveryEngine(config);

  switch (command) {
    case 'scan': {
      const mode = args[1] === 'fast' ? 'fast' : 'full';
      console.log(`Starting ${mode} scan...`);
      const snapshot = await engine.scan({ mode: mode as 'fast' | 'full' });
      console.log(`Scan complete. Snapshot ID: ${snapshot.id}`);
      console.log(`Capabilities detected: ${snapshot.capabilities.length}`);
      break;
    }
    case 'inventory': {
      const currentPath = path.resolve(config.output.inventory_dir, 'current.json');
      if (fs.existsSync(currentPath)) {
        const current = JSON.parse(fs.readFileSync(currentPath, 'utf-8'));
        console.log(JSON.stringify(current, null, 2));
      } else {
        console.log('No inventory found. Run "scan" first.');
      }
      break;
    }
    case 'capabilities': {
      const capsPath = path.resolve(config.output.inventory_dir, 'capabilities.json');
      if (fs.existsSync(capsPath)) {
        const caps = JSON.parse(fs.readFileSync(capsPath, 'utf-8'));
        console.log(JSON.stringify(caps, null, 2));
      } else {
        console.log('No capabilities found. Run "scan" first.');
      }
      break;
    }
    case 'changes': {
      const changesPath = path.resolve(config.output.inventory_dir, 'changes.json');
      if (fs.existsSync(changesPath)) {
        const changes = JSON.parse(fs.readFileSync(changesPath, 'utf-8'));
        console.log(JSON.stringify(changes, null, 2));
      } else {
        console.log('No changes found. Run "scan" at least twice.');
      }
      break;
    }
    case 'snapshots': {
      const snapshots = engine.listSnapshots();
      console.log('Available snapshots:');
      for (const s of snapshots) {
        console.log(`  ${s.id} - ${s.timestamp} (${s.scan_mode}) - ${s.capabilities.length} capabilities`);
      }
      break;
    }
    case 'compare': {
      const prevId = args[1];
      const currId = args[2];
      if (!prevId || !currId) {
        console.log('Usage: james-discovery compare <prev_snapshot_id> <curr_snapshot_id>');
        process.exit(1);
      }
      const report = await engine.compareSnapshots(prevId, currId);
      console.log(`Changes detected: ${report.summary.total_changes}`);
      console.log(`  Added: ${report.summary.added}`);
      console.log(`  Removed: ${report.summary.removed}`);
      console.log(`  Modified: ${report.summary.modified}`);
      break;
    }
    case 'diagnose': {
      console.log('JAMES Discovery Diagnostics');
      console.log('============================');
      console.log(`Platform: ${engine['adapter'].platform}`);
      console.log(`Inventory dir: ${config.output.inventory_dir}`);
      console.log(`Snapshots dir: ${config.snapshot.directory}`);
      
      const currentPath = path.resolve(config.output.inventory_dir, 'current.json');
      console.log(`Current inventory exists: ${fs.existsSync(currentPath)}`);
      
      const capsPath = path.resolve(config.output.inventory_dir, 'capabilities.json');
      console.log(`Capabilities file exists: ${fs.existsSync(capsPath)}`);
      
      const snapshots = engine.listSnapshots();
      console.log(`Snapshots available: ${snapshots.length}`);
      
      if (snapshots.length > 0) {
        const latest = snapshots[0];
        console.log(`Latest snapshot: ${latest.id} (${latest.timestamp})`);
        console.log(`  Capabilities: ${latest.capabilities.length}`);
        console.log(`  Hardware items: ${Object.keys(latest.hardware).length}`);
        console.log(`  Software items: ${Object.keys(latest.software).length}`);
        console.log(`  Network items: ${Object.keys(latest.network).length}`);
        console.log(`  JAMES items: ${Object.keys(latest.james).length}`);
      }
      break;
    }
    case 'export': {
      const format = args[1] || 'json';
      const outputPath = args[2] || `inventory-export-${Date.now()}.${format}`;
      
      const currentPath = path.resolve(config.output.inventory_dir, 'current.json');
      if (!fs.existsSync(currentPath)) {
        console.log('No inventory found. Run "scan" first.');
        process.exit(1);
      }
      
      const current = JSON.parse(fs.readFileSync(currentPath, 'utf-8'));
      const capsPath = path.resolve(config.output.inventory_dir, 'capabilities.json');
      const capabilities = fs.existsSync(capsPath) ? JSON.parse(fs.readFileSync(capsPath, 'utf-8')) : [];
      
      const exportData = {
        ...current,
        capabilities,
        exported_at: new Date().toISOString(),
      };
      
      let output: string;
      if (format === 'json') {
        output = JSON.stringify(exportData, null, 2);
      } else if (format === 'yaml') {
        const yaml = await import('yaml');
        output = yaml.stringify(exportData);
      } else {
        console.log('Unsupported format. Use json or yaml.');
        process.exit(1);
      }
      
      fs.writeFileSync(outputPath, output);
      console.log(`Exported to ${outputPath}`);
      break;
    }
    case 'help':
    default:
      console.log(`
JAMES Discovery CLI

Usage: james-discovery <command> [options]

Commands:
  scan [fast|full]     Run a discovery scan (default: full)
  inventory            Show current inventory
  capabilities         Show detected capabilities
  changes              Show changes from last comparison
  snapshots            List available snapshots
  compare <prev> <curr> Compare two snapshots
  diagnose             Show diagnostic information
  export [json|yaml] [file]  Export inventory to file
  help                 Show this help
      `);
  }
}

main().catch(err => {
  console.error('Error:', err.message);
  process.exit(1);
});