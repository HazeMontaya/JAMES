"""Skill composer - builds skill graphs for goals"""

import re
from dataclasses import dataclass, field
from typing import Any

from .models import SkillSpec
from .registry import SkillRegistry


@dataclass
class ComposedStep:
    skill_name: str
    inputs: dict[str, Any] = field(default_factory=dict)
    depends_on: list[str] = field(default_factory=list)
    order: int = 0


@dataclass
class SkillComposition:
    goal: str = ""
    skills: list[ComposedStep] = field(default_factory=list)

    def skill_names(self) -> list[str]:
        return [s.skill_name for s in self.skills]


class SkillComposer:
    def __init__(self, registry: SkillRegistry):
        self._registry = registry

    def compose_from_keywords(self, goal: str, available: list[str]) -> SkillComposition:
        goal_lower = goal.lower()
        skills: list[SkillSpec] = []
        for name in available:
            spec = self._registry.get(name)
            if spec is not None:
                skills.append(spec)

        selected: list[ComposedStep] = []
        used = set()

        # Simple scoring by keyword relevance
        scored = []
        for spec in skills:
            score = self._score_spec(spec, goal_lower)
            if score > 0:
                scored.append((score, spec))

        scored.sort(key=lambda x: x[0], reverse=True)

        for _, spec in scored:
            if spec.name in used:
                continue
            used.add(spec.name)
            step = ComposedStep(skill_name=spec.name, order=len(selected))
            inputs = {}
            # Pass goal as input if skill has a 'topic' or 'goal' input
            for sin in spec.inputs:
                if sin.name in ("topic", "goal", "subject", "problem"):
                    inputs[sin.name] = goal
            step.inputs = inputs
            selected.append(step)

        # Chain dependencies
        for i, step in enumerate(selected):
            if i > 0:
                step.depends_on = [selected[i - 1].skill_name]
                step.order = i

        return SkillComposition(goal=goal, skills=selected)

    def _score_spec(self, spec: SkillSpec, goal_lower: str) -> float:
        score = 0.0
        tokens = [spec.name, spec.category, spec.description, spec.purpose]
        goal_words = set(re.findall(r"[a-zäöüß]+", goal_lower))

        for t in tokens:
            for word in re.findall(r"[a-zäöüß]+", t.lower()):
                if word in goal_words:
                    score += 1.5
                else:
                    # prefix match to catch language variants (markt ~ market, recherche ~ research)
                    for gw in goal_words:
                        if len(word) >= 5 and word[:4] == gw[:4]:
                            score += 0.75
                            break
        for tag in spec.tags:
            if tag.lower() in goal_lower:
                score += 3.0
        for kw in ["research", "markt", "market", "analyse", "analyze", "recherche", "recherch"]:
            if kw in goal_lower and kw in " ".join(tokens).lower():
                score += 2.0
        return score
