package flickrt

import (
	"fmt"
	"math"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"
)

// Console is Effect's `Console` as Node's console: the current fiber's,
// which `with_console` replaces. Like an instance of Node's `Console` it
// keeps its own group indentation, counters and timers.
type Console struct {
	mu       sync.Mutex
	toStderr bool // `to_stderr`: what goes to stdout goes to stderr too
	indent   string
	counts   map[string]int
	timers   map[string]time.Time
}

// consoleKey is where `with_console` puts a console in the environment.
const consoleKey = "effect/Console"

// defaultConsole is the global console of the process.
var defaultConsole = &Console{}

// currentConsole is the console of the fiber.
func currentConsole(fb *Fiber) *Console {
	if c, ok := fb.env.get(consoleKey); ok {
		return c.(*Console)
	}
	return defaultConsole
}

// write writes a line as Node's console does: each line of the text
// indented by the open groups.
func (c *Console) write(toStderr bool, text string) {
	c.mu.Lock()
	indent := c.indent
	c.mu.Unlock()
	if indent != "" {
		text = indent + strings.ReplaceAll(text, "\n", "\n"+indent)
	}
	if toStderr || c.toStderr {
		StderrLine(text)
	} else {
		StdoutLine(text)
	}
}

// log is `console.log` of a string.
func (c *Console) log(text string) { c.write(false, text) }

// error is `console.error` of a string.
func (c *Console) error(text string) { c.write(true, text) }

// warning is what `process.emitWarning` writes for a misused label: the
// first one of the process with Node's hint.
func (c *Console) warning(text string) {
	hint := ""
	warnHint.Do(func() { hint = "\n(Use `node --trace-warnings ...` to show where the warning was created)" })
	c.error(fmt.Sprintf("(node:%d) Warning: %s%s", os.Getpid(), text, hint))
}

var warnHint sync.Once

func ConsoleCurrent(fb *Fiber) Value { return currentConsole(fb) }

func ConsoleWith(fb *Fiber, console, task Value) Value {
	return withEnv(fb, consoleKey, console, task)
}

// ConsoleToStderr is a new console whose streams are both stderr.
func ConsoleToStderr() Value { return &Console{toStderr: true} }

func ConsoleLog(fb *Fiber, message Value) Value {
	currentConsole(fb).log(message.(string))
	return Unit
}

func ConsoleError(fb *Fiber, message Value) Value {
	currentConsole(fb).error(message.(string))
	return Unit
}

// ConsoleTrace writes the message and the Flick lines of the stack, where
// Node writes the JS stack.
func ConsoleTrace(fb *Fiber, message Value) Value {
	stack := strings.TrimRight(flickFrames(), "\n")
	if stack != "" {
		stack = "\n" + stack
	}
	currentConsole(fb).error("Trace: " + message.(string) + stack)
	return Unit
}

func ConsoleAssert(fb *Fiber, condition, message Value) Value {
	if !condition.(bool) {
		currentConsole(fb).error("Assertion failed: " + message.(string))
	}
	return Unit
}

// ConsoleClear clears the terminal when the console's stdout is one.
func ConsoleClear(fb *Fiber) Value {
	out := os.Stdout
	if currentConsole(fb).toStderr {
		out = os.Stderr
	}
	if isTTY(out.Fd()) && os.Getenv("TERM") != "dumb" {
		if out == os.Stderr {
			StderrWrite("\x1b[1;1H\x1b[0J")
		} else {
			StdoutWrite("\x1b[1;1H\x1b[0J")
		}
	}
	return Unit
}

func ConsoleCount(fb *Fiber, label Value) Value {
	c := currentConsole(fb)
	l := label.(string)
	c.mu.Lock()
	if c.counts == nil {
		c.counts = map[string]int{}
	}
	c.counts[l]++
	n := c.counts[l]
	c.mu.Unlock()
	c.log(l + ": " + strconv.Itoa(n))
	return Unit
}

func ConsoleCountReset(fb *Fiber, label Value) Value {
	c := currentConsole(fb)
	l := label.(string)
	c.mu.Lock()
	_, ok := c.counts[l]
	delete(c.counts, l)
	c.mu.Unlock()
	if !ok {
		c.warning("Count for '" + l + "' does not exist")
	}
	return Unit
}

// ConsoleDir writes the value as `util.inspect` shows it.
func ConsoleDir(fb *Fiber, value Value) Value {
	currentConsole(fb).log(inspect(value))
	return Unit
}

