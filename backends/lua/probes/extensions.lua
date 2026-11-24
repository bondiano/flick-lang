-- The LuaJIT extensions a profile may enable: `bit`, `jit` and `ffi`.

print("bit", type(bit))
if bit then
  print(bit.tobit(2^31), bit.band(-1, 255), bit.lshift(1, 31), bit.rshift(-1, 28))
end

print("jit", type(jit))
if jit then print((jit.status())) end

-- WoW has no `require`; the call then fails inside pcall
local ok, ffi = pcall(require, "ffi")
print("ffi", ok)
if ok then
  -- exact in a 64-bit integer, rounded in a double
  print(tostring(ffi.new("int64_t", 2) ^ 53 + 1), 2^53 + 1 == 2^53)
  print(tostring(ffi.new("uint64_t", 0) - 1))
end
