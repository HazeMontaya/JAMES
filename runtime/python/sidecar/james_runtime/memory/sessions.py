"""Session continuity: structured records plus human-readable daily notes."""
from __future__ import annotations
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
import json
from typing import Any

@dataclass
class SessionSummary:
    session_id: str
    objective: str
    started_at: str
    ended_at: str | None = None
    actions: list[str] = field(default_factory=list)
    decisions: list[str] = field(default_factory=list)
    files_changed: list[str] = field(default_factory=list)
    errors: list[str] = field(default_factory=list)
    fixes: list[str] = field(default_factory=list)
    lessons: list[str] = field(default_factory=list)
    result: str | None = None
    def to_dict(self) -> dict[str, Any]:
        return self.__dict__.copy()

class SessionRecorder:
    def __init__(self, root: str | Path) -> None:
        self.root=Path(root).expanduser().resolve()
        self.sessions_dir=self.root/"sessions"; self.daily_dir=self.root/"01 - Daily Notes"
        self.sessions_dir.mkdir(parents=True,exist_ok=True); self.daily_dir.mkdir(parents=True,exist_ok=True)
        self.current: SessionSummary|None=None
    def start(self, objective: str, session_id: str) -> SessionSummary:
        if not objective.strip() or not session_id.strip(): raise ValueError("objective and session_id are required")
        if self.current is not None: raise RuntimeError("a session is already active")
        self.current=SessionSummary(session_id,objective.strip(),datetime.now(timezone.utc).isoformat()); self._persist(); return self.current
    def record(self, *, action=None, decision=None, file_changed=None, error=None, fix=None, lesson=None) -> None:
        if self.current is None: raise RuntimeError("no active session")
        for value,bucket in ((action,self.current.actions),(decision,self.current.decisions),(file_changed,self.current.files_changed),
                             (error,self.current.errors),(fix,self.current.fixes),(lesson,self.current.lessons)):
            if value and value.strip(): bucket.append(value.strip())
        self._persist()
    def finish(self,result: str)->SessionSummary:
        if self.current is None: raise RuntimeError("no active session")
        self.current.ended_at=datetime.now(timezone.utc).isoformat(); self.current.result=result.strip(); self._persist()
        finished=self.current; self._write_daily_note(finished); self.current=None; return finished
    def _persist(self)->None:
        assert self.current is not None
        (self.sessions_dir/f"{self.current.session_id}.json").write_text(json.dumps(self.current.to_dict(),indent=2,ensure_ascii=False),encoding="utf-8")
    def _write_daily_note(self,s:SessionSummary)->None:
        stamp=datetime.now().astimezone(); month=self.daily_dir/stamp.strftime("%Y-%m"); month.mkdir(parents=True,exist_ok=True)
        path=month/f"{stamp:%Y-%m-%d}.md"; existing=path.read_text(encoding="utf-8") if path.exists() else f"# {stamp:%Y-%m-%d}\n"
        lines=["",f"## Session {s.session_id}","",f"**Objective:** {s.objective}","","### What Got Done"]+[f"- {x}" for x in s.actions] or []
        lines += ["","### Decisions Made"]+[f"- {x}" for x in s.decisions] + ["","### Notes Touched"]+[f"- {x}" for x in s.files_changed]
        lines += ["","### Errors / Fixes"]+[f"- Error: {x}" for x in s.errors]+[f"- Fix: {x}" for x in s.fixes]
        lines += ["","### Lessons"]+[f"- {x}" for x in s.lessons]+["",f"**Result:** {s.result or 'unknown'}",""]
        path.write_text(existing.rstrip()+"\n"+"\n".join(lines),encoding="utf-8")
