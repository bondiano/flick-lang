-- The Lua primitive kernel, first chunk: the table `K` every generated
-- chunk reads as `ns.k`, `Int` arithmetic, the 32-bit bit operations, and
-- the hooks of the built-in classes on primitive values.
--
-- Portable Lua 5.1: no `bit` library, no `math.huge`, only the globals
-- every profile's API snapshot defines. A Flick `Int` is a double holding
-- a 32-bit integer, never `-0`; `Unit` is `true`. The constructors of
-- `Option` and `Ordering` come from the generated chunk `lang.lua`, which
-- loads after the kernel and before any module.
local _, ns = ...
local K = {}
ns.k = K

local floor, fmod, frexp = math.floor, math.fmod, math.frexp
local format, sub, gsub, match, rep = string.format, string.sub, string.gsub, string.match, string.rep
local concat = table.concat
local pairs, tonumber, tostring, type, error = pairs, tonumber, tostring, type, error

local TWO30, TWO31, TWO32 = 1073741824, 2147483648, 4294967296

-- Int

-- `x` modulo 2^32 as a signed 32-bit integer: `x` is an integer below 2^62
-- in magnitude, so the floored `%` is exact, and `-0` becomes `0`.
local function wrap32(x)
  return (x + TWO31) % TWO32 - TWO31
end
K.wrap32 = wrap32

-- The low 32 bits of a product: of halves of 16 bits, every partial
-- product stays below 2^53.
local function mul32(a, b)
  a, b = a % TWO32, b % TWO32
  local al, bl = a % 65536, b % 65536
  local ah, bh = (a - al) / 65536, (b - bl) / 65536
  return wrap32(((ah * bl + al * bh) % 65536) * 65536 + al * bl)
end
K.mul32 = mul32

function K.add32(a, b)
  return wrap32(a + b)
end

function K.sub32(a, b)
  return wrap32(a - b)
end

function K.neg32(a)
  return wrap32(-a)
end

-- Truncates toward zero; a zero divisor gives 0, `min / -1` wraps to `min`.
function K.div32(a, b)
  if b == 0 then
    return 0
  end
  local q = a / b
  if q >= 0 then
    q = floor(q)
  else
    q = -floor(-q)
  end
  return wrap32(q)
end

-- The dividend's sign; a zero divisor gives 0. `fmod` is exact here.
function K.rem32(a, b)
  if b == 0 then
    return 0
  end
  local r = fmod(a, b)
  if r == 0 then
    return 0
  end
  return r
end

-- Wrapping binary exponentiation; a negative power truncates.
function K.pow32(a, e)
  if e < 0 then
    if a == 1 then
      return 1
    elseif a == -1 then
      return e % 2 == 0 and 1 or -1
    end
    return 0
  end
  local r = 1
  while e > 0 do
    if e % 2 == 1 then
      r = mul32(r, a)
    end
    a = mul32(a, a)
    e = floor(e / 2)
  end
  return r
end

-- Bits of 32-bit words, without a `bit` library: tables of the operations
-- on two 4-bit numbers, indexed by `16 * a + b`, walked a nibble at a time.

local AND, XOR = {}, {}
for a = 0, 15 do
  for b = 0, 15 do
    local x, y, both, either, p = a, b, 0, 0, 1
    for _ = 1, 4 do
      local i, j = x % 2, y % 2
      if i == 1 and j == 1 then
        both = both + p
      end
      if i ~= j then
        either = either + p
      end
      x, y, p = (x - i) / 2, (y - j) / 2, p * 2
    end
    AND[a * 16 + b], XOR[a * 16 + b] = both, either
  end
end

-- The operation `t` on the words of `a` and `b`, any integers: unsigned.
local function bitwise(t, a, b)
  a, b = a % TWO32, b % TWO32
  local r, p = 0, 1
  for _ = 1, 8 do
    local x, y = a % 16, b % 16
    r = r + t[x * 16 + y] * p
    a, b, p = (a - x) / 16, (b - y) / 16, p * 16
  end
  return r
