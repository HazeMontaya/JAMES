"""Skill loader - YAML discovery + Python entry points"""

import importlib
from collections.abc import Awaitable, Callable
from pathlib import Path
from typing import Any, cast

import structlog
import yaml

from .models import SkillInput, SkillOutput, SkillSpec, SkillStatus, SkillStepSpec, VerificationSpec

logger = structlog.get_logger()


class SkillLoader:
    def __init__(self, skill_dirs: list[str]):
        self._skill_dirs = [Path(d).expanduser() for d in skill_dirs]

    def load_all(self) -> list[SkillSpec]:
        specs = []
        for dir_path in self._skill_dirs:
            if not dir_path.exists():
                continue
            for skill_dir in dir_path.iterdir():
                if skill_dir.is_dir():
                    yaml_path = skill_dir / "skill.yaml"
                    if yaml_path.exists():
                        try:
                            spec = self.load(yaml_path)
                            specs.append(spec)
                        except Exception as e:
                            logger.warning("Failed to load skill", path=str(skill_dir), error=str(e))
        return specs

    def load(self, yaml_path: Path) -> SkillSpec:
        with yaml_path.open(encoding="utf-8") as f:
            data = yaml.safe_load(f)

        raw = data.get("skill", data)
        spec = SkillSpec(
            name=raw.get("name", ""),
            version=raw.get("version", "1.0.0"),
            category=raw.get("category", "general"),
            description=raw.get("description", ""),
            purpose=raw.get("purpose", ""),
            status=SkillStatus(raw.get("status", "enabled")),
            prerequisites=raw.get("prerequisites", []),
            inputs=[SkillInput(**inp) for inp in raw.get("inputs", [])],
            outputs=[SkillOutput(**out) for out in raw.get("outputs", [])],
            steps=[self._parse_step(s) for s in raw.get("steps", [])],
            metrics=raw.get("metrics", []),
            verification=[VerificationSpec(**v) for v in raw.get("verification", [])],
            failure_modes=raw.get("failure_modes", []),
            tags=raw.get("tags", []),
        )

        entry = raw.get("entry_point")
        if entry:
            spec.entry_point = entry
            spec.async_execute = self._load_entry_point(entry)

        return spec

    def _parse_step(self, data: dict[str, Any]) -> SkillStepSpec:
        return SkillStepSpec(
            id=data.get("id", ""),
            type=data.get("type", "llm"),
            name=data.get("name", ""),
            description=data.get("description", ""),
            skill_ref=data.get("skill", data.get("skill_ref")),
            tool_ref=data.get("tool", data.get("tool_ref")),
            function=data.get("function"),
            inputs=data.get("inputs", {}),
            inputs_from=data.get("inputs_from", {}),
            expected_schema=data.get("expected_schema"),
        )

    def _load_entry_point(self, entry: str) -> Callable[[dict[str, Any]], Awaitable[dict[str, Any]]] | None:
        module_path, _, func_name = entry.rpartition(":")
        if not module_path:
            return None
        try:
            module = importlib.import_module(module_path)
            return cast(Callable[[dict[str, Any]], Awaitable[dict[str, Any]]], getattr(module, func_name))
        except Exception as e:
            logger.error("Failed to load skill entry point", entry=entry, error=str(e))
            return None
