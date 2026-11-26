-- The Lua primitive kernel: the fixed-width numbers but `Int`, and the
-- conversions between all of them, as docs/backends/numeric-policy.md says.
-- `Int8` to `UInt32` are doubles that hold them; `Float32` a double that
-- holds a binary32, rounded after each operation; `Int64` and `UInt64` are
-- `BigInt` values in their range (`bigint.lua`).
local _, ns = ...
local K = ns.k

local floor, frexp = math.floor, math.frexp
local type = type

local TWO32 = 4294967296

-- Wrapping widths up to 32 bits

-- `x`, an integer below 2^53 in magnitude, modulo 2^bits, signed or not.
local function wrapper(bits, signed)
  local modulus, half = 2 ^ bits, 2 ^ (bits - 1)
  if signed then
    return function(x)
      return (x + half) % modulus - half
    end
  end
  return function(x)
    return x % modulus
  end
end

local function rem(a, b)
  if b == 0 then
    return 0
  end
  local r = math.fmod(a, b)
  if r == 0 then
    return 0
  end
  return r
end

-- The arithmetic of one width: `add_u8`, `div_i16`, …
local function width(name, bits, signed)
  local wrap = wrapper(bits, signed)
  local mul
  if bits <= 16 then
    mul = function(a, b)
      return wrap(a * b)
    end
  else
    mul = function(a, b)
      return wrap(K.mul32(a, b))
    end
  end
  K["wrap_" .. name] = wrap
  K["add_" .. name] = function(a, b)
    return wrap(a + b)
  end
  K["sub_" .. name] = function(a, b)
    return wrap(a - b)
  end
  K["mul_" .. name] = mul
  K["neg_" .. name] = function(a)
    return wrap(-a)
  end
  K["div_" .. name] = function(a, b)
    if b == 0 then
      return 0
    end
    local q = a / b
    return wrap(q >= 0 and floor(q) or -floor(-q))
  end
  K["rem_" .. name] = rem
  K["pow_" .. name] = function(a, e)
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
        r = mul(r, a)
      end
      a = mul(a, a)
      e = floor(e / 2)
    end
    return r
  end
end

width("i8", 8, true)
width("i16", 16, true)
width("u8", 8, false)
width("u16", 16, false)
width("u32", 32, false)

-- Float32

-- `x` rounded to the nearest binary32, ties to even, subnormals and
-- overflow to infinity included: its mantissa cut to the bits binary32
-- keeps at its exponent. Every step is exact on doubles.
local function fround(x)
  if x ~= x or x == 0 or x == 1 / 0 or x == -1 / 0 then
    return x
  end
  local sign = 1
  if x < 0 then
    sign, x = -1, -x
  end
  local m, e = frexp(x)
  local bits = 24
  if e < -125 then
    bits = 24 - (-125 - e)
  end
  local scaled = m * 2 ^ bits
  local r = floor(scaled)
  local frac = scaled - r
  if frac > 0.5 or (frac == 0.5 and r % 2 == 1) then
    r = r + 1
  end
  local value = r * 2 ^ (e - bits)
  if value >= 2 ^ 128 then
    value = 1 / 0
  end
  return sign * value
end
K.fround = fround

function K.add_f32(a, b)
  return fround(a + b)
end
function K.sub_f32(a, b)
  return fround(a - b)
end
function K.mul_f32(a, b)
  return fround(a * b)
end
function K.div_f32(a, b)
  return fround(a / b)
end
function K.rem_f32(a, b)
  return fround(math.fmod(a, b))
end
function K.pow_f32(a, b)
  return fround(K.fpow(a, b))
end

-- Int64 and UInt64: `BigInt` arithmetic wrapped to 64 bits

local function wide(name, signed)
  local function wrap(x)
    return K.big_wrap(x, 64, signed)
  end
  K["add_" .. name] = function(a, b)
    return wrap(K.big_add(a, b))
  end
  K["sub_" .. name] = function(a, b)
    return wrap(K.big_sub(a, b))
  end
  K["mul_" .. name] = function(a, b)
    return wrap(K.big_mul(a, b))
  end
  K["neg_" .. name] = function(a)
    return wrap(K.big_neg(a))
  end
  K["div_" .. name] = function(a, b)
    return wrap(K.big_div(a, b))
  end
  K["rem_" .. name] = function(a, b)
    return wrap(K.big_rem(a, b))
  end
  -- wrapping binary exponentiation; a negative signed power truncates
  K["pow_" .. name] = function(a, e)
    local one = K.big_of_int(1)
    if e.sign < 0 then
      if K.equals(a, one) then
        return one
      elseif K.equals(a, K.big_of_int(-1)) then
        local _, r = K.big_divmod(e, K.big_of_int(2))
        return r.sign == 0 and one or K.big_of_int(-1)
      end
      return K.big_zero
    end
    local r, base, two = one, a, K.big_of_int(2)
    while e.sign > 0 do
      local q, bit = K.big_divmod(e, two)
      if bit.sign ~= 0 then
        r = wrap(K.big_mul(r, base))
      end
      base = wrap(K.big_mul(base, base))
      e = q
    end
    return r
  end