// ConsoleDirxml is `console.log` of the value: a string as it is.
func ConsoleDirxml(fb *Fiber, value Value) Value {
	currentConsole(fb).log(formatArg(value))
	return Unit
}

// formatArg is a value as `console.log` writes an argument.
func formatArg(v Value) string {
	if s, ok := v.(string); ok {
		return s
	}
	return inspectCustom(v)
}

// group opens a group: Node writes the label, `undefined` without one,
// as Effect passes it.
func (c *Console) group(label Value) {
	text := "undefined"
	if l, ok := some(label); ok {
		text = l.(string)
	}
	c.log(text)
	c.indentGroup()
}

// indentGroup opens a group without a label, as `console.group()`.
func (c *Console) indentGroup() {
	c.mu.Lock()
	c.indent += "  "
	c.mu.Unlock()
}

func (c *Console) groupEnd() {
	c.mu.Lock()
	if len(c.indent) >= 2 {
		c.indent = c.indent[2:]
	}
	c.mu.Unlock()
}

func ConsoleGroup(fb *Fiber, options Value) Value {
	c := currentConsole(fb)
	c.group(record(options).Get("label"))
	fb.owner.add(fb, func(*Fiber, *Cause) Value {
		c.groupEnd()
		return Unit
	})
	return Unit
}

func ConsoleWithGroup(fb *Fiber, options, task Value) Value {
	c := currentConsole(fb)
	c.group(record(options).Get("label"))
	defer c.groupEnd()
	return Run(fb, task)
}

func (c *Console) time(label string) {
	c.mu.Lock()
	if c.timers == nil {
		c.timers = map[string]time.Time{}
	}
	_, exists := c.timers[label]
	if !exists {
		c.timers[label] = time.Now()
	}
	c.mu.Unlock()
	if exists {
		c.warning("Label '" + label + "' already exists for console.time()")
	}
}

// timeLog writes the time of the timer, and ends it for `timeEnd`.
func (c *Console) timeLog(method, label, extra string) {
	c.mu.Lock()
	start, ok := c.timers[label]
	if ok && method == "timeEnd" {
		delete(c.timers, label)
	}
	c.mu.Unlock()
	if !ok {
		c.warning("No such label '" + label + "' for console." + method + "()")
		return
	}
	text := label + ": " + formatElapsed(float64(time.Since(start).Nanoseconds())/1e6)
	if extra != "" {
		text += " " + extra
	}
	c.log(text)
}

// formatElapsed is Node's `formatTime` of milliseconds.
func formatElapsed(ms float64) string {
	hours, minutes, seconds := 0, 0, 0.0
	if ms >= 1000 {
		if ms >= 60000 {
			if ms >= 3600000 {
				hours = int(ms / 3600000)
				ms = math.Mod(ms, 3600000)
			}
			minutes = int(ms / 60000)
			ms = math.Mod(ms, 60000)
		}
		seconds = ms / 1000
	}
	if hours != 0 || minutes != 0 {
		whole, frac, _ := strings.Cut(strconv.FormatFloat(seconds, 'f', 3, 64), ".")
		if len(whole) < 2 {
			whole = "0" + whole
		}
		if hours != 0 {
			return fmt.Sprintf("%d:%02d:%s.%s (h:mm:ss.mmm)", hours, minutes, whole, frac)
		}
		return fmt.Sprintf("%d:%s.%s (m:ss.mmm)", minutes, whole, frac)
	}
	if seconds != 0 {
		return strconv.FormatFloat(seconds, 'f', 3, 64) + "s"
	}
	fixed, _ := strconv.ParseFloat(strconv.FormatFloat(ms, 'f', 3, 64), 64)
	return ShowNumber(fixed) + "ms"
}

func ConsoleTime(fb *Fiber, label Value) Value {
	c := currentConsole(fb)
	l := label.(string)
	c.time(l)
	fb.owner.add(fb, func(*Fiber, *Cause) Value {
		c.timeLog("timeEnd", l, "")
		return Unit
	})
	return Unit
}

func ConsoleTimeLog(fb *Fiber, label, message Value) Value {
	currentConsole(fb).timeLog("timeLog", label.(string), message.(string))
	return Unit
}

func ConsoleWithTime(fb *Fiber, label, task Value) Value {
	c := currentConsole(fb)
	l := label.(string)
	c.time(l)
	defer c.timeLog("timeEnd", l, "")
	return Run(fb, task)
}
