# JAMES Security & Audit

## Principles

1. **Never log or persist secrets.** All values entering logs, memory, or the audit store pass through redaction.
2. **Append-only audit.** Goal/skill execution records are written, never mutated.
3. **Tamper evidence.** Each audit entry stores a SHA-256 fingerprint over its own content.
4. **Local trusted store.** SQLite files under `~/.james/memory/` — protect via filesystem permissions.

## Secret Detection

`james_core.security` provides:

- `is_secret_key(name)` — heuristic on key names (case-insensitive):
  `password`, `token`, `api_key`, `secret`, `key`, `client_secret`, `authorization_header`, `credentials`, `private_key`, `passphrase`.
- `redact_secrets(input)` — recursive over `dict` / `list` / `tuple` / `str`:
  - keys matching `is_secret_key` → value replaced with `[REDACTED]`
  - string values matching secret patterns → the matching span replaced
- `redact_secrets_in_text(text)` — replaces matches of:

  | Pattern                           | Example input                                   |
  |-----------------------------------|-------------------------------------------------|
  | Bearer tokens                     | `Authorization: Bearer abcdef...`                |
  | OpenAI-style `sk-...`             | `key: sk-abcdef...`                              |
  | `key=value` credentials           | `token=supersecretvalue123456`                   |
  | GitHub fine-grained `ghp_...`     | `ghp_abcdefghijklm...`                           |
  | AWS access keys `AKIA...`         | `AKIAIOSFODNN7EXAMPLE`                           |
  | Slack `xox[baprs]-...`            | `xoxb-1234567890-...`                            |

- `sanitize_text(text, max_length=100_000)` — strips control characters (keeps `\n`, `\t`) and caps length, for safe storage/logging.

## Audit Log

`james_core.audit.AuditLog` — append-only SQLite backing store.

```sql
CREATE TABLE audit_entries (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    actor TEXT      NOT NULL,   -- e.g. "cli"
    action TEXT     NOT NULL,   -- e.g. "goal.run", "skill.run"
    resource TEXT   NOT NULL,   -- the goal or skill name
    outcome TEXT    NOT NULL,   -- success | failure | warning | info
    severity TEXT   NOT NULL,   -- info | warning | error | critical
    details TEXT,               -- JSON, secret-redacted
    duration_ms REAL,
    fingerprint TEXT NOT NULL,  -- sha256 of (actor|action|resource|outcome|severity|details|created_at)
    created_at TEXT NOT NULL
);
```

- WAL mode for crash safety; index on `created_at`.
- `record()` redacts details first, computes the fingerprint, inserts.
- `query(limit, actor, action, outcome, severity)` — newest first.

### Enums

`AuditOutcome`: `SUCCESS`, `FAILURE`, `WARNING`, `INFO`
`AuditSeverity`: `INFO`, `WARNING`, `ERROR`, `CRITICAL`

## CLI

```bash
james audit                              # last 20
james audit --limit 100
james audit --action goal.run --actor cli
james audit --outcome failure            # filter by result
james audit --severity error --limit 50
```

## Integration

`GoalRunner` (`james-cli`) writes:

- `goal.run` — start and final entries (resource = goal, includes skills used, success flag)
- `skill.run` — per skill (resource = skill name, outcome from `SkillResult.success`, duration from `result.duration_ms`)

The same apply for failures (e.g. no skill matched → `goal.run` outcome `failure`).

## Config Hygiene

- Store API keys only in the credential vault (`paths.credentials`), never in code or config.
- Keep `~/.james` permissions owner-only.
- Rotate keys regularly; audit query contract:
  ```bash
  james audit --action skill.run --outcome failure
  ```
- For forensics, export the raw DB:
  ```bash
  sqlite3 ~/.james/memory/audit.db "SELECT * FROM audit_entries ORDER BY id;"
  ```

## Testing

See `tests/unit/test_security_audit.py`:

- redaction of nested dicts/lists
- pattern redaction in text
- `sanitize_text` behavior
- audit record/query round-trip, redaction-on-write, filters, empty DB
- Msg-free aiosqlite tests (no NATS needed)