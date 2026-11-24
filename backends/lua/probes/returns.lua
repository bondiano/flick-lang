-- Result adjustment: only the last expression of a list expands, and
-- parentheses cut a call to one value.

local function three() return 1, nil, nil end
local function none() end
local function count(...) return select("#", ...) end

print(count(three()))
print(count(none()))
print(count((three())))
print(count(three(), 2))
print(count(2, three()))
print(count({three()}))
print(select("#", unpack({three(), three()}, 1, 4)))

local a, b, c, d = three()
print(a, b, c, d)
print(count(nil, nil))

local packed = {n = count(three()), three()}
print(packed.n, packed[1], packed[2], packed[3])

print(select(2, "a", "b", "c"))
print(select(-1, "a", "b", "c"))
print((select("#")))
