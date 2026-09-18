"""Built-in tools for JAMES agents"""
import asyncio
import ast
import json
import os
import subprocess
import sys
import time
import platform
from pathlib import Path
from typing import Any, Dict

from james_runtime.tools.base import Tool, ToolResult


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

    ALLOWED_NAMES = {"abs": abs, "round": round, "min": min, "max": max, "sum": sum,
                     "pow": pow, "len": len, "int": int, "float": float, "pi": __import__('math').pi,
                     "e": __import__('math').e, "sqrt": __import__('math').sqrt,
                     "log": __import__('math').log, "sin": __import__('math').sin,
                     "cos": __import__('math').cos, "tan": __import__('math').tan,
                     "ceil": __import__('math').ceil, "floor": __import__('math').floor}

    async def _run(self, expression: str = "", **kwargs) -> ToolResult:
        try:
            tree = ast.parse(expression, mode='eval')
            code = compile(tree, "<math>", "eval")
            result = eval(code, {"__builtins__": {}}, self.ALLOWED_NAMES)
            return ToolResult(success=True, output=str(result))
        except Exception as e:
            return ToolResult(success=False, output="", error=f"Math error: {e}")


class PythonExec(Tool):
    name = "python_exec"
    description = (
        "Execute Python code in a sandboxed subprocess. "
        "Returns stdout+stderr. Timeout 10s. Use print() for output."
    )
    parameters = {
        "code": {"type": "string", "description": "Python code to execute (use print() for output)"},
    }

    WORKSPACE = Path(r"S:\Temp\opencode\james_workspace")

    async def _run(self, code: str = "", **kwargs) -> ToolResult:
        self.WORKSPACE.mkdir(parents=True, exist_ok=True)
        try:
            proc = await asyncio.create_subprocess_exec(
                sys.executable, "-u", "-c", code,
                stdout=asyncio.subprocess.PIPE,
                stderr=asyncio.subprocess.PIPE,
                cwd=str(self.WORKSPACE),
            )
            stdout, stderr = await asyncio.wait_for(proc.communicate(), timeout=10.0)
            out = (stdout or b"").decode(errors="replace").strip()
            err = (stderr or b"").decode(errors="replace").strip()
            output = out + ("\n--- STDERR ---\n" + err if err else "")
            return ToolResult(success=proc.returncode == 0, output=output or "(no output)")
        except asyncio.TimeoutError:
            try:
                proc.kill()
            except Exception:
                pass
            return ToolResult(success=False, output="", error="Execution timed out (10s)")
        except Exception as e:
            return ToolResult(success=False, output="", error=str(e))


class FileReader(Tool):
    name = "file_read"
    description = "Read the contents of a file. Max 8000 chars."
    parameters = {
        "path": {"type": "string", "description": "Absolute path to the file"},
    }

    async def _run(self, path: str = "", **kwargs) -> ToolResult:
        p = Path(path)
        if not p.exists():
            return ToolResult(success=False, output="", error="File not found")
        try:
            content = p.read_text(encoding="utf-8", errors="replace")[:8000]
            return ToolResult(success=True, output=content)
        except Exception as e:
            return ToolResult(success=False, output="", error=str(e))


class FileWriter(Tool):
    name = "file_write"
    description = "Write content to a file. Overwrites existing file."
    parameters = {
        "path": {"type": "string", "description": "Absolute path"},
        "content": {"type": "string", "description": "Content to write"},
    }

    async def _run(self, path: str = "", content: str = "", **kwargs) -> ToolResult:
        try:
            Path(path).parent.mkdir(parents=True, exist_ok=True)
            Path(path).write_text(content, encoding="utf-8")
            return ToolResult(success=True, output=f"Written {len(content)} bytes to {path}")
        except Exception as e:
            return ToolResult(success=False, output="", error=str(e))


class FileList(Tool):
    name = "file_list"
    description = "List files in a directory. Returns file names and sizes."
    parameters = {
        "path": {"type": "string", "description": "Directory path to list"},
    }

    async def _run(self, path: str = ".", **kwargs) -> ToolResult:
        p = Path(path)
        if not p.is_dir():
            return ToolResult(success=False, output="", error="Not a directory")
        try:
            items = []
            for item in sorted(p.iterdir())[:50]:
                kind = "d" if item.is_dir() else "f"
                size = item.stat().st_size if item.is_file() else 0
                items.append(f"[{kind}] {item.name} ({size:,} bytes)" if kind == "f" else f"[d] {item.name}/")
            return ToolResult(success=True, output="\n".join(items) or "(empty)")
        except Exception as e:
            return ToolResult(success=False, output="", error=str(e))


class HttpGet(Tool):
    name = "http_get"
    description = "Fetch a URL and return the first 6000 characters of the response body."
    parameters = {
        "url": {"type": "string", "description": "URL to fetch"},
    }

    async def _run(self, url: str = "", **kwargs) -> ToolResult:
        try:
            import httpx
            async with httpx.AsyncClient(timeout=15, follow_redirects=True) as client:
                resp = await client.get(url)
                text = resp.text[:6000]
                return ToolResult(
                    success=resp.is_success,
                    output=f"HTTP {resp.status_code}\n\n{text}",
                    metadata={"status_code": resp.status_code},
                )
        except Exception as e:
            return ToolResult(success=False, output="", error=str(e))


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
        except Exception as e:
            return ToolResult(success=False, output="", error=str(e))


def default_tools() -> list:
    """All built-in tools"""
    return [
        GetTime(), MathEval(), PythonExec(), Calculator(),
        FileReader(), FileWriter(), FileList(), HttpGet(),
    ]