end

local function band(a, b)
  return bitwise(AND, a, b)
end

local function bxor(a, b)
  return bitwise(XOR, a, b)
end

-- `a >>> n`, `0 <= n < 32`.
local function shr(a, n)
  return floor((a % TWO32) / 2 ^ n)
end

K.band, K.bxor, K.shr = band, bxor, shr

-- Hooks

-- `==`: structural. SameValueZero on numbers; strings by their bytes,
-- which in WTF-8 are their code units; a list, tuple, record or tagged
-- value by what it holds, but for the dictionaries a tagged value holds at
-- `$1`, …, as the JS runtime skips them; a function by its identity.
local function equals(a, b)
  if a == b or (a ~= a and b ~= b) then
    return true
  elseif type(a) ~= "table" or type(b) ~= "table" then
    return false
  end
  for key, value in pairs(a) do
    if type(key) ~= "string" or sub(key, 1, 1) ~= "$" then
      local other = b[key]
      if other == nil or not equals(value, other) then
        return false
      end
    end
  end
  for key in pairs(b) do
    if a[key] == nil then
      return false
    end
  end
  return true
end
K.equals = equals

-- Total: NaN above every number, `-0` equal to `0`, `false` before `true`,
-- strings by UTF-16 code units.
function K.compare(a, b)
  local t = type(a)
  if t == "string" then
    local c = K.compare_strings(a, b)
    return c < 0 and K.LT or c > 0 and K.GT or K.EQ
  elseif t == "boolean" then
    if a == b then
      return K.EQ
    end
    return a and K.GT or K.LT
  end
  if a < b then
    return K.LT
  elseif a > b then
    return K.GT
  elseif a == b then
    return K.EQ
  elseif a ~= a then
    return b ~= b and K.EQ or K.GT
  end
  return K.LT
end

-- `-0`, computed as the chunk runs: a constant `-0` of Lua 5.1 would be one
-- with any `0` of the same function, so generated code reads this one.
local zero = 0
K.negzero = -zero

-- The hash of the JS and Go runtimes, bit for bit (`runtime/src/rt/hash.ts`).

local function optimize(n)
  local u = n % TWO32
  local high = u >= TWO31
  if floor(u / TWO30) % 2 == 1 then
    u = u - TWO30
  end
  if high then
    u = u + TWO30
  end
  return wrap32(u)
end

-- The 32-bit MurmurHash3 finalizer.
local function mix(h)
  local x = bxor(h, shr(h, 16))
  x = mul32(x, 0x85ebca6b)
  x = bxor(x, shr(x, 13))
  x = mul32(x, 0xc2b2ae35)
  return wrap32(bxor(x, shr(x, 16)))
end

local function combine(a, b)
  return mix(mul32(a, 0x9e3779b1) + mul32(b, 0x85ebca6b))
end
K.combine = combine

-- The high and low words of the IEEE 754 binary64 bits of `x`, NaN as
-- the canonical quiet NaN.
local function words(x)
  if x ~= x then
    return 0x7ff80000, 0
  end
  local sign = 0
  if x < 0 or 1 / x < 0 then
    sign, x = TWO31, -x
  end
  if x == 1 / 0 then
    return sign + 0x7ff00000, 0
  end
  local m, e = frexp(x)
  local biased, fraction = e + 1022, nil
  if biased <= 0 then
    -- subnormal: x is `fraction * 2^-1074`, scaled in two exact steps
    biased, fraction = 0, x * 2 ^ 1022 * 2 ^ 52
  else
    fraction = (m * 2 - 1) * 2 ^ 52
  end
  local low = fraction % TWO32
  return sign + biased * 1048576 + (fraction - low) / TWO32, low
end

