"""Email Drafter skill - Python entry point (deterministic fallback)"""

from typing import Any

GREETINGS = {
    "formal": "Dear {recipient},",
    "friendly": "Hi {recipient},",
    "concise": "{recipient},",
}

CLOSINGS = {
    "formal": "Kind regards,\n[Your Name]",
    "friendly": "Best,\n[Your Name]",
    "concise": "Thanks,\n[Your Name]",
}


async def execute(inputs: dict[str, Any]) -> dict[str, Any]:
    recipient = inputs.get("recipient", "").strip() or "there"
    objective = inputs.get("objective", inputs.get("instructions", "")).strip()
    tone = inputs.get("tone", "professional").strip().lower()

    greeting = GREETINGS.get(tone, GREETINGS["formal"]).format(recipient=recipient)
    closing = CLOSINGS.get(tone, CLOSINGS["formal"])

    if not objective:
        body = "I would like to discuss something briefly when you have a moment."
    else:
        body = f"I wanted to reach out regarding: {objective}. Please let me know if you would be open to a short conversation at your convenience."

    lines = [
        f"Subject: {objective[:80] if objective else 'Quick conversation'}",
        "",
        greeting,
        "",
        body,
        "",
        "I look forward to hearing from you.",
        "",
        closing,
    ]
    text = "\n".join(lines)

    return {
        "text": text,
        "email": text,
        "tone": tone,
        "model": "local_fallback",
        "fallback": True,
        "word_count": len(text.split()),
    }