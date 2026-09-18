"""JAMES Workflows - orchestrated agent pipelines"""
from james_runtime.workflows.golden_path import (
    GoldenPathWorkflow,
    Workflow,
    WorkflowResult,
)

__all__ = ["GoldenPathWorkflow", "Workflow", "WorkflowResult"]