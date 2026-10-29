package flickrt

import (
	"bufio"
	"fmt"
	"os"
)

var stdout = bufio.NewWriterSize(os.Stdout, 64<<10)

// StdoutLine writes a line to stdout.
func StdoutLine(text Value) Value {
	stdout.WriteString(wellFormed(text.(string)))
	stdout.WriteByte('\n')
	return Unit
}

// StdoutWrite writes text to stdout without a line break.
func StdoutWrite(text Value) Value {
	stdout.WriteString(wellFormed(text.(string)))
	return Unit
}

// StderrLine writes a line to stderr, after what stdout holds so far.
func StderrLine(text Value) Value {
	stdout.Flush()
	os.Stderr.WriteString(wellFormed(text.(string)) + "\n")
	return Unit
}

// PlatformArgs is the arguments of the program.
func PlatformArgs() Value {
	out := make([]Value, len(os.Args)-1)
	for i, a := range os.Args[1:] {
		out[i] = a
	}
	return out
}

// PlatformEnv is the environment variable `name`, if set.
func PlatformEnv(name Value) Value {
	if v, ok := os.LookupEnv(name.(string)); ok {
		return Some(v)
	}
	return None
}

// Todo is `todo`: code not written yet fails when it runs.
func Todo(message Value) Value { panic(Defect{"Error: " + message.(string)}) }

// Assert is a failing `let assert`: its place and message, and the value
// it missed.
func Assert(message, value Value) Value {
	panic(Defect{fmt.Sprintf("Error: %s: %s", message.(string), Format(value))})
}

// Echo is `echo`: shows a value and where it was echoed, and returns it.
func Echo(location, value Value) Value {
	StderrLine(location.(string) + " " + Format(value))
	return value
}

// Format shows any value for debugging.
func Format(v Value) string {
	switch x := v.(type) {
	case []Value:
		out := "["
		for i, e := range x {
			if i > 0 {
				out += ","
			}
			out += Format(e)
		}
		return out + "]"
	case *Data:
		out := "{\"_tag\":" + QuoteJSON(x.Tag)
		for i, f := range x.Fields {
			out += fmt.Sprintf(",%q:%s", nativeField(x.Tag, i), Format(f))
		}
		return out + "}"
	case Record:
		out := "{"
		for i, k := range sortedKeys(x) {
			if i > 0 {
				out += ","
			}
			out += QuoteJSON(k) + ":" + Format(x[k])
		}
		return out + "}"
	}
	return Show(v).(string)
}

// Main runs the program: `run` starts `main`. A defect is reported on
// stderr with exit code 1.
func Main(run func()) {
	code := 0
	func() {
		defer func() {
			if r := recover(); r != nil {
				stdout.Flush()
				if d, ok := r.(Defect); ok {
					fmt.Fprintln(os.Stderr, d.Message)
				} else {
					fmt.Fprintln(os.Stderr, r)
				}
				code = 1
			}
		}()
		run()
	}()
	stdout.Flush()
	os.Exit(code)
}
