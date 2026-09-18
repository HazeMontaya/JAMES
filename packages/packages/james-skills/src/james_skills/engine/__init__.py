"""JAMES Skill Engine facade"""

from pathlib import Path
from typing import Any

from .composer import SkillComposer, SkillComposition
from .executor import SkillExecutor
from .loader import SkillLoader
from .models import SkillContext, SkillPlan, SkillResult, SkillSpec
from .registry import SkillRegistry

__all__ = [
    "SkillComposer",
    "SkillComposition",
    "SkillContext",
    "SkillEngine",
    "SkillExecutor",
    "SkillLoader",
    "SkillPlan",
    "SkillRegistry",
    "SkillResult",
    "SkillSpec",
]


class SkillEngine:
    def __init__(
        self,
        skill_dirs: list[str] | None = None,
        builtin_dirs: list[str] | None = None,
    ):
        builtin = builtin_dirs or [str(Path(__file__).parent.parent / "builtin")]
        dirs = (skill_dirs or [str(Path("~/.james/skills").expanduser())]) + builtin
        self._loader = SkillLoader(dirs)
        self._registry = SkillRegistry(self._loader)
        self._executor = SkillExecutor(self._registry)
        self._composer = SkillComposer(self._registry)
        self.discover()

    def discover(self) -> None:
        self._registry.discover()

    def registry(self) -> SkillRegistry:
        return self._registry

    def list_skills(self) -> list[SkillSpec]:
        return self._registry.list()

    def get_skill(self, name: str) -> SkillSpec | None:
        return self._registry.get(name)

    def compose(self, goal: str, available: list[str] | None = None) -> SkillComposition:
        av = available or self._registry.names()
        return self._composer.compose_from_keywords(goal, av)

    async def plan(self, spec: SkillSpec, ctx: SkillContext) -> SkillPlan:
        return await self._executor.plan(spec, ctx)

    async def execute(
        self,
        spec: SkillSpec,
        ctx: SkillContext,
        plan: SkillPlan | None = None,
    ) -> SkillResult:
        return await self._executor.execute(spec, ctx, plan)

    async def run_skill(
        self,
        name: str,
        context: dict[str, Any],
        memory: Any = None,
        world: Any = None,
        model_router: Any = None,
        event_bus: Any = None,
    ) -> SkillResult:
        spec = self._registry.get(name)
        if not spec:
            raise ValueError(f"Skill '{name}' not found")
        ctx = SkillContext(inputs=context, memory=memory, world=world, model_router=model_router, event_bus=event_bus)
        return await self._executor.execute(spec, ctx)
