-- Which boundaries a coroutine can yield across.

local function attempt(label, body)
  local co = coroutine.create(body)
  local results = {coroutine.resume(co)}
  for i = 1, #results do results[i] = (tostring(results[i]):gsub("^[^:]*:%d+: ", "")) end
  print(label, coroutine.status(co), table.concat(results, " "))
end

attempt("plain", function() coroutine.yield("yielded") end)
attempt("pcall", function() return pcall(coroutine.yield, "yielded") end)
attempt("metamethod", function()
  local t = setmetatable({}, {__index = function() return coroutine.yield("yielded") end})
  return t.x
end)
attempt("sort comparator", function()
  table.sort({3, 1, 2}, function(a, b) coroutine.yield("yielded"); return a < b end)
end)
attempt("error", function() error("inside", 0) end)

print(coroutine.wrap(function() return coroutine.running() ~= nil end)())
