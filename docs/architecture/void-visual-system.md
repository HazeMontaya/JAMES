# JAMES Void Visual System

Status: verbindlicher visueller und interaktiver Vertrag

## 1. Ziel

James-Void soll nicht wie ein normales Dashboard, Cyberpunk-HUD oder eine reine Animation wirken. Der Nutzer soll JAMES beim Wahrnehmen, Denken, Entscheiden, Ausfuehren, Verifizieren und Erholen sehen.

> Animation ist Information. Jede sichtbare Bewegung benoetigt eine semantische Ursache.

## 2. Visuelle Ebenen

Void besitzt zwei Hauptbereiche:

- **Live Brain:** dauerhaftes Zentrum und Projektion des echten JAMES-Zustands
- **Dynamic Context Layer:** relevante Informationen, Tasks, Agenten, Module, Geraete, Prozesse, Browser, Forschung, Fehler und Controls

Der Inhalt des Dynamic Context Layers ist nicht statisch. Er wird aus `UiIntent` und aktuellen State-Projektionen abgeleitet.

## 3. Brain-Anatomie

Das Brain ist keine menschliche Organ- oder Schaedel-Darstellung. Es ist eine synthetische kognitive Struktur aus:

- Core-Kern
- funktionalen Knoten
- Verbindungen
- Signalfluss
- Aktivitaetsfeld
- Fokus- und Sicherheitsringen

Funktionale Zonen koennen intern repraesentiert werden als:

```text
MEMORY     REASONING     PLANNING
INPUT      CORE          ACTION
OUTPUT     VERIFY        RECOVERY
```

Nicht jede Zone oder jedes Modul ist dauerhaft sichtbar. Nur relevante Aktivitaet wird visualisiert.

## 4. Brain-Layer

### Core Layer

Der innere Kern zeigt, dass JAMES aktiv und erreichbar ist. Im Zustand `IDLE` pulsiert er langsam und ruhig.

### Neural Network Layer

Knoten und Kanten repraesentieren relevante aktive Systeme, Module, Agenten oder Verarbeitungsschritte. Ihre Anzahl bleibt begrenzt, damit Aktivitaet lesbar bleibt.

### Signal Flow Layer

Bewegte Signale zeigen Richtung und Ursache eines realen Informationsflusses:

```text
INPUT -> PROCESSING -> CAPABILITY -> VERIFICATION -> OUTPUT
```

### Activity Field

Ein dezentes Feld zeigt Systemintensitaet. Es darf nur aus echten Ressourcen-/Aktivitaetsdaten abgeleitet werden, nicht aus Zufall.

## 5. Brain State Machine

Mindestens folgende Zustaende werden definiert:

```text
IDLE
LISTENING
UNDERSTANDING
THINKING
PLANNING
SEARCHING
EXECUTING
VERIFYING
COMPLETED
WAITING
LEARNING
WARNING
ERROR
RECOVERING
SLEEPING
OFFLINE
RESOURCE_LIMITED
SECURITY_LOCK
```

Jeder Zustand definiert:

- Motion Profile
- Signalrichtung und -dichte
- Geschwindigkeit
- Lichtintensitaet
- Partikel-/Knotenverhalten
- sichtbaren UI-Fokus
- moegliche Controls
- optionalen Sound

## 6. Semantische Motion Profiles

- `IDLE`: langsames Atmen/Pulsieren, wenige Signale, keine Hektik
- `LISTENING`: Signale bewegen sich zum Brain, Input-Zonen reagieren
- `THINKING`: zirkulierende Signale, temporaere Knoten, erkundete Verbindungen
- `PLANNING`: geordnete geometrische Pfade und sichtbare Plan-Schritte
- `EXECUTING`: Signale verlassen das Brain zu Capability-, Modul- oder Panel-Knoten
- `VERIFYING`: Signale kehren zurueck, Ergebnisse werden geprueft und eingespeist
- `ERROR`: betroffener Bereich stoppt oder isoliert sich; keine Alarmshow
- `RECOVERING`: Isolation, Diagnose, Fallback, Wiederverbindung und Resume werden sichtbar
- `SECURITY_LOCK`: kontrollierter Ring und sichtbare Policy-Entscheidung `ALLOW`, `DENY` oder `ASK`
- `SLEEPING`: minimale Bewegung und minimale Aktualisierung, Brain bleibt sichtbar

## 7. Kausalitaet

Das Visualsystem darf keinen Zustand vortaeuschen. Es bezieht mindestens ein:

