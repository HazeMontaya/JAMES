# JAMES Discovery / Environment Manager

Automatische Erkennung, Inventarisierung und Überwachung der lokalen JAMES-Umgebung.

## Übersicht

Der JAMES Environment Manager ist ein modularer Discovery-Dienst, der:

1. **Hardware** erkennt (CPU, RAM, GPU, VRAM, NPU, Storage, USB, Audio, Kameras, Bluetooth, Netzwerk)
2. **Software** inventarisiert (installierte Programme, Dienste, Developer Tools, Runtimes, Browser, Container, WSL, AI-Runtimes)
3. **Netzwerk** analysiert (Interfaces, IPs, Gateway, DNS, mDNS, lokale Geräte)
4. **JAMES** selbst prüft (Module, Plugins, Config, Git-Status, Services)
5. **Capabilities** ableitet (automatisch aus der Umgebung)
6. **Snapshots** erstellt und **Änderungen** erkennt

## Architektur

```
tools/discovery/
├── src/
│   ├── core/
│   │   ├── interfaces.ts    # TypeScript-Interfaces für alle Datenstrukturen
│   │   └── engine.ts        # DiscoveryEngine - Orchestrierung
│   ├── adapters/
│   │   ├── base.ts          # BaseAdapter mit Capability Detection Logik
│   │   ├── windows.ts       # Vollständige Windows-Implementierung (WMI, PowerShell)
│   │   └── stubs.ts         # Stubs für Linux, macOS, Android, iOS
│   ├── cli/
│   │   └── index.ts         # CLI-Entry-Point
│   └── utils/               # Hilfsfunktionen
├── tests/                   # Unit-Tests
├── config.schema.json       # JSON-Schema für Konfiguration
├── run.ps1                  # PowerShell Wrapper
└── package.json             # npm-Konfiguration
```

## Adapter-Konzept

- **PlatformAdapter Interface** definiert den Vertrag
- **WindowsAdapter** - Voll implementiert (WMI + PowerShell)
- **LinuxAdapter, MacOSAdapter, AndroidAdapter, iOSAdapter** - Stubs mit definierten Interfaces
- Automatische Plattform-Erkennung via `process.platform`

## Capability Detection

Capabilities werden automatisch aus der Umgebung abgeleitet:

| Capability ID | Bedingung | Kategorie |
|--------------|-----------|-----------|
| `ai.local.gpu_inference` | GPU vorhanden | ai |
| `ai.local.cpu_inference` | CPU ≥4 Kerne, RAM ≥8GB | ai |
| `ai.local.npu_inference` | NPU vorhanden | ai |
| `voice.input` | Mikrofon erkannt | voice |
| `voice.output` | Lautsprecher erkannt | voice |
| `browser.automation` | Browser installiert | automation |
| `container.runtime` | Docker läuft | container |
| `container.runtime.podman` | Podman läuft | container |
| `wsl2.environment` | WSL2 installiert | system |
| `device.bluetooth` | Bluetooth-Adapter | device |
| `smart_home.home_assistant` | Home Assistant erkannt | smart_home |
| `ai.local.ollama` | Ollama läuft | ai |
| `dev.vscode` | VS Code installiert | development |
| `dev.git` | Git installiert | development |
| `device.camera` | Kamera erkannt | device |
| `network.local` | Netzwerk-Interface up | network |
| `network.device_discovery` | Lokale Geräte gefunden | network |

Jede Capability enthält:
- `id`, `name`, `category`
- `detected` (boolean)
- `provider`, `version`
- `dependencies` (string[])
- `risk_level` (low/medium/high/critical)
- `status` (available/unavailable/degraded/unknown)

## Installation

```powershell
cd S:\JAMES\tools\discovery
npm install
```

## Verwendung

### PowerShell Wrapper (empfohlen)
```powershell
cd S:\JAMES\tools\discovery
.\run.ps1 scan full        # Vollständiger Scan
.\run.ps1 scan fast        # Schneller Scan
.\run.ps1 inventory        # Aktuelles Inventar anzeigen
.\run.ps1 capabilities     # Erkannte Capabilities
.\run.ps1 changes          # Letzte Änderungen
.\run.ps1 snapshots        # Alle Snapshots listen
.\run.ps1 compare <prev> <curr>  # Zwei Snapshots vergleichen
.\run.ps1 diagnose         # Diagnose-Info
.\run.ps1 export json      # Export als JSON
```

### Direkt mit Node.js
```bash
cd S:\JAMES\tools\discovery
npx ts-node src/cli/index.ts scan full
npx ts-node src/cli/index.ts inventory
npx ts-node src/cli/index.ts capabilities
```

## Konfiguration

Die Konfiguration erfolgt über `config.schema.json`. Wichtige Optionen:

```json
{
  "scan": {
    "mode": "full",           // "fast" oder "full"
    "timeout_seconds": 120,
    "parallel": true,
    "include": ["hardware", "software", "network", "james", "capabilities"]
  },
  "adapters": {
    "platform": "auto"        // auto, windows, linux, macos, android, ios
  },
  "capability_detection": {
    "enabled": true
  },
  "snapshot": {
    "directory": ".james/inventory/snapshots",
    "max_snapshots": 50,
    "retention_days": 30
  }
}
```

## Output-Dateien

Nach jedem Scan werden folgende Dateien aktualisiert:

| Datei | Inhalt |
|-------|--------|
| `.james/inventory/current.json` | Kompletter aktueller Zustand |
| `.james/inventory/capabilities.json` | Erkannte Capabilities |
| `.james/inventory/changes.json` | Letzter Change-Report |
| `.james/inventory/snapshots/*.json` | Historische Snapshots |

## Datenqualität

Jeder Wert enthält Metadaten:
```json
{
  "value": "...",
  "source": "wmi|powershell|registry|cli|filesystem",
  "detected_at": "2026-09-15T...",
  "confidence": 0.95
}
```

Nicht ermittelbare Werte: `"unknown"` mit `confidence: 0` und `metadata.reason`.

## Sicherheit

- Keine Credentials, Passwörter, Cookies
- Keine externen Scans
- Nur lokale/autorisierte Ressourcen
- Sensible Daten werden redacted (Konfiguration: `security.redact_sensitive`)

## Tests

```powershell
cd S:\JAMES\tools\discovery
npm test
```

Tests decken ab:
- Hardware Detection
- Software Detection
- Capability Detection
- Snapshot Creation
- Change Detection
- Missing/Malformed Data
- Permissions/Errors

## Extending

### Neue Capability hinzufügen
In `src/adapters/base.ts` in `getCapabilities()` Methode.

### Neuer Adapter
1. Implementiere `PlatformAdapter` Interface
2. Registriere in `engine.ts` `createAdapter()`
3. Füge Tests hinzu

### Custom Capability Rules
In `config.schema.json` unter `capability_detection.custom_rules`:

```json
{
  "id": "custom.my_capability",
  "name": "My Custom Capability",
  "category": "custom",
  "condition": "hardware.gpu.value.length > 0 && hardware.gpu.value[0].adapter_ram_gb >= 8",
  "dependencies": ["cuda"],
  "risk_level": "low",
  "provider": "custom"
}
```

## Lizenz

Part of JAMES Project.