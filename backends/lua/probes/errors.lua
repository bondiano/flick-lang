-- Error values, positions and what the protected calls return.

local function strip(message)
  return (tostring(message):gsub("^[^:]*:%d+: ", ""))
end

print(pcall(error, "boom"))
local ok, message = pcall(function() error("boom") end)
print(ok, strip(message), message ~= "boom")
print(pcall(function() error("boom", 0) end))

-- a number message gains a position and becomes a string
local ok2, number = pcall(function() error(42) end)
print(ok2, type(number), strip(number))

local value = {}
local ok3, raised = pcall(error, value)
print(ok3, raised == value)
print(select("#", pcall(error)))
print(pcall(error))

print(xpcall(function() error("x", 0) end, function(e) return "handled " .. e end))
print(xpcall(function() return 1, 2 end, print))
print(xpcall(function() error("x", 0) end, function() error("y", 0) end))

local ok4, indexed = pcall(function() local x; return x.y end)
print(ok4, strip(indexed))
local ok5, compared = pcall(function() return {} < {} end)
print(ok5, strip(compared))
