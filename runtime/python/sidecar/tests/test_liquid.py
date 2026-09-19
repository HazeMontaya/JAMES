from james_runtime.steering.liquid import liquid_race

async def test_liquid_race_material_upgrades():
    async def generate(model):
        return {"choices": [{"message": {"content": {"a": "short", "b": "A much longer answer with substantially more unique words and useful structure."}[model]}}]}
    updates = []
    result = await liquid_race(["a", "b"], generate, min_delta=1, on_update=updates.append)
    assert result.winner is not None
    assert result.winner.model == "b"
    assert len(updates) >= 1