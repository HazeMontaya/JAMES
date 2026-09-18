# JAMES UI Orchestration

Status: verbindlicher Architekturvertrag

## 1. Rolle von Void

`James-Void` ist die primaere adaptive und interaktive Oberflaeche von JAMES. Es ist gleichzeitig:

- Benutzeroberflaeche
- Live-Statusanzeige
- Visualisierung
- Steuerzentrale
- Kontextanzeige
- Einstiegspunkt fuer Interaktion
- Praesentationsschicht fuer Module, Agenten und Tasks

Void ist nicht die Quelle der Fachlogik. Die Wahrheit liegt in `JamesState`, Events, Registry, Tasks, Memory, Policies und Audit.

Das Dashboard ist keine konkurrierende Oberflaeche, sondern eine strukturierte bzw. erweiterte Darstellung innerhalb desselben UI-Systems.

## 2. Live Brain

Der Void enthaelt dauerhaft ein zentrales Live Brain. Es ist keine dekorative Animation, sondern eine Projektion des echten Systemzustands.

Die visuelle Sprache und die deterministischen Motion Profiles sind in [void-visual-system.md](void-visual-system.md) festgelegt. Das Brain ist keine dekorative Animation; jede Bewegung benoetigt eine semantische Ursache.

Moegliche Brain-Zustaende:

- `IDLE`
- `THINKING`
- `PLANNING`
- `SEARCHING`
- `EXECUTING`
- `WAITING`
- `LEARNING`
- `WARNING`
- `ERROR`
- `RECOVERING`
- `SLEEPING`

Jeder Zustand muss aus einem nachvollziehbaren JAMES-State oder Event entstehen. Die Darstellung darf sich veraendern, ohne eine falsche Aktivitaet vorzutaeuschen.

## 3. UI-Orchestrator

Der UI-Orchestrator empfaengt eine State-Projektion aus:

- aktuellem Goal und Plan
- laufenden Tasks und Agenten
- Capabilities und Modulen
- Events und Fehlern
- Ressourcen
- Security-/Permission-Status
- Nutzerkontext
- Geraetekontext

Er erzeugt einen deklarativen `UiIntent`. Beispiel:

```json
{
  "mode": "research",
  "focus": "sources",
  "brain_state": "searching",
  "show": ["brain", "progress", "sources", "findings"],
  "open": ["research_panel"],
  "actions": ["open_source", "pause_task"]
}
```

Die UI rendert diesen Intent. Sie entscheidet nicht selbst ueber fachliche Ziele, Permissions oder Execution.

## 4. Relevanz und Prioritaet

JAMES entscheidet dynamisch, was sichtbar ist. Die Reihenfolge ist:

```text
JAMES State
  -> Context
  -> Relevance
  -> Priority
  -> UI Intent
  -> Void Renderer
```

Prioritaeten:

1. Sicherheitsentscheidungen und Benutzerbestaetigungen
2. Fehler, Blockaden und Recovery
3. aktive Nutzerziele und laufende Aufgaben
4. aktive Agenten und Capability-Ausfuehrung
5. relevante Ergebnisse und Ressourcen
6. optionale Details und Historie

Der Nutzer kann den Fokus aufklappen, anheften, ausblenden oder als Arbeitsbereich speichern.

## 5. Oberflaechenformen

Void kann je nach Kontext anzeigen oder oeffnen:

- zentrale Hauptansicht
- Panel
- Fenster
- Tab
- Dialog
- Notification
- Fortschrittsansicht
- Bestaetigungsansicht
- spezialisierte Modulansicht

UI-Expansion ist selbst eine Capability und wird niemals als Sicherheitsumgehung verwendet.

## 6. UI-Capabilities

Vorgesehene Vertraege:

```text
ui.present
ui.panel.open
ui.panel.close
ui.window.open
ui.window.close
ui.tab.open
ui.focus
ui.notify
ui.request_input
ui.show_progress
ui.show_confirmation
```

Jeder UI-Aufruf besitzt Caller, Ziel, Scope, Datenklassifizierung, Ablauf und Policy-Entscheidung. Ein Agent darf kein Terminal oder beliebiges Fenster oeffnen, um den Capability Broker zu umgehen.

## 7. Interaktion

Text, Sprache, Tastatur, Touch, Buttons und externe Clients erzeugen denselben `UserIntent` und danach denselben `ActionRequest`.

Eine Aktionsdarstellung zeigt mindestens:

- Absicht
- Capability
- Ziel und Scope
- Risiko
- benoetigte Permissions
- Policy-Ergebnis
- Status
- Ergebnis oder Fehler
- naechste Aktion bzw. Rueckgaengigkeit

Bestaetigungen sind an ActionRequest, Caller, Scope und Ablaufzeit gebunden.

## 8. Responsive Renderer

Der gleiche `UiIntent` wird von unterschiedlichen Renderern dargestellt:

```text
James-Void Desktop
James-Void Mobile
James-Void Tablet
James-Void Web
James-Void Embedded
```

Renderer duerfen Inhalte wegen Displaygroesse, Eingabemethode oder Geraeteklasse anders anordnen, aber keine fachlichen Regeln veraendern.

## 9. Dashboard-Modus

Das Dashboard ist der erweiterte operative Void-Modus. Es verwendet dieselbe State- und Event-Quelle und kann feste Arbeitsbereiche anbieten:

- Overview
- Chat
- Tasks
- Modules
- AI
- Memory
- Agents
- Devices
- Capabilities
- Events
- Automation
- System
- Security
- Settings

Dashboard-Aktionen laufen exakt wie Void-Aktionen ueber `ActionRequest`, Broker, Permission, Policy, Execution und Audit.

## 10. Portabilitaet und Unabhaengigkeit

Void darf keine Windows-Screen-, Taskbar-, Prozess- oder Dateisystem-APIs direkt aufrufen. Der Renderer nutzt nur UI-Intents, State-Projektionen und Platform Ports.

JAMES muss ohne Void funktionieren. Chat, Tasks, API, CLI, Events und Runtime bleiben ohne den Renderer verfuegbar.

## 11. Abnahme

Der UI-Vertrag gilt als erfuellt, wenn:

1. Live Brain-Zustaende aus echten State-/Event-Daten entstehen.
2. JAMES waehrend einer Task den UI-Fokus selbst aendern kann.
3. Eine Panel-/Fensterentscheidung als UI-Capability auditiert wird.
4. Void keine fachliche Logik oder Permission-Pruefung besitzt.
5. Dashboard und Void dieselbe State-Quelle verwenden.
6. Eine UI-Aktion und eine Agentenaktion denselben Brokerpfad verwenden.
7. Ein Rendererwechsel den Core und die Fachlogik nicht veraendert.
8. Ohne Void alle Kernfunktionen weiter arbeiten.

Die zusaetzlichen visuellen Abnahmekriterien stehen in [void-visual-system.md](void-visual-system.md).
