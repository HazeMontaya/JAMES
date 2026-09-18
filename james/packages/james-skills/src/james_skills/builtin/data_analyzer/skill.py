"""Data Analyzer skill - Python entry point (deterministic fallback)"""

import contextlib
import csv
import io
import json
import statistics
from collections import Counter
from typing import Any


def _parse_rows(data: str) -> tuple[list[str], list[dict[str, Any]]]:
    stripped = data.strip()
    if not stripped:
        return [], []

    headers: list[str] = []
    rows: list[dict[str, Any]] = []

    try:
        parsed = json.loads(stripped)
        if isinstance(parsed, list) and parsed and isinstance(parsed[0], dict):
            headers = list(parsed[0].keys())
            rows = parsed
            return headers, rows
    except json.JSONDecodeError:
        pass

    try:
        reader = csv.DictReader(io.StringIO(stripped))
        sample: list[dict[str, Any]] = []
        for entry in reader:
            converted: dict[str, Any] = {}
            for k, v in entry.items():
                try:
                    converted[k] = float(v)
                except (TypeError, ValueError):
                    converted[k] = v
            sample.append(converted)
        if sample:
            valid: list[dict[str, Any]] = []
            for r in sample:
                if any(r.values()):
                    valid.append(r)
            if valid:
                headers = list(valid[0].keys())
                rows = valid
                return headers, rows
    except csv.Error:
        pass

    return [], []


def _as_float(v: Any) -> float | None:
    if isinstance(v, bool):
        return None
    try:
        return float(v)
    except (TypeError, ValueError):
        return None


def _summarize(headers: list[str], rows: list[dict[str, Any]]) -> list[str]:
    lines = [f"## Summary\n{len(rows)} records, {len(headers)} fields: {', '.join(headers)}", ""]

    numerical: dict[str, list[float]] = {}
    categorical: dict[str, Counter[str]] = {}
    for h in headers:
        raw_vals = [r.get(h) for r in rows]
        numeric_vals: list[float] = []
        for v in raw_vals:
            f = _as_float(v)
            if f is not None:
                numeric_vals.append(f)
        if numeric_vals:
            numerical[h] = numeric_vals
        else:
            cats = Counter(str(v) for v in raw_vals if v is not None and str(v).strip())
            if cats:
                categorical[h] = cats

    for h, vals in numerical.items():
        mean = sum(vals) / len(vals)
        low = min(vals)
        high = max(vals)
        line = f"- **{h}**: min={low:g}, max={high:g}, mean={mean:g}"
        if len(vals) > 1:
            with contextlib.suppress(statistics.StatisticsError):
                line += f", median={statistics.median(vals):g}"
        lines.append(line)

    for h, cats in categorical.items():
        top = cats.most_common(3)
        parts = ", ".join(f"{k} ({c})" for k, c in top)
        lines.append(f"- **{h}** (categorical): top values -> {parts}")

    return lines


async def execute(inputs: dict[str, Any]) -> dict[str, Any]:
    data = inputs.get("data", inputs.get("instructions", "")).strip()
    question = inputs.get("question", "").strip()

    headers, rows = _parse_rows(data)
    if not headers:
        return {
            "text": "## Data Analysis\nCould not parse the provided data as CSV or JSON.",
            "model": "local_fallback",
            "fallback": True,
            "success": False,
            "records": 0,
            "fields": 0,
        }

    lines = ["# Data Analysis Report", ""]
    if question:
        lines.append(f"## Question\n{question}")
        lines.append("")
    lines.extend(_summarize(headers, rows))
    lines.append("")
    lines.append("## Caveat")
    lines.append("Statistical summary only. Connect a model router for deeper pattern analysis.")

    return {
        "text": "\n".join(lines),
        "model": "local_fallback",
        "fallback": True,
        "success": True,
        "records": len(rows),
        "fields": len(headers),
        "insight_count": len([line for line in lines if line.startswith("- **")]),
    }