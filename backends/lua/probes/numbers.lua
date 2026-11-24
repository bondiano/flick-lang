-- Flick's Int wraps at 32 bits; a Lua 5.1 number is a double. The plain
-- operators diverge from Flick here, and helpers built on doubles recover
-- the exact results the numeric policy prescribes.

local function wrap(x)
  x = x % 4294967296
  if x >= 2147483648 then x = x - 4294967296 end
  return x
end

-- 16-bit halves keep every partial product below 2^53
local function mul(a, b)
  local high, low = math.floor(a / 65536), a % 65536
  return wrap((high * b) % 65536 * 65536 + low * b)
end

local function div(a, b)
  if b == 0 then return 0 end
  local q = a / b
  return wrap(q < 0 and math.ceil(q) or math.floor(q))
end

local function rem(a, b)
  if b == 0 then return 0 end
  local r = math.fmod(a, b)
  return r == 0 and 0 or r
end

local function show(x) return string.format("%.0f", x) end

print("2^53", show(2^53))
print("2^53 + 1", show(2^53 + 1))
print("2^53 - 1 + 1 exact", (2^53 - 1) + 1 == 2^53)
print("max + 1", show(2147483647 + 1), show(wrap(2147483647 + 1)))
print("min - 1", show(wrap(-2147483648 - 1)))
print("65536 * 65536", show(65536 * 65536), show(mul(65536, 65536)))
print("max * max", show(2147483647 * 2147483647), show(wrap(2147483647 * 2147483647)),
  show(mul(2147483647, 2147483647)))
print("-7 / 2", -7 / 2, show(div(-7, 2)))
print("-7 % 2", -7 % 2, show(rem(-7, 2)))
print("7 % -2", 7 % -2, show(rem(7, -2)))
print("min / -1", show(-2147483648 / -1), show(div(-2147483648, -1)))
print("min % -1", show(rem(-2147483648, -1)), 1 / math.fmod(-2147483648, -1))
print("5 / 0", 5 / 0, show(div(5, 0)))
print("5 % 0", (5 % 0) ~= (5 % 0), show(rem(5, 0)))

-- a loop long enough for a tracing JIT to compile it
local sum = 0
for i = 1, 200000 do sum = wrap(sum + mul(i, i)) end
print("checksum", show(sum))
