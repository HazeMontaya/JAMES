"""Built-in tools for JAMES agents.

The built-ins are intentionally conservative: filesystem access is confined to the
JAMES workspace, outbound HTTP blocks private-network destinations by default, and
Python execution runs in an isolated interpreter environment with bounded time/output.
Higher-risk capabilities must still be gated by the Rust capability broker.
"""
import asyncio
import ast
import ipaddress
import os
import socket
import sys
from pathlib import Path
from typing import Any
from urllib.parse import urljoin, urlsplit

from james_runtime.tools.base import Tool, ToolResult


DEFAULT_WORKSPACE = Path(r"S:\Temp\opencode\james_workspace")
MAX_TOOL_OUTPUT = 16_000
MAX_FILE_READ = 8_000
HTTP_TIMEOUT_SECONDS = 15.0
HTTP_MAX_REDIRECTS = 3


def workspace_root() -> Path:
    """Return the configured JAMES workspace root."""
    return Path(os.environ.get("JAMES_WORKSPACE_ROOT", str(DEFAULT_WORKSPACE))).expanduser().resolve()


def safe_workspace_path(raw_path: str, *, must_exist: bool = False) -> Path:
    """Resolve a path and reject traversal/symlink escapes from the workspace."""
    if not raw_path or not isinstance(raw_path, str):
        raise ValueError("path is required")

    root = workspace_root()
    candidate = Path(raw_path)
    if not candidate.is_absolute():
        candidate = root / candidate

    # realpath resolves existing symlinks and normalizes .. components.
    resolved = Path(os.path.realpath(candidate))
    try:
        resolved.relative_to(root)
    except ValueError as exc:
        raise PermissionError("path is outside the JAMES workspace") from exc

    if must_exist and not resolved.exists():
        raise FileNotFoundError("File not found")
    return resolved


def _bounded_output(value: str, limit: int = MAX_TOOL_OUTPUT) -> str:
    if len(value) <= limit:
        return value
    return value[:limit] + f"\n--- OUTPUT TRUNCATED ({len(value) - limit} chars) ---"


class GetTime(Tool):
    name = "get_current_time"
    description = "Get the current date and time in ISO format."
    parameters = {}

    async def _run(self, **kwargs) -> ToolResult:
        from datetime import datetime, timezone
        return ToolResult(
            success=True,
            output=datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        )


class MathEval(Tool):
    name = "math_eval"
    description = "Safely evaluate a mathematical expression (Python math syntax)."
    parameters = {
        "expression": {"type": "string", "description": "Math expression to evaluate, e.g. '2**10 + 5'"},
    }

    ALLOWED_NAMES = {
        "abs": abs, "round": round, "min": min, "max": max, "sum": sum,
        "pow": pow, "len": len, "int": int, "float": float,
        "pi": __import__("math").pi, "e": __import__("math").e,
        "sqrt": __import__("math").sqrt, "log": __import__("math").log,
        "sin": __import__("math").sin, "cos": __import__("math").cos,
        "tan": __import__("math").tan, "ceil": __import__("math").ceil,
        "floor": __import__("math").floor,
    }

    async def _run(self, expression: str = "", **kwargs) -> ToolResult:
        try:
            tree = ast.parse(expression, mode="eval")
            code = compile(tree, "<math>", "eval")
            result = eval(code, {"__builtins__": {}}, self.ALLOWED_NAMES)
            return ToolResult(success=True, output=str(result))
        except Exception as exc:
            return ToolResult(success=False, output="", error=f"Math error: {exc}")


