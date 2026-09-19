from pathlib import Path

from james_runtime.memory.bootstrap import bootstrap
from james_runtime.memory.resolver import ContextResolver
from james_runtime.memory.skills import SkillDefinition, SkillStore
from james_runtime.steering.autotune import compute_autotune
from james_runtime.steering.feedback import FeedbackStore
from james_runtime.steering.stm import transform_text

def test_autotune_is_transparent():
    result = compute_autotune("Implement and debug a Python function")
    assert result.context == "code"
    assert result.confidence > 0
    assert result.profile.temperature < 0.5

def test_feedback_ema_persists(tmp_path: Path):
    store = FeedbackStore(tmp_path / "feedback.json")
    learned = store.update("code", 1, {"temperature": 0.2}, {"temperature": 0.5})
    assert 0.0 < learned["temperature"] <= 0.2
    reloaded = FeedbackStore(tmp_path / "feedback.json")
    assert reloaded.get("code")["temperature"] == learned["temperature"]

def test_stm_modules_are_deterministic():
    text, applied = transform_text("Please note that\n\n\nAnswer.", ["hedge_reducer", "direct_mode"])
    assert applied == ["hedge_reducer", "direct_mode"]
    assert "Please note" not in text

def test_context_resolver_finds_skill_and_memory(tmp_path: Path):
    root = tmp_path / "vault"
    vault = bootstrap(root)
    vault.write_note("Rust Repository", "Repository architecture and Rust build notes.", kind="project", tags=["rust", "repo"])
    skills = SkillStore(root / "skills")
    skills.save(SkillDefinition(name="repo-review", description="Review repositories", triggers=["repository", "rust"], procedure=["inspect tree"]))
    matches = ContextResolver(vault, skills).resolve("review rust repository")
    assert matches
    assert any(m.kind == "skill" and m.name == "repo-review" for m in matches)