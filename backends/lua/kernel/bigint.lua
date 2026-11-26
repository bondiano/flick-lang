-- The Lua primitive kernel: `BigInt`, and `Int64` and `UInt64`, which are
-- `BigInt` values within their range as on the JS target (`asIntN`,
-- `asUintN`): never a double, which would round them.
--
-- A value is `{ sign = -1 | 0 | 1, [1..n] = limbs }`, the limbs of its
-- magnitude in base 2^24 from the least, without a zero last limb: a
-- product of two limbs and a carry stay below 2^53, and two equal values
-- have equal tables, which `K.equals` compares.
local _, ns = ...
local K = ns.k

local floor, frexp = math.floor, math.frexp
local concat = table.concat
local format, rep, sub = string.format, string.rep, string.sub
local error, tonumber, tostring = error, tonumber, tostring

local BASE = 16777216 -- 2^24

local function make(sign, limbs)
  local n = #limbs
  while n > 0 and limbs[n] == 0 do
    limbs[n] = nil
    n = n - 1
  end
  limbs.sign = n == 0 and 0 or sign
  return limbs
end

-- `K.big(sign, limbs…)`: a literal the generated code writes.
function K.big(sign, ...)
  return make(sign, { ... })
end

-- The value of a whole double below 2^53 in magnitude.
local function of_number(n)
  local sign = n < 0 and -1 or 1
  n = n < 0 and -n or n
  local limbs = {}
  while n > 0 do
    local limb = n % BASE
    limbs[#limbs + 1] = limb
    n = (n - limb) / BASE
  end
  return make(sign, limbs)
end
K.big_of_int = of_number

local ZERO = make(0, {})

-- Magnitudes

local function compare_magnitude(a, b)
  if #a ~= #b then
    return #a < #b and -1 or 1
  end
  for i = #a, 1, -1 do
    if a[i] ~= b[i] then
      return a[i] < b[i] and -1 or 1
    end
  end
  return 0
end

local function add_magnitude(a, b)
  local out, carry = {}, 0
  for i = 1, (#a > #b and #a or #b) do
    local s = (a[i] or 0) + (b[i] or 0) + carry
    carry = s >= BASE and 1 or 0
    out[i] = s - carry * BASE
  end
  if carry > 0 then
    out[#out + 1] = carry
  end
  return out
end

-- `a - b` of magnitudes, `a >= b`.
local function sub_magnitude(a, b)
  local out, borrow = {}, 0
  for i = 1, #a do
    local d = a[i] - (b[i] or 0) - borrow
    borrow = d < 0 and 1 or 0
    out[i] = d + borrow * BASE
  end
  return out
end

local function mul_magnitude(a, b)
  local out = {}
  for i = 1, #a + #b do
    out[i] = 0
  end
  for i = 1, #a do
    local carry, ai = 0, a[i]
    for j = 1, #b do
      local t = out[i + j - 1] + ai * b[j] + carry
      carry = floor(t / BASE)
      out[i + j - 1] = t - carry * BASE
    end
    local k = i + #b
    while carry > 0 do
      local t = out[k] + carry
      carry = floor(t / BASE)
      out[k] = t - carry * BASE
      k = k + 1
    end
  end
  return out
end

-- The quotient and remainder of a magnitude by one limb `d > 0`.
local function divmod_small(a, d)
  local q, r = {}, 0
  for i = #a, 1, -1 do
    local t = r * BASE + a[i]
    q[i] = floor(t / d)
    r = t - q[i] * d
  end
  return q, r
end

-- The quotient and remainder of magnitudes, `#b >= 2`: Knuth's algorithm D,
-- the divisor shifted until its last limb has its top bit.
local function divmod_magnitude(a, b)
  if compare_magnitude(a, b) < 0 then
    local r = {}
    for i = 1, #a do
      r[i] = a[i]
    end
    return {}, r
  end
  local shift = 1
  while b[#b] * shift < BASE / 2 do
    shift = shift * 2
  end
  local u = mul_magnitude(a, { shift })
  local v = mul_magnitude(b, { shift })
  while #v > 0 and v[#v] == 0 do
    v[#v] = nil
  end
  local n, m = #v, #u - #v
  u[#u + 1] = 0
  local q = {}
  local top, next_top = v[n], v[n - 1]
  for j = m, 0, -1 do
    local numerator = u[j + n + 1] * BASE + u[j + n]
    local qhat = floor(numerator / top)
    local rhat = numerator - qhat * top
    while qhat >= BASE or qhat * next_top > rhat * BASE + u[j + n - 1] do
      qhat = qhat - 1
      rhat = rhat + top
      if rhat >= BASE then
        break
      end
    end
    -- u[j .. j+n] -= qhat * v
    local borrow, carry = 0, 0
    for i = 1, n do
      local p = qhat * v[i] + carry
      carry = floor(p / BASE)
      local d = u[j + i] - (p - carry * BASE) - borrow
      borrow = d < 0 and 1 or 0
      u[j + i] = d + borrow * BASE
    end
    local d = u[j + n + 1] - carry - borrow
    borrow = d < 0 and 1 or 0
    u[j + n + 1] = d + borrow * BASE
    if borrow > 0 then
      -- one too many: add v back
      qhat = qhat - 1
      local c = 0
      for i = 1, n do
        local s = u[j + i] + v[i] + c
        c = s >= BASE and 1 or 0
        u[j + i] = s - c * BASE
      end
      u[j + n + 1] = (u[j + n + 1] + c) % BASE
    end
    q[j + 1] = qhat
  end
  local r = {}
  for i = 1, n do
    r[i] = u[i]
  end
  local remainder, _ = divmod_small(r, shift)
  return q, remainder
end

local function divmod(a, b)
  if #b == 1 then
    local q, r = divmod_small(a, b[1])
    return q, { r }
  end
  return divmod_magnitude(a, b)
end

-- Signed arithmetic

local function negate(a)
  local out = {}
  for i = 1, #a do
    out[i] = a[i]
  end
  return make(-a.sign, out)
end
K.big_neg = negate

local function add(a, b)
  if a.sign == 0 then
    return b
  elseif b.sign == 0 then
    return a
  elseif a.sign == b.sign then
    return make(a.sign, add_magnitude(a, b))
  end
  local c = compare_magnitude(a, b)
  if c == 0 then
    return ZERO
  elseif c > 0 then
    return make(a.sign, sub_magnitude(a, b))
  end
  return make(b.sign, sub_magnitude(b, a))
end
K.big_add = add

local function subtract(a, b)
  return add(a, negate(b))
end
K.big_sub = subtract

local function multiply(a, b)
  if a.sign == 0 or b.sign == 0 then
    return ZERO
  end
  return make(a.sign * b.sign, mul_magnitude(a, b))
end
K.big_mul = multiply

-- Truncating division: toward zero, the remainder with the dividend's
-- sign; a zero divisor gives `0n` for both.
local function quotient_remainder(a, b)
  if b.sign == 0 or a.sign == 0 then
    return ZERO, ZERO
  end
  local q, r = divmod(a, b)
  return make(a.sign * b.sign, q), make(a.sign, r)
end
K.big_divmod = quotient_remainder

function K.big_div(a, b)
  return (quotient_remainder(a, b))
end

function K.big_rem(a, b)
  local _, r = quotient_remainder(a, b)
  return r
end

local function compare(a, b)
  if a.sign ~= b.sign then
    return a.sign < b.sign and -1 or 1
  end
  local c = compare_magnitude(a, b)
  -- not `c * sign`, which is `-0` for equal negative values
  if a.sign < 0 then
    return -c + 0
  end
  return c
end
K.big_compare = compare

-- `a ** e`, `e >= 0`; a negative exponent is a defect.
function K.big_pow(a, e)
  if e.sign < 0 then
    error("BigInt: a negative exponent", 0)
  end
  local result, base = make(1, { 1 }), a
  local bits = {}
  local digits = { sign = e.sign }
  for i = 1, #e do
    digits[i] = e[i]
  end
  while #digits > 0 do
    local q, r = divmod_small(digits, 2)
    bits[#bits + 1] = r
    digits = make(1, q)
  end
  for i = 1, #bits do
    if bits[i] == 1 then
      result = multiply(result, base)
    end
    if i < #bits then
      base = multiply(base, base)
    end
  end
  return result
end

-- `2 ^ k`, `k >= 0`, of its limbs at once.
local function power_of_two(k)
  local limbs, full = {}, floor(k / 24)
  for i = 1, full do
    limbs[i] = 0
  end
  limbs[full + 1] = 2 ^ (k - full * 24)
  return make(1, limbs)
end
K.big_power_of_two = power_of_two

-- Text

local CHUNK, CHUNK_DIGITS = 10000000, 7

-- The decimal digits of `a`, `-` before a negative one, no suffix.
local function decimal(a)
  if a.sign == 0 then
    return "0"
  end
  local parts, digits = {}, {}
  for i = 1, #a do
    digits[i] = a[i]
  end
  while #digits > 0 do
    local q, r = divmod_small(digits, CHUNK)
    parts[#parts + 1] = r
    local n = #q
    while n > 0 and q[n] == 0 do
      q[n] = nil
      n = n - 1
    end
    digits = q
  end
  local out = { a.sign < 0 and "-" or "", tostring(parts[#parts]) }
  for i = #parts - 1, 1, -1 do
    out[#out + 1] = format("%07d", parts[i])
  end
  return concat(out)
end
K.big_decimal = decimal

-- The value of decimal digits, `-` before a negative one.
function K.big_parse(text)
  local sign, digits = 1, text
  if sub(text, 1, 1) == "-" then
    sign, digits = -1, sub(text, 2)
  end
  local value = ZERO
  local scale = make(1, { CHUNK })
  local start = (#digits - 1) % CHUNK_DIGITS + 1
  local i = 1
  while i <= #digits do
    local stop = i == 1 and start or i + CHUNK_DIGITS - 1
    local piece = tonumber(sub(digits, i, stop))
    value = add(i == 1 and value or multiply(value, scale), of_number(piece))
    i = stop + 1
  end
  return sign < 0 and negate(value) or value
end

-- Doubles

-- The number of significant bits of a magnitude.
local function bit_length(a)
  if #a == 0 then
    return 0
  end
  local top, bits = a[#a], 0
  while top >= 1 do
    top = floor(top / 2)
    bits = bits + 1
  end
  return (#a - 1) * 24 + bits
end

-- The bit `k` of a magnitude, from 0.
local function bit(a, k)
  local limb = a[floor(k / 24) + 1] or 0
  return floor(limb / 2 ^ (k % 24)) % 2
end

-- The magnitude rounded to `precision` significant bits, ties to even, as
-- the mantissa and the exponent it multiplies.
local function rounded(a, precision)
  local n = bit_length(a)
  if n <= precision then
    local value = 0
    for i = #a, 1, -1 do
      value = value * BASE + a[i]
    end
    return value, 0
  end
  local drop = n - precision
  local mantissa = 0
  for k = n - 1, drop, -1 do
    mantissa = mantissa * 2 + bit(a, k)
  end
  local half, rest = bit(a, drop - 1) == 1, false
  for k = drop - 2, 0, -1 do
    if bit(a, k) == 1 then
      rest = true
      break
    end
  end
  if half and (rest or mantissa % 2 == 1) then
    mantissa = mantissa + 1
  end
  return mantissa, drop
end

-- `BigInt.to_number`: the nearest double, ties to even.
function K.big_to_number(a)
  local mantissa, exponent = rounded(a, 53)
  local value = mantissa * 2 ^ exponent
  return a.sign < 0 and -value or value
end

-- The nearest binary32 of a whole value, rounded once.
function K.big_to_float32(a)
  local mantissa, exponent = rounded(a, 24)
  local value = mantissa * 2 ^ exponent
  if value >= 2 ^ 128 then
    value = 1 / 0
  end
  return a.sign < 0 and -value or value
end

-- `BigInt.from_number`: a whole finite double exactly, `-0` as `0n`.
function K.big_of_double(x)
  if x ~= x or x == 1 / 0 or x == -1 / 0 or x ~= floor(x) then
    return nil
  end
  local sign = x < 0 and -1 or 1
  x = x < 0 and -x or x
  local m, e = frexp(x)
  if e <= 53 then
    return of_number(sign * x)
  end
  -- `x` is `m * 2^53`, a whole number, times `2^(e - 53)`
  local value = of_number(m * 2 ^ 53)
  local power = power_of_two(e - 53)
  value = multiply(value, power)
  return sign < 0 and negate(value) or value
end

-- Widths

-- `a` modulo 2^bits, read as signed or unsigned: the magnitude's low bits,
-- the complement of a negative value's.
function K.big_wrap(a, bits, signed)
  if a.sign == 0 then
    return a
  end
  local limbs, full = {}, floor(bits / 24)
  for i = 1, full do
    limbs[i] = a[i] or 0
  end
  local extra = bits - full * 24
  if extra > 0 then
    limbs[full + 1] = (a[full + 1] or 0) % 2 ^ extra
  end
  local low = make(1, limbs)
  local modulus = power_of_two(bits)
  if a.sign < 0 and low.sign ~= 0 then
    low = subtract(modulus, low)
  end
  if signed and compare(low, power_of_two(bits - 1)) >= 0 then
    low = subtract(low, modulus)
  end
  return low
end

-- `Int64` and `UInt64` bits: two words of 32 bits, low first.
function K.big_words(a)
  local u = K.big_wrap(a, 64, false)
  local low = (u[1] or 0) + (u[2] or 0) * BASE
  local lo = low % 4294967296
  local hi = (low - lo) / 4294967296 + (u[3] or 0) * 2 ^ 16
  return lo, hi
end

function K.big_of_words(lo, hi, signed)
  local value = add(multiply(of_number(hi), of_number(4294967296)), of_number(lo))
  return K.big_wrap(value, 64, signed)
end

K.big_zero = ZERO
