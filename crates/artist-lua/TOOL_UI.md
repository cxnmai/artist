# Tool presentation

A registered tool's `execute(args, ctx)` may return optional `ui` metadata alongside its normal result table. No separate frontend Lua runtime or rendering callback is required.

| Field | Meaning |
| --- | --- |
| `text` | Required model-facing tool result, unchanged by UI metadata. |
| `is_error` | Optional result error flag, default false. |
| `ui.short` | Required string when `ui` is supplied: compact summary of the operation. |
| `ui.long` | Optional string: expanded detail, such as a diff or detailed input/output. |

Both views are plain text; newlines are supported. No Markdown, terminal escape sequences, or diff-specific styling API is provided by this framework.

The backend emits `ui` on the typed `AgentEvent::ToolResult` after execution. It does **not** store UI metadata in `Conversation`, send it to the model, include it in compaction, or count it toward context occupancy. Other frontends can use the same data without decoding provider JSON.

The TUI groups each tool call and result into one navigable transcript block:

- With UI metadata, the normal view shows `short`.
- Selecting the tool in NAV shows `short` followed by `long`.
- If `long` is absent, the expanded view shows the summary and raw normalized I/O JSON.
- Without UI metadata, both views show raw normalized I/O JSON.
- Moving selection away, or returning to INS, collapses the tool.
- Page and wheel scrolling inside expanded tools retain selection so long details remain readable.

Existing string results and result tables without `ui` remain valid. Invalid UI metadata falls back to JSON without changing a completed tool's model-facing result or error flag. Cancellation-generated results have no custom UI. Partial argument events remain JSON until complete typed arguments are available.

Built-in tools have no custom presentation yet. This is only the transport and rendering framework; it does not compute write/edit diffs or install example tool renderers. Session persistence is not implemented, including persistence of UI metadata.
