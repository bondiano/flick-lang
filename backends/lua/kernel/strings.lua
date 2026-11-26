-- The Lua primitive kernel: strings. A Flick string is a Lua string in
-- WTF-8, UTF-8 that may also hold a lone surrogate as the three bytes
-- `ED A0..BF xx`; a surrogate pair is never stored as two halves, so equal
-- strings have equal bytes. Lengths and positions count UTF-16 code units,
-- as JS does. The functions follow `runtime-go/flickrt/string.go`.
local _, ns = ...
local K = ns.k

local byte, char, sub, find, format = string.byte, string.char, string.sub, string.find, string.format
local concat = table.concat
local floor = math.floor

-- Whether every byte is ASCII, so that a place in units is a place in bytes.
local function ascii(s)
  return not find(s, "[\128-\255]")
end

-- The code point at byte `i` and its length in bytes, surrogates included.
-- A malformed byte, which no Flick string holds, is U+FFFD of one byte.
local function decode(s, i)
  local b = byte(s, i)
  if b < 0x80 then
    return b, 1
  elseif b >= 0xF0 then
    local b2, b3, b4 = byte(s, i + 1, i + 3)
    if b4 then
      return (b - 0xF0) * 262144 + (b2 - 0x80) * 4096 + (b3 - 0x80) * 64 + (b4 - 0x80), 4
    end
  elseif b >= 0xE0 then
    local b2, b3 = byte(s, i + 1, i + 2)
    if b3 then
      return (b - 0xE0) * 4096 + (b2 - 0x80) * 64 + (b3 - 0x80), 3
    end
  elseif b >= 0xC0 then
    local b2 = byte(s, i + 1)
    if b2 then
      return (b - 0xC0) * 64 + (b2 - 0x80), 2
    end
  end
  return 0xFFFD, 1
end

-- `r` in WTF-8.
local function encode(r)
  if r < 0x80 then
    return char(r)
  elseif r < 0x800 then
    return char(0xC0 + floor(r / 64), 0x80 + r % 64)
  elseif r < 0x10000 then
    return char(0xE0 + floor(r / 4096), 0x80 + floor(r / 64) % 64, 0x80 + r % 64)
  end
  return char(
    0xF0 + floor(r / 262144),
    0x80 + floor(r / 4096) % 64,
    0x80 + floor(r / 64) % 64,
    0x80 + r % 64
  )
end
K.encode = encode

-- The UTF-16 code units of `s`, from 1.
local function units(s)
  local out, n, i, len = {}, 0, 1, #s
  while i <= len do
    local r, size = decode(s, i)
    i = i + size
    if r >= 0x10000 then
      r = r - 0x10000
      out[n + 1], out[n + 2] = 0xD800 + floor(r / 1024), 0xDC00 + r % 1024
      n = n + 2
    else
      n = n + 1
      out[n] = r
    end
  end
  return out
end
K.units = units

-- The string of the units `from..to` of `us`, pairs joined.
local function from_units(us, from, to)
  local out, n, i = {}, 0, from
  while i <= to do
    local u = us[i]
    local next_unit = us[i + 1]
    if u >= 0xD800 and u <= 0xDBFF and i < to and next_unit >= 0xDC00 and next_unit <= 0xDFFF then
      u = 0x10000 + (u - 0xD800) * 1024 + next_unit - 0xDC00
      i = i + 1
    end
    n = n + 1
    out[n] = encode(u)
    i = i + 1
  end
  return concat(out)
end

-- The number of UTF-16 code units: one for each lead byte, two for the
-- lead byte of a code point past the BMP.
local function length(s)
  if ascii(s) then
    return #s
  end
  local n = 0
  for i = 1, #s do
    local b = byte(s, i)
    if b < 0x80 or (b >= 0xC0 and b < 0xF0) then
      n = n + 1
    elseif b >= 0xF0 then
      n = n + 2
    end
  end
  return n
end
K.length = length

-- `++`: a high surrogate at the end of `a` and a low one at the start of
-- `b` become one pair.
function K.concat(a, b)
  local la = #a
  if la >= 3 and #b >= 3 and byte(a, la - 2) == 0xED and byte(b, 1) == 0xED then
    local hi = decode(a, la - 2)
    local lo = decode(b, 1)
    if hi >= 0xD800 and hi <= 0xDBFF and lo >= 0xDC00 and lo <= 0xDFFF then
      return sub(a, 1, la - 3) .. encode(0x10000 + (hi - 0xD800) * 1024 + lo - 0xDC00) .. sub(b, 4)
    end
  end
  return a .. b
end

-- The first UTF-16 unit of the code point `r`.
local function first_unit(r)
  if r >= 0x10000 then
    return 0xD800 + floor((r - 0x10000) / 1024)
  end
  return r
end

