-- OpenCode Go / DeepSeek V4 Pro. Set OPENCODE_GO_API_KEY in the environment.
local session_id = string.format("artist-%d-%08x", os.time(), math.random(0, 0xffffffff))

-- Override the bundled base prompt, then append another section.
artist.set_system_prompt(function(ctx)
  return "You are " .. ctx.model_name .. " operating in Artist. Solve coding tasks carefully."
end)
artist.append_system_prompt("Prefer focused changes and explain the result concisely.")

-- Default tools were registered by default.lua before this file was loaded.
-- Removing one prevents both advertisement and execution through the registry.
artist.unregister_tool("write")

artist.model({
  name = "deepseek-v4-pro",
  adapter = "chat_completions",
  endpoint = "https://opencode.ai/zen/go/v1/chat/completions",
  auth = { bearer_env = "OPENCODE_GO_API_KEY" },
  headers = {
    ["User-Agent"] = "artist/0.1.0",              -- literal header value
    ["x-opencode-session"] = function() return session_id end, -- dynamic header
  },
  options = {}, -- Optional API body fields, e.g. max_tokens if supported.
})

-- Custom tools use the same registration path as the built-ins.
artist.register_tool({
  name = "add", description = "Add two numbers",
  parameters = {
    type = "object",
    properties = {a = {type = "number"}, b = {type = "number"}},
    required = {"a", "b"},
  },
  execute = function(args, ctx)
    return {text = tostring(args.a + args.b), is_error = false}
  end,
})

-- Optional: select context before each model request. The default includes all.
-- Indices in included_blocks refer to the original conversation and are zero-based.
-- artist.select_context(function(conversation)
--   local blocks = {}
--   for entry_index, entry in ipairs(conversation.entries) do
--     if entry.type == "assistant" then
--       local kept = {}
--       for block_index, block in ipairs(entry.blocks) do
--         if block.type ~= "thinking" then table.insert(kept, block_index - 1) end
--       end
--       blocks[entry_index - 1] = kept
--     end
--   end
--   return {include_system_prompt = true, included_blocks = blocks}
-- end)

-- JSON helpers: artist.json.decode(text), artist.json.encode(value), artist.json.null.
-- For a non-Chat-Completions API, replace artist.model above with the generic
-- artist.model({name = "...", request = function(context, tools) ... end,
--               response = function(status, headers, body) ... end}) form.
