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

Input editing, generation, chat, and the command palette are not wired yet. All color choices remain in `src/colors.rs`.
