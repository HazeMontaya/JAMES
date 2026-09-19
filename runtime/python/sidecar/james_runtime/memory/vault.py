"""Markdown-first persistent memory with a SQLite retrieval index."""
from __future__ import annotations
from dataclasses import dataclass
from datetime import datetime, timezone
import re
import sqlite3
from pathlib import Path
from typing import Any

@dataclass(frozen=True)
class MemoryNote:
    note_id: str
    path: str
    title: str
    kind: str
    status: str
    project: str | None
    tags: tuple[str, ...]
    updated_at: str

class MemoryVault:
    """Markdown remains authoritative; SQLite is a disposable derived index."""
    def __init__(self, root: str | Path) -> None:
        self.root = Path(root).expanduser().resolve()
        self.root.mkdir(parents=True, exist_ok=True)
        self.db_path = self.root / ".james-memory.sqlite3"
        self._init_db()
    def _connect(self) -> sqlite3.Connection:
        conn = sqlite3.connect(self.db_path)
        conn.row_factory = sqlite3.Row
        return conn
    def _init_db(self) -> None:
        with self._connect() as db:
            db.execute("""CREATE TABLE IF NOT EXISTS notes (
                note_id TEXT PRIMARY KEY, path TEXT NOT NULL UNIQUE, title TEXT NOT NULL,
                kind TEXT NOT NULL, status TEXT NOT NULL, project TEXT, tags TEXT NOT NULL,
                updated_at TEXT NOT NULL)""")
            db.execute("CREATE INDEX IF NOT EXISTS idx_notes_status ON notes(status)")
            db.execute("CREATE INDEX IF NOT EXISTS idx_notes_project ON notes(project)")
    @staticmethod
    def _slug(value: str) -> str:
        value = re.sub(r"[^a-zA-Z0-9._-]+", "-", value.strip().lower())
        return value.strip("-") or "note"
    @staticmethod
    def _frontmatter(meta: dict[str, Any]) -> str:
        lines = ["---"]
        for key, value in meta.items():
            encoded = "[" + ", ".join(str(v) for v in value) + "]" if isinstance(value, list) else str(value)
            lines.append(f"{key}: {encoded}")
        lines.append("---")
        return "\n".join(lines)
    def write_note(self, title: str, body: str, *, kind: str = "reference", status: str = "active",
                   project: str | None = None, tags: list[str] | tuple[str, ...] = (),
                   relative_path: str | None = None, note_id: str | None = None) -> MemoryNote:
        if not title.strip() or not body.strip():
            raise ValueError("title and body must not be empty")
        note_id = note_id or self._slug(title)
        rel = Path(relative_path or f"notes/{note_id}.md")
        if rel.is_absolute() or ".." in rel.parts:
            raise ValueError("relative_path must remain inside the vault")
        path = self.root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        now = datetime.now(timezone.utc).isoformat()
        path.write_text(self._frontmatter({"id":note_id,"status":status,"kind":kind,"project":project or "","tags":list(tags),"updated_at":now}) + f"\n\n# {title.strip()}\n\n{body.strip()}\n", encoding="utf-8")
        note = MemoryNote(note_id, str(rel).replace("\\","/"), title.strip(), kind, status, project, tuple(tags), now)
        with self._connect() as db:
            db.execute("""INSERT INTO notes(note_id,path,title,kind,status,project,tags,updated_at)
                VALUES(?,?,?,?,?,?,?,?) ON CONFLICT(note_id) DO UPDATE SET
                path=excluded.path,title=excluded.title,kind=excluded.kind,status=excluded.status,
                project=excluded.project,tags=excluded.tags,updated_at=excluded.updated_at""",
                (note.note_id,note.path,note.title,note.kind,note.status,note.project,"\x1f".join(note.tags),note.updated_at))
        return note
    def read_note(self, note_id: str) -> str:
        with self._connect() as db:
            row = db.execute("SELECT path FROM notes WHERE note_id=?", (note_id,)).fetchone()
        if not row: raise KeyError(note_id)
        return (self.root / row["path"]).read_text(encoding="utf-8")
    def search(self, query: str, *, limit: int = 10) -> list[MemoryNote]:
        tokens = [t for t in re.findall(r"[\w-]+", query.lower()) if t]
        if not tokens: return []
        clauses, params = [], []
        for token in tokens:
            p = f"%{token}%"
            clauses.append("(lower(title) LIKE ? OR lower(path) LIKE ? OR lower(tags) LIKE ? OR lower(project) LIKE ?)")
            params.extend([p,p,p,p])
        params.append(max(1, limit))
        with self._connect() as db:
            rows = db.execute("SELECT * FROM notes WHERE " + " OR ".join(clauses) + " ORDER BY updated_at DESC LIMIT ?", params).fetchall()
        return [self._row_to_note(row) for row in rows]
    def list_active(self, *, limit: int = 100) -> list[MemoryNote]:
        with self._connect() as db:
            rows = db.execute("SELECT * FROM notes WHERE status='active' ORDER BY updated_at DESC LIMIT ?", (max(1,limit),)).fetchall()
        return [self._row_to_note(row) for row in rows]
    def rebuild_index(self) -> int:
        with self._connect() as db: db.execute("DELETE FROM notes")
        count = 0
        for path in self.root.rglob("*.md"):
            if path.name.startswith("."): continue
            content = path.read_text(encoding="utf-8")
            meta = self._parse_frontmatter(content)
            title = self._title_from_content(content)
            rel = str(path.relative_to(self.root)).replace("\\","/")
            note = MemoryNote(str(meta.get("id") or self._slug(path.stem)), rel, title,
                              str(meta.get("kind","reference")), str(meta.get("status","active")),
                              str(meta.get("project") or "") or None, tuple(meta.get("tags",[])),
                              str(meta.get("updated_at") or datetime.fromtimestamp(path.stat().st_mtime,tz=timezone.utc).isoformat()))
            with self._connect() as db:
                db.execute("INSERT OR REPLACE INTO notes VALUES(?,?,?,?,?,?,?,?)",
                           (note.note_id,note.path,note.title,note.kind,note.status,note.project,"\x1f".join(note.tags),note.updated_at))
            count += 1
        return count
    @staticmethod
    def _parse_frontmatter(content: str) -> dict[str, Any]:
        if not content.startswith("---\n"): return {}
        end = content.find("\n---",4)
        if end < 0: return {}
        data = {}
        for line in content[4:end].splitlines():
            if ":" not in line: continue
            key,value = line.split(":",1); value=value.strip()
            if value.startswith("[") and value.endswith("]"):
                value=[v.strip() for v in value[1:-1].split(",") if v.strip()]
            data[key.strip()] = value
        return data
    @staticmethod
    def _title_from_content(content: str) -> str:
        for line in content.splitlines():
            if line.startswith("# "): return line[2:].strip()
        return "Untitled"
    @staticmethod
    def _row_to_note(row: sqlite3.Row) -> MemoryNote:
        return MemoryNote(row["note_id"],row["path"],row["title"],row["kind"],row["status"],row["project"] or None,
                          tuple(filter(None,row["tags"].split("\x1f"))),row["updated_at"])
