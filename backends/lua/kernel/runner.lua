-- The runner of `flick test` on a Lua interpreter: the pure tests of each
-- test module, selected, run and reported as `runtime/src/core/suites.ts`
-- does, with the same human and JSON reports and exit code. Only a test
-- build loads it; it reads the interpreter's `io` and `os`.
local _, ns = ...
local K = ns.k

local concat = table.concat
local find, gsub = string.find, string.gsub
local floor = math.floor
local write = io.write
local clock = os.clock
local pcall, tonumber, tostring, type = pcall, tonumber, tostring, type

-- The options of the runner's arguments, as `parseTestArgs` reads them.
local function options(argv)
  local o = { filter = "", reporter = "human", color = false, list = false, forbid_only = false }
  local i = 1
  while argv[i] do
    local flag, value = argv[i], argv[i + 1] or ""
    if flag == "--watched" or flag == "--list" or flag == "--forbid-only" then
      if flag == "--list" then
        o.list = true
      elseif flag == "--forbid-only" then
        o.forbid_only = true
      end
      i = i + 1
    elseif flag == "--filter" or flag == "--seed" or flag == "--reporter" or flag == "--color"
      or flag == "--jobs" or flag == "--progress" or flag == "--timeout" then
      if flag == "--filter" then
        o.filter = value
      elseif flag == "--seed" then
        o.seed = tonumber(value)
      elseif flag == "--reporter" then
        o.reporter = value
      elseif flag == "--color" then
        o.color = value == "always"
      end
      i = i + 2
    else
      i = i + 1
    end
  end
  return o
end

