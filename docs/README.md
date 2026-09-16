# JAMES

Universelle, modulare, lokale und erweiterbare KI-Plattform.

> **JAMES selbst ist nur das Herz. Alles, was JAMES kann, ist ein Modul.**

## Dokumentation

| Dokument | Inhalt |
|----------|--------|
| [STRUCTURE.md](STRUCTURE.md) | Ordnungsschema des Repositories |
| [BOOTSTRAP.md](BOOTSTRAP.md) | Discovery-Prinzip & Setup |
| [discovery.md](discovery.md) | Environment-Discovery-Tool (TS) |
| [architecture/*.md](architecture/) | Kern-Architektur (Core, Events, Capabilities, Registry, Tasks, Security) |

## Schnellstart

```powershell
# Core (unveränderliches Herz)
cd S:\JAMES\core
cargo check          # Verifikation: 0 Fehler

# Module (alle James-*-Fähigkeiten)
cd S:\JAMES\modules
cargo check
```

- `james-app` (core/crates/james-app) — Zero-Module-Pfad: Core startet ohne Module
- `james-system` (modules/james-system) — Voll-Assembly: Core + alle Module

## Grundprinzip

- **Core bleibt klein.** Alle Fähigkeiten sind `James-*`-Module.
- **AI ist ein Modul,** nicht der Core.
- **Jede Aktion** läuft durch Capability → Permission → Policy → Execution → Verification → Audit.
- **Void und Dashboard** sind dynamische Darstellungen desselben JAMES.
- **James-SelfMade** ist optionale, kontrollierte Autonomie — kein zweiter Core.