-- OpenCode Go / DeepSeek V4 Pro. Set OPENCODE_GO_API_KEY in the environment.
local session_id = string.format("artist-%d-%08x", os.time(), math.random(0, 0xffffffff))
-- Snapshot of Pi's OpenCode Go catalog (last-modified 2026-09-28):
-- https://pi.dev/api/models/providers/opencode-go?types=chat%2Cimage%2Cclassifier
-- The live OpenCode /models response has IDs only, so unknown models remain unannotated.
local catalog = {
  ["deepseek-v4-flash"] = { api = "chat", context_window = 1000000, max_output_tokens = 384000, reasoning_map = { low = "low", high = "high", max = "max" }, thinking_format = "deepseek", requires_reasoning_content = true },
  ["deepseek-v4-flash-vision-exp"] = { api = "chat", context_window = 1000000, max_output_tokens = 384000, reasoning_map = { low = "low", high = "high", max = "max" }, thinking_format = "deepseek", requires_reasoning_content = true },
  ["deepseek-v4-pro"] = { api = "chat", context_window = 1000000, max_output_tokens = 384000, reasoning_map = { high = "high", max = "max" }, thinking_format = "deepseek", requires_reasoning_content = true },
  ["deepseek-v4.1-flash"] = { api = "chat", context_window = 1000000, max_output_tokens = 384000, reasoning_map = { low = "low", high = "high", max = "max" }, thinking_format = "deepseek", requires_reasoning_content = true },
  ["glm-5.2"] = { api = "chat", context_window = 1000000, max_output_tokens = 131072, reasoning_map = { high = "high", max = "max" } },
  ["glm-5.3"] = { api = "chat", context_window = 1000000, max_output_tokens = 131072, reasoning_map = { low = "low", high = "high", max = "max" } },
  ["glm-5.3-flash"] = { api = "chat", context_window = 1000000, max_output_tokens = 131072, reasoning_map = { low = "low", high = "high", max = "max" } },
  ["gpt-5.6-luna"] = { api = "responses", context_window = 1050000, max_output_tokens = 128000, reasoning_map = { low = "low", medium = "medium", high = "high", xhigh = "xhigh", max = "max" } },
  ["gpt-6-luna"] = { api = "responses", context_window = 1050000, max_output_tokens = 128000, reasoning_map = { off = "none", low = "low", medium = "medium", high = "high", xhigh = "xhigh", max = "max" } },
  ["grok-4.6"] = { api = "responses", context_window = 500000, max_output_tokens = 500000, reasoning_map = { low = "low", medium = "medium", high = "high", xhigh = "xhigh" } },
  ["grok-4.7"] = { api = "responses", context_window = 500000, max_output_tokens = 500000, reasoning_map = { low = "low", medium = "medium", high = "high", xhigh = "xhigh" } },
  ["hy3"] = { api = "chat", context_window = 256000, max_output_tokens = 128000, reasoning_map = { off = "none", low = "low", high = "high" } },
  ["hy4-preview"] = { api = "chat", context_window = 1024000, max_output_tokens = 64000, reasoning_map = { off = "none", high = "high" } },
  ["kimi-k2.7-code"] = { api = "chat", context_window = 262144, max_output_tokens = 262144 },
  ["kimi-k3"] = { api = "chat", context_window = 1048576, max_output_tokens = 131072, reasoning_map = { max = "max" } },
  ["longcat-2.0"] = { api = "chat", context_window = 1000000, max_output_tokens = 131072 },
  ["longcat-2.5-preview-free"] = { api = "chat", context_window = 1000000, max_output_tokens = 131072 },
  ["mimo-v2.5"] = { api = "chat", context_window = 1000000, max_output_tokens = 128000 },
  ["mimo-v2.5-pro"] = { api = "chat", context_window = 1048576, max_output_tokens = 128000 },
  ["mimo-v2.6-flash"] = { api = "chat", context_window = 1048576, max_output_tokens = 131072 },
  ["mimo-v2.6-pro"] = { api = "chat", context_window = 1048576, max_output_tokens = 131072 },
  ["minimax-m2.7"] = { api = "chat", context_window = 204800, max_output_tokens = 131072 },
  ["minimax-m3"] = { api = "messages", context_window = 1000000, max_output_tokens = 131072 },
  ["muse-spark-1.2-contributor"] = { api = "responses", context_window = 1048576, max_output_tokens = 131072, reasoning_map = { minimal = "minimal", low = "low", medium = "medium", high = "high", xhigh = "xhigh" } },
  ["muse-spark-1.3-contributor"] = { api = "responses", context_window = 1048576, max_output_tokens = 131072, reasoning_map = { minimal = "minimal", low = "low", medium = "medium", high = "high", xhigh = "xhigh" } },
  ["qwen3.7-plus"] = { api = "chat", context_window = 1000000, max_output_tokens = 65536 },
  ["qwen3.8-flash"] = { api = "messages", context_window = 1000000, max_output_tokens = 131072 },
  ["qwen3.8-max"] = { api = "chat", context_window = 1000000, max_output_tokens = 131072, reasoning_map = { low = "low", medium = "medium", xhigh = "xhigh" } },
  ["space-bunny-free"] = { api = "chat", context_window = 1048576, max_output_tokens = 524288, reasoning_map = { low = "low", medium = "medium", high = "high", xhigh = "xhigh", max = "max" } },
}
local models, model_info = {}, {}
for id, entry in pairs(catalog) do
  table.insert(models, id)
  model_info[id] = {
    adapter = entry.api ~= "chat" and entry.api or nil,
    context_window = entry.context_window,
    max_output_tokens = entry.max_output_tokens,
    reasoning_map = entry.reasoning_map,
    thinking_format = entry.thinking_format,
  }
end
table.sort(models)

-- Override the bundled base prompt, then append another section.
artist.set_system_prompt(function(ctx)
  return "You are " .. ctx.model_name .. " operating in Artist. Solve coding tasks carefully."
end)
artist.append_system_prompt("Prefer focused changes and explain the result concisely.")

-- Default tools were registered by default.lua before this file was loaded.
-- Removing one prevents both advertisement and execution through the registry.
artist.unregister_tool("write")

artist.provider({
  name = "opencode-go",
  -- /models adds new IDs; Pi's catalog supplies metadata for known ones.
  -- Unknown IDs use the default Chat Completions adapter until cataloged.
  discover = { endpoint = "https://opencode.ai/zen/go/v1/models" },
  models = models,
  model_info = model_info,
  default_model = "deepseek-v4-pro",
  adapter = "chat_completions",
  adapters = {
    responses = { adapter = "openai_responses", endpoint = "https://opencode.ai/zen/go/v1/responses" },
    messages = {
      adapter = "anthropic_messages", endpoint = "https://opencode.ai/zen/go/v1/messages",
      auth = { api_key_env = "OPENCODE_GO_API_KEY" },
    },
  },
  endpoint = "https://opencode.ai/zen/go/v1/chat/completions",
  auth = { bearer_env = "OPENCODE_GO_API_KEY" },
  headers = {
    ["User-Agent"] = "artist/0.1.0",              -- literal header value
    ["x-opencode-session"] = function() return session_id end, -- dynamic header
  },
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
-- For an unsupported API, use artist.provider({name = "...", models = {"..."},
--   request = function(context, tools, model_id) ... end,
--   response = function(status, headers, body) ... end}) instead.
