-- The Lua primitive kernel: `Bytes`, a Lua string of the bytes, and UTF-8,
-- the bytes a string has outside Flick. A Flick string is WTF-8 (see
-- `strings.lua`); UTF-8 is WTF-8 without surrogates.
local _, ns = ...
local K = ns.k

local byte, char, find, gsub, sub = string.byte, string.char, string.find, string.gsub, string.sub
local concat = table.concat
local floor = math.floor
local unpack = unpack

-- How many bytes one `string.char` or `string.byte` call takes or gives,
-- well below the C stack Lua 5.1 lends a call.
local CHUNK = 4096

-- `Bytes.pack_list`: the bytes of a list of integers, or the index and value
-- of the first outside `0..255`.
function K.bytesFromList(xs)
  local parts, n = {}, #xs
  for start = 1, n, CHUNK do
    local stop = start + CHUNK - 1
    if stop > n then
      stop = n
    end
    for i = start, stop do
      local x = xs[i]
      if x < 0 or x > 255 or x ~= floor(x) then
        return K.err({ i - 1, x })
      end
    end
    parts[#parts + 1] = char(unpack(xs, start, stop))
  end
  return K.ok(concat(parts))
end

-- `Bytes.to_list`.
function K.bytesToList(b)
  local out, n = {}, #b
  for start = 1, n, CHUNK do
    local stop = start + CHUNK - 1
    if stop > n then
      stop = n
    end
    local chunk = { byte(b, start, stop) }
    for i = 1, #chunk do
      out[start + i - 1] = chunk[i]
    end
  end
  return out
end

-- `Bytes.encode_utf8`: a lone surrogate, the only WTF-8 that is not UTF-8,
-- as U+FFFD.
function K.bytesEncodeUtf8(s)
  return (gsub(s, "\237[\160-\191][\128-\191]", "\239\191\189"))
end

-- The bytes the second byte of a sequence led by `lead` may be, and how
-- many continuation bytes follow it: what UTF-8 allows, without overlong
-- forms, surrogates or code points past U+10FFFF.
local function second(lead)
  if lead >= 0xC2 and lead <= 0xDF then
    return 0x80, 0xBF, 1
  elseif lead == 0xE0 then
    return 0xA0, 0xBF, 2
  elseif lead == 0xED then
    return 0x80, 0x9F, 2
  elseif lead >= 0xE1 and lead <= 0xEF then
    return 0x80, 0xBF, 2
  elseif lead == 0xF0 then
    return 0x90, 0xBF, 3
  elseif lead >= 0xF1 and lead <= 0xF3 then
    return 0x80, 0xBF, 3
  elseif lead == 0xF4 then
    return 0x80, 0x8F, 3
  end
  return nil
end

-- The end of the sequence that starts at `i` of `s`, and whether it is UTF-8;
-- an ill-formed one ends after its longest well-formed start, which
-- `TextDecoder` replaces by one U+FFFD.
local function sequence(s, i)
  local lead = byte(s, i)
  if lead < 0x80 then
    return i, true
  end
  local low, high, more = second(lead)
  if not low then
    return i, false
  end
  for k = 1, more do
    local b = byte(s, i + k)
    if not b or b < low or b > high then
      return i + k - 1, false
    end
    low, high = 0x80, 0xBF
  end
  return i + more, true
end

-- Whether every byte of `s` is ASCII.
local function ascii(s)
  return not find(s, "[\128-\255]")
end

-- `Bytes.decode_utf8`: the string of valid UTF-8, a byte order mark kept.
function K.bytesDecodeUtf8(b)
  if ascii(b) then
    return K.some(b)
  end
  local i, n = 1, #b
  while i <= n do
    local stop, valid = sequence(b, i)
    if not valid then
      return K.none
    end
    i = stop + 1
  end
  return K.some(b)
end

-- The string of bytes a host gives: UTF-8 as it is, each ill-formed part
-- as U+FFFD, so that every Flick string is WTF-8.
function K.from_host(s)
  if ascii(s) then
    return s
  end
  local out, n, i, len, clean = {}, 0, 1, #s, true
  while i <= len do
    local stop, valid = sequence(s, i)
    n = n + 1
    if valid then
      out[n] = sub(s, i, stop)
    else
      out[n], clean = "\239\191\189", false
    end
    i = stop + 1
  end
  if clean then
    return s
  end
  return concat(out)
end
