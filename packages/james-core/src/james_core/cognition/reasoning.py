"""Reasoning Engine for JAMES"""

from dataclasses import dataclass
from enum import Enum
from typing import Any

from .model_router import ModelRouter


class ReasoningStrategy(Enum):
    COT = "chain_of_thought"
    TOT = "tree_of_thought"
    REACT = "react"
    SELF_CONSISTENCY = "self_consistency"


@dataclass
class ReasoningStep:
    step_type: str
    content: str
    confidence: float
    metadata: dict[str, Any] | None = None


@dataclass
class ReasoningResult:
    conclusion: str
    steps: list[ReasoningStep]
    overall_confidence: float
    strategy_used: ReasoningStrategy
    metadata: dict[str, Any] | None = None


class ReasoningEngine:
    def __init__(self, model_router: ModelRouter):
        self._model_router = model_router

    async def reason(
        self,
        problem: str,
        context: str = "",
        strategy: ReasoningStrategy = ReasoningStrategy.COT,
        max_steps: int = 10,
    ) -> ReasoningResult:
        if strategy == ReasoningStrategy.COT:
            return await self._chain_of_thought(problem, context, max_steps)
        elif strategy == ReasoningStrategy.REACT:
            return await self._react(problem, context, max_steps)
        elif strategy == ReasoningStrategy.SELF_CONSISTENCY:
            return await self._self_consistency(problem, context)
        else:
            return await self._chain_of_thought(problem, context, max_steps)

    async def _chain_of_thought(self, problem: str, context: str, max_steps: int) -> ReasoningResult:
        prompt = f"""Problem: {problem}

Context: {context}

Think through this step by step. Show your reasoning clearly.

Step 1:"""

        response = await self._model_router.generate(prompt, temperature=0.3)

        steps = self._parse_cot_steps(response.content)
        conclusion = steps[-1].content if steps else response.content
        overall_confidence = sum(s.confidence for s in steps) / len(steps) if steps else 0.5

        return ReasoningResult(
            conclusion=conclusion,
            steps=steps,
            overall_confidence=overall_confidence,
            strategy_used=ReasoningStrategy.COT,
            metadata={"raw_response": response.content},
        )

    async def _react(self, problem: str, context: str, max_steps: int) -> ReasoningResult:
        prompt = f"""Problem: {problem}

Context: {context}

Use the ReAct format:
Thought: [your reasoning]
Action: [tool/action to take]
Observation: [result]
... (repeat until solution)

Thought:"""

        response = await self._model_router.generate(prompt, temperature=0.3)

        steps = self._parse_react_steps(response.content)
        conclusion = self._extract_react_conclusion(response.content)
        overall_confidence = sum(s.confidence for s in steps) / len(steps) if steps else 0.5

        return ReasoningResult(
            conclusion=conclusion,
            steps=steps,
            overall_confidence=overall_confidence,
            strategy_used=ReasoningStrategy.REACT,
            metadata={"raw_response": response.content},
        )

    async def _self_consistency(self, problem: str, context: str) -> ReasoningResult:
        results = []
        for _ in range(3):
            result = await self._chain_of_thought(problem, context, max_steps=5)
            results.append(result)

        best = max(results, key=lambda r: r.overall_confidence)
        if best.metadata is not None:
            best.metadata["all_attempts"] = len(results)
        return best

    def _parse_cot_steps(self, text: str) -> list[ReasoningStep]:
        steps = []
        lines = text.split("\n")
        current_step = ""
        step_num = 0

        for line in lines:
            if line.strip().startswith(("Step", "step", "Thought:", "thought:")):
                if current_step:
                    steps.append(ReasoningStep(
                        step_type="cot",
                        content=current_step.strip(),
                        confidence=0.7,
                    ))
                current_step = line
                step_num += 1
            else:
                current_step += "\n" + line

        if current_step:
            steps.append(ReasoningStep(
                step_type="cot",
                content=current_step.strip(),
                confidence=0.7,
            ))

        return steps

    def _parse_react_steps(self, text: str) -> list[ReasoningStep]:
        steps = []
        for line in text.split("\n"):
            line = line.strip()
            if line.startswith(("Thought:", "Action:", "Observation:")):
                step_type = line.split(":")[0].lower()
                content = line[len(step_type)+1:].strip()
                steps.append(ReasoningStep(
                    step_type=step_type,
                    content=content,
                    confidence=0.7,
                ))
        return steps

    def _extract_react_conclusion(self, text: str) -> str:
        lines = text.split("\n")
        for line in reversed(lines):
            if line.strip().startswith(("Answer:", "Conclusion:", "Final:")):
                return line.split(":", 1)[1].strip()
        return "No clear conclusion extracted"
