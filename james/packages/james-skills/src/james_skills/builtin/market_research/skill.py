"""Market Research skill - Python entry point"""

from typing import Any


async def execute(inputs: dict[str, Any]) -> dict[str, Any]:
    """Fallback entry point when no model router is available."""
    topic = inputs.get("topic", inputs.get("instructions", "unknown"))
    context = inputs.get("context", {})

    return {
        "text": f"""# Market Research: {topic}

## Demand
Signals of demand for '{topic}' need to be validated via live web research when a model router + world access is available.

## Competitors
Competitor landscape analysis requires live data.

## Opportunities
Opportunity assessment pending live research.

## Caveat
This is a structural fallback output. Run with a connected model router and world access for a real analysis.
""",
        "model": "local_fallback",
        "fallback": True,
        "topic": topic,
    }
