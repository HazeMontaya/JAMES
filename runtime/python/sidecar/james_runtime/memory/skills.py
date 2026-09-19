"""Persistent Jobs/Skills with a lesson loop."""
from __future__ import annotations
from dataclasses import dataclass, field
from pathlib import Path
import re

@dataclass
class SkillDefinition:
    name: str
    description: str
    triggers: list[str]=field(default_factory=list)
    inputs: list[str]=field(default_factory=list)
    outputs: list[str]=field(default_factory=list)
    procedure: list[str]=field(default_factory=list)
    quality_checks: list[str]=field(default_factory=list)
    lessons: list[str]=field(default_factory=list)
    permissions: list[str]=field(default_factory=list)
    def to_markdown(self)->str:
        def bullets(v): return "\n".join(f"- {x}" for x in v) or "- None."
        return f"""---
type: skill
status: active
name: {self.name}
---

# {self.name}

{self.description}

## Triggers
{bullets(self.triggers)}

## Inputs
{bullets(self.inputs)}

## Outputs
{bullets(self.outputs)}

## Procedure
{bullets(self.procedure)}

## Quality Checks
{bullets(self.quality_checks)}

## Permissions
{bullets(self.permissions)}

## Lessons
{bullets(self.lessons)}
"""
    @classmethod
    def from_markdown(cls,content:str)->"SkillDefinition":
        m=re.search(r"^# (.+)$",content,re.MULTILINE); name=m.group(1).strip() if m else "unnamed"
        def section(header):
            m=re.search(rf"^## {re.escape(header)}\n(.*?)(?=^## |\Z)",content,re.MULTILINE|re.DOTALL)
            return [x[2:].strip() for x in m.group(1).splitlines() if x.startswith("- ")] if m else []
        description_match=re.search(rf"^# {re.escape(name)}\\n\\n(.*?)(?=^## |\\Z)",content,re.MULTILINE|re.DOTALL)
        description=description_match.group(1).strip() if description_match else ""
        return cls(name=name,description=description,triggers=section("Triggers"),inputs=section("Inputs"),outputs=section("Outputs"),
                   procedure=section("Procedure"),quality_checks=section("Quality Checks"),permissions=section("Permissions"),lessons=section("Lessons"))

class SkillStore:
    def __init__(self,root:str|Path)->None: self.root=Path(root).expanduser().resolve(); self.root.mkdir(parents=True,exist_ok=True)
    def path_for(self,name:str)->Path:
        safe=re.sub(r"[^a-zA-Z0-9._-]+","-",name.strip().lower()).strip("-")
        if not safe: raise ValueError("skill name must not be empty")
        return self.root/f"{safe}.md"
    def save(self,skill:SkillDefinition)->Path:
        p=self.path_for(skill.name); p.write_text(skill.to_markdown(),encoding="utf-8"); return p
    def load(self,name:str)->SkillDefinition:
        p=self.path_for(name)
        if not p.exists(): raise KeyError(name)
        return SkillDefinition.from_markdown(p.read_text(encoding="utf-8"))
    def add_lesson(self,name:str,lesson:str)->Path:
        s=self.load(name)
        if lesson.strip() and lesson.strip() not in s.lessons: s.lessons.append(lesson.strip())
        return self.save(s)
    def list(self)->list[SkillDefinition]:
        return [SkillDefinition.from_markdown(p.read_text(encoding="utf-8")) for p in sorted(self.root.glob("*.md"))]