class PythonExec(Tool):
    name = "python_exec"
    description = (
        "Execute Python code in an isolated subprocess workspace. "
        "Timeout 10s; stdout/stderr are bounded. The capability remains broker-gated."
    )
    parameters = {
        "code": {"type": "string", "description": "Python code to execute (use print() for output)"},
    }

    TIMEOUT_SECONDS = 10.0
    MAX_OUTPUT = 16_000

    async def _run(self, code: str = "", **kwargs) -> ToolResult:
        root = workspace_root()
        root.mkdir(parents=True, exist_ok=True)

        # Deliberately do not inherit the parent environment wholesale: this keeps
        # API keys, tokens and unrelated runtime secrets out of agent subprocesses.
        env = {
            key: os.environ[key]
            for key in ("PATH", "SYSTEMROOT", "WINDIR", "TEMP", "TMP", "HOME", "USERPROFILE")
            if key in os.environ
        }
        env["PYTHONNOUSERSITE"] = "1"

        try:
            proc = await asyncio.create_subprocess_exec(
                sys.executable,
                "-I",
                "-S",
                "-u",
                "-c",
                code,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
                cwd=str(root),
                env=env,
            )
            stdout, stderr = await asyncio.wait_for(
                proc.communicate(),
                timeout=self.TIMEOUT_SECONDS,
            )
            out = (stdout or b"").decode(errors="replace").strip()
            err = (stderr or b"").decode(errors="replace").strip()
            output = out + ("\n--- STDERR ---\n" + err if err else "")
            output = _bounded_output(output, self.MAX_OUTPUT)
            return ToolResult(
                success=proc.returncode == 0,
                output=output or "(no output)",
                metadata={"workspace": str(root), "returncode": proc.returncode},
            )
        except asyncio.TimeoutError:
            try:
                proc.kill()
            except Exception:
                pass
            return ToolResult(success=False, output="", error="Execution timed out (10s)")
        except Exception as exc:
            return ToolResult(success=False, output="", error=str(exc))


class FileReader(Tool):
    name = "file_read"
    description = "Read a UTF-8 file inside the JAMES workspace. Max 8000 chars."
    parameters = {
        "path": {"type": "string", "description": "Workspace-relative or absolute path inside the JAMES workspace"},
    }

    async def _run(self, path: str = "", **kwargs) -> ToolResult:
        try:
            p = safe_workspace_path(path, must_exist=True)
            if not p.is_file():
                return ToolResult(success=False, output="", error="Not a file")
            content = p.read_text(encoding="utf-8", errors="replace")[:MAX_FILE_READ]
            return ToolResult(success=True, output=content, metadata={"path": str(p)})
        except Exception as exc:
            return ToolResult(success=False, output="", error=str(exc))


class FileWriter(Tool):
    name = "file_write"
    description = "Write UTF-8 content inside the JAMES workspace. Overwrites existing files."
    parameters = {
        "path": {"type": "string", "description": "Workspace-relative or absolute path inside the JAMES workspace"},
        "content": {"type": "string", "description": "Content to write"},
    }

    async def _run(self, path: str = "", content: str = "", **kwargs) -> ToolResult:
        try:
            p = safe_workspace_path(path)
            # Resolve the parent separately so a symlinked directory cannot redirect
            # a write outside the workspace.
            p.parent.mkdir(parents=True, exist_ok=True)
            p = safe_workspace_path(str(p))
            p.write_text(content, encoding="utf-8")
            return ToolResult(success=True, output=f"Written {len(content)} bytes to {p}")
        except Exception as exc:
            return ToolResult(success=False, output="", error=str(exc))


class FileList(Tool):
    name = "file_list"
    description = "List files/directories inside the JAMES workspace."
    parameters = {
        "path": {"type": "string", "description": "Workspace-relative or absolute directory inside the JAMES workspace"},
    }

    async def _run(self, path: str = ".", **kwargs) -> ToolResult:
        try:
            p = safe_workspace_path(path, must_exist=True)
            if not p.is_dir():
                return ToolResult(success=False, output="", error="Not a directory")
            items = []
            for item in sorted(p.iterdir(), key=lambda x: x.name.lower())[:50]:
                resolved = Path(os.path.realpath(item))
                try:
                    resolved.relative_to(workspace_root())
                except ValueError:
                    continue
                kind = "d" if item.is_dir() else "f"
                size = item.stat().st_size if item.is_file() else 0
                items.append(
                    f"[{kind}] {item.name} ({size:,} bytes)"
                    if kind == "f"
                    else f"[d] {item.name}/"
                )
            return ToolResult(success=True, output="\n".join(items) or "(empty)")
        except Exception as exc:
            return ToolResult(success=False, output="", error=str(exc))


