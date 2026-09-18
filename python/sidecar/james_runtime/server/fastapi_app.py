"""JAMES Runtime FastAPI Server"""
import asyncio
import logging
from contextlib import asynccontextmanager
from fastapi import FastAPI, HTTPException, Request
from fastapi.responses import StreamingResponse
from pydantic import BaseModel
from typing import Optional, List, Dict, Any

from james_runtime.config import get_settings
from james_runtime.core.runtime import JamesRuntime
from james_runtime.core.requests import CompletionRequest, StreamingRequest, RoutingRequest, RuntimeRequest

logger = logging.getLogger(__name__)

# Global runtime instance
runtime: Optional[JamesRuntime] = None


@asynccontextmanager
async def lifespan(app: FastAPI):
    global runtime
    settings = get_settings()
    runtime = JamesRuntime(settings)
    await runtime.initialize()
    logger.info(f"JAMES Runtime started on port {settings.http_port}")
    yield
    await runtime.shutdown()
    logger.info("JAMES Runtime shut down")


app = FastAPI(
    title="JAMES AI Runtime",
    description="Unified AI inference runtime with multi-engine routing",
    version="0.1.0",
    lifespan=lifespan,
)


# Request/Response models for OpenAPI
class ChatCompletionRequest(BaseModel):
    model: str
    messages: List[Dict[str, str]]
    temperature: float = 0.7
    max_tokens: Optional[int] = None
    stream: bool = False
    tools: Optional[List[Dict]] = None


class ChatCompletionResponse(BaseModel):
    id: str
    object: str = "chat.completion"
    created: int
    model: str
    choices: List[Dict]
    usage: Optional[Dict] = None


class ModelsResponse(BaseModel):
    object: str = "list"
    data: List[Dict]


class AgentRunRequest(BaseModel):
    input: str
    agent_id: str = "default"
    model: str = "llama-3.1-8b-instruct"
    max_steps: int = 6


class GoldenPathRequest(BaseModel):
    input: str
    agent_id: str = "default"
    model: str = "llama-3.1-8b-instruct"
    max_steps: int = 6


@app.get("/health")
async def health():
    """Health check endpoint"""
    if runtime is None:
        raise HTTPException(status_code=503, detail="Runtime not initialized")
    health = await runtime.check_health()
    return health


@app.get("/v1/models", response_model=ModelsResponse)
async def list_models():
    """List available models (OpenAI compatible)"""
    if runtime is None:
        raise HTTPException(status_code=503, detail="Runtime not initialized")
    models = runtime.list_models()
    return ModelsResponse(data=[
        {
            "id": m["id"],
            "object": "model",
            "created": 0,
            "owned_by": m["provider"],
        }
        for m in models
    ])


@app.post("/v1/chat/completions")
async def chat_completions(request: ChatCompletionRequest):
    """OpenAI-compatible chat completions endpoint"""
    if runtime is None:
        raise HTTPException(status_code=503, detail="Runtime not initialized")
    
    # Convert to internal request
    req = CompletionRequest(
        model=request.model,
        messages=[{"role": m["role"], "content": m["content"]} for m in request.messages],
        temperature=request.temperature,
        max_tokens=request.max_tokens,
        stream=request.stream,
        tools=request.tools,
    )
    
    if request.stream:
        return StreamingResponse(
            runtime.stream(req),
            media_type="text/event-stream",
        )
    else:
        response = await runtime.generate(req)
        return {
            "id": response.id,
            "object": "chat.completion",
            "created": response.created,
            "model": response.model,
            "choices": response.choices,
            "usage": response.usage,
        }


@app.post("/v1/route")
async def route_model(request: RoutingRequest):
    """Get routing decision for a task"""
    if runtime is None:
        raise HTTPException(status_code=503, detail="Runtime not initialized")
    return await runtime.route_model(request)


@app.post("/v1/runtime/select")
async def select_runtime(request: RuntimeRequest):
    """Get runtime selection for a model"""
    if runtime is None:
        raise HTTPException(status_code=503, detail="Runtime not initialized")
    return runtime.select_runtime(request)


@app.get("/v1/hardware")
async def get_hardware():
    """Get hardware profile"""
    if runtime is None:
        raise HTTPException(status_code=503, detail="Runtime not initialized")
    return runtime.get_hardware_profile()


@app.get("/v1/budget")
async def get_budget():
    """Get budget status"""
    if runtime is None:
        raise HTTPException(status_code=503, detail="Runtime not initialized")
    health = await runtime.check_health()
    return health.get("budget", {})


@app.post("/v1/budget/update")
async def update_budget(request: Request):
    """Update budget policy (admin)"""
    if runtime is None:
        raise HTTPException(status_code=503, detail="Runtime not initialized")
    # Would update budget policy
    return {"status": "ok"}


# Internal API for Rust Core
@app.post("/internal/generate")
async def internal_generate(request: CompletionRequest):
    """Internal endpoint for Rust Core"""
    if runtime is None:
        raise HTTPException(status_code=503, detail="Runtime not initialized")
    return await runtime.generate(request)


@app.post("/internal/stream")
async def internal_stream(request: StreamingRequest):
    """Internal streaming endpoint for Rust Core"""
    if runtime is None:
        raise HTTPException(status_code=503, detail="Runtime not initialized")
    return StreamingResponse(
        runtime.stream(request),
        media_type="text/event-stream",
    )


@app.post("/internal/route")
async def internal_route(request: RoutingRequest):
    """Internal routing endpoint"""
    if runtime is None:
        raise HTTPException(status_code=503, detail="Runtime not initialized")
    return await runtime.route_model(request)


@app.get("/internal/health")
async def internal_health():
    """Internal health check"""
    if runtime is None:
        raise HTTPException(status_code=503, detail="Runtime not initialized")
    return await runtime.check_health()


# ============ Agent & Workflow Endpoints ============

@app.post("/v1/agents/run")
async def agent_run(request: AgentRunRequest):
    """Run an agent on a query using tools and the reasoning loop."""
    if runtime is None:
        raise HTTPException(status_code=503, detail="Runtime not initialized")
    result = await runtime.agent_system.run(
        request.input, agent_id=request.agent_id, model=request.model,
        max_steps=request.max_steps,
    )
    return result.to_dict()


@app.post("/v1/workflows/golden-path")
async def golden_path(request: GoldenPathRequest):
    """Run the canonical Golden Path workflow end-to-end."""
    if runtime is None:
        raise HTTPException(status_code=503, detail="Runtime not initialized")
    from james_runtime.workflows.golden_path import GoldenPathWorkflow
    workflow = GoldenPathWorkflow(runtime, runtime.agent_system)
    result = await workflow.run(
        request.input, agent_id=request.agent_id, model=request.model,
        max_steps=request.max_steps,
    )
    return result.to_dict()


@app.get("/v1/agents")
async def list_agents():
    """List agent system status: tools and memory."""
    if runtime is None:
        raise HTTPException(status_code=503, detail="Runtime not initialized")
    return runtime.agent_system.status()


@app.get("/v1/tools")
async def list_tools():
    """List available tools with schemas."""
    if runtime is None:
        raise HTTPException(status_code=503, detail="Runtime not initialized")
    return {"tools": runtime.agent_system.tool_registry.schemas()}


if __name__ == "__main__":
    import uvicorn
    settings = get_settings()
    uvicorn.run(
        "james_runtime.server.fastapi_app:app",
        host="0.0.0.0",
        port=settings.http_port,
        log_level=settings.log_level.lower(),
    )