"""User Memory for JAMES - Profile, Preferences, Context"""

import json
from pathlib import Path
from typing import Any

import aiofiles
import structlog

from ..models import MemoryEntry, RetrievalQuery, RetrievalResult
from ..vault import VaultManager

logger = structlog.get_logger()


class UserMemory:
    def __init__(self, vault_path: str = "~/.james/vault"):
        self._vault = VaultManager(vault_path)
        self._profile_path = Path(vault_path).expanduser() / "profile" / "user.md"
        self._preferences_path = Path(vault_path).expanduser() / "profile" / "preferences.json"
        self._context_path = Path(vault_path).expanduser() / "profile" / "context.json"
        self._preferences: dict[str, Any] = {}
        self._context: dict[str, Any] = {}

    async def initialize(self) -> None:
        await self._vault.initialize()
        await self._load_profile()
        await self._load_preferences()
        await self._load_context()

    async def _load_profile(self) -> None:
        if self._profile_path.exists():
            self._profile = await self._vault.read_file(self._profile_path)
        else:
            self._profile = ""

    async def _load_preferences(self) -> None:
        if self._preferences_path.exists():
            async with aiofiles.open(self._preferences_path) as f:
                self._preferences = json.loads(await f.read())
        else:
            self._preferences = {}

    async def _load_context(self) -> None:
        if self._context_path.exists():
            async with aiofiles.open(self._context_path) as f:
                self._context = json.loads(await f.read())
        else:
            self._context = {}

    async def save_preferences(self) -> None:
        self._preferences_path.parent.mkdir(parents=True, exist_ok=True)
        async with aiofiles.open(self._preferences_path, "w") as f:
            await f.write(json.dumps(self._preferences, indent=2))

    async def save_context(self) -> None:
        self._context_path.parent.mkdir(parents=True, exist_ok=True)
        async with aiofiles.open(self._context_path, "w") as f:
            await f.write(json.dumps(self._context, indent=2))

    async def get_profile(self) -> str:
        return self._profile

    async def update_profile(self, profile: str) -> None:
        self._profile = profile
        await self._vault.write_file("profile/user.md", profile)

    async def get_preference(self, key: str, default: Any = None) -> Any:
        return self._preferences.get(key, default)

    async def set_preference(self, key: str, value: Any) -> None:
        self._preferences[key] = value
        await self.save_preferences()

    async def get_context(self, key: str, default: Any = None) -> Any:
        return self._context.get(key, default)

    async def set_context(self, key: str, value: Any) -> None:
        self._context[key] = value
        await self.save_context()

    async def update_context(self, updates: dict[str, Any]) -> None:
        self._context.update(updates)
        await self.save_context()

    async def store(self, entry: MemoryEntry) -> str:
        return entry.id

    async def retrieve(self, entry_id: str) -> MemoryEntry | None:
        return None

    async def query(self, query: RetrievalQuery) -> RetrievalResult:
        return RetrievalResult(entries=[], total_found=0, query=query, retrieval_time_ms=0)

    async def close(self) -> None:
        await self.save_preferences()
        await self.save_context()
