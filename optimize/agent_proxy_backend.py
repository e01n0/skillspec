"""Agent Proxy backend — routes LLM calls through the hosting agent via stdout + FIFO.

Requests go to stdout (newline-delimited JSON). The Rust CLI reads them and
prints them for the agent.  Responses come back through a named pipe (FIFO)
that the agent writes to from a separate command.

Protocol:
  Request (→ stdout):  {"type":"llm_request", "id":"...", "role":"optimizer|target", ...}
  Response (← FIFO):   {"content":"...", "usage":{...}}
"""
from __future__ import annotations

import json
import os
import sys
import uuid
from typing import Any

from .common import TokenTracker, CompatAssistantMessage

tracker = TokenTracker()

_OPTIMIZER_DEPLOYMENT = "agent-proxy-optimizer"
_TARGET_DEPLOYMENT = "agent-proxy-target"
_REASONING_EFFORT: str | None = None
_PIPE_PATH: str | None = None


def set_pipe_path(path: str) -> None:
    global _PIPE_PATH
    _PIPE_PATH = path


def _read_response(request_id: str) -> dict:
    """Read a JSON response — from the FIFO if set, otherwise from stdin."""
    if _PIPE_PATH:
        with open(_PIPE_PATH, "r") as f:
            response_line = f.readline()
    else:
        response_line = sys.stdin.readline()

    if not response_line or not response_line.strip():
        raise RuntimeError(
            f"Agent proxy: no response received for request {request_id}. "
            f"Write a JSON response to: {_PIPE_PATH or 'stdin'}"
        )

    try:
        return json.loads(response_line.strip())
    except json.JSONDecodeError as e:
        raise RuntimeError(f"Agent proxy: invalid JSON response: {e}") from e


def _make_request(
    *,
    role: str,
    stage: str,
    messages: list[dict[str, Any]] | None = None,
    system: str | None = None,
    user: str | None = None,
    max_completion_tokens: int = 16384,
    tools: list[dict[str, Any]] | None = None,
    tool_choice: str | dict[str, Any] | None = None,
    return_message: bool = False,
    timeout: int | None = None,
) -> tuple[Any, dict[str, int]]:
    """Emit a JSON request to stdout and read the response from the FIFO."""

    if messages is None:
        messages = []
        if system:
            messages.append({"role": "system", "content": system})
        if user:
            messages.append({"role": "user", "content": user})

    request_id = str(uuid.uuid4())[:8]
    request = {
        "type": "llm_request",
        "id": request_id,
        "role": role,
        "phase": stage,
        "messages": messages,
        "max_tokens": max_completion_tokens,
    }
    if tools:
        request["tools"] = tools
    if tool_choice:
        request["tool_choice"] = tool_choice

    sys.stdout.write(json.dumps(request) + "\n")
    sys.stdout.flush()

    response = _read_response(request_id)

    content = response.get("content", "")
    usage = response.get("usage", {})
    prompt_tokens = usage.get("input_tokens", 0) or usage.get("prompt_tokens", 0)
    completion_tokens = usage.get("output_tokens", 0) or usage.get("completion_tokens", 0)

    usage_dict = {
        "prompt_tokens": prompt_tokens,
        "completion_tokens": completion_tokens,
        "total_tokens": prompt_tokens + completion_tokens,
    }

    tracker.record(stage, prompt_tokens, completion_tokens)

    if return_message:
        msg = CompatAssistantMessage(content=content)
        return msg, usage_dict

    return content, usage_dict


# ── Public API (matches router.py dispatch signatures) ───────────────────────

def chat_optimizer(
    system: str,
    user: str,
    max_completion_tokens: int = 16384,
    retries: int = 5,
    stage: str = "optimizer",
    timeout: int | None = None,
) -> tuple[str, dict[str, int]]:
    return _make_request(
        role="optimizer", stage=stage,
        system=system, user=user,
        max_completion_tokens=max_completion_tokens,
        timeout=timeout,
    )


def chat_target(
    system: str,
    user: str,
    max_completion_tokens: int = 16384,
    retries: int = 5,
    stage: str = "target",
    timeout: int | None = None,
) -> tuple[str, dict[str, int]]:
    return _make_request(
        role="target", stage=stage,
        system=system, user=user,
        max_completion_tokens=max_completion_tokens,
        timeout=timeout,
    )


def chat_with_deployment(
    deployment: str,
    system: str,
    user: str,
    max_completion_tokens: int = 16384,
    retries: int = 5,
    stage: str = "custom",
    timeout: int | None = None,
) -> tuple[str, dict[str, int]]:
    role = "optimizer" if "optimizer" in deployment else "target"
    return _make_request(
        role=role, stage=stage,
        system=system, user=user,
        max_completion_tokens=max_completion_tokens,
        timeout=timeout,
    )


def chat_optimizer_messages(
    messages: list[dict[str, Any]],
    max_completion_tokens: int = 16384,
    retries: int = 5,
    stage: str = "optimizer",
    *,
    tools: list[dict[str, Any]] | None = None,
    tool_choice: str | dict[str, Any] | None = None,
    return_message: bool = False,
    timeout: int | None = None,
) -> tuple[Any, dict[str, int]]:
    return _make_request(
        role="optimizer", stage=stage,
        messages=messages,
        max_completion_tokens=max_completion_tokens,
        tools=tools, tool_choice=tool_choice,
        return_message=return_message,
        timeout=timeout,
    )


def chat_target_messages(
    messages: list[dict[str, Any]],
    max_completion_tokens: int = 16384,
    retries: int = 5,
    stage: str = "target",
    *,
    tools: list[dict[str, Any]] | None = None,
    tool_choice: str | dict[str, Any] | None = None,
    return_message: bool = False,
    timeout: int | None = None,
) -> tuple[Any, dict[str, int]]:
    return _make_request(
        role="target", stage=stage,
        messages=messages,
        max_completion_tokens=max_completion_tokens,
        tools=tools, tool_choice=tool_choice,
        return_message=return_message,
        timeout=timeout,
    )


def chat_messages_with_deployment(
    deployment: str,
    messages: list[dict[str, Any]],
    max_completion_tokens: int = 16384,
    retries: int = 5,
    stage: str = "custom",
    *,
    tools: list[dict[str, Any]] | None = None,
    tool_choice: str | dict[str, Any] | None = None,
    return_message: bool = False,
    timeout: int | None = None,
) -> tuple[Any, dict[str, int]]:
    role = "optimizer" if "optimizer" in deployment else "target"
    return _make_request(
        role=role, stage=stage,
        messages=messages,
        max_completion_tokens=max_completion_tokens,
        tools=tools, tool_choice=tool_choice,
        return_message=return_message,
        timeout=timeout,
    )


def get_token_summary() -> dict[str, dict[str, int]]:
    return tracker.summary()


def reset_token_tracker() -> None:
    tracker.reset()


def set_reasoning_effort(effort: str | None) -> None:
    global _REASONING_EFFORT
    _REASONING_EFFORT = effort


def set_target_deployment(deployment: str) -> None:
    global _TARGET_DEPLOYMENT
    _TARGET_DEPLOYMENT = deployment


def set_optimizer_deployment(deployment: str) -> None:
    global _OPTIMIZER_DEPLOYMENT
    _OPTIMIZER_DEPLOYMENT = deployment
