-- The Lua primitive kernel: `Float` beyond Lua's operators, and numbers
-- read from text as JS's `Number` reads them.
local _, ns = ...
local K = ns.k

local floor, sqrt = math.floor, math.sqrt
local byte, find, match, sub = string.byte, string.find, string.match, string.sub
local ipairs, tonumber = ipairs, tonumber

-- The JS globals `Math.floor`, `Math.sqrt` and `Number` the standard
-- library binds: IEEE 754 on doubles, `-0` and NaN kept; an `Int` is a
-- double already.
K.Math = { floor = floor, sqrt = sqrt }
function K.Number(x)
  return x
end

-- `x % y` of `Float`: the dividend's sign, NaN for a zero divisor or an
-- infinite dividend, as C's `fmod` and JS's `%` compute it.
K.fmod = math.fmod

-- `x ** y` of `Float`: C's `pow` but where JS's `Math.pow` differs, a NaN
-- exponent and `±1 ** ±Infinity`, which are NaN.
function K.fpow(x, y)
  if y ~= y or ((x == 1 or x == -1) and (y == 1 / 0 or y == -1 / 0)) then
    return 0 / 0
  end
  return x ^ y
end

-- The JS white space `Number` trims: ASCII, then the Unicode spaces in
-- UTF-8.
local SPACES = {
  "\194\160", -- U+00A0
  "\225\154\128", -- U+1680
  "\226\128\168", -- U+2028
  "\226\128\169", -- U+2029
  "\226\128\175", -- U+202F
  "\226\129\159", -- U+205F
  "\227\128\128", -- U+3000
  "\239\187\191", -- U+FEFF
}

-- The length of the white space at byte `i` of `s`, or 0.
local function space(s, i)
  local b = byte(s, i)
  if b == 32 or (b and b >= 9 and b <= 13) then
    return 1
  elseif b == 0xE2 and byte(s, i + 1) == 0x80 then
    local c = byte(s, i + 2)
    if c and c >= 0x80 and c <= 0x8A then -- U+2000 to U+200A
      return 3
    end
  end
  for _, w in ipairs(SPACES) do
    if sub(s, i, i + #w - 1) == w then
      return #w
    end
  end
  return 0
end

-- `s` without the JS white space at both ends.
local function trim(s)
  local i, j = 1, #s
  while i <= j do
    local n = space(s, i)
    if n == 0 then
      break
    end
    i = i + n
  end
  -- from the end: a space is one byte or ends a sequence of two or three
  while j >= i do
    local found = false
    for n = 1, 3 do
      if j - n + 1 >= i and space(s, j - n + 1) == n then
        j, found = j - n, true
        break
      end
    end
    if not found then
      break
    end
  end
  return sub(s, i, j)
end
K.trim = trim

local DIGITS = { x = 16, X = 16, o = 8, O = 8, b = 2, B = 2 }

-- The double nearest the digits of `text` in `base`, a power of two, ties
-- to even: their bits, the first 53 kept and the rest rounding them.
local function radix(digits, base)
  local per = base == 16 and 4 or base == 8 and 3 or 1
  local bits = {}
  for i = 1, #digits do
    local d = tonumber(sub(digits, i, i), base)
    for k = per - 1, 0, -1 do
      bits[#bits + 1] = floor(d / 2 ^ k) % 2
    end
  end
  local first = 1
  while bits[first] == 0 do
    first = first + 1
  end
  local n = #bits - first + 1
  if n <= 0 then
    return 0
  end
  local value = 0
  local kept = n < 53 and n or 53
  for i = first, first + kept - 1 do
    value = value * 2 + bits[i]
  end
  if n > 53 then
    local half, rest = bits[first + 53], false
    for i = first + 54, #bits do
      if bits[i] == 1 then
        rest = true
        break
      end
    end
    if half == 1 and (rest or value % 2 == 1) then
      value = value + 1
    end
    value = value * 2 ^ (n - 53)
  end
  return value
end

-- Whether `text` is a decimal literal of JS: a sign, then `Infinity` or
-- digits with a point and an exponent, either side of the point possibly
-- empty but not both.
local function decimal(text)
  local body = match(text, "^[+-]?(.*)$")
  if body == "Infinity" then
    return true
  end
  local mantissa, exponent = match(body, "^([0-9.]*)([eE]?[+-]?[0-9]*)$")
  if not mantissa or mantissa == "" or mantissa == "." then
    return false
  elseif find(mantissa, "%..*%.") then
    return false
  elseif exponent ~= "" and not find(exponent, "^[eE][+-]?[0-9]+$") then
    return false
  end
  return true
end

-- `String.to_number`: the number `s` spells in JS syntax after its white
-- space; blank and malformed text are `None`, `NaN` is `Some NaN`.
function K.stringToNumber(s)
  local text = trim(s)
  if text == "" then
    return K.none
  elseif text == "NaN" then
    return K.some(0 / 0)
  end
  local prefix, digits = match(text, "^0([xXoObB])(.+)$")
  if prefix then
    local base = DIGITS[prefix]
    local valid = base == 16 and "^[0-9a-fA-F]+$" or base == 8 and "^[0-7]+$" or "^[01]+$"
    if not find(digits, valid) then
      return K.none
    end
    return K.some(radix(digits, base))
  elseif not decimal(text) then
    return K.none
  end
  local body = match(text, "^[+-]?(.*)$")
  if body == "Infinity" then
    return K.some(sub(text, 1, 1) == "-" and -1 / 0 or 1 / 0)
  end
  -- C's `strtod` rounds the decimal digits once, as JS does
  return K.some(tonumber(text))
end
