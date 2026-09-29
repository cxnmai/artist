-- OpenCode Go / DeepSeek V4 Pro. Set OPENCODE_GO_API_KEY or replace this placeholder.
local api_key = os.getenv("OPENCODE_GO_API_KEY") or "REPLACE_WITH_OPENCODE_GO_API_KEY"
local session_id = string.format("artist-%d-%08x", os.time(), math.random(0, 0xffffffff))

artist.model({
  name = "deepseek-v4-pro",
  request = function(context, tools)
    if api_key == "REPLACE_WITH_OPENCODE_GO_API_KEY" then
      error("Set OPENCODE_GO_API_KEY or replace the placeholder in configs/opencode-go.lua")
    end
    local messages = {}
    if context.system_prompt ~= "" then
      table.insert(messages, {role = "system", content = context.system_prompt})
    end
    for _, entry in ipairs(context.entries) do
      if entry.type == "user" then
        table.insert(messages, {role = "user", content = entry.text})
      elseif entry.type == "tool_result" then
        table.insert(messages, {role = "tool", tool_call_id = entry.tool_call_id, content = entry.text})
      elseif entry.type == "assistant" then
        local text, thinking, calls = {}, {}, {}
        for _, block in ipairs(entry.blocks) do
          if block.type == "text" then
            table.insert(text, block.text)
          elseif block.type == "thinking" then
            table.insert(thinking, block.text)
          elseif block.type == "tool_call" then
            table.insert(calls, {
              id = block.id, type = "function",
              ["function"] = {name = block.name, arguments = artist.json.encode(block.arguments)},
            })
          end
        end
        local message = {role = "assistant", content = table.concat(text)}
        if #thinking > 0 then message.reasoning_content = table.concat(thinking) end
        if #calls > 0 then message.tool_calls = calls end
        table.insert(messages, message)
      end
    end
    local api_tools = {}
    for _, tool in ipairs(tools) do
      table.insert(api_tools, {
        type = "function",
        ["function"] = {
          name = tool.name, description = tool.description, parameters = tool.parameters,
        },
      })
    end
    return {
      url = "https://opencode.ai/zen/go/v1/chat/completions",
      headers = {
        Authorization = "Bearer " .. api_key,
        ["User-Agent"] = "artist/0.1.0",
        ["x-opencode-session"] = session_id,
      },
      body = {model = "deepseek-v4-pro", messages = messages, tools = api_tools},
    }
  end,
  response = function(status, _, body)
    if status ~= 200 then
      error("OpenCode Go HTTP " .. status .. ": " .. artist.json.encode(body))
    end
    local choice = body.choices and body.choices[1]
    if not choice or not choice.message then error("OpenCode Go response has no message") end
    local message = choice.message
    local events = {}
    if type(message.reasoning_content) == "string" and message.reasoning_content ~= "" then
      table.insert(events, {type = "thinking_delta", text = message.reasoning_content})
    end
    if type(message.content) == "string" and message.content ~= "" then
      table.insert(events, {type = "text_delta", text = message.content})
    end
    for _, call in ipairs(message.tool_calls or {}) do
      table.insert(events, {type = "tool_call_start", id = call.id, name = call["function"].name})
      table.insert(events, {type = "tool_call_arguments_delta", id = call.id, text = call["function"].arguments})
      table.insert(events, {type = "tool_call_end", id = call.id})
    end
    table.insert(events, {type = "finished", reason = choice.finish_reason})
    return events
  end,
})
