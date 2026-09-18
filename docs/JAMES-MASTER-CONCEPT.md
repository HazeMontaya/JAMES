# JAMES Master Concept

Status: verbindliche Leitdefinition
Version: 1.0
Datum: 2026-09-17

## 1. Definition

JAMES ist eine universelle, lokale, modulare und erweiterbare Runtime fuer Faehigkeiten.

> JAMES Core ist das portable Herz. Alles, was JAMES konkret kann, ist ein `James-*`-Modul.

JAMES wird zuerst auf Windows als Referenzplattform entwickelt und betrieben. Windows ist das erste Lieferziel, aber keine Architekturgrenze.

## 2. Das unveraenderliche Modell

```text
JAMES Core
  -> Platform Interface
  -> Platform Adapter
  -> Environment Discovery
  -> Capability Snapshot
  -> Capability Resolver
  -> Module Resolver
  -> Permission / Policy
  -> Module Activation
  -> Execution / Verification / Audit
  -> Void / Dashboard / API / CLI
```

Der Core darf ohne optionale Module starten:

```text
JAMES ONLINE
Core: RUNNING
Modules: 0
Capabilities: 0
```

Danach entsteht die konkrete JAMES-Instanz aus Plattform, Umgebung, Ressourcen, installierten Modulen, Providern und Policies.

## 3. Core-Grenze

Der Core enthaelt nur fundamentale Infrastruktur:

- Runtime und Lifecycle
- Instanz-Identity
- Module Manager und Registry
- Capability Registry, Resolver und Dependency Resolution
- Event Bus
- Permission System und Policy Engine
- Configuration, State und Storage-Abstraktion
- Security Boundary
- Audit, Health, Diagnostics und Recovery
- Platform Ports, aber keine konkreten OS-Implementierungen

Der Core enthaelt nicht: AI, Chat, Voice, Memory, Tasks, Browser, Web Research, Filesystem, Devices, Dashboard, Void, Applications, Economics, SelfModify oder Replication.

## 4. Portable-first

```text
Portable Core -> Platform Interface -> Platform Adapter -> OS API
```

Der Core kennt keine Windows-APIs, festen Windows-Pfade, Windows-UI oder Windows-spezifische Prozesslogik.

Portable Platform Ports umfassen mindestens:

- Filesystem
- Processes
- Applications
- Networking
- Devices
- Audio und Camera
- Display
- Compute
- Notifications
- System Information
- Power
- Platform Security

Zuerst wird `james-platform-windows` umgesetzt. Andere Plattformen duerfen anfangs `unknown` oder `unsupported` melden, aber keine Funktion vortaeuschen.

## 5. Module und Capabilities

Ein Modul ist eine Implementierung. Eine Capability ist ein stabiler Vertrag.

```text
Task fordert: ai.inference
Resolver findet: lokale, Cloud- oder Remote-Provider
```

Module deklarieren:

- Identitaet und Version
- bereitgestellte Capabilities
- Dependencies
- Plattformen
- Permissions
- Trust-Klasse und Artefaktintegritaet
- Ressourcenprofil
- Execution Target
- Konfiguration, Health, Migration und Update

Mehrere Module duerfen dieselbe Capability bereitstellen. Der Core entscheidet nach Verfuegbarkeit, Datenschutz, Kosten, Performance, Ressourcen, Zuverlaessigkeit und Policy.

## 6. Instanzaufbau

```text
Core Start
  -> Environment Scan
  -> Detection
  -> Registration
  -> Capability Snapshot
  -> Module Candidates
  -> Compatibility
  -> Dependencies
  -> Permissions
  -> Trust
  -> Policy
  -> Resource Check
  -> Detect / Recommend / Install
  -> Register
  -> Enable
  -> Start
  -> Health Check
  -> Instance Ready
```

Installationsmodi:

- `DETECT`: erkennen, nichts installieren
- `RECOMMEND`: Vorschlag mit Begruendung, Risiko und Alternativen
- `AUTONOMOUS_INSTALL`: nur durch explizite Policy, verifizierte Quelle und akzeptables Risiko

## 7. Sicherheitsvertrag

Jede Aktion folgt zwingend:

```text
Capability -> Permission -> Policy -> Validation -> Execution Boundary
-> Verification -> Memory/Event -> Audit
```

Policy-Ergebnisse: `ALLOW`, `DENY`, `ASK`, `CONDITIONAL`.

Risiken: `LOW`, `MEDIUM`, `HIGH`, `CRITICAL`.

Die KI besitzt keinen direkten Shell-, Filesystem-, Netzwerk-, Wallet- oder Systemzugriff. Externe Daten sind untrusted input. Identity Root, Security Boundary, Permission/Policy Enforcement, Audit Integrity und Kill/Disable-Mechanismen sind nicht durch Agenten oder SelfMade veraenderbar.

## 8. Bedienung und Oberflaechen

`James-Void` ist die primaere adaptive und interaktive Oberflaeche von JAMES. Das Live Brain bleibt im Zentrum und repraesentiert den echten Zustand, zum Beispiel `IDLE`, `THINKING`, `PLANNING`, `SEARCHING`, `EXECUTING`, `WAITING`, `WARNING`, `ERROR`, `RECOVERING` oder `SLEEPING`.

