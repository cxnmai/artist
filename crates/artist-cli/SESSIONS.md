# Durable sessions

Every CLI invocation with prompts creates a new session unless `-r` selects one:

```sh
artist -p "Investigate this project" --config /path/to/config.lua
artist -p "Continue the investigation" -r <session-id> --config /path/to/config.lua
```

Multiple prompts in one invocation share the session. Normal CLI output remains
frontend-safe NDJSON; session IDs are not printed. Session discovery is available
through the `artist-session` metadata/list API and the local database, not an
implicit "continue latest" behavior. `list(directory, cwd)` uses an indexed cwd
query; `transcript(directory, id, after, limit)` provides safe, cursor-based
transcript pages without taking the session's writer lock. `limit` bounds scanned
records, so callers must follow `next_after` even when a page has no messages.

## Location

The embedded native Turso engine stores sessions locally; no cloud service or
credentials are required. The shared database lives at `<directory>/sessions.db`.
Defaults:

- Linux: `$XDG_DATA_HOME/artist/sessions`, or `~/.local/share/artist/sessions`.
- macOS: `~/Library/Application Support/artist/sessions`.
- Windows: `%LOCALAPPDATA%\artist\sessions`.

Override the directory in trusted Lua configuration:

```lua
artist.sessions({ directory = "/absolute/path/to/sessions" })
```

This configuration is shared by CLI and ACP. Moving to a different storage root
changes which sessions can be discovered/resumed. Session storage can contain
sensitive prompts, tool output, and opaque model replay data. Unix directories and
files are restricted to the current user. This is not encryption at rest.

## History and context

Ordered, versioned records are authoritative. They contain the initial cwd,
settings and system prompt, canonical messages, settings changes, usage anchors,
compaction checkpoints, and terminal turn outcomes. Records have stable sequence
IDs within a session. Metadata contains creation and last-activity timestamps for
directory-scoped session listings; it is updated in the same transactions as
history. Timestamps use Unix epoch milliseconds.

The transcript consists of original messages and tool results. A compaction
record stores the replacement context projection and its summary at a particular
history boundary. It does not delete or overwrite earlier messages. Continuation
seeks the latest indexed checkpoint and replays only its suffix; transcript display
still sees all original messages. A derived `resume_points` index caches the
boundary's settings and unresolved tool IDs with an integrity checksum. Its context
comes from the original canonical compaction record, not a second authoritative
copy. Compaction and its index update commit together. Missing or damaged entries
fall back to canonical replay and repair the index. The Rust API
`rebuild_resume_index(directory, id)` explicitly rebuilds this derived state without
changing canonical messages. Frontends must use safe display projections, not
expose opaque provider replay data.

## Continuation

Resume restores the recorded canonical cwd, system prompt, model and reasoning.
Run from that same directory; missing/mismatched directories fail rather than
silently redirecting tools. Current Lua configuration supplies credentials, tools,
and runtime policy; its system-prompt callbacks do not overwrite a resumed prompt.

`--model` and `--reasoning` explicitly override saved settings. An explicit model
selection uses that model's default reasoning unless reasoning is also specified.
Switching provider/model preserves original history but removes incompatible
thinking/native replay blocks from the model-context projection and clears its
usage estimate. A saved model unavailable in current configuration is an error;
Artist does not silently substitute another model.

ACP `session/new` creates a durable session; `session/load` restores one and sends
the original frontend-safe transcript before replying. Both frontends use the
same store and engine persistence boundaries. ACP replay reads 256 scanned records
per page and releases the storage lock between pages and notifications, including
when a page has only metadata and no visible messages.

Schema v1/v2 databases upgrade automatically to v3 under the storage lock. Migration
does not scan every session; a legacy session's first resume may scan its archive
once to build the index. Subsequent compacted resumes seek directly to it. Sessions
without a compaction still load their full active context. Older binaries cannot
open the newer schema: restart CLI/ACP processes after updating Artist.

## Durability

Token deltas are streamed to the frontend and accumulated in memory, not written
to disk. A user message is committed before model work; completed assistant
responses and their usage anchors are committed together before tool execution;
each tool result is committed before proceeding. Compaction/settings changes are
committed before use. Terminal state is committed before reporting turn completion.
Writes use transactions with FULL synchronous durability. Persistence failure
or a dropped persistence future poisons that writer: reopen/replay before doing
more work instead of silently continuing with potentially divergent live state.

Turso's experimental multiprocess WAL is disabled. A short global OS lock covers
each storage operation; all engine handles close before that lock is released.
Different CLI/ACP sessions can interleave operations, without holding the database
lock while models or tools run. Complete storage operations run on blocking workers,
not the frontend executor. A separate lifetime lock prevents competing writers for
the same session, including workers whose awaiting callers have been cancelled.
Readers use the same operation lock. Checked WAL truncation runs when the WAL
reaches 4 MiB on open; a large transaction can exceed that size until the next open.
Whole-transcript reads, uncompacted resumes, index rebuilding, and large active
suffixes can still hold the lock longer than small pages. See the
[performance measurements and remaining limits](../artist-session/PERFORMANCE.md).

A crash during streaming can lose the incomplete response. On resume, unresolved
tool calls receive durable error results saying their execution outcome is unknown.
They are never rerun automatically: filesystem or external side effects may have
happened before the crash. Writer locks are released by the OS after process death.

Do not copy just the main database file while it is active: committed records may
still be in its WAL. Close all users before copying the database and its companion
files, or use an engine-supported consistent backup mechanism.
