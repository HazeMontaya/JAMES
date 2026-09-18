# JAMES

Universelle, modulare, lokale und erweiterbare KI-Plattform.

> **JAMES selbst ist nur das Herz. Alles, was JAMES kann, ist ein Modul.**

## Dokumentation

| Dokument | Inhalt |
|----------|--------|
| [JAMES-MASTER-CONCEPT.md](JAMES-MASTER-CONCEPT.md) | Verbindliche Identitaet, Architektur, Sicherheits- und Entwicklungsregeln |
| [PROJECT-CONTINUATION.md](PROJECT-CONTINUATION.md) | Aktueller Projektstand, Entscheidungen, naechste Bloecke und Verifikation |
| [AUDIT.md](AUDIT.md) | Reproduzierbarer Audit des erreichbaren JAMES-Workspaces |
| [CURRENT-STATE.md](CURRENT-STATE.md) | Verifizierte Gates und offene Implementierungsgrenzen |
| [MIGRATION-MATRIX.md](MIGRATION-MATRIX.md) | JARVIS/AUTOMATON-Migrationsstand und Blockaden |
| [DEPENDENCY-MAP.md](DEPENDENCY-MAP.md) | Workspace-Abhaengigkeiten und Architekturgrenzen |
| [FUNCTIONAL-GAP-ANALYSIS.md](FUNCTIONAL-GAP-ANALYSIS.md) | Verifizierter Nutzbarkeitsstand, fehlende Funktionen und Abnahmegates |
| [STRUCTURE.md](STRUCTURE.md) | Ordnungsschema des Repositories |
| [discovery.md](discovery.md) | Environment-Discovery-Tool (TS) |
| [architecture/*.md](architecture/) | Kern-Architektur (Core, Capabilities, Security) |
| [architecture/ui-orchestration.md](architecture/ui-orchestration.md) | Void, Live Brain, UI-Orchestrator und Dashboard-Vertrag |
| [architecture/void-visual-system.md](architecture/void-visual-system.md) | Visuelle Sprache, Brain-Zustaende, Motion und Window Intelligence |

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
- **Void ist die primaere adaptive Oberflaeche mit dem Live Brain im Zentrum.**
- **UI-Aktionen sind Capabilities und keine Sicherheitsumgehung.**
- **Animation ist Information: Das Live Brain visualisiert echten JAMES-State.**
- **James-SelfMade** ist optionale, kontrollierte Autonomie — kein zweiter Core.
- **PC-first bedeutet Windows als erste Referenzplattform, nicht Windows-Abhängigkeit.**
- **Die konkrete JAMES-Instanz entsteht aus Environment, Modulen, Capabilities, Ressourcen und Policy.**
- **Neue Sitzungen lesen zuerst `PROJECT-CONTINUATION.md` und danach `JAMES-MASTER-CONCEPT.md`.**