def _public_http_url(url: str) -> str:
    parsed = urlsplit(url)
    if parsed.scheme.lower() not in {"http", "https"}:
        raise ValueError("only http:// and https:// URLs are allowed")
    if not parsed.hostname:
        raise ValueError("URL hostname is required")
    if parsed.username or parsed.password:
        raise ValueError("URL credentials are not allowed")

    host = parsed.hostname.rstrip(".")
    try:
        ip = ipaddress.ip_address(host)
        addresses = [ip]
    except ValueError:
        try:
            infos = socket.getaddrinfo(host, parsed.port or (443 if parsed.scheme == "https" else 80), type=socket.SOCK_STREAM)
        except OSError as exc:
            raise ValueError(f"hostname resolution failed: {exc}") from exc
        addresses = [ipaddress.ip_address(info[4][0]) for info in infos]

    blocked = [
        address for address in addresses
        if (
            address.is_private
            or address.is_loopback
            or address.is_link_local
            or address.is_multicast
            or address.is_reserved
            or address.is_unspecified
        )
    ]
    if blocked:
        raise PermissionError("HTTP destination resolves to a private or otherwise non-public network")
    return url


class HttpGet(Tool):
    name = "http_get"
    description = "Fetch a public HTTP(S) URL; private/local network destinations are blocked."
    parameters = {
        "url": {"type": "string", "description": "Public HTTP(S) URL to fetch"},
    }

    async def _run(self, url: str = "", **kwargs) -> ToolResult:
        try:
            import httpx

            current_url = _public_http_url(url)
            async with httpx.AsyncClient(
                timeout=HTTP_TIMEOUT_SECONDS,
                follow_redirects=False,
                trust_env=False,
            ) as client:
                for _ in range(HTTP_MAX_REDIRECTS + 1):
                    resp = await client.get(current_url)
                    if resp.is_redirect:
                        location = resp.headers.get("location")
                        if not location:
                            return ToolResult(success=False, output="", error="Redirect without Location header")
                        current_url = _public_http_url(urljoin(current_url, location))
                        continue

                    body = resp.text[:6000]
                    return ToolResult(
                        success=resp.is_success,
                        output=f"HTTP {resp.status_code}\n\n{body}",
                        metadata={"status_code": resp.status_code, "url": current_url},
                    )
                return ToolResult(success=False, output="", error="Too many redirects")
        except Exception as exc:
            return ToolResult(success=False, output="", error=str(exc))


class Calculator(Tool):
    name = "calculator"
    description = "Perform basic arithmetic: add, subtract, multiply, divide."
    parameters = {
        "operation": {"type": "string", "description": "One of: add, subtract, multiply, divide"},
        "a": {"type": "string", "description": "First number"},
        "b": {"type": "string", "description": "Second number"},
    }

    OPS = {
        "add": lambda a, b: a + b,
        "subtract": lambda a, b: a - b,
        "multiply": lambda a, b: a * b,
        "divide": lambda a, b: a / b if b != 0 else "Error: division by zero",
    }

    async def _run(self, operation: str = "", a: str = "0", b: str = "0", **kwargs) -> ToolResult:
        op = self.OPS.get(operation.lower())
        if not op:
            return ToolResult(success=False, output="", error=f"Unknown operation: {operation}")
        try:
            result = op(float(a), float(b))
            return ToolResult(success=True, output=str(result))
        except Exception as exc:
            return ToolResult(success=False, output="", error=str(exc))


def default_tools() -> list:
    """All built-in tools."""
    return [
        GetTime(), MathEval(), PythonExec(), Calculator(),
        FileReader(), FileWriter(), FileList(), HttpGet(),
    ]
