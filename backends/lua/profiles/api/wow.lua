-- Prints the API snapshot of a WoW client: every function the client's
-- Blizzard_APIDocumentationGenerated documents, plus the Lua environment
-- names listed in `wow-lua.txt`, sorted and without duplicates.
--
-- usage: lua wow.lua wow-lua.txt <wow-ui-source>/Interface/AddOns/Blizzard_APIDocumentationGenerated/*.lua
-- The documentation files read enums and constants of the client; a
-- placeholder that absorbs every operation stands in for them.

local any = {}
local function itself() return any end
setmetatable(any, { __index = itself, __call = itself, __add = itself, __sub = itself,
  __mul = itself, __div = itself, __unm = itself, __concat = itself })
setmetatable(_G, { __index = itself })

local names = {}
APIDocumentation = {
  AddDocumentationTable = function(_, t)
    if t.Type ~= "System" then return end
    for _, f in ipairs(t.Functions or {}) do
      names[#names + 1] = t.Namespace and (t.Namespace .. "." .. f.Name) or f.Name
    end
  end,
}

for line in io.lines(arg[1]) do
  if line:match("^[%a_]") then names[#names + 1] = line end
end
for i = 2, #arg do assert(loadfile(arg[i]))() end

table.sort(names)
for i, name in ipairs(names) do
  if name ~= names[i - 1] then print(name) end
end
