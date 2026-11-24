-- Compile-time limits of the host VM: each line compiles a chunk at a limit
-- or one past it. Lua 5.1 raises a constant overflow out of loadstring.

local function try(label, n, source)
  local ok, chunk, err = pcall(loadstring, source, "=limit")
  local outcome = ok and (chunk and "ok" or err) or chunk
  print(label, n, (tostring(outcome):gsub("^limit:%d+: ", "")))
end

local function names(n)
  local t = {}
  for i = 1, n do t[i] = "a" .. i end
  return table.concat(t, ",")
end

local function locals(n) return "local " .. names(n) end

local function upvalues(n)
  return "local " .. names(n) .. " return function() return " .. names(n) .. " end"
end

-- fractional numbers, so no constant is an immediate operand
local function constants(n)
  local t = {"local x"}
  for i = 1, n do t[#t + 1] = "x=" .. i .. ".5" end
  return table.concat(t, " ")
end

-- a call of n arguments after one local: n + 2 or n + 3 frame slots
local function arguments(n)
  local t = {}
  for i = 1, n do t[i] = "1" end
  return "local f = print f(" .. table.concat(t, ",") .. ")"
end

for _, n in ipairs({200, 201}) do try("locals", n, locals(n)) end
for _, n in ipairs({60, 61}) do try("upvalues", n, upvalues(n)) end
for _, n in ipairs({65536, 65537, 262143, 262144}) do try("constants", n, constants(n)) end
for _, n in ipairs({246, 247, 248}) do try("arguments", n, arguments(n)) end