Das Brain ist eine synthetische kognitive Struktur aus Core, relevanten Knoten, Verbindungen, Signalfluss, Aktivitaetsfeld und Fokus. Es ist kein menschliches Organ, kein Cyberpunk-HUD und keine zufaellige Daueranimation. Geschwindigkeit, Pulse, Flow, Connection, Separation, Focus und Fade haben nur Bedeutung, wenn sie aus echtem JAMES-State entstehen.

Ein UI-Orchestrator erzeugt aus State, Goals, Tasks, Agenten, Events, Ressourcen und Security-Status einen deklarativen `UiIntent`. JAMES entscheidet damit, welche Informationen, Panels, Fenster, Tabs, Dialoge oder Bestaetigungen relevant sind. Void rendert diesen Intent und besitzt keine eigene fachliche Wahrheit.

Void, Dashboard, API und CLI greifen auf dasselbe `JamesState`-Modell zu. Das Dashboard ist der strukturierte bzw. erweiterte operative Void-Modus, nicht ein zweites System.

Jede sichtbare Aktion erzeugt einen `ActionRequest` mit Caller, Capability, Ziel, Scope, Risiko, Input, Correlation-ID und erwartetem Effekt. UI-Aktionen und Agentenaktionen benutzen denselben Broker-, Permission-, Policy-, Execution-, Verification- und Auditpfad.

UI-Faehigkeiten umfassen unter anderem `ui.present`, `ui.panel.open`, `ui.window.open`, `ui.tab.open`, `ui.focus`, `ui.notify`, `ui.request_input`, `ui.show_progress` und `ui.show_confirmation`. Die UI darf niemals als Sicherheitsumgehung dienen.

Die Visualpipeline lautet `State -> Context -> Relevance -> Priority -> UI Intent -> Animation -> Renderer`. Das vollstaendige Visual- und Motion-System ist in `docs/architecture/void-visual-system.md` definiert.

Ohne Void muss JAMES funktionieren. Ohne Dashboard muss JAMES funktionieren. Renderer fuer Desktop, Mobile, Tablet, Web und Embedded veraendern nur die Darstellung, nicht die Fachlogik.

## 9. Agenten, Memory und Tasks

Agenten besitzen Identity, Rolle, Goal, Plan, Memory-Scope, Capability-Allowlist, Policy-Profil, Budget, Workspace und Audit-Kontext.

Memory-Typen: Working, Episodic, Semantic, Procedural, Relationship.

Task-Zustaende: `CREATED`, `QUEUED`, `RUNNING`, `WAITING`, `PAUSED`, `BLOCKED`, `COMPLETED`, `FAILED`, `CANCELLED`, `RECOVERING`.

Scheduler und Heartbeat duerfen keine Policy umgehen. Neustart muss laufende Aufgaben wiederherstellen koennen.

## 10. Verteilte Zukunft

Jede JAMES-Instanz besitzt eigene Identity, Environment, Module, Capabilities, Policies, Ressourcen und Audit-Kontext.

Remote Execution ist eine normale Remote-Capability mit lokaler und entfernter Policy, Budget, Verification und Audit. Erreichbarkeit bedeutet niemals automatisch Vertrauen.

## 11. SelfMade

James-SelfMade ist eine optionale Orchestrierungsschicht, kein zweiter Core.

```text
MISSION -> OBSERVE -> UNDERSTAND -> RESOURCE CHECK -> PLAN
-> RISK/COST -> POLICY -> PERMISSION -> EXECUTE -> VERIFY
-> PERSIST -> ADAPT -> SLEEP
```

Die ersten SelfMade-Versionen simulieren und planen nur. SelfModify und Replication brauchen eigene Identity-, Budget-, Workspace-, Review- und Kill-Mechanismen.

## 12. Entwicklungsregel

Jede Aufgabe folgt:

```text
AUDIT -> PLAN -> TASK CONTRACT -> IMPLEMENT -> BUILD -> TEST
-> REVIEW -> SECURITY REVIEW -> ACCEPTANCE GATE -> VERIFIED
```

`IMPLEMENTED` ist nicht automatisch `VERIFIED`. Keine stillen Architekturveraenderungen, keine abgeschwaechten Tests und keine TODO-Loesungen als Fertigmeldung.

## 13. Entwicklungsreihenfolge

1. Core, Event Bus, Registry
2. Capability, Identity, Permission, Policy, Audit
3. Platform Ports und Windows Adapter
4. Discovery und Environment Model
5. Capability/Module Resolver und Reconciliation
6. Storage und Recovery
7. Runtime/API
8. Text, Chat, AI, Model Router
9. Memory und Tasks
10. Filesystem, Processes, Browser, Voice und Devices
11. Void und Dashboard
12. Agents und verteilte Capabilities
13. SelfMade, Economics, SelfModify und Replication
14. weitere Plattformen und Packaging

## 14. Erster nutzbarer Meilenstein

```text
Windows JAMES Instance
= Core
+ Windows Discovery
+ TextInput/TextOutput
+ Chat
+ AI Provider
+ Memory
+ Tasks
+ Capability Broker
+ Audit
+ lokale Runtime/API
```

Dieser Meilenstein muss ohne Void und Dashboard technisch funktionieren und einen kontrollierten Chat-/Capability-Pfad nachweisen.
