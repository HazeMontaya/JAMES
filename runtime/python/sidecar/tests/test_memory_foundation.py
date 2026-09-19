from pathlib import Path
from james_runtime.memory.bootstrap import bootstrap
from james_runtime.memory.events import EventLog
from james_runtime.memory.sessions import SessionRecorder
from james_runtime.memory.skills import SkillDefinition, SkillStore

def test_bootstrap_and_markdown_memory(tmp_path: Path):
    vault=bootstrap(tmp_path/"memory")
    note=vault.write_note("Project Alpha","The current milestone is persistence.",kind="project",tags=["alpha","persistence"])
    assert note.note_id=="project-alpha"
    assert vault.search("persistence")[0].note_id==note.note_id
    assert "Project Alpha" in vault.read_note(note.note_id)

def test_session_continuity_and_daily_note(tmp_path: Path):
    root=tmp_path/"memory"; recorder=SessionRecorder(root)
    recorder.start("build memory foundation","s1")
    recorder.record(action="implemented vault",lesson="index is derived")
    result=recorder.finish("complete")
    assert result.result=="complete"
    assert list((root/"01 - Daily Notes").rglob("*.md"))

def test_event_log_roundtrip(tmp_path: Path):
    log=EventLog(tmp_path/"events.jsonl"); created=log.append("MEMORY_UPDATED",{"note":"x"})
    assert log.tail(1)[0].event_id==created.event_id

def test_skill_lessons(tmp_path: Path):
    store=SkillStore(tmp_path/"skills")
    store.save(SkillDefinition(name="research-repo",description="Inspect a repository.",triggers=["analyze repository"],procedure=["inspect tree","read files"],quality_checks=["verify"]))
    store.add_lesson("research-repo","Do not infer implementation from README alone.")
    assert store.load("research-repo").lessons==["Do not infer implementation from README alone."]
