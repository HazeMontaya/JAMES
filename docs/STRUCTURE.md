# JAMES — Repository-Struktur

Dieses Dokument ist das Ordnungsmanifest des Repos. Es beschreibt, welche Verzeichnisse
existieren, was hineingehört und warum.

## Wurzel (`S:\JAMES`)

| Eintrag | Art | Zweck | Anschauen? |
|---------|-----|-------|-----------|
| `core/` | Quellcode | JAMES Core — das unveränderliche Herz (12 Crates) | selten, nur Git/Cargo |
| `modules/` | Quellcode | Alle `James-*`-Module (17 Module + Assembly + System) | selten, nur Git/Cargo |
| `tools/` | Werkzeuge | Entwicklungshilfen: discovery (TS), diagnostics-bin (Rust) | selten |
| `docs/` | Dokumentation | Einziger Ort zum Lesen: Architektur, BOOTSTRAP, STRUCTURE | **lesen** |
| `.james/` | Laufzeit | config, logs, state, secrets, inventory — automatisch erzeugt | nie (gitignored) |
| `.git/` | Git | Versionsverwaltung | nie |
| `opencode.json` | Config | opencode-Konfiguration | selten |
| `.env.example` | Template | Umgebungsvariablen-Vorlage (Secrets gehören nie ins Repo) | selten |
| `.gitignore` | Git | Ignorier-Regeln | selten |

## Regel

> Was du nie ansehen musst: `core/`, `modules/`, `tools/` (Quellcode) und `.james/` (Daten).
> Was du lesen sollst: nur `docs/`.

Es gibt auf Wurzelebene **keine weiteren Ordner**. Jede neue Fähigkeit ist ein `James-*`-Modul
unter `modules/` — niemals ein neuer Wurzel-Ordner.

## Build-Artefakte

Alle Build-Artefakte sind regenerierbar und gitignored:

- `target/` (Rust) — entsteht bei `cargo build`
- `node_modules/`, `dist/`, `coverage/` (TS) — entstehen bei `npm install` / `npm test`

Sie dürfen jederzeit gelöscht werden (`Remove-Item target -Recurse -Force`), um Platz freizugeben.

## Laufzeit-Daten (`.james/`)

- `config/` — james.toml
- `state/` — health.json, core-status.json
- `logs/` — audit.jsonl, james-core.log
- `secrets/` — verschlüsselte Geheimnisse (Phase 4+; immer leer im Repo)
- `inventory/` — Discovery-Ergebnisse (JSONs + REPORT.md); Snapshots/current.json sind gitignored