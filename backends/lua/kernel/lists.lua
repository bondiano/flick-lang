-- The Lua primitive kernel: lists, and the tests a test module builds.
-- A list is a table of its elements from 1; no element is `nil` (`Unit`
-- is `true`), so `#` is its length.
local _, ns = ...
local K = ns.k

local pairs = pairs

-- `List.length`.
function K.listLength(xs)
  return #xs
end

-- `List.get`: the element at `i`, counting from 0, if any.
function K.listGet(i, xs)
  if i >= 0 and i < #xs then
    return K.some(xs[i + 1])
  end
  return K.none
end

-- `List.initialize`: `[f 0, …, f (n - 1)]`, calling `f` in that order.
function K.listInitialize(n, f)
  local out = {}
  for i = 0, n - 1 do
    out[i + 1] = f(i)
  end
  return out
end

-- `++` on lists.
function K.list_concat(xs, ys)
  local out, n = {}, #xs
  for i = 1, n do
    out[i] = xs[i]
  end
  for i = 1, #ys do
    out[n + i] = ys[i]
  end
  return out
end

-- `Test.test`, `Test.group`, `Test.skip` and `Test.only`: the tree the
-- runner walks, as the JS runtime's `testing.ts` builds it.

function K.testPure(name, body)
  return { kind = "pure", name = name, body = body }
end

function K.testGroup(name, tests)
  return { kind = "group", name = name, tests = tests }
end

local function marked(test, mode)
  local out = {}
  for key, value in pairs(test) do
    out[key] = value
  end
  out.mode = mode
  return out
end

function K.testSkip(test)
  return marked(test, "skip")
end

function K.testOnly(test)
  return marked(test, "only")
end
