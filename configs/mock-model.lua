-- Minimal adapter for dev/mock-model/server.py (buffered Chat Completions).
artist.model({
  name = "artist-mock",
  request = function(context, tools)
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
        local text, calls = {}, {}
        for _, block in ipairs(entry.blocks) do
          if block.type == "text" then
            table.insert(text, block.text)
          elseif block.type == "tool_call" then
            table.insert(calls, {
              id = block.id, type = "function",
              ["function"] = {name = block.name, arguments = artist.json.encode(block.arguments)},
            })
          end
        end
        local message = {role = "assistant", content = table.concat(text)}
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
      url = "http://127.0.0.1:8765/v1/chat/completions",
      body = {model = "artist-mock", messages = messages, tools = api_tools},
    }
  end,
  response = function(status, _, body)
    if status ~= 200 then error("model API returned " .. status) end
    local choice = body.choices[1]
    local events = {}
    local message = choice.message
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
