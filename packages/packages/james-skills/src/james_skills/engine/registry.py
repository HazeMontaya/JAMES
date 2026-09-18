"""Skill registry"""


import builtins

from .loader import SkillLoader
from .models import SkillSpec, SkillStatus


class SkillRegistry:
    def __init__(self, loader: SkillLoader):
        self._loader = loader
        self._skills: dict[str, SkillSpec] = {}
        self._all_skills: dict[str, SkillSpec] = {}

    def discover(self) -> None:
        self._all_skills = {}
        for spec in self._loader.load_all():
            self._all_skills[spec.name] = spec
            if spec.status == SkillStatus.ENABLED:
                self._skills[spec.name] = spec

    def register(self, spec: SkillSpec) -> None:
        self._all_skills[spec.name] = spec
        if spec.status == SkillStatus.ENABLED:
            self._skills[spec.name] = spec

    def get(self, name: str) -> SkillSpec | None:
        return self._skills.get(name)

    def get_any(self, name: str) -> SkillSpec | None:
        return self._all_skills.get(name)

    def list(self) -> list[SkillSpec]:
        return sorted(self._skills.values(), key=lambda s: s.name)

    def list_by_category(self, category: str) -> builtins.list[SkillSpec]:
        return [s for s in self._skills.values() if s.category == category]

    def has(self, name: str) -> bool:
        return name in self._skills

    def names(self) -> builtins.list[str]:
        return sorted(self._skills.keys())

    def categories(self) -> builtins.list[str]:
        return sorted({s.category for s in self._skills.values()})
