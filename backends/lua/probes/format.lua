-- tostring spells numbers with "%.14g" (LUA_NUMBER_FMT), not the shortest
-- round-trip spelling of Flick's show; NaN and signed zero vary by host.

local function negative_zero()
  local z = 0
  z = -z
  return z
end

print(tostring(0.1))
print(tostring(1 / 3))
print(tostring(2^53))
print(tostring(123456789012))
print(tostring(1e15))
print(tostring(1e21))
print(tostring(2^63))
print(tostring(1 / 0), tostring(-1 / 0))
print(tostring(negative_zero()))
print(string.format("%.17g", 0.1))
print(tonumber("0x10"), tonumber("1e2"), tonumber(" 5 "), tonumber("5x"))

-- the spelling of a NaN is the C library's
print(tostring(0 / 0))

-- in Lua 5.1 a -0.0 literal shares the constant slot of an equal 0
local function literal()
  local zero = 0
  return -0.0, zero
end
print(1 / literal())
