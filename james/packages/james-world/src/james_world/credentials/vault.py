"""Credential Vault - encrypted credential storage for JAMES World Access"""

import base64
import json
from pathlib import Path
from typing import cast

import structlog
from cryptography.fernet import Fernet, InvalidToken
from cryptography.hazmat.primitives import hashes
from cryptography.hazmat.primitives.kdf.pbkdf2 import PBKDF2HMAC

logger = structlog.get_logger()


class CredentialVault:
    """Encrypt credentials per-capability. LLM never sees secrets directly."""

    def __init__(self, vault_path: str = "~/.james/credentials/vault.key"):
        self._key_path = Path(vault_path).expanduser()
        self._data_path = self._key_path.with_suffix(".json")
        self._fernet: Fernet | None = None

    def initialize(self, passphrase: str | None = None) -> None:
        self._key_path.parent.mkdir(parents=True, exist_ok=True)
        if self._key_path.exists():
            key = self._key_path.read_bytes()
        else:
            if passphrase:
                salt = b"james-vault-salt"
                kdf = PBKDF2HMAC(algorithm=hashes.SHA256(), length=32, salt=salt, iterations=100_000)
                key = base64.urlsafe_b64encode(kdf.derive(passphrase.encode()))
            else:
                key = Fernet.generate_key()
            self._key_path.write_bytes(key)
        self._fernet = Fernet(key)

    def _ensure_initialized(self) -> Fernet:
        if self._fernet is None:
            self.initialize()
        assert self._fernet is not None
        return self._fernet

    def store(self, capability: str, credential: dict[str, str]) -> None:
        fernet = self._ensure_initialized()
        data = self._load_data()
        encrypted = fernet.encrypt(json.dumps(credential).encode())
        data[capability] = base64.b64encode(encrypted).decode()
        self._save_data(data)
        logger.info("Credential stored", capability=capability, encrypted=True)

    def get(self, capability: str) -> dict[str, str] | None:
        fernet = self._ensure_initialized()
        data = self._load_data()
        enc = data.get(capability)
        if not enc:
            return None
        try:
            decrypted = fernet.decrypt(base64.b64decode(enc.encode()))
            return cast(dict[str, str], json.loads(decrypted.decode()))
        except InvalidToken:
            logger.error("Credential decryption failed", capability=capability)
            return None

    def set(self, capability: str, **kwargs: str) -> None:
        self.store(capability, kwargs)

    def delete(self, capability: str) -> bool:
        self._ensure_initialized()
        data = self._load_data()
        if capability in data:
            del data[capability]
            self._save_data(data)
            return True
        return False

    def has(self, capability: str) -> bool:
        self._ensure_initialized()
        return capability in self._load_data()

    def list_capabilities(self) -> list[str]:
        self._ensure_initialized()
        return list(self._load_data().keys())

    def _load_data(self) -> dict[str, str]:
        if self._data_path.exists():
            return cast(dict[str, str], json.loads(self._data_path.read_text()))
        return {}

    def _save_data(self, data: dict[str, str]) -> None:
        self._data_path.write_text(json.dumps(data, indent=2))
