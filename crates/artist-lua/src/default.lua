artist.set_system_prompt(function(context)
  return "You are the language model " .. context.model_name
    .. " operating inside of the agentic harness 'Artist.'"
end)

-- Built-ins use the same registration and execution path as user tools.
for _, definition in ipairs(artist.native.definitions()) do
  local name = definition.name
  artist.register_tool({
    name = name,
    description = definition.description,
    parameters = definition.parameters,
    execute = function(args, ctx)
      return artist.native.execute(name, args, ctx.cwd)
    end,
  })
end
