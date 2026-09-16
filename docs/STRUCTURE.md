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
| `out/` | Build-Artefakte | Einzige Ablagestelle für alles Regenerierbare (Rust `target`), gitignored | nie |
| `.james/` | Laufzeit | config, logs, state, secrets, inventory — automatisch erzeugt | nie (gitignored) |
| `.git/` | Git | Versionsverwaltung | nie |
| `opencode.json` | Config | opencode-Konfiguration | selten |
| `.env.example` | Template | Umgebungsvariablen-Vorlage (Secrets gehören nie ins Repo) | selten |
| `.gitignore` | Git | Ignorier-Regeln | selten |

## Regel

> Was du nie ansehen musst: `core/`, `modules/`, `tools/` (Quellcode), `.james/` (Daten) und `out/` (Artefakte).
> Was du lesen sollst: nur `docs/`.

Es gibt auf Wurzelebene **keine weiteren Ordner**. Jede neue Fähigkeit ist ein `James-*`-Modul
unter `modules/` — niemals ein neuer Wurzel-Ordner.

## Build-Artefakte (alles in `out/`)

**Es gibt nur einen Artefakt-Ordner: `S:\JAMES\out\`** — gitignored, regenerierbar, jederzeit
löschbar. Nichts Regenerierbares liegt jemals neben Quellcode.

- `out/core/` — Cargo-Target des Core-Workspace
- `out/modules/` — Cargo-Target des Module-Workspace
- `out/diagnostics-bin/` — Cargo-Target des Tools `diagnostics-bin`
- `node_modules/`, `dist/`, `coverage/` (in `tools/discovery`) — entstehen bei `npm install`/`npm test`, gitignored

Die Targets werden per `build.target-dir` in `.cargo/config.toml` jedes Workspaces
(`core/`, `modules/`, `tools/diagnostics-bin/`) dorthin umgeleitet — ein `cargo build` in diesen
Ordnern erzeugt also **nie** ein lokales `target/`, sondern immer `out/`.

## Laufzeit-Daten (`.james/`)

- `config/` — james.toml
- `state/` — health.json, core-status.json
- `logs/` — audit.jsonl, james-core.log
- `secrets/` — verschlüsselte Geheimnisse (Phase 4+; immer leer im Repo)
- `inventory/` — Discovery-Ergebnisse (JSONs + REPORT.md); Snapshots/current.json sind gitignored