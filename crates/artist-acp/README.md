# Artist ACP

Run Artist as a client-independent Agent Client Protocol (ACP v1) server:

```sh
cargo run -p artist -- acp --config dev/configs/opencode-go.lua
# Installed binary:
artist acp --config /absolute/path/to/config.lua
```

`--model` and `--reasoning` select the defaults for every session. Configure
provider credentials through the existing Lua configuration/environment.

The client launches this command and communicates using newline-delimited
JSON-RPC on stdin/stdout. Diagnostics go to stderr. EOF stops the server.
Each `session/new` creates independent conversation history using its supplied
absolute `cwd`; Artist never changes the process working directory.

Supported: `initialize`, `session/new`, `session/prompt`, `session/cancel`,
streamed assistant text/thoughts, and tool-call status/input/output updates.
Prompts accept text and resource links. Links are passed to the model as names/URIs
for tool-based access, not fetched automatically. Sessions live only for the server process.
Overlapping prompts within a session are rejected; separate sessions can run
concurrently. Cancellation uses Artist's cooperative cancellation token.

Not yet implemented: session persistence/loading, MCP servers, images/audio,
embedded context, protocol model/mode selection, or editor-hosted filesystem
and terminal execution. Only implemented capabilities are advertised.

**Tools currently execute locally under the existing Lua policy, just like the
CLI. No ACP permission requests are issued yet. Do not assume an editor approval
setting gates file writes or shell commands.** Custom Lua code must not launch
processes that write directly to stdout, which is reserved for ACP traffic.

The adapter uses the 0.9 Rust SDK's local-future API because Artist's embedded
Lua runtime is non-`Send`. This is an SDK version choice, not a custom protocol.
The existing NDJSON CLI remains available unchanged.
