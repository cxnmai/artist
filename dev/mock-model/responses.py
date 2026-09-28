"""Scripted OpenAI Chat Completions-shaped replies."""

import json
import random

WORDS = ("amber", "canvas", "orbit", "quiet", "river", "spark")
RNG = random.Random(0)


def reply(request: dict, scenario: str) -> dict:
    messages = request.get("messages", [])
    if not isinstance(messages, list):
        raise ValueError("messages must be an array")
    has_result = any(isinstance(item, dict) and item.get("role") == "tool" for item in messages)

    if scenario in ("read_then_answer", "tool_error") and has_result:
        content = "Received tool result: " + str(messages[-1].get("content", ""))[:120]
        message = {"role": "assistant", "content": content}
        reason = "stop"
    elif scenario in ("read_then_answer", "tool_error", "unknown_tool", "bad_arguments"):
        name = "missing_tool" if scenario == "unknown_tool" else "read"
        arguments = "{oops" if scenario == "bad_arguments" else json.dumps(
            {"path": "does-not-exist.txt" if scenario == "tool_error" else "Cargo.toml"}
        )
        message = {
            "role": "assistant",
            "content": None,
            "tool_calls": [{
                "id": "call_mock_1",
                "type": "function",
                "function": {"name": name, "arguments": arguments},
            }],
        }
        reason = "tool_calls"
    else:
        message = {"role": "assistant", "content": " ".join(RNG.choices(WORDS, k=6))}
        reason = "stop"

    return {
        "id": "chatcmpl-mock",
        "object": "chat.completion",
        "model": request.get("model", "artist-mock"),
        "choices": [{"index": 0, "message": message, "finish_reason": reason}],
        "usage": {"prompt_tokens": 12, "completion_tokens": 8, "total_tokens": 20},
    }


def stream_frames(response: dict):
    """SSE frames; tool arguments are deliberately split across deltas."""
    choice = response["choices"][0]
    message = choice["message"]
    base = {"id": response["id"], "object": "chat.completion.chunk", "model": response["model"]}
    if message.get("tool_calls"):
        call = message["tool_calls"][0]
        arguments = call["function"]["arguments"]
        deltas = [
            {"tool_calls": [{"index": 0, "id": call["id"], "type": "function", "function": {"name": call["function"]["name"], "arguments": ""}}]},
            {"tool_calls": [{"index": 0, "function": {"arguments": arguments[:3]}}]},
            {"tool_calls": [{"index": 0, "function": {"arguments": arguments[3:]}}]},
        ]
    else:
        content = message["content"]
        midpoint = max(1, len(content) // 2)
        deltas = [{"content": content[:midpoint]}, {"content": content[midpoint:]}]
    for delta in deltas:
        yield "data: " + json.dumps(base | {"choices": [{"index": 0, "delta": delta, "finish_reason": None}]}) + "\n\n"
    yield "data: " + json.dumps(base | {"choices": [{"index": 0, "delta": {}, "finish_reason": choice["finish_reason"]}]}) + "\n\n"
    yield "data: [DONE]\n\n"