local function hash_number(n)
  if n == floor(n) and n >= -TWO31 and n < TWO31 then
    return optimize(n)
  end
  local high, low = words(n)
  return optimize(combine(high, low))
end

-- djb2 over the UTF-16 code units, from the last.
local function hash_string(s)
  local h = 5381
  local units = K.units(s)
  for i = #units, 1, -1 do
    h = wrap32(bxor(h * 33, units[i]))
  end
  return optimize(h)
end
K.hash_string = hash_string

-- A hash consistent with `equals`.
function K.hash(a)
  local t = type(a)
  if t == "number" then
    return hash_number(a)
  elseif t == "string" then
    return hash_string(a)
  elseif t == "boolean" then
    return optimize(mix(hash_string(tostring(a))))
  end
  error("the Lua kernel hashes primitive values only")
end

-- `show` of a number: the shortest digits that read back as it, written
-- as JS writes a number, keeping `-0`.

-- The decimal digits of `digits` plus `step` (1 or -1) in the last place;
-- `nil` when that changes their count.
local function step_digits(digits, step)
  local n = #digits
  local out = {}
  local carry = step
  for i = n, 1, -1 do
    local d = tonumber(sub(digits, i, i)) + carry
    carry = 0
    if d > 9 then
      d, carry = 0, 1
    elseif d < 0 then
      d, carry = 9, -1
    end
    out[i] = d
  end
  local text = concat(out)
  if carry ~= 0 or sub(text, 1, 1) == "0" then
    return nil
  end
  return text
end

-- The shortest digits `s` and the exponent `n` of a finite positive `x`:
-- `x` is what `0.s * 10^n` reads back as. Of the candidates with the
-- fewest digits, the nearest is the correctly rounded one when it reads
-- back; at a power of two the interval that reads back is uneven, so a
-- neighbour of it may read back where it does not.
local function shortest(x)
  for p = 1, 17 do
    local text = format("%." .. (p - 1) .. "e", x)
    local lead, rest, exponent = match(text, "^(%d)%.?(%d*)e([-+]%d+)$")
    local digits = lead .. rest
    local n = tonumber(exponent) + 1
    local found = tonumber(text) == x and digits or nil
    if not found then
      for step = 1, -1, -2 do
        local other = step_digits(digits, step)
        if other and tonumber("0." .. other .. "e" .. n) == x then
          found = other
        end
      end
    end
    if found then
      return (gsub(found, "0+$", "")), n
    end
  end
  error("no shortest digits")
end

local function show_number(x)
  if x ~= x then
    return "NaN"
  elseif x == 1 / 0 then
    return "Infinity"
  elseif x == -1 / 0 then
    return "-Infinity"
  elseif x == 0 then
    return 1 / x < 0 and "-0" or "0"
  elseif x < 0 then
    return "-" .. show_number(-x)
  end
  local s, n = shortest(x)
  local k = #s
  if k <= n and n <= 21 then
    return s .. rep("0", n - k)
  elseif 0 < n and n <= 21 then
    return sub(s, 1, n) .. "." .. sub(s, n + 1)
  elseif -6 < n and n <= 0 then
    return "0." .. rep("0", -n) .. s
  end
  local e = n - 1
  local sign = e < 0 and "-" or "+"
  e = e < 0 and -e or e
  if k == 1 then
    return s .. "e" .. sign .. e
  end
  return sub(s, 1, 1) .. "." .. sub(s, 2) .. "e" .. sign .. e
end
K.show_number = show_number

-- A primitive value as a JS literal: a string quoted as `JSON.stringify`
-- quotes it.
function K.show(a)
  local t = type(a)
  if t == "number" then
    return show_number(a)
  elseif t == "string" then
    return K.quote(a)
  elseif t == "boolean" then
    return a and "true" or "false"
  end
  error("the Lua kernel shows primitive values only")
end

-- `Int.to_string` and `Float.to_string`: as `show` writes a number.
K.stringFromNumber = show_number
