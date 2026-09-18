# JAMES Project Continuation

Dieses Dokument ist der Einstieg fuer eine spaetere Sitzung. Es beschreibt, was JAMES ist, welche Entscheidungen verbindlich sind, was bereits vorhanden ist und womit weitergearbeitet werden soll.

## Aktueller Stand

JAMES ist ein Rust-Projekt mit getrennten Workspaces:

- `core/`: Core-Crates
- `modules/`: First-Party-Module und Assembly
- `tools/discovery/`: TypeScript-Discovery, Windows konkret, andere Plattformen als Stubs
- `docs/`: Architektur und Projektvertrag

Wichtige vorhandene Bausteine:

- Core-Lifecycle, Event Bus, Registry, Capability Registry, Services, Tasks, Scheduler und Health
- Capability Broker mit Permission, Policy, Input/Output-Schema und Audit-Events
- Identity Registry und tamper-evident In-Memory-Audit
- Module Host mit Manifest-/Lifecycle-Modell
- First-Party-Assembly fuer AI, Chat, Memory, Tasks, Scheduler, Voice, Browser, WebResearch, Void und Dashboard
- lokale App-API auf Loopback
- Windows Discovery mit Hardware-, Software-, Netzwerk- und JAMES-Inventar

Seit dem letzten dokumentierten Stand umgesetzt:

- Capability Broker lehnt deaktivierte/unverfuegbare Capabilities ab.
- `CONDITIONAL` Policy-Entscheidungen werden ohne Bedingungsauswertung fail-closed abgelehnt.
- `ChatHandler` ist ein Core-neutraler API-Vertrag; Core-only liefert ohne Handler ehrlich `503` statt eines Fake-Acks.
- `james-system` verbindet stdin und lokale HTTP-API mit demselben echten Void-/AI-Handler.
- `James-Assembly` besitzt einen Broker-Executor fuer `memory.read`; der Pfad ist durch einen Integrationstest verifiziert.
- stdin und HTTP-API teilen denselben Assembly-ChatHandler; `/memory [limit]` nutzt den Brokerpfad.
- Start, API-Bindung und geordneter Shutdown der vollstaendigen Assembly wurden als Smoke-Test ausgefuehrt.

Wichtige aktuelle Grenzen:

- Module Loader fuer Native ist noch ein Mock; WASM und Process Loader sind noch nicht implementiert.
- Die Assembly ist statisch verdrahtet.
- Der Capability Broker ist noch nicht der durchgaengige Ausfuehrungspfad aller Module.
- Die Chat-API ist in `james-system` echt an Void/AI angeschlossen; `james-app` bleibt bewusst Core-only und liefert ohne Handler `503`.
- API-Authentifizierung und Confirmation Flow muessen vervollstaendigt werden.
- Audit/Identity/Registry/Task-Persistenz ist noch nicht durchgaengig restart-faehig.
- Discovery ausserhalb Windows ist bewusst unvollstaendig.
- Lokale Builds koennen an der sccache/CARGO_INCREMENTAL-Umgebung scheitern; das ist zuerst als Infrastrukturproblem zu klaeren.
- Nach einer vollstaendigen Artefaktbereinigung kann ein frischer Windows-Rebuild aktuell an fehlendem `dlltool.exe` scheitern; der Rust-Quellcode war vor der Bereinigung bereits erfolgreich kompiliert und getestet.

## Verbindliche Architekturentscheidungen

- PC-first: Windows-PC ist die erste Referenz- und Lieferplattform.
- Portable-first: Core, Domain-Modelle, Daten und UI-Vertraege bleiben plattformneutral.
- Core bleibt klein und enthaelt keine konkreten Benutzerfaehigkeiten.
- Capabilities sind Vertraege; Module sind Implementierungen; Provider sind austauschbar.
- Resolver bestimmen passende Module/Provider anhand von Environment, Ressourcen, Trust, Permissions und Policy.
- Detect, Recommend und Autonomous Install sind getrennte Modi.
- Void und Dashboard sind Projektionen desselben Zustands.
- Void ist die primaere adaptive Oberflaeche mit dauerhaftem Live Brain; Dashboard ist der erweiterte operative Void-Modus.
- Ein UI-Orchestrator erzeugt deklarative `UiIntent`; Renderer fuehren keine Fachlogik oder Permission-Pruefung aus.
- UI-Aktionen sind Capabilities und laufen wie Agentenaktionen durch Broker, Policy, Verification und Audit.
- Das Live Brain ist eine kausale State-Projektion; keine Fake-Animation, kein Cyberpunk-HUD und keine zufaellige Aktivitaet.
- Void nutzt eine eigene Motion Language und Window Intelligence fuer Panels, Fenster, Tabs und fokussierte Dialoge.
- Riskante Aktionen laufen niemals an Broker/Policy vorbei.
- SelfMade ist spaet, optional und kontrolliert.
- `Automaton` wird nicht als JAMES-Modulname verwendet; die autonome Ebene heisst `James-SelfMade`.