end

wide("i64", true)
wide("u64", false)

-- Bits

-- On `Int`: 32-bit two's complement, the count masked to 5 bits.
function K.bitAnd(a, b)
  return K.wrap32(K.band(a, b))
end
function K.bitOr(a, b)
  return K.wrap32(K.bor(a, b))
end
function K.bitXor(a, b)
  return K.wrap32(K.bxor(a, b))
end
function K.bitNot(a)
  return -1 - a
end
function K.shiftLeft(a, n)
  return K.wrap32((a % TWO32) * 2 ^ (n % 32))
end
function K.shiftRight(a, n)
  return floor(a / 2 ^ (n % 32))
end
function K.shiftRightUnsignedInt(a, n)
  return K.wrap32(K.shr(a, n % 32))
end

-- On `Int64`: its two words, the count masked to 6 bits.
local function on_words(f)
  return function(a, b)
    local alo, ahi = K.big_words(a)
    local blo, bhi = K.big_words(b)
    return K.big_of_words(f(alo, blo), f(ahi, bhi), true)
  end
end
K.int64BitAnd = on_words(K.band)
K.int64BitOr = on_words(K.bor)
K.int64BitXor = on_words(K.bxor)
function K.int64BitNot(a)
  return K.big_sub(K.big_of_int(-1), a)
end
function K.int64ShiftLeft(a, n)
  local power = K.big_power_of_two(n % 64)
  return K.big_wrap(K.big_mul(a, power), 64, true)
end
-- floor division by 2^n: toward negative infinity
local function floor_div(a, power)
  local q, r = K.big_divmod(a, power)
  if r.sign < 0 then
    q = K.big_sub(q, K.big_of_int(1))
  end
  return q
end
function K.int64ShiftRight(a, n)
  return floor_div(a, K.big_power_of_two(n % 64))
end
function K.int64ShiftRightUnsigned(a, n)
  local u = K.big_wrap(a, 64, false)
  return K.big_wrap(floor_div(u, K.big_power_of_two(n % 64)), 64, true)
end

-- Conversions, of any number: a double or a `BigInt` value

-- Whether `x` is a `BigInt` value, which `Int64` and `UInt64` are too.
local function big(x)
  return type(x) == "table"
end

local function wrapping(bits, signed)
  local wrap = wrapper(bits, signed)
  return function(x)
    if big(x) then
      return K.big_to_number(K.big_wrap(x, bits, signed))
    end
    return wrap(x)
  end
end

K.toInt8 = wrapping(8, true)
K.toInt16 = wrapping(16, true)
K.toInt32 = wrapping(32, true)
K.toUInt8 = wrapping(8, false)
K.toUInt16 = wrapping(16, false)
K.toUInt32 = wrapping(32, false)

function K.toInt64(x)
  return K.big_wrap(big(x) and x or K.big_of_int(x), 64, true)
end
function K.toUInt64(x)
  return K.big_wrap(big(x) and x or K.big_of_int(x), 64, false)
end
function K.toBigInt(x)
  return big(x) and x or K.big_of_int(x)
end
-- rounded once to binary32, a `BigInt` value from its integer
function K.toFloat32(x)
  return big(x) and K.big_to_float32(x) or fround(x)
end
function K.toFloat64(x)
  return big(x) and K.big_to_number(x) or x
end

-- `Int.truncate`: the whole part modulo 2^32; NaN and the infinities are 0.
function K.intTruncate(x)
  if x ~= x or x == 1 / 0 or x == -1 / 0 then
    return 0
  end
  local whole = x >= 0 and floor(x) or -floor(-x)
  -- the low 32 bits of a whole double, which may be far beyond 2^53
  return K.wrap32(whole % TWO32)
end

-- `BigInt.from_number`.
function K.bigintFromNumber(x)
  local value = K.big_of_double(x)
  return value and K.some(value) or K.none
end

-- `show` of `Int64` and `UInt64`: decimal, no suffix.
function K.showInt64(x)
  return K.big_decimal(x)
end
K.showUInt64 = K.showInt64
