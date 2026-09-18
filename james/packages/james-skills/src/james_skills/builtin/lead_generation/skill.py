"""Lead Generation skill - Python entry point (deterministic fallback)"""

from typing import Any

CHANNELS = ["LinkedIn", "Cold Email", "Web Search", "Referrals", "Industry Events"]

TEMPLATES = """1. LinkedIn: "I noticed {offer} aligns with how you scale. Would a 20-min walkthrough help?"
2. Cold Email: "Subject: Ideas for {offer}\n\nHi, we help teams like yours with {offer}. Open to a quick chat?"
3. Referral: "Most of our best fits come from referrals - know anyone trialing {offer}?\"""" + " "


async def execute(inputs: dict[str, Any]) -> dict[str, Any]:
    offer = inputs.get("offer", inputs.get("topic", inputs.get("instructions", "unknown")))
    ctx = inputs.get("context")

    web_lines: list[str] = []
    world = getattr(ctx, "world", None) if ctx is not None else None
    if world is not None and hasattr(world, "web_search"):
        try:
            result = await world.web_search(query=f"lead generation {offer}")
            content = str(result.get("content", ""))[:200].replace("\n", " ").strip()
            if content:
                web_lines.append(f"- {content[:180]}")
        except Exception:
            pass

    lines = [
        f"# Lead Generation: {offer}",
        "",
        "## Ideal Customer Profile",
        f"Businesses actively seeking or scaling '{offer}' - founders, growth and revenue owners.",
        "",
        "## Lead Sources / Channels",
    ]
    for i, ch in enumerate(CHANNELS, 1):
        lines.append(f"{i}. {ch}")
    lines.append("")
    lines.append("## Outreach Templates")
    lines.append(TEMPLATES.format(offer=offer))
    if web_lines:
        lines.append("")
        lines.append("## Live Research")
        lines.extend(web_lines)

    return {"text": "\n".join(lines), "model": "local_fallback", "fallback": True, "offer": offer}