"""Cognitive State Machine for JAMES"""

import contextlib
from collections.abc import Awaitable, Callable
from dataclasses import dataclass, field
from datetime import UTC, datetime
from enum import Enum
from typing import Any

import anyio


class CognitiveState(Enum):
    IDLE = "idle"
    LISTENING = "listening"
    UNDERSTANDING = "understanding"
    PLANNING = "planning"
    SEARCHING = "searching"
    REASONING = "reasoning"
    EXECUTING = "executing"
    WAITING = "waiting"
    VERIFYING = "verifying"
    LEARNING = "learning"
    SUCCESS = "success"
    WARNING = "warning"
    ERROR = "error"
    RECOVERING = "recovering"
    SLEEP = "sleep"


TRANSITIONS: dict[CognitiveState, set[CognitiveState]] = {
    CognitiveState.IDLE: {CognitiveState.LISTENING, CognitiveState.SLEEP},
    CognitiveState.LISTENING: {CognitiveState.UNDERSTANDING, CognitiveState.IDLE},
    CognitiveState.UNDERSTANDING: {CognitiveState.PLANNING, CognitiveState.IDLE, CognitiveState.ERROR},
    CognitiveState.PLANNING: {CognitiveState.SEARCHING, CognitiveState.REASONING, CognitiveState.EXECUTING, CognitiveState.ERROR},
    CognitiveState.SEARCHING: {CognitiveState.REASONING, CognitiveState.PLANNING, CognitiveState.ERROR},
    CognitiveState.REASONING: {CognitiveState.EXECUTING, CognitiveState.PLANNING, CognitiveState.VERIFYING, CognitiveState.ERROR},
    CognitiveState.EXECUTING: {CognitiveState.WAITING, CognitiveState.VERIFYING, CognitiveState.ERROR},
    CognitiveState.WAITING: {CognitiveState.EXECUTING, CognitiveState.VERIFYING, CognitiveState.ERROR},
    CognitiveState.VERIFYING: {CognitiveState.LEARNING, CognitiveState.EXECUTING, CognitiveState.SUCCESS, CognitiveState.WARNING, CognitiveState.ERROR},
    CognitiveState.LEARNING: {CognitiveState.SUCCESS, CognitiveState.IDLE},
    CognitiveState.SUCCESS: {CognitiveState.IDLE, CognitiveState.LISTENING},
    CognitiveState.WARNING: {CognitiveState.IDLE, CognitiveState.RECOVERING},
    CognitiveState.ERROR: {CognitiveState.RECOVERING, CognitiveState.IDLE},
    CognitiveState.RECOVERING: {CognitiveState.IDLE, CognitiveState.PLANNING},
    CognitiveState.SLEEP: {CognitiveState.IDLE},
}


@dataclass
class StateTransition:
    from_state: CognitiveState
    to_state: CognitiveState
    timestamp: datetime = field(default_factory=lambda: datetime.now(UTC))
    metadata: dict[str, Any] = field(default_factory=dict)


class CognitiveStateMachine:
    def __init__(self, initial_state: CognitiveState = CognitiveState.IDLE):
        self._state = initial_state
        self._history: list[StateTransition] = []
        self._listeners: list[Callable[[CognitiveState, CognitiveState], Awaitable[None]]] = []
        self._lock = anyio.Lock()

    @property
    def state(self) -> CognitiveState:
        return self._state

    @property
    def history(self) -> list[StateTransition]:
        return self._history.copy()

    def add_listener(self, listener: Callable[[CognitiveState, CognitiveState], Awaitable[None]]) -> None:
        self._listeners.append(listener)

    async def transition(self, new_state: CognitiveState, metadata: dict[str, Any] | None = None) -> bool:
        async with self._lock:
            if new_state not in TRANSITIONS.get(self._state, set()):
                return False

            old_state = self._state
            transition = StateTransition(
                from_state=old_state,
                to_state=new_state,
                metadata=metadata or {}
            )
            self._state = new_state
            self._history.append(transition)

            for listener in self._listeners:
                with contextlib.suppress(Exception):
                    await listener(old_state, new_state)

            return True

    def can_transition(self, new_state: CognitiveState) -> bool:
        return new_state in TRANSITIONS.get(self._state, set())

    def get_valid_transitions(self) -> set[CognitiveState]:
        return TRANSITIONS.get(self._state, set()).copy()

    def reset(self) -> None:
        self._state = CognitiveState.IDLE
        self._history.clear()
