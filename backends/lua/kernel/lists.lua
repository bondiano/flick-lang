-- The Lua primitive kernel: lists, record rows, and the tests a test module
-- builds. A list is a table of its elements from 1; no element is `nil`
-- (`Unit` is `true`), so `#` is its length. A record is a table of its
-- labels.
local _, ns = ...
local K = ns.k

local pairs = pairs
local sort = table.sort

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

-- Record rows. Each function takes the dictionaries of its constraints
-- first; only `Project`'s holds what it needs, the labels of the narrower
-- record.

local function copy(r)
  local out = {}
  for label, value in pairs(r) do
    out[label] = value
  end
  return out
end

-- `Record.get`.
function K.recordGet(_, label, r)
  return r[label]
end

-- `Record.extend`.
function K.recordExtend(_, _, label, value, r)
  local out = copy(r)
  out[label] = value
  return out
end

-- `Record.remove`.
function K.recordRemove(_, label, r)
  local out = copy(r)
  out[label] = nil
  return out
end

-- `Record.project`: the fields of the labels the dictionary lists.
function K.recordProject(labels, r)
  local out = {}
  for i = 1, #labels do
    out[labels[i]] = r[labels[i]]
  end
  return out
end

-- `Record.merge_disjoint`.
function K.recordMergeDisjoint(_, a, b)
  local out = copy(a)
  for label, value in pairs(b) do
    out[label] = value
  end
  return out
end

-- `Record.labels`: in canonical order, by UTF-16 code units.
function K.recordLabels(_, r)
  local out = {}
  for label in pairs(r) do
    out[#out + 1] = label
  end
  sort(out, function(a, b)
    return K.compare_strings(a, b) < 0
  end)
  return out
end

-- `Test.test`, `Test.group`, `Test.skip` and `Test.only`: the tree the
-- runner walks, as the JS runtime's `testing.ts` builds it.

function K.testPure(name, body)
  return { kind = "pure", name = name, body = body }
end

-- An effect test: the runner reports it skipped and never runs its task,
-- which the backend does not generate.
function K.testEffect(name)
  return { kind = "effect", name = name }
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
