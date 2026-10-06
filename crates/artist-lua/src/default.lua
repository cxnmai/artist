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

local function header(name, description)
  return "\27[1m" .. name .. "\27[0m \27[38;5;245m" .. description .. "\27[0m"
end

local function line_count(text)
  if text == "" then return 0 end
  local _, count = text:gsub("\n", "")
  return count + (text:sub(-1) == "\n" and 0 or 1)
end

local function footer(result, added, removed)
  local detail
  if added then
    detail = "\27[92m+" .. added .. "\27[90m \27[91m-" .. removed .. "\27[90m"
  else
    local count = line_count(result.text)
    detail = count .. (count == 1 and " line" or " lines")
  end
  return "\27[90m" .. (result.is_error and "Failed" or "Done")
    .. string.format(" in %.3fs (", result.elapsed_seconds or 0) .. detail .. ")\27[0m"
end

local function diff_view(result)
  if result.is_error then return result.text end
  if type(result.diff) ~= "string" then
    return "Diff unavailable: previous contents could not be read as UTF-8."
  end
  if result.diff == "" then return "No changes.", 0, 0 end
  local lines, added, removed = {}, 0, 0
  local old_line, new_line
  for line in (result.diff:gsub("\n$", "") .. "\n"):gmatch("(.-)\n") do
    -- Number changed lines, omit unified headers/context, and prevent injected styling.
    line = line:gsub("[%z\1-\8\11-\31\127]", "")
    local old_start, new_start = line:match("^@@ %-(%d+),?%d* %+(%d+),?%d* @@")
    if old_start then
      old_line, new_line = tonumber(old_start), tonumber(new_start)
    elseif old_line then
      local prefix = line:sub(1, 1)
      if prefix == "+" then
        lines[#lines + 1] = "\27[92m" .. new_line .. " + " .. line:sub(2) .. "\27[0m"
        new_line, added = new_line + 1, added + 1
      elseif prefix == "-" then
        lines[#lines + 1] = "\27[91m" .. old_line .. " - " .. line:sub(2) .. "\27[0m"
        old_line, removed = old_line + 1, removed + 1
      elseif prefix == " " then
        old_line, new_line = old_line + 1, new_line + 1
      end
    end
  end
  return table.concat(lines, "\n"), added, removed
end

local function command_description(command)
  if type(command) ~= "string" then return "Run <invalid command>" end
  local first = command:match("[^\n]+") or command
  local extra = math.max(0, line_count(command) - 1)
  return "Run " .. first .. (extra > 0 and (" + " .. extra .. (extra == 1 and " more step" or " more steps")) or "")
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
      short = header("read", display_path(args, ctx)) .. "\n" .. footer(result),
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
    local diff, added, removed = diff_view(result)
    result.ui = {
      short = header("edit", display_path(args, ctx)) .. "\n" .. diff .. "\n" .. footer(result, added, removed),
      long = "", -- Like the reference, edit changes are visible inline.
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
      short = header("write", display_path(args, ctx)) .. "\n" .. footer(result),
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
    result.ui = {short = header("bash", command_description(args.command)) .. "\n" .. footer(result), long = result.text}
    return result
  end,
})
