artist.set_system_prompt(function(context)
  return "You are the language model " .. context.model_name
    .. " operating inside of the agentic harness 'Artist.'"
end)

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
    return artist.native.execute("read", args, ctx.cwd)
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
    return artist.native.execute("edit", args, ctx.cwd)
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
    return artist.native.execute("write", args, ctx.cwd)
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
    return artist.native.execute("bash", args, ctx.cwd)
  end,
})
