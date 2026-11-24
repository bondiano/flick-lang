-- Lua strings are byte strings; Flick indexes strings by UTF-16 code units.

-- UTF-16 code units of well-formed UTF-8: one per scalar, two above U+FFFF
local function units(s)
  local n = 0
  for i = 1, #s do
    local b = s:byte(i)
    if b < 0x80 or b >= 0xC0 then n = n + (b >= 0xF0 and 2 or 1) end
  end
  return n
end

local samples = {
  "hello",
  "h\195\169llo",
  "\208\159\209\128\208\184\208\178\208\181\209\130",
  "\240\157\132\158",
}
for _, s in ipairs(samples) do print(#s, units(s)) end

local accented = "h\195\169llo"
print(#accented:sub(2, 2), accented:byte(2))

local nul = "a\0b"
print(#nul, nul:byte(2), nul == "a", #(nul .. nul))
print(#string.format("%s", nul))
print(string.format("%q", nul))
