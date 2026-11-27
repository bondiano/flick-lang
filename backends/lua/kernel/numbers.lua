-- The Lua primitive kernel: `Float` beyond Lua's operators, and numbers
-- read from text as JS's `Number` reads them.
local _, ns = ...
local K = ns.k

local floor, frexp, sqrt = math.floor, math.frexp, math.sqrt
local byte, find, match, sub = string.byte, string.find, string.match, string.sub
local ipairs, tonumber, type = ipairs, tonumber, type

-- fdlibm's constants for `log`: ln 2 in two parts, and the coefficients of
-- its polynomial.
local LN2_HI, LN2_LO = 6.93147180369123816490e-01, 1.90821492927058770002e-10
local LG1, LG2, LG3, LG4 = 6.666666666666735130e-01, 3.999999999940941908e-01, 2.857142874366239149e-01, 2.222219843214978396e-01
local LG5, LG6, LG7 = 1.818357216161805012e-01, 1.531383769920937332e-01, 1.479819860511658591e-01

-- `Math.log`: the natural logarithm as fdlibm's `__ieee754_log` (e_log.c)
-- computes it, which is what JS gives. `frexp` stands for its reads of the
-- high word: `hx`, the top 20 bits of the significand, and `k`, the exponent.
local function log(x)
  if x ~= x or x == 1 / 0 then
    return x
  elseif x < 0 then
    return 0 / 0
  elseif x == 0 then
    return -1 / 0
  end
  local m, e = frexp(x)
  local hx = floor((2 * m - 1) * 1048576)
  local k = e - 1
  -- x, or x / 2, into [sqrt(2)/2, sqrt(2))
  if hx >= 0x6a09c then
    x, k = m, k + 1
  else
    x = 2 * m
  end
  local f = x - 1
  local dk = k
  if hx == 0 or hx >= 0xffffe then
    -- |f| < 2^-20
    if f == 0 then
      if k == 0 then
        return 0
      end
      return dk * LN2_HI + dk * LN2_LO
    end
    local r = f * f * (0.5 - 0.33333333333333333 * f)
    if k == 0 then
      return f - r
    end
    return dk * LN2_HI - ((r - dk * LN2_LO) - f)
  end
  local s = f / (2 + f)
  local z = s * s
  local w = z * z
  local t1 = w * (LG2 + w * (LG4 + w * LG6))
  local t2 = z * (LG1 + w * (LG3 + w * (LG5 + w * LG7)))
  local r = t2 + t1
  if hx >= 0x6147a and hx <= 0x6b851 then
    local hfsq = 0.5 * f * f
    if k == 0 then
      return f - (hfsq - s * (hfsq + r))
    end
    return dk * LN2_HI - ((hfsq - (s * (hfsq + r) + dk * LN2_LO)) - f)
  end
  if k == 0 then
    return f - s * (f - r)
  end
  return dk * LN2_HI - ((s * (f - r) - dk * LN2_LO) - f)
end

-- The JS globals of the kernel the standard library binds: `Math.floor`,
-- `Math.sqrt` and `Math.log`, IEEE 754 on doubles, `-0` and NaN kept;
-- `Number`, `BigInt` and `String` between numbers and `BigInt` values.
K.Math = { floor = floor, sqrt = sqrt, log = log }
-- an `Int` exactly; a `BigInt` value rounded to the nearest double
function K.Number(x)
  if type(x) == "table" then
    return K.big_to_number(x)
  end
  return x
end
-- `into : Int -> BigInt`, exactly, and `BigInt.to_string`: decimal, no suffix
function K.BigInt(x)
  return K.big_of_int(x)
end
function K.String(x)
  return K.big_decimal(x)
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