- echte Task-/Execution-Zustaende
- Event- und Correlation-Daten
- Capability- und Modulstatus
- Provider-/Agentenstatus
- CPU/GPU/RAM- und Ressourcenwerte
- Permission-/Policy-Entscheidungen
- Verification-Ergebnisse
- Recovery-Zustaende

Ein hoher GPU-Wert darf die Aktivitaet erhoehen, aber allein keinen `THINKING`-Zustand behaupten. Geschwindigkeit ist ein Signal, nicht die alleinige Statusquelle.

## 8. Kontextbewegung

Informationen koennen im Void priorisiert werden:

```text
BACKGROUND -> NORMAL -> IMPORTANT -> URGENT -> CRITICAL
```

Relevante Inhalte naehern sich dem Brain, werden groesser oder erhalten Fokus. Nicht mehr relevante Inhalte ziehen sich zurueck, werden kleiner oder verschwinden in einen aufrufbaren Verlauf.

Darstellung beschreibt:

- Position: `CENTER`, `LEFT`, `RIGHT`, `TOP`, `BOTTOM`, `FLOATING`, `OVERLAY`, `WINDOW`, `TAB`
- Groesse: `MICRO`, `SMALL`, `MEDIUM`, `LARGE`, `FOCUS`, `FULLSCREEN`
- Dauer: `MOMENTARY`, `TEMPORARY`, `PERSISTENT`, `UNTIL_RESOLVED`
- Prioritaet: `BACKGROUND`, `NORMAL`, `IMPORTANT`, `URGENT`, `CRITICAL`

## 9. Window Intelligence

JAMES oeffnet keine Oberflaechen blind. Der UI-Orchestrator prueft:

- Informationsumfang
- aktuelle freie Flaeche
- Interaktionskomplexitaet
- Risiko und Bestaetigungsbedarf
- Nutzerfokus
- Geraeteklasse
- bereits offene Panels/Fenster

Beispiele:

- kurze Statusinformation: Void-Kontext
- umfangreiche Recherche: Panel
- grosse Browser-Interaktion: eigenes Fenster
- kritische Freigabe: fokussierter Dialog

Alle Oeffnungen sind UI-Capabilities und werden policygemaess behandelt.

## 10. Farb- und Klangsemantik

Grundpalette:

- Hintergrund: nahezu schwarz
- Primaer/Aktivitaet/Fokus: warmes Gold, sparsam
- Text: warmes Weiss
- Sekundaertext: gedimmtes Grau
- Erfolg: kontrolliertes Gruen
- Warnung: Amber
- Fehler: kontrolliertes Rot
- Information: kuehles neutrales Signal

Gold bedeutet Aufmerksamkeit oder Aktivitaet, nicht Dekoration. Partikel, Glitches, Neonregen, dauernde Rotation, zufaellige Explosionen und Gaming-HUD-Effekte sind ausgeschlossen.

Optionaler Sound ist deaktivierbar und bleibt dezent:

```text
IDLE -> kaum hoerbarer Grundton
INPUT -> kurzer Impuls
THINKING -> subtile Textur
ACTION -> dezenter Impuls
SUCCESS -> kurze Bestaetigung
WARNING/ERROR -> kontrolliertes Signal
```

## 11. Technische Pipeline

```text
JAMES STATE
  -> TASK / EVENTS / RESOURCES / SECURITY
  -> VOID CONTEXT ENGINE
  -> RELEVANCE ENGINE
  -> PRIORITY ENGINE
  -> PRESENTATION ENGINE
  -> VOID STATE / UI INTENT
  -> ANIMATION ENGINE
  -> RENDERER
```

Die Animation Engine nimmt deterministische State- und Motion-Parameter entgegen. Sie erzeugt keine fachlichen Entscheidungen.

## 12. Abnahme

Das Visualsystem gilt als korrekt, wenn:

1. Brain-Zustaende aus echten Events und State entstehen.
2. Recherche, Code-Build, Systemmonitoring und Security-Entscheidungen unterschiedliche semantische Darstellungen besitzen.
3. Fehler sichtbar isoliert, diagnostiziert und recovered werden koennen.
4. Animation bei fehlender Aktivitaet reduziert wird.
5. Relevante Informationen sich fokussiert naehern und danach zuruecktreten.
6. Fenster-/Panel-Oeffnungen als UI-Capabilities auditiert werden.
7. Ein Rendererwechsel die Semantik nicht veraendert.
8. Keine Animation eine nicht existierende Aktivitaet behauptet.
9. Die zentrale Void-Ansicht ohne statische Dashboard-Widgetpflicht funktioniert.
10. Das System ohne Void weiter lauffaehig bleibt.
