# Tool presentation

A registered tool's `execute(args, ctx)` may return optional `ui` metadata alongside its normal result table. No separate frontend Lua runtime or rendering callback is required.

| Field | Meaning |
| --- | --- |
| `text` | Required model-facing tool result, unchanged by UI metadata. |
| `is_error` | Optional result error flag, default false. |
| `ui.short` | Required string when `ui` is supplied: compact summary of the operation. |
| `ui.long` | Optional string: expanded detail, such as a diff or detailed input/output. |

Both views remain strings, with newlines and ANSI SGR colors/styles supported by the TUI. Standard/bright colors, 256-color palettes, RGB colors, bold, dim, italic, underline, reverse, strikethrough, and resets are supported. For example, `"\27[31m-old\27[0m\n\27[32m+new\27[0m"` in Lua colors removed/added text red/green. There is no diff-specific logic or Markdown rendering.

Only styling is interpreted. Cursor movement, screen clearing, OSC titles/links/clipboard commands, DCS, bell, carriage returns, and other terminal controls are ignored rather than sent to the terminal. Tabs expand to four spaces. Styling is parsed once per view update; layout measures the resulting text without escape bytes. NAV tool selection uses bold emphasis instead of color inversion to preserve supplied foreground/background colors. Colors apply to text spans, not automatically to the remainder of a row.

The backend emits `ui` on the typed `AgentEvent::ToolResult` after execution. It does **not** store UI metadata in `Conversation`, send it to the model, include it in compaction, or count it toward context occupancy. Other frontends can use the same data without decoding provider JSON.

The TUI groups each tool call and result into one navigable transcript block:

- With UI metadata, the normal view shows `short`.
- Selecting the tool in NAV shows `short` followed by `long`.
- If `long` is absent, the expanded view shows the summary and raw normalized I/O JSON.
- Without UI metadata, both views show raw normalized I/O JSON.
- Moving selection away, or returning to INS, collapses the tool.
- Page and wheel scrolling inside expanded tools retain selection so long details remain readable.

Existing string results and result tables without `ui` remain valid. Invalid UI metadata falls back to JSON without changing a completed tool's model-facing result or error flag. Cancellation-generated results have no custom UI. Partial argument events remain JSON until complete typed arguments are available.

Default tool views are declared in `src/default.lua`: bold lowercase tool names, muted file paths or `Run <command>` descriptions, and muted `Done in …s` footers with output line counts. Errors use `Failed` footers. Edit changes appear inline as numbered red/green text with addition/deletion counts; write diffs expand on NAV selection. Read and bash expand their contents/output. Multiline commands summarize the first line and remaining step count. Tool views retain horizontal padding but have no shaded band; default diffs have no background coloring.

`artist.native.display_path(path, cwd)` returns paths relative to the working/project directory when inside it, otherwise absolute paths. `artist.native.execute` returns normal `text`/`is_error`, measured `elapsed_seconds`, and optional raw `diff` data for successful edit/write operations. Lua owns all summary strings, diff colors, and fallback messages. Edits use the exact validated before/after contents; writes capture a before-image, treating new files as additions. Unreadable/non-UTF-8 before-images leave diff data absent without changing successful write semantics. A no-change write returns an empty diff. Custom wrappers can reuse these data helpers or supply their own `ui`.

Session persistence is not implemented, including persistence of UI metadata.
