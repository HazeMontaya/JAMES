"""Task -> relevant skills/memory resolver.

Scores transparent lexical matches so the agent can explain why context was loaded.
"""
from __future__ import annotations
from dataclasses import dataclass
import re
from .skills import SkillStore, SkillDefinition
from .vault import MemoryVault, MemoryNote

@dataclass(frozen=True)
class ContextMatch:
    kind: str
    name: str
    score: float
    reason: str

class ContextResolver:
    def __init__(self, vault: MemoryVault, skills: SkillStore) -> None:
        self.vault = vault
        self.skills = skills

    @staticmethod
    def _tokens(text: str) -> set[str]:
        return {x for x in re.findall(r"[\w-]+", text.lower()) if len(x) > 2}

    def resolve(self, task: str, *, skill_limit: int = 5, memory_limit: int = 8) -> list[ContextMatch]:
        tokens = self._tokens(task)
        matches: list[ContextMatch] = []
        for skill in self.skills.list():
            skill_tokens = self._tokens(" ".join([
                skill.name, skill.description, *skill.triggers, *skill.inputs, *skill.outputs,
                *skill.procedure, *skill.lessons,
            ]))
            overlap = tokens & skill_tokens
            if overlap:
                score = len(overlap) / max(1, len(tokens))
                matches.append(ContextMatch("skill", skill.name, score, "token overlap: " + ", ".join(sorted(overlap)[:8])))
        matches.sort(key=lambda x: (-x.score, x.name))
        matches = matches[:skill_limit]

        for note in self.vault.search(task, limit=memory_limit):
            note_tokens = self._tokens(" ".join([note.title, note.path, note.project or "", *note.tags]))
            overlap = tokens & note_tokens
            score = len(overlap) / max(1, len(tokens))
            matches.append(ContextMatch("memory", note.note_id, score, "indexed metadata overlap"))
        return sorted(matches, key=lambda x: (-x.score, x.kind, x.name))
