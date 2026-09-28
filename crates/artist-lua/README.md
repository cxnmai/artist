# Lua interface (buffered model responses)

Run with `cargo run -p artist -- -p "prompt" --config path/to/config.lua`. No model is configured by default. See `examples/mock-model.lua` for a working adapter to the local mock API.

- `artist.model({ request = function(context, tools) ... end, response = function(status, headers, body) ... end })` registers the model. `request` returns `{url, method?, headers?, body, timeout?}`. Rust serializes `body` as JSON and sends it with `reqwest`. `response` receives a Rust-decoded JSON table and returns an array of `ModelEvent` tables (with `type` fields such as `text_delta`, `tool_call_start`, `tool_call_arguments_delta`, `tool_call_end`, `finished`). Non-2xx responses are passed to `response` too.
- `artist.register_tool({name, description, parameters, execute = function(args, ctx) ... end})` registers a tool. `parameters` is JSON Schema; `ctx.cwd` is the current working directory. `execute` returns a string or `{text, is_error?}` and may call async native functions.
- `artist.native.definitions()` provides the built-in schemas. Bundled `lua/default.lua` registers read, edit, write, and bash through `artist.native.execute(name, args, cwd)`.
- `artist.select_context(function(conversation) ... end)` optionally returns a `ContextSelection` table before each request; fields include `include_system_prompt`, `included_entries`, and `included_blocks`. Omit it to include all history. Selection does not mutate the stored conversation.
- `artist.json.decode(string)` and `artist.json.encode(value)` use Rust's JSON implementation. JSON null is `artist.json.null` in Lua, not `nil`.

The first slice uses complete JSON responses; model streaming decoding is not wired yet. `artist -p` writes newline-delimited JSON events to stdout. Lua config files are trusted code.
