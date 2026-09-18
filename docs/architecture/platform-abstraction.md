# JAMES Platform Abstraction

Status: verbindlicher Architekturvertrag fuer die PC-first-Entwicklung

## Ziel

JAMES wird zuerst unter Windows betrieben, aber der Core bleibt portabel. Plattformabhaengige Funktionen laufen ausschliesslich ueber Ports und Adapter.

```text
Domain/Core
  -> Platform Port
  -> Platform Adapter
  -> Operating System
```

## Ports

Der portable Core darf nur abstrahierte Dienste verwenden:

- `FileSystemPort`
- `ProcessPort`
- `ApplicationPort`
- `NetworkPort`
- `DevicePort`
- `AudioPort`
- `CameraPort`
- `DisplayPort`
- `ComputePort`
- `NotificationPort`
- `SystemInfoPort`
- `PowerPort`
- `PlatformSecurityPort`
- `DataDirectoryPort`

Die konkreten Traits und Rueckgabemodelle muessen serialisierbare, plattformneutrale Domain-Typen verwenden.

## Adapter

Die erste Implementierung ist Windows:

```text
james-platform
james-platform-windows
```

Spaetere Adapter:

```text
james-platform-linux
james-platform-macos
james-platform-android
james-platform-ios
james-platform-container
```

Nicht implementierte Plattformen liefern `Unsupported` oder `Unknown` mit Quelle und Grund. Leere Listen duerfen nicht als erfolgreiche Detection interpretiert werden.

## Daten- und Pfadregeln

Core-Code darf keine festen OS-Pfade kennen. Fachliche Daten werden ueber logische Bereiche adressiert:

```text
identity
configuration
memory
tasks
agents
modules
events
state
audit
inventory
```

Der `DataDirectoryPort` bestimmt den physischen Ort. Export, Import, Migration und Recovery verwenden portable Formate.

## Fehlervertrag

Platform Ports unterscheiden mindestens:

- `Unsupported`
- `PermissionDenied`
- `NotFound`
- `Unavailable`
- `Timeout`
- `InvalidInput`
- `Io`
- `Unknown`

Der Core darf aus einem Plattformfehler keine erfolgreiche Capability ableiten.

## Testvertrag

Jeder Port erhaelt:

- ein In-Memory-Test-Double
- Tests fuer Erfolg, fehlende Ressource, PermissionDenied und Unsupported
- mindestens einen Adapter-Contract-Test
- keine Voraussetzung fuer reale Hardware in Core-Tests

## UI- und AI-Regel

Void, Dashboard und API lesen State/Events/Presentation Models. Sie verwenden keine OS-APIs direkt.

James-AI verwendet Provider-Ports. Ein Providerwechsel darf den Core nicht veraendern.

## Abnahme

Die PC-Referenzphase ist erst portabel genug, wenn:

1. Core-Tests ohne Windows APIs laufen.
2. Windows-Discovery ueber einen Adapter gespeist wird.
3. Daten ausserhalb des Defaultpfads exportiert und wiederhergestellt werden koennen.
4. fehlende Plattformdienste als `unknown`/`unsupported` sichtbar sind.
5. Void und Dashboard dieselbe State-/Event-Quelle verwenden.
