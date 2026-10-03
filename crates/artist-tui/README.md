# Artist TUI

```sh
cargo run -p artist-tui -- --config dev/configs/opencode-go.lua
cargo run -p artist-tui -- --config path.lua --model model-id --reasoning high
```

The TUI uses the configured Lua runtime directly, retaining conversation history between prompts. The ribbon displays mode, provider/model, reasoning, context occupancy, and working directory; `*` beside the mode indicates an active turn. Unknown model windows display an unknown value rather than a fabricated percentage.

Without configuration, the window opens with unset backend fields. Configuration/discovery runs asynchronously on the same Tokio thread as Lua, so mode switching and quitting remain available while loading. Rendering reads app state, not Lua or network APIs. Long model labels and directory paths shorten on narrow terminals.

- Starts in `INS`.
- Esc switches to `NAV`; `i` returns to `INS`.
- `q` quits in `NAV`.
- Ctrl-C cancels an active turn; when idle it quits. A second Ctrl-C before cancellation completes also quits.

The white-framed input box above the ribbon edits a local draft in `INS`, using the main background inside. It starts at one editable line (three rows including borders) and grows with newlines and soft-wrapped text, up to the available terminal height; longer drafts scroll to keep the cursor visible. Arrow keys move through the draft, including wrapped rows. Backspace/Delete, Home/End, and bracketed paste are supported. `NAV` preserves the draft without editing it.

Shift-Enter inserts a newline. Enhanced keyboard reporting is requested so compatible terminals can distinguish it from Enter; Ctrl-J is a newline fallback for legacy terminals. Enter submits a nonempty draft to the backend and clears it only after acceptance. User messages appear once, using the ribbon background across their full rows. While a turn is active, typing can prepare the next draft, but Enter leaves it untouched until the active turn finishes. Without a loaded provider, submission displays a JSON error and preserves the draft.

Chat scrolls within the area above the fixed input and ribbon. In `NAV`, `j`/`k` or arrows select message blocks, Page Up/Down scroll by viewport, `g` jumps to the first message, and `G` returns to the latest. The selected user message is inverted across its full rows: light background, dark ribbon-colored text. Mouse-wheel scrolling also moves chat without moving the editor or ribbon. Drafts survive navigation; sending a message follows the latest output again.

All navigation uses one typed `ChatBlock` list. User events create styled user blocks; all other frontend-safe backend events are temporarily dumped as raw JSON blocks, including streamed deltas, tool events, committed assistant messages, usage, compaction, errors, and terminal events. This is JSON presentation of typed `AgentEvent` batches, not raw provider HTTP payloads or hidden replay data. Incoming output does not pull the viewport back to the bottom while navigating older blocks.

One Tokio local task owns the non-Send Lua runtime and conversation. It executes tools, streams model output, retries transient requests, and performs configured compaction through the same backend as the CLI. Backend/UI channels carry typed event vectors, not JSON. The UI remains responsive during asynchronous model/tool work. Leaving the TUI aborts outstanding backend work; synchronous Lua/file operations cannot be preempted until they yield.

Normalized response/tool rendering, the command palette, and session persistence remain deferred. All color choices remain in `src/colors.rs`.
