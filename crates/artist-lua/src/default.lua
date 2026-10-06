-- Automatic compaction uses the current model; configuration may replace this policy.
artist.set_compaction({
  enabled = true,
  reserve_tokens = 16384,
  keep_recent_tokens = 20000,
  max_summary_tokens = 4096,
  compact = artist.compaction.summarize,
})

artist.set_system_prompt(function(context)
  return "You are the language model " .. context.model_name
    .. " operating inside of the agentic harness 'Artist.'"
end)

-- Presentation stays in Lua; native helpers supply paths and raw before/after diffs.
local function display_path(args, ctx)
  if type(args.path) ~= "string" then return "<invalid path>" end
  return artist.native.display_path(args.path, ctx.cwd)
end

local function diff_view(result)
  if result.is_error then return result.text end
  if type(result.diff) ~= "string" then
    return "Diff unavailable: previous contents could not be read as UTF-8."
  end
  if result.diff == "" then return "No changes." end
  local lines = {}
  for line in (result.diff:gsub("\n$", "") .. "\n"):gmatch("(.-)\n") do
    -- File contents must not inject their own terminal styling into a diff.
    line = line:gsub("[%z\1-\8\11-\31\127]", "")
    local prefix = line:sub(1, 1)
    if prefix == "+" then line = "\27[92m" .. line .. "\27[0m"
    elseif prefix == "-" then line = "\27[91m" .. line .. "\27[0m" end
    lines[#lines + 1] = line
  end
  return table.concat(lines, "\n")
end

artist.register_tool({
  name = "read",
  description = "Read the contents of a text file. Output is truncated to 2000 lines or 50KB (whichever is hit first). Use offset/limit for large files. When you need the full file, continue with offset until complete.",
  parameters = {
    type = "object",
    properties = {
      path = {type = "string", description = "File path"},
      offset = {type = "number", description = "Starting line (1-indexed)"},
      limit = {type = "number", description = "Maximum number of lines"},
    },
    required = {"path"},
  },
  execute = function(args, ctx)
    local result = artist.native.execute("read", args, ctx.cwd)
    result.ui = {
      short = (result.is_error and "Read failed: " or "Read ") .. display_path(args, ctx),
      long = result.text,
    }
    return result
  end,
})

artist.register_tool({
  name = "edit",
  description = "Edit a single file using exact text replacement. Every edits[].oldText must match a unique, non-overlapping region of the original file. If two changes affect the same block or nearby lines, merge them into one edit instead of emitting overlapping edits. Do not include large unchanged regions just to connect distant changes.",
  parameters = {
    type = "object",
    properties = {
      path = {type = "string"},
      edits = {
        type = "array",
        items = {
          type = "object",
          properties = {
            oldText = {type = "string"},
            newText = {type = "string"},
          },
          required = {"oldText", "newText"},
        },
      },
    },
    required = {"path", "edits"},
  },
  execute = function(args, ctx)
    local result = artist.native.execute("edit", args, ctx.cwd)
    result.ui = {
      short = (result.is_error and "Edit failed: " or "Edited ") .. display_path(args, ctx),
      long = diff_view(result),
    }
    return result
  end,
})

artist.register_tool({
  name = "write",
  description = "Write content to a file. Creates the file if it doesn't exist, overwrites if it does. Automatically creates parent directories.",
  parameters = {
    type = "object",
    properties = {
      path = {type = "string"},
      content = {type = "string"},
    },
    required = {"path", "content"},
  },
  execute = function(args, ctx)
    local result = artist.native.execute("write", args, ctx.cwd)
    result.ui = {
      short = (result.is_error and "Write failed: " or "Wrote to ") .. display_path(args, ctx),
      long = diff_view(result),
    }
    return result
  end,
})

artist.register_tool({
  name = "bash",
  description = "Execute a bash command in the current working directory. Returns stdout and stderr. Output is truncated to last 2000 lines or 50KB (whichever is hit first). If truncated, full output is saved to a temp file. Optionally provide a timeout in seconds.",
  parameters = {
    type = "object",
    properties = {
      command = {type = "string"},
      timeout = {type = "number", description = "Optional timeout in seconds"},
    },
    required = {"command"},
  },
  execute = function(args, ctx)
    local result = artist.native.execute("bash", args, ctx.cwd)
    result.ui = {short = type(args.command) == "string" and args.command or "Bash", long = result.text}
    return result
  end,
})
