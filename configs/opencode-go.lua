-- OpenCode Go / DeepSeek V4 Pro. Set OPENCODE_GO_API_KEY.
local session_id = string.format("artist-%d-%08x", os.time(), math.random(0, 0xffffffff))

-- To replace Artist's default system prompt, uncomment:
-- artist.set_system_prompt(function(ctx)
--   return "You are " .. ctx.model_name .. ", a coding assistant in Artist."
-- end)

artist.model({
  name = "deepseek-v4-pro",
  adapter = "chat_completions",
  endpoint = "https://opencode.ai/zen/go/v1/chat/completions",
  auth = { bearer_env = "OPENCODE_GO_API_KEY" },
  headers = {
    ["User-Agent"] = "artist/0.1.0",
    ["x-opencode-session"] = function() return session_id end,
  },
})
