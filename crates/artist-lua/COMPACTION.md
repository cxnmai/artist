# Client-side context compaction

`default.lua` registers the Rust-backed summarizer. Automatic compaction uses the currently selected model, adapter, authentication, and mapped reasoning settings. It runs before a model request, including after a complete tool batch, when:

```text
context_tokens > context_window - reserve_tokens
```

An unknown context window disables automatic triggering. Defaults target large context windows; override the budgets for smaller models. The reserve and retained-context-plus-summary budgets must fit the selected window. Tools and system text also occupy context, so leave margin rather than treating these estimates as hard tokenizer limits.

## Lua configuration

```lua
artist.set_compaction({
  enabled = true,
  reserve_tokens = 16384,
  keep_recent_tokens = 20000,
  max_summary_tokens = 4096,
  instructions = "Preserve implementation decisions and unresolved issues.",
  compact = artist.compaction.summarize,
})
```

All fields have defaults, including `compact`. To disable automatic compaction:

```lua
artist.set_compaction({ enabled = false })
```

Registration replaces the policy; omitted settings revert to defaults.

## Custom compactor

```lua
artist.set_compaction({
  compact = function(context, options)
    -- options includes the budgets and current model_name, adapter_name,
    -- mapped reasoning_level, thinking_format, and requires_reasoning_content.
    options.instructions = "Prioritize the user's requirements and remaining work."
    return artist.compaction.summarize(context, options)
  end,
})
```

The callback is asynchronous-capable. Its arguments are copies, not mutable Rust history. Return `nil` to skip, or a result containing:

- `context`: replacement `Conversation` (`system_prompt`, `entries`, `last_usage`).
- `summary`: nonempty displayable text.
- `removed_entries`: number of prior entries replaced.
- `usage`: an optional array of display `usage` events from summarization.

Rust validates the result, preserves the system prompt, verifies tool-call/result pairing, and resets the usage anchor before activating replacement context. Custom implementations are responsible for provider replay compatibility and meaningful reduction. Triggering still uses the configured threshold; a custom callback does not run on every request.

## Default summarizer

- Keeps approximately `keep_recent_tokens`, walking backwards by complete assistant/tool-result groups. A single group may exceed this budget. If there is no useful older prefix to summarize, it skips compaction.
- Serializes the old prefix and any previous summary into a standalone, tool-free, buffered request. Tool-result text is limited to 2,000 characters; opaque provider data is never put into the summary prompt.
- Requests structured goals, constraints, progress, decisions, next steps, critical context, and file information. Repeated compaction updates the previous summary.
- Caps output using the adapter's output-budget field and the model's known maximum. Generic Lua HTTP adapters receive `context.max_output_tokens` in their request callback and must honor it themselves.
- Stores a synthetic `summary` context entry, rendered as an ordinary user message with `<summary>` delimiters, followed by recent normalized messages. It does not alter the original system prompt.
- Clears retained thinking and opaque provider replay blocks after rewriting the prefix, because signed/native reasoning may be bound to the old conversation. Recent text, tool calls, and results remain. This is portable text summarization, not native server-side compaction or hidden-reasoning preservation.
- Rejects empty, incomplete, truncated, or tool-calling summary responses. Failure stops the turn with its usual error event; cancellation emits its usual cancelled event. Neither commits replacement context.

Successful compaction emits a frontend-safe `compacted` event containing the summary, removed-entry count, and summarization usage, followed by updated context usage. Summary usage is separate from the conversation's model-response usage anchor. Existing transcript events are not erased; session persistence is not implemented by this feature.

The builtin can be called from asynchronous Lua code with the same `(context, options)` arguments. It computes a result without applying it; only the engine's compaction boundary activates that result. Native server-side compaction and a manual CLI command are not implemented here.