## Naechster sinnvoller Arbeitsblock

### Block A: Platform Ports

Ziel: Core kann portable Plattformdienste verwenden, ohne Windows APIs zu importieren.

Betroffene Zielbereiche:

- `core/crates/james-platform/`
- `core/crates/james-platform-windows/`
- `core/crates/james-core/`
- `tools/discovery/`

Ergebnis:

- Traits fuer Filesystem, Processes, Networking, Devices, Audio, Camera, Display, Compute, Notifications, System Information, Power und Platform Security
- Windows Adapter als erste Implementierung
- Test-Double fuer Core-Tests
- `unknown`/`unsupported` statt Fake-Unterstuetzung

### Block B: Environment Model und Capability Snapshot

Ziel: Discovery-Daten werden als portable, versionierte Domain-Modelle an den Core uebergeben.

Ergebnis:

- Environment Snapshot mit Quelle, Confidence, Status, Zeit und Metadata
- getrennte Werte fuer detected, available, degraded, unavailable und unknown
- Capability Snapshot fuer die konkrete JAMES-Instanz
- keine direkte Abhaengigkeit des Core von TypeScript-Strukturen

### Block C: Resolver und Reconciliation

Ziel: Aus einer angeforderten Capability werden geeignete Module/Provider ausgewaehlt.

Ergebnis:

```text
Requirement -> Candidates -> Compatibility -> Dependencies
-> Permissions -> Trust -> Policy -> Resources -> Selection
-> Detect/Recommend/Install -> Register -> Enable -> Start -> Verify
```

### Block D: Durchgaengiger Agentenpfad

Ziel:

```text
Chat -> AI -> Plan -> Capability Broker -> Executor -> Verification -> Audit -> Antwort
```

Der Transport- und AI-Pfad ist als ChatHandler und System-CLI vorhanden. `memory.read` laeuft bereits ueber den Broker; offen bleibt die direkte Auswahl und Ausfuehrung dieser Capability aus dem Agenten-/Tool-Plan.

### Block E: UI Orchestration

Ziel:

```text
JAMES State -> Context -> Relevance -> Priority -> UI Intent -> Void Renderer
```

Ergebnis:

- Live Brain aus echten State-/Event-Zustaenden
- UI-Capabilities fuer Panels, Fenster, Tabs, Focus, Notifications und Confirmations
- Void als primaere adaptive Oberflaeche
- Dashboard als erweiterter Void-/Operations-Modus
- Desktop-, Mobile- und Web-Renderer ohne fachliche Logik im Renderer
- semantische Brain-Motion, Activity Field, Fokus-/Recovery-Darstellung und deterministische Window Intelligence

## Task-Contract-Vorlage

Jede neue Aufgabe muss vor Implementierung diese Felder enthalten:

```text
TASK:
OBJECTIVE:
WHY:
ARCHITECTURAL LAYER:
SCOPE:
NON-GOALS:
COMPONENTS:
FILES:
CONTRACTS:
DATA:
EVENTS:
PERMISSIONS:
SECURITY:
ERROR HANDLING:
TESTS:
ACCEPTANCE CRITERIA:
VERIFICATION COMMANDS:
ROLLBACK:
DEPENDENCIES:
```

## Verifikation

Bevor Ergebnisse als verifiziert gelten:

1. Repository-Zustand pruefen.
2. Betroffene Dateien und Symbole lesen.
3. Kleinsten Task-Contract erstellen.
4. Implementieren.
5. Engsten Test oder Compile-Check ausfuehren.
6. Security- und Architekturfolgen pruefen.
7. Acceptance Criteria abhaken.
8. Erst dann naechsten Block starten.

Geplante Kommandos, sobald die Toolchain funktioniert:

```powershell
$env:CARGO_INCREMENTAL = "0"
Push-Location core
cargo check
cargo test
Pop-Location

Push-Location modules
cargo check
cargo test
Pop-Location

Push-Location tools/discovery
npm test
Pop-Location
```

## Lese-Reihenfolge fuer eine neue Sitzung

1. `docs/PROJECT-CONTINUATION.md`
2. `docs/JAMES-MASTER-CONCEPT.md`
3. `docs/README.md`
4. `docs/architecture/core.md`
5. `docs/architecture/capabilities.md`
6. `docs/architecture/security-boundaries.md`
7. `docs/architecture/platform-abstraction.md`
8. `docs/architecture/ui-orchestration.md`
9. `docs/architecture/void-visual-system.md`
10. erst danach konkrete Quellcode-Dateien

## Fortsetzungsregel

Nicht von frueheren Annahmen ausgehen. Zuerst Repository, Tests, Build und aktuelle Dateien pruefen. Ziel ist nicht, alles gleichzeitig zu implementieren, sondern jeden Architekturblock mit Tests und nachvollziehbaren Acceptance Criteria zu verifizieren.
