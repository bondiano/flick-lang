-- Prints every global path a host defines, two levels below `_G`, sorted:
-- the API snapshot of an executable host. `arg` and the module caches of
-- `package` are left out; they repeat what the globals already name.

local skip = { _G = true, arg = true, package = { loaded = true, preload = true } }
local names = {}

local function walk(t, prefix, depth, skips)
  for k, v in pairs(t) do
    if type(k) == "string" and k:match("^[%a_][%w_]*$") and not (skips and skips[k] == true) then
      local name = prefix .. k
      names[#names + 1] = name
      if type(v) == "table" and depth < 2 then
        local nested = skips and type(skips[k]) == "table" and skips[k] or nil
        walk(v, name .. ".", depth + 1, nested)
      end
    end
  end
end

walk(_G, "", 0, skip)
table.sort(names)
for _, name in ipairs(names) do print(name) end
