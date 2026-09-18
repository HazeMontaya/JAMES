"""Memory Models for JAMES"""

from dataclasses import dataclass, field
from datetime import UTC, datetime
from enum import Enum
from typing import Any
from uuid import uuid4


class MemoryType(Enum):
    EPISODIC = "episodic"
    SEMANTIC = "semantic"
    PROCEDURAL = "procedural"
    BUSINESS = "business"
    USER = "user"


@dataclass
class MemoryEntry:
    id: str = field(default_factory=lambda: str(uuid4()))
    memory_type: MemoryType = MemoryType.SEMANTIC
    content: str = ""
    metadata: dict[str, Any] = field(default_factory=dict)
    tags: list[str] = field(default_factory=list)
    embedding: list[float] | None = None
    created_at: datetime = field(default_factory=lambda: datetime.now(UTC))
    updated_at: datetime = field(default_factory=lambda: datetime.now(UTC))
    access_count: int = 0
    last_accessed: datetime | None = None
    source: str = ""
    confidence: float = 1.0


@dataclass
class RetrievalQuery:
    query: str = ""
    memory_types: list[MemoryType] = field(default_factory=list)
    tags: list[str] = field(default_factory=list)
    filters: dict[str, Any] = field(default_factory=dict)
    limit: int = 10
    min_confidence: float = 0.0
    use_vector_search: bool = True
    context: dict[str, Any] = field(default_factory=dict)


@dataclass
class RetrievalResult:
    entries: list[MemoryEntry] = field(default_factory=list)
    total_found: int = 0
    query: RetrievalQuery | None = None
    retrieval_time_ms: float = 0.0
