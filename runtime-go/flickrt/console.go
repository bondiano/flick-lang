package flickrt

// Effect's `Console`: the console of the fiber, by default the process's,
// writing as Node's `console` does.

import (
	"math"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"
)

const consoleKey = "effect/Console"

type console struct {
	out, err func(string) // write text as it is
	mu       sync.Mutex
	indent   string
	counts   map[string]int
	timers   map[string]time.Time
}

func newConsole(out, err func(string)) *console {
	return &console{out: out, err: err, counts: map[string]int{}, timers: map[string]time.Time{}}
}

var defaultConsole = newConsole(func(s string) { StdoutWrite(s) }, stderrWrite)

func currentConsole(fb *Fiber) *console {
	if c, ok := fb.env.get(consoleKey); ok {
		return c.(*console)
	}
	return defaultConsole
}

// line writes the text to `to`, each of its lines indented by the groups.
func (c *console) line(to func(string), text string) {
	c.mu.Lock()
	indent := c.indent
	c.mu.Unlock()
	if indent != "" {
		text = indent + strings.ReplaceAll(text, "\n", "\n"+indent)
	}
	to(text + "\n")
}

func (c *console) log(text string)  { c.line(c.out, text) }
func (c *console) warn(text string) { c.line(c.err, text) }

func consoleOp(write func(*console, string)) func(*Fiber, Value) Value {
	return func(fb *Fiber, text Value) Value { write(currentConsole(fb), text.(string)); return Unit }
}

var (
	ConsoleLog   = consoleOp((*console).log)
	ConsoleInfo  = consoleOp((*console).log)
	ConsoleDebug = consoleOp((*console).log)
	ConsoleWarn  = consoleOp((*console).warn)
	ConsoleError = consoleOp((*console).warn)
	// shortcut: no stack follows `Trace: …`, as Go frames are not the program's; print the fiber's spans when Tracer keeps them
	ConsoleTrace = consoleOp(func(c *console, text string) { c.warn("Trace: " + text) })
)

func ConsoleAssert(fb *Fiber, ok, text Value) Value {
	if !ok.(bool) {
		currentConsole(fb).warn("Assertion failed: " + text.(string))
	}
	return Unit
}

func ConsoleCurrent(fb *Fiber) Value { return currentConsole(fb) }

func ConsoleWithConsole(fb *Fiber, c, task Value) Value { return withEnv(fb, consoleKey, c, task) }

// ConsoleToStderr is a console writing everything to stderr.
func ConsoleToStderr() Value { return newConsole(stderrWrite, stderrWrite) }

// ConsoleClear clears a terminal; Node does nothing when stdout is not one.
func ConsoleClear(fb *Fiber) Value {
	c := currentConsole(fb)
	if c == defaultConsole && fileIsTerminal(os.Stdout) {
		c.out("\x1b[1;1H\x1b[0J")
	}
	return Unit
}

func ConsoleCount(fb *Fiber, label Value) Value {
	c := currentConsole(fb)
	c.mu.Lock()
	c.counts[label.(string)]++
	n := c.counts[label.(string)]
	c.mu.Unlock()
	c.log(label.(string) + ": " + strconv.Itoa(n))
	return Unit
}

func ConsoleCountReset(fb *Fiber, label Value) Value {
	c := currentConsole(fb)
	c.mu.Lock()
	_, ok := c.counts[label.(string)]
	delete(c.counts, label.(string))
	c.mu.Unlock()
	if !ok {
		c.warn("Count for '" + label.(string) + "' does not exist")
	}
	return Unit
}

func ConsoleDir(fb *Fiber, v Value) Value {
	currentConsole(fb).log(inspect(v))
	return Unit
}

func ConsoleDirxml(fb *Fiber, v Value) Value {
	if s, ok := v.(string); ok {
		currentConsole(fb).log(s)
	} else {
		currentConsole(fb).log(inspect(v))
	}
	return Unit
}

func (c *console) group(options Value) {
	if label, ok := record(options).Get("label").(*Data); ok && label.Tag == "Some" {
		c.log(label.Fields[0].(string))
	}
	c.mu.Lock()
	c.indent += "  "
	c.mu.Unlock()
}

func (c *console) groupEnd() {
	c.mu.Lock()
	if len(c.indent) >= 2 {
		c.indent = c.indent[2:]
	}
	c.mu.Unlock()
}

// ConsoleGroup indents until the scope closes.
func ConsoleGroup(fb *Fiber, options Value) Value {
	c := currentConsole(fb)
	c.group(options)
	fb.owner.add(fb, func(*Fiber, *Cause) Value { c.groupEnd(); return Unit })
	return Unit
}

func ConsoleWithGroup(fb *Fiber, options, task Value) Value {
	c := currentConsole(fb)
	c.group(options)
	defer c.groupEnd()
	return Run(fb, task)
}

// consoleTime is Node's formatTime of milliseconds.
func consoleTime(ms float64) string {
	if ms >= 60000 {
		minutes := math.Floor(ms / 60000)
		seconds := (ms - minutes*60000) / 1000
		if minutes >= 60 {
			hours := math.Floor(minutes / 60)
			minutes -= hours * 60
			return strconv.Itoa(int(hours)) + ":" + pad2(minutes) + ":" + pad2(seconds) + " (h:mm:ss.mmm)"
		}
		return strconv.Itoa(int(minutes)) + ":" + pad2(seconds) + " (m:ss.mmm)"
	}
	if ms >= 1000 {
		return strconv.FormatFloat(ms/1000, 'f', 3, 64) + "s"
	}
	return strconv.FormatFloat(math.Round(ms*1000)/1000, 'f', -1, 64) + "ms"
}

