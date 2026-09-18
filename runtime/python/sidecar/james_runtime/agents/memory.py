"""JAMES Persistent Memory Store"""
import json
import time
from pathlib import Path
from typing import Any, Dict, List, Optional


class MemoryStore:
    """Simple persistent memory backed by JSONL."""

    def __init__(self, path: Optional[Path] = None):
        self._path = Path(path) if path else Path(".james/memory/memory.jsonl")
        self._path.parent.mkdir(parents=True, exist_ok=True)
        self._memories: List[Dict[str, Any]] = []
        self._load()

    def _load(self) -> None:
        if not self._path.exists():
            return
        for line in self._path.read_text(encoding="utf-8", errors="replace").splitlines():
            line = line.strip()
            if line:
                try:
                    self._memories.append(json.loads(line))
                except json.JSONDecodeError:
                    continue

    def remember(self, content: str, namespace: str = "default", key: Optional[str] = None) -> Dict[str, Any]:
        memory = {
            "content": content,
            "namespace": namespace,
            "key": key or str(time.time_ns()),
            "timestamp": int(time.time()),
        }
        self._memories.append(memory)
        with self._path.open("a", encoding="utf-8") as f:
            f.write(json.dumps(memory, ensure_ascii=False) + "\n")
        return memory

    def recall(self, query: Optional[str] = None, namespace: Optional[str] = None, limit: int = 10) -> List[Dict[str, Any]]:
        """Return memories, optionally filtered by keyword query and namespace."""
        query_lower = (query or "").lower()
        results = []
        for m in reversed(self._memories):
            if namespace and m.get("namespace") != namespace:
                continue
            if query_lower:
                content = m.get("content", "").lower()
                key = (m.get("key") or "").lower()
                if query_lower not in content and query_lower not in key:
                    continue
            results.append(m)
            if len(results) >= limit:
                break
        return results

    def recent(self, limit: int = 20) -> List[Dict[str, Any]]:
        return list(reversed(self._memories[-limit:]))

    def count(self) -> int:
        return len(self._memories)


class MemoryTools:
    """Memory exposed as tools for agents"""

    def __init__(self, store: MemoryStore):
        self.store = store

    def register(self, registry) -> None:
        from james_runtime.tools.base import Tool, ToolResult
        store = self.store

        class MemoryRemember(Tool):
            name = "memory_remember"
            description = "Store a fact in long-term memory for future sessions."
            parameters = {
                "content": {"type": "string", "description": "Fact to remember"},
                "namespace": {"type": "string", "description": "Memory namespace (default: default)"},
            }

            def __init__(self, store):
                self._store = store

            async def _run(self, content: str = "", namespace: str = "default", **kwargs) -> ToolResult:
                self._store.remember(content, namespace)
                return ToolResult(success=True, output=f"Remembered: {content[:80]}")

        class MemoryRecall(Tool):
            name = "memory_recall"
            description = "Recall remembered facts matching a keyword."
            parameters = {
                "query": {"type": "string", "description": "Keyword to search memories"},
                "namespace": {"type": "string", "description": "Namespace (optional)"},
            }

            def __init__(self, store):
                self._store = store

            async def _run(self, query: str = "", namespace: str = "", **kwargs) -> ToolResult:
                ns = namespace or None
                found = self._store.recall(query or None, ns, limit=8)
                if not found:
                    return ToolResult(success=True, output="No memories found.")
                lines = [f"- [{m['timestamp']}] {m['content']}" for m in found]
                return ToolResult(success=True, output="\n".join(lines))

        registry.register(MemoryRemember(store))
        registry.register(MemoryRecall(store))