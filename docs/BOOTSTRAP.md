# JAMES Bootstrap

## Ziel
Baue und verwalte das Projekt JAMES unter:
`S:\JAMES`

JAMES soll langfristig eine modulare, lokale, plattformübergreifende Agent-/Automation-Plattform werden.

## Wichtige Regel
Vor jeder Installation, Änderung oder Löschung:
1. Umgebung analysieren.
2. Bestehende Software erkennen.
3. Hardware erkennen.
4. Netzwerk erkennen.
5. vorhandene Geräte erkennen, soweit technisch und rechtlich zulässig.
6. vorhandene JAMES-Dateien prüfen.
7. Abhängigkeiten prüfen.
8. anschließend erst eine Änderung vorschlagen oder durchführen.

Nichts unnötig überschreiben.

## Erfassungsbereiche

### Host
- Betriebssystem
- Version
- Architektur
- Computername
- Benutzer
- CPU
- RAM
- GPU
- VRAM
- NPU
- Datenträger
- freier Speicher
- USB-Geräte
- Audio
- Mikrofon
- Kameras
- Netzwerkadapter
- IP-Konfiguration
- Bluetooth
- verfügbare Container-Runtimes

### Software
Erkennen:
- Git
- Python
- Node.js
- Rust
- Cargo
- Docker
- Podman
- WSL
- PowerShell
- Windows Terminal
- Browser
- lokale Datenbanken
- lokale LLM-Runtimes
- Ollama
- andere relevante AI-Runtimes
- vorhandene Entwicklerwerkzeuge

### Netzwerk
Erkennen:
- aktive Interfaces
- lokale IPs
- Subnetze
- Gateway
- DNS
- erreichbare Geräte
- relevante Dienste

Keine aggressiven Portscans und keine Zugriffe auf fremde Systeme.

### JAMES
Prüfen:
- Repository
- Module
- Konfiguration
- Plugins
- Datenbanken
- Logs
- Tests
- Build-System
- Abhängigkeiten

## Output
Erzeuge zunächst nur eine Analyse unter:
`S:\JAMES\.james\inventory\`

Mindestens:
- host.json
- hardware.json
- software.json
- network.json
- devices.json
- capabilities.json
- recommendations.json

Zusätzlich:
`S:\JAMES\.james\inventory\REPORT.md`

## Architekturprinzip
JAMES darf niemals davon ausgehen, dass eine Fähigkeit vorhanden ist.
Jede Fähigkeit muss:
Discovery → Detection → Registration → Permission → Execution
durchlaufen.

## Sicherheitsprinzip
Kein blindes:
- Vollzugriffsrecht
- automatisches Löschen
- Credential-Auslesen
- Umgehen von Sicherheitsmechanismen
- Zugriff auf fremde Systeme
- Deaktivieren von Schutzmechanismen

Administrative Fähigkeiten dürfen später existieren, müssen aber über das JAMES Capability-/Permission-System kontrolliert werden.

## Installationen
Installiere zunächst nichts.
Erstelle zuerst einen vollständigen Bestandsbericht.
Danach soll eine Liste erstellt werden:
1. bereits vorhanden
2. fehlt
3. optional
4. kritisch
5. empfohlen
6. später erforderlich

Erst nach Prüfung soll die nächste Installationsphase vorbereitet werden.