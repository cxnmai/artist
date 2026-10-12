-- ACP reserves stdout for protocol traffic, including while loading config.
io.stdout = io.stderr
io.output(io.stderr)
io.write = function(...) return io.stderr:write(...) end
print = function(...)
  local parts = {}
  for i = 1, select("#", ...) do
    parts[i] = tostring(select(i, ...))
  end
  io.stderr:write(table.concat(parts, "\t"), "\n")
end
