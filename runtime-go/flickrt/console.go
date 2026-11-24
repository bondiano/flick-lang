package flickrt

// The console the logger writes to: the process's, writing as Node's
// `console` does, with the indentation of its groups.

import (
	"strconv"
	"strings"
	"sync"
)

type console struct {
	out, err func(string) // write text as it is
	mu       sync.Mutex
	indent   string
}

var defaultConsole = &console{out: func(s string) { StdoutWrite(s) }, err: stderrWrite}

func currentConsole(*Fiber) *console { return defaultConsole }

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

func (c *console) group(options Value) {
	if label := record(options).Get("label"); IsSome(label) {
		c.log(UnwrapSome(label).(string))
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

// inspect is Node's `util.inspect` of a value, on one line.
// shortcut: no line breaks past 72 columns and no depth limit, as Node's; port them when a program inspects large values
func inspect(v Value) string {
	switch x := asData(v).(type) {
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
			parts[i] = inspectKey(k) + ": " + inspect(x.vals[i])
		}
		return "{ " + strings.Join(parts, ", ") + " }"
	case *Data:
		parts := []string{"_tag: " + inspect(x.Tag)}
		for i, f := range x.Fields() {
			parts = append(parts, nativeField(x.Tag, i)+": "+inspect(f))
		}
		return "{ " + strings.Join(parts, ", ") + " }"
	}
	return Format(v)
}