-- The tests with their full names, `Module > group > name`, and the marks
-- they inherit.
local function flatten(prefix, tests, out, skip, only)
  for i = 1, #tests do
    local t = tests[i]
    local name = prefix .. " > " .. t.name
    local own_skip, own_only = skip or t.mode == "skip", only or t.mode == "only"
    if t.kind == "group" then
      flatten(name, t.tests, out, own_skip, own_only)
    else
      out[#out + 1] = { name = name, test = t, skip = own_skip, only = own_only }
    end
  end
  return out
end

-- The message of a defect: an error value without the position Lua adds.
local function defect(err)
  local text = type(err) == "string" and err or tostring(err)
  return (gsub(text, "^[^\n]-:%d+: ", "", 1))
end

-- The result of the test `t`.
local function result(t)
  local ok, outcome = pcall(t.body, true)
  if not ok then
    return { status = "failed", kind = "defect", message = defect(outcome) }
  end
  local failed = K.assertion_message(outcome)
  if failed then
    return { status = "failed", kind = "assertion", message = failed }
  end
  return { status = "passed" }
end

-- Why the runner reports a selected effect test skipped: the Lua target has
-- no runtime to run its task yet.
local EFFECT = "an effect test needs the Lua runtime"

local function json(r)
  local parts = { '{"name":', K.quote(r.name), ',"status":"', r.status, '"' }
  if r.reason then
    parts[#parts + 1] = ',"reason":' .. K.quote(r.reason)
  end
  if r.kind then
    parts[#parts + 1] = ',"kind":"' .. r.kind .. '"'
  end
  if r.message then
    parts[#parts + 1] = ',"message":' .. K.quote(r.message)
  end
  parts[#parts + 1] = "}"
  return concat(parts)
end

local function human(r, completed, total, ms, color)
  local reset = color and "\27[0m" or ""
  local dim = color and "\27[2m" or ""
  local marker = r.status == "passed" and ((color and "\27[32m" or "") .. "\226\156\147" .. reset)
    or ((color and "\27[31m" or "") .. "\226\156\151" .. reset)
  local text = "  " .. marker .. " " .. dim .. "[" .. completed .. "/" .. total .. "]" .. reset .. " " .. r.name
    .. "  " .. dim .. "(" .. floor(ms) .. " ms)" .. reset .. "\n"
  if r.status == "failed" then
    text = text .. "    " .. gsub((r.kind or "failure") .. ": " .. (r.message or ""), "\n", "\n    ") .. "\n"
  end
  return text
end

-- Runs `suites`, `{module, tests}` pairs, by the arguments `argv`; the exit
-- code: 0 when every selected test passed, 1 otherwise.
function K.run(suites, argv)
  local o = options(argv)
  local seed = o.seed
  if not seed or seed ~= floor(seed) then
    math.randomseed(os.time())
    seed = floor(math.random() * 2 ^ 31)
  end
  local all = {}
  for i = 1, #suites do
    flatten(suites[i][1], suites[i][2], all, false, false)
  end
  local marked, any_only = {}, false
  for i = 1, #all do
    if all[i].only then
      marked[#marked + 1] = all[i]
      any_only = any_only or not all[i].skip
    end
  end
  if o.forbid_only and #marked > 0 then
    write("--forbid-only: ", #marked, " test", #marked == 1 and " is" or "s are", " marked only\n")
    for i = 1, #marked do
      write("  ", marked[i].name, "\n")
    end
    return 1
  end
  local selected, skipped = {}, {}
  for i = 1, #all do
    local t = all[i]
    if find(t.name, o.filter, 1, true) then
      if not t.skip and (not any_only or t.only) then
        selected[#selected + 1] = t
      else
        skipped[#skipped + 1] = { name = t.name, status = "skipped" }
      end
    end
  end
  if o.list then
    for i = 1, #selected do
      write(selected[i].name, "\n")
    end
    return 0
  end
  local is_human, color = o.reporter ~= "json", o.color
  if is_human then
    write("  ", color and "\27[1;36mflick test\27[0m" or "flick test", "  ", #selected, " tests\n\n")
  end
  local started = clock()
  local results, failed = {}, 0
  for i = 1, #selected do
    local t = selected[i]
    if t.test.kind == "effect" then
      skipped[#skipped + 1] = { name = t.name, status = "skipped", reason = EFFECT }
    else
      local at = clock()
      local r = result(t.test)
      r.name = t.name
      results[#results + 1] = r
      if r.status == "failed" then
        failed = failed + 1
      end
      if is_human then
        write(human(r, i, #selected, (clock() - at) * 1000, color))
      end
    end
  end
  local passed = #results - failed
  if not is_human then
    local tests = {}
    for i = 1, #results do
      tests[i] = json(results[i])
    end
    for i = 1, #skipped do
      tests[#tests + 1] = json(skipped[i])
    end
    local counts = '{"seed":' .. seed .. ',"passed":' .. passed .. ',"failed":' .. failed
    if #skipped > 0 then
      counts = counts .. ',"skipped":' .. #skipped
    end
    write(counts, ',"tests":[', concat(tests, ","), "]}\n")
    return failed > 0 and 1 or 0
  end
  local reset = color and "\27[0m" or ""
  local dim = color and "\27[2m" or ""
  local passed_color = color and passed > 0 and "\27[32m" or dim
  local failed_color = color and failed > 0 and "\27[31m" or dim
  if #skipped > 0 then
    write("\n")
  end
  for i = 1, #skipped do
    local why = skipped[i].reason and ": " .. skipped[i].reason or ""
    write("  ", dim, "- ", skipped[i].name, "  (skipped", why, ")", reset, "\n")
  end
  local also = #skipped > 0 and (" \194\183 " .. dim .. #skipped .. reset .. " skipped") or ""
  write("\n  Tests  ", passed_color, passed, reset, " passed \194\183 ", failed_color, failed, reset, " failed", also,
    " \194\183 ", #results + #skipped, " total\n")
  write("  Time   ", dim, floor((clock() - started) * 1000), " ms", reset, "\n")
  if failed > 0 then
    write("seed: ", seed, " (rerun with --seed ", seed, ")\n")
  end
  return failed > 0 and 1 or 0
end