func pad2(x float64) string {
	s := strconv.FormatFloat(x, 'f', 3, 64)
	if x < 10 {
		s = "0" + s
	}
	return strings.TrimSuffix(s, ".000")
}

func (c *console) timeStart(label string) {
	c.mu.Lock()
	_, running := c.timers[label]
	if !running {
		c.timers[label] = time.Now()
	}
	c.mu.Unlock()
	if running {
		c.warn("Warning: Label '" + label + "' already exists for console.time()")
	}
}

func (c *console) timeLog(label, data string, end bool) {
	c.mu.Lock()
	start, ok := c.timers[label]
	if end {
		delete(c.timers, label)
	}
	c.mu.Unlock()
	if !ok {
		c.warn("Warning: No such label '" + label + "' for console.time" + map[bool]string{true: "End", false: "Log"}[end] + "()")
		return
	}
	text := label + ": " + consoleTime(float64(time.Since(start).Nanoseconds())/1e6)
	if data != "" {
		text += " " + data
	}
	c.log(text)
}

// ConsoleTimeScoped starts a timer that the scope's close ends.
func ConsoleTimeScoped(fb *Fiber, label Value) Value {
	c, l := currentConsole(fb), label.(string)
	c.timeStart(l)
	fb.owner.add(fb, func(*Fiber, *Cause) Value { c.timeLog(l, "", true); return Unit })
	return Unit
}

func ConsoleTimeLog(fb *Fiber, label, data Value) Value {
	currentConsole(fb).timeLog(label.(string), data.(string), false)
	return Unit
}

func ConsoleWithTime(fb *Fiber, label, task Value) Value {
	c, l := currentConsole(fb), label.(string)
	c.timeStart(l)
	defer c.timeLog(l, "", true)
	return Run(fb, task)
}

// ConsoleTable is Node's `console.table` of the rows, with the columns
// `properties` names, or every key of the rows.
func ConsoleTable(fb *Fiber, properties, rows Value) Value {
	values := "Values"
	var columns []string
	given, hasValues := false, false
	if p, ok := properties.(*Data); ok && p.Tag == "Some" {
		columns, given = stringsOf(p.Fields[0]), true
	}
	seen := map[string]bool{}
	for _, row := range List(rows) {
		r, ok := row.(*Record)
		if !ok {
			hasValues = true
			continue
		}
		for _, k := range r.keys {
			if !given && !seen[k] {
				seen[k] = true
				columns = append(columns, k)
			}
		}
	}
	head := append([]string{"(index)"}, columns...)
	if hasValues {
		head = append(head, values)
	}
	table := [][]string{head}
	for i, row := range List(rows) {
		cells := []string{strconv.Itoa(i)}
		r, isRecord := row.(*Record)
		for _, k := range columns {
			cell := ""
			if isRecord && r.index(k) >= 0 {
				cell = inspect(r.Get(k))
			}
			cells = append(cells, cell)
		}
		if hasValues {
			cell := ""
			if !isRecord {
				cell = inspect(row)
			}
			cells = append(cells, cell)
		}
		table = append(table, cells)
	}
	currentConsole(fb).log(renderTable(table))
	return Unit
}

func renderTable(rows [][]string) string {
	widths := make([]int, len(rows[0]))
	for _, row := range rows {
		for i, cell := range row {
			widths[i] = max(widths[i], promptWidth(cell))
		}
	}
	rule := func(left, middle, right string) string {
		parts := make([]string, len(widths))
		for i, w := range widths {
			parts[i] = strings.Repeat("─", w+2)
		}
		return left + strings.Join(parts, middle) + right
	}
	line := func(row []string) string {
		parts := make([]string, len(row))
		for i, cell := range row {
			parts[i] = cell + strings.Repeat(" ", widths[i]-promptWidth(cell))
		}
		return "│ " + strings.Join(parts, " │ ") + " │"
	}
	out := []string{rule("┌", "┬", "┐"), line(rows[0]), rule("├", "┼", "┤")}
	for _, row := range rows[1:] {
		out = append(out, line(row))
	}
	return strings.Join(append(out, rule("└", "┴", "┘")), "\n")
}

// inspect is Node's `util.inspect` of a value, on one line.
// shortcut: no line breaks past 72 columns and no depth limit, as Node's; port them when a program inspects large values
func inspect(v Value) string {
	switch x := v.(type) {
	case string:
		return "'" + strings.ReplaceAll(strings.ReplaceAll(x, "\\", "\\\\"), "'", "\\'") + "'"
	case int32, float64:
		return StringFromNumber(x).(string)
	case bool:
		return strconv.FormatBool(x)
	case []Value:
		if len(x) == 0 {
			return "[]"
		}
		parts := make([]string, len(x))
		for i, e := range x {
			parts[i] = inspect(e)
		}
		return "[ " + strings.Join(parts, ", ") + " ]"
	case *Record:
		if len(x.keys) == 0 {
			return "{}"
		}
		parts := make([]string, len(x.keys))
		for i, k := range x.keys {
			parts[i] = k + ": " + inspect(x.vals[i])
		}
		return "{ " + strings.Join(parts, ", ") + " }"
	case *Data:
		parts := []string{"_tag: " + inspect(x.Tag)}
		for i, f := range x.Fields {
			parts = append(parts, nativeField(x.Tag, i)+": "+inspect(f))
		}
		return "{ " + strings.Join(parts, ", ") + " }"
	}
	return Format(v)
}
