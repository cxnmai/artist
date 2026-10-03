# Artist TUI

```sh
cargo run -p artist-tui -- --config dev/configs/opencode-go.lua
cargo run -p artist-tui -- --config path.lua --model model-id --reasoning high
```

Only the status ribbon is connected to the backend. It displays mode, provider/model, reasoning, context occupancy, and working directory. The context estimate includes the configured system prompt; unknown model windows display an unknown value rather than a fabricated percentage.

Without configuration, the window opens with unset backend fields. Configuration/discovery runs asynchronously on the same Tokio thread as Lua, so mode switching and quitting remain available while loading. Rendering reads app state, not Lua or network APIs. Long model labels and directory paths shorten on narrow terminals.

- Starts in `INS`.
- Esc switches to `NAV`; `i` returns to `INS`.
- `q` quits in `NAV`; Ctrl-C quits anywhere.

The white-framed input box above the ribbon edits a local draft in `INS`, using the main background inside. It starts at one editable line (three rows including borders) and grows with newlines and soft-wrapped text, up to the available terminal height; longer drafts scroll to keep the cursor visible. Arrow keys move through the draft, including wrapped rows. Backspace/Delete, Home/End, and bracketed paste are supported. `NAV` preserves the draft without editing it.

Shift-Enter inserts a newline. Enhanced keyboard reporting is requested so compatible terminals can distinguish it from Enter; Ctrl-J is a newline fallback for legacy terminals. Plain Enter intentionally does nothing—prompt submission, generation, chat, and the command palette are not wired yet. All color choices remain in `src/colors.rs`.
