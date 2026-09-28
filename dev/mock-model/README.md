# Local mock model API

```sh
cd dev/mock-model
uv run python server.py
```

From the repository root, run `cargo run -p artist -- -p "read the manifest" --config configs/mock-model.lua` to exercise the Lua adapter against this server.

POST OpenAI Chat Completions-shaped JSON to `http://127.0.0.1:8765/v1/chat/completions`. The default `read_then_answer` scenario first requests `read` of `Cargo.toml`; once a subsequent request includes a `role: "tool"` message, it returns a final answer. `stream: true` returns SSE with deliberately split tool arguments and transport writes.

Choose a scenario with `--scenario NAME` or the per-request `X-Mock-Scenario` header:

- `read_then_answer` — working tool round-trip (default)
- `tool_error` — reads a nonexistent file, then acknowledges the result
- `unknown_tool` — requests `missing_tool`
- `bad_arguments` — requests `read` with invalid JSON arguments
- `text` — seeded pseudo-random words without a tool call
- `malformed_json` — invalid JSON response
- `http_error` — HTTP 429 with an error body
- `slow` — waits two seconds before responding

GET `/health` for readiness and `/requests` for the last 100 received request bodies. It binds only to `127.0.0.1`; don't send real credentials or secrets to this development server.