-- The unit after the first of the code point `r`, which ends before byte
-- `at` of `s`: its low surrogate, or the first unit of what follows; the
-- end of a string is the least.
local function second_unit(s, at, r)
  if r >= 0x10000 then
    return 0xDC00 + (r - 0x10000) % 1024
  elseif at > #s then
    return -1
  end
  return first_unit((decode(s, at)))
end

-- The order of `a` and `b` by UTF-16 code units: negative, 0 or positive.
-- Byte order is unit order except where a code point past the BMP, whose
-- first unit is a high surrogate, meets one from U+E000 to U+FFFF.
function K.compare_strings(a, b)
  local la, lb = #a, #b
  local i = 1
  while i <= la and i <= lb and byte(a, i) == byte(b, i) do
    i = i + 1
  end
  if i > la or i > lb then
    return la - lb
  end
  -- both differ inside the code point starting at the last lead byte
  while i > 1 and floor(byte(a, i) / 64) == 2 do
    i = i - 1
  end
  local x, n = decode(a, i)
  local y, m = decode(b, i)
  local ux, uy = first_unit(x), first_unit(y)
  if ux ~= uy then
    return ux - uy
  end
  return second_unit(a, i + n, x) - second_unit(b, i + m, y)
end

-- `String.char_code_at`: the unit at `k`, if any.
function K.charCodeAt(k, s)
  if k < 0 then
    return K.none
  end
  local n, i, len = 0, 1, #s
  while i <= len do
    local r, size = decode(s, i)
    i = i + size
    if r < 0x10000 then
      if n == k then
        return K.some(r)
      end
      n = n + 1
    else
      r = r - 0x10000
      if n == k then
        return K.some(0xD800 + floor(r / 1024))
      elseif n + 1 == k then
        return K.some(0xDC00 + r % 1024)
      end
      n = n + 2
    end
  end
  return K.none
end

-- `i` clamped into `0..n`, a negative one counting from the end, as JS
-- `slice` takes it.
local function position(i, n)
  if i < 0 then
    i = i + n
  end
  if i < 0 then
    return 0
  elseif i > n then
    return n
  end
  return i
end

-- `String.slice`: JS `s.slice(start, stop)`.
function K.slice(start, stop, s)
  if ascii(s) then
    local from, to = position(start, #s), position(stop, #s)
    if from >= to then
      return ""
    end
    return sub(s, from + 1, to)
  end
  local us = units(s)
  local from, to = position(start, #us), position(stop, #us)
  if from >= to then
    return ""
  end
  return from_units(us, from + 1, to)
end

-- Whether `s` holds a lone surrogate, whose bytes are in no string where it
-- is half of a pair, so that a search for it goes by units.
local function lone(s)
  return find(s, "\237[\160-\191]") ~= nil
end

-- The unit position of the first, or with `last` the last, `part` in `s`,
-- comparing UTF-16 code units.
local function unit_index(s, part, last)
  local us, want = units(s), units(part)
  local n, w = #us - #want, #want
  for i = 0, n do
    local k = last and n - i or i
    local same = true
    for j = 1, w do
      if us[k + j] ~= want[j] then
        same = false
        break
      end
    end
    if same then
      return K.some(k)
    end
  end
  return K.none
end

-- `String.index_of`: the unit position of the first `part` in `s`.
function K.indexOf(part, s)
  if lone(part) then
    return unit_index(s, part, false)
  end
  local at = find(s, part, 1, true)
  if not at then
    return K.none
  end
  return K.some(length(sub(s, 1, at - 1)))
end

-- `String.last_index_of`: the unit position of the last `part` in `s`.
function K.lastIndexOf(part, s)
  if lone(part) then
    return unit_index(s, part, true)
  end
  -- `find` clamps its start to the end, so the walk stops past it
  local found, at, len = nil, 1, #s
  while at <= len + 1 do
    local next_at = find(s, part, at, true)
    if not next_at then
      break
    end
    found, at = next_at, next_at + 1
  end
  if not found then
    return K.none
  end
  return K.some(length(sub(s, 1, found - 1)))
end

local ESCAPES = {
  [34] = '\\"',
  [92] = "\\\\",
  [8] = "\\b",
  [12] = "\\f",
  [10] = "\\n",
  [13] = "\\r",
  [9] = "\\t",
}

-- `show` of a string: JS `JSON.stringify`; a lone surrogate is escaped.
function K.quote(s)
  if not find(s, '[%z\1-\31"\\\237]') then
    return '"' .. s .. '"'
  end
  local out, n, i, len = { '"' }, 1, 1, #s
  while i <= len do
    local r, size = decode(s, i)
    local escape = ESCAPES[r]
    n = n + 1
    if escape then
      out[n] = escape
    elseif r < 0x20 or (r >= 0xD800 and r <= 0xDFFF) then
      out[n] = format("\\u%04x", r)
    else
      out[n] = sub(s, i, i + size - 1)
    end
    i = i + size
  end
  out[n + 1] = '"'
  return concat(out)
end
