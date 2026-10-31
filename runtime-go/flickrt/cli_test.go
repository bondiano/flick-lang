package flickrt

import (
	"bufio"
	"bytes"
	"reflect"
	"strings"
	"testing"
)

func cliTestStringDict() Value {
	return NewDict().Set("parse", Fn(func(_ *Fiber, text Value) Value { return Succeed(Ok(text)) }))
}

func cliTestIntDict(parsed *int) Value {
	return NewDict().Set("parse", Fn(func(_ *Fiber, text Value) Value {
		return Eff(func(*Fiber) Value {
			if parsed != nil {
				*parsed++
			}
			n := IntParse(text)
			if isSome(n) {
				return Ok(value(n))
			}
			return Err("not an Int: " + text.(string))
		})
	}))
}

func cliTestRun(t *testing.T, fb *Fiber, command Value, args ...string) (string, *Cause) {
	t.Helper()
	var output bytes.Buffer
	outMu.Lock()
	previous := stdout
	previous.Flush()
	stdout = bufio.NewWriter(&output)
	outMu.Unlock()
	defer func() {
		outMu.Lock()
		stdout.Flush()
		stdout = previous
		outMu.Unlock()
	}()
	values := make([]Value, len(args))
	for i, arg := range args {
		values[i] = arg
	}
	_, cause := catch(func() Value { return CliRunVersion(fb, "2.3.4", command, values) })
	outMu.Lock()
	stdout.Flush()
	outMu.Unlock()
	return output.String(), cause
}

func cliTestError(t *testing.T, cause *Cause, contains string) {
	t.Helper()
	if cause == nil || cause.kind != causeFail {
		t.Fatalf("want CliError containing %q, got %v", contains, cause)
	}
	err, ok := cause.err.(*Data)
	if !ok || err.Tag != cliErrorTag || len(err.Fields) != 1 {
		t.Fatalf("want CliError, got %s", Format(cause.err))
	}
	if !strings.Contains(err.Fields[0].(string), contains) {
		t.Fatalf("error %q does not contain %q", err.Fields[0], contains)
	}
}

func TestCliImmutableReusableSpecs(t *testing.T) {
	prefix := CliSpecCommand("entry")
	for _, label := range []string{"one", "two", "three"} {
		prefix = CliSpecArg(cliTestStringDict(), Unit, Unit, label, "", CliMultiRequired, prefix)
	}
	left := CliSpecArg(cliTestStringDict(), Unit, Unit, "left_field", "Left field", CliMultiRequired, prefix)
	right := CliSpecArg(cliTestIntDict(nil), Unit, Unit, "right_field", "Right field", CliMultiRequired, prefix)
	left = CliSpecDescription("Only the left branch", left)
	var seen []Value
	left = CliSpecHandle(Fn(func(_ *Fiber, options Value) Value {
		return Eff(func(*Fiber) Value {
			seen = append(seen, Field(options, "left_field"))
			if record(options).index("right_field") >= 0 {
				t.Fatal("right branch changed the left branch's arguments")
			}
			return Unit
		})
	}), left)
	right = CliSpecHandle(Fn(func(_ *Fiber, options Value) Value {
		return Eff(func(*Fiber) Value {
			seen = append(seen, Field(options, "right_field"))
			return Unit
		})
	}), right)
	group := CliSpecGroup(left)
	for _, test := range []struct {
		command Value
		last    string
	}{{left, "a"}, {right, "7"}, {left, "b"}} {
		if output, cause := cliTestRun(t, rootFiber(), test.command, "x", "y", "z", test.last); cause != nil || output != "" {
			t.Fatalf("run: output=%q cause=%v", output, cause)
		}
	}
	if !reflect.DeepEqual(seen, []Value{"a", int32(7), "b"}) {
		t.Fatalf("reused specs parsed %v", seen)
	}
	output, cause := cliTestRun(t, rootFiber(), right, "--help")
	if cause != nil || strings.Contains(output, "Only the left branch") || strings.Contains(output, "left-field") || !strings.Contains(output, "right-field") {
		t.Fatalf("description or arguments leaked between branches: %q, %v", output, cause)
	}
	output, cause = cliTestRun(t, rootFiber(), prefix, "--help")
	if cause != nil || strings.Contains(output, "left-field") || strings.Contains(output, "right-field") {
		t.Fatalf("derived arguments mutated the original spec: %q, %v", output, cause)
	}
	_, cause = cliTestRun(t, rootFiber(), group, "x", "y", "z", "c")
	cliTestError(t, cause, "Missing subcommand")
	if _, cause = cliTestRun(t, rootFiber(), left, "x", "y", "z", "d"); cause != nil || seen[len(seen)-1] != "d" {
		t.Fatalf("group removed a reusable command's handler: %v", cause)
	}

	root := CliSpecCommand("tree")
	for _, name := range []string{"one", "two", "three"} {
		root = CliSpecSubcommand(CliSpecGroup(CliSpecCommand(name)), "", root)
	}
	branch := CliSpecSubcommand(CliSpecGroup(CliSpecCommand("left")), "", root)
	other := CliSpecSubcommand(CliSpecGroup(CliSpecCommand("right")), "", root)
	for _, test := range []struct {
		command Value
		present string
		absent  string
	}{{branch, "left", "right"}, {other, "right", "left"}, {root, "three", "left"}} {
		output, cause := cliTestRun(t, rootFiber(), test.command, "--help")
		if cause != nil || !strings.Contains(output, test.present) || strings.Contains(output, test.absent) {
			t.Fatalf("subcommands leaked between branches: %q, %v", output, cause)
		}
	}
}

func TestCliValueParsersAndHandlersUseExecutionFiber(t *testing.T) {
	const key = "test.cli.scale"
	var current *Fiber
	parsed, handled, resolved := 0, 0, 0
	dict := NewDict().Lazy("parse", func(fb *Fiber) Value {
		resolved++
		if fb != current {
			t.Fatal("dictionary resolved outside the execution fiber")
		}
		return Fn(func(fb *Fiber, text Value) Value {
			if fb != current {
				t.Fatal("parser applied outside the execution fiber")
			}
			return Eff(func(fb *Fiber) Value {
				if fb != current {
					t.Fatal("parser effect ran outside the execution fiber")
				}
				parsed++
				n := IntParse(text)
				if !isSome(n) {
					return Err("not an Int")
				}
				scale, _ := fb.env.get(key)
				return Ok(value(n).(int32) * scale.(int32))
			})
		})
	})
	var results []Value
	command := CliSpecCommand("collect")
	command = CliSpecArg(cliTestStringDict(), Unit, Unit, "title", "", CliMultiRequired, command)
	command = CliSpecArg(dict, Unit, Unit, "scaled_numbers", "", CliMultiSome, command)
	command = CliSpecArg(cliTestStringDict(), Unit, Unit, "tail", "", CliMultiRequired, command)
	command = CliSpecHandle(Fn(func(fb *Fiber, options Value) Value {
		if fb != current {
			t.Fatal("handler applied outside the execution fiber")
		}
		return Eff(func(fb *Fiber) Value {
			if fb != current {
				t.Fatal("handler effect ran outside the execution fiber")
			}
			handled++
			numbers := Field(options, "scaled_numbers")
			if len(List(numbers)) == 0 {
				t.Fatal("some produced an empty NonEmpty")
			}
			results = append(results, []Value{Field(options, "title"), numbers, Field(options, "tail")})
			return Unit
		})
	}), command)
	group := CliSpecGroup(CliSpecSubcommand(command, "", CliSpecCommand("batch")))
	root := CliSpecGroup(CliSpecSubcommand(group, "", CliSpecCommand("garden")))
	if parsed != 0 || handled != 0 || resolved != 0 {
		t.Fatal("constructing a spec ran callbacks")
	}
	for _, test := range []struct {
		scale int32
		args  []string
	}{{2, []string{"batch", "collect", "first", "-2", "3", "end"}}, {5, []string{"batch", "collect", "second", "0", "last"}}} {
		current = rootFiber()
		current.env = current.env.with(key, test.scale)
		if output, cause := cliTestRun(t, current, root, test.args...); cause != nil || output != "" {
			t.Fatalf("run: output=%q cause=%v", output, cause)
		}
	}
	want := []Value{[]Value{"first", []Value{int32(-4), int32(6)}, "end"}, []Value{"second", []Value{int32(0)}, "last"}}
	if !reflect.DeepEqual(results, want) || parsed != 3 || handled != 2 || resolved != 1 {
		t.Fatalf("results=%v parse=%d handle=%d resolve=%d", results, parsed, handled, resolved)
	}
}

func TestCliRejectsInputBeforeHandlers(t *testing.T) {
	parsed, handled := 0, 0
	handler := Fn(func(*Fiber, Value) Value {
		return Eff(func(*Fiber) Value { handled++; return Unit })
	})
	one := CliSpecHandle(handler, CliSpecArg(cliTestIntDict(&parsed), Unit, Unit, "number", "", CliMultiRequired, CliSpecCommand("one")))
	many := CliSpecHandle(handler, CliSpecArg(cliTestIntDict(&parsed), Unit, Unit, "numbers", "", CliMultiSome, CliSpecCommand("many")))
	empty := CliSpecHandle(handler, CliSpecCommand("empty"))
	branch := CliSpecGroup(CliSpecSubcommand(empty, "", CliSpecCommand("branch")))
	root := CliSpecCommand("garden")
	for _, child := range []Value{one, many, empty, branch} {
		root = CliSpecSubcommand(child, "", root)
	}
	root = CliSpecGroup(root)
	for _, test := range []struct {
		name     string
		args     []string
		message  string
		parseRun bool
	}{
		{"unknown command", []string{"absent"}, "Unknown command", false},
		{"nested unknown command", []string{"branch", "absent"}, "Unknown command", false},
		{"missing required", []string{"one"}, "Missing required argument <number>", false},
		{"missing some", []string{"many"}, "Missing required argument <numbers>", false},
		{"extra required", []string{"one", "1", "2"}, "Unexpected argument", false},
		{"extra empty", []string{"empty", "x"}, "Unexpected argument", false},
		{"malformed required", []string{"one", "abc"}, "not an Int: abc", true},
		{"malformed some", []string{"many", "1", "bad"}, "not an Int: bad", true},
		{"negative malformed", []string{"one", "-1e999"}, "not an Int", true},
		{"unknown flag", []string{"--unknown"}, "Unknown flag", false},
		{"unknown child flag", []string{"one", "1", "--unknown"}, "Unknown flag", false},
		{"unknown short flag", []string{"one", "-x"}, "Unknown flag", false},
		{"wizard", []string{"--wizard"}, "Unsupported built-in", false},
		{"trace", []string{"empty", "--trace"}, "Unsupported built-in", false},
		{"log level", []string{"--log-level", "debug"}, "Unsupported built-in", false},
		{"log level equals", []string{"--log-level=debug"}, "Unsupported built-in", false},
		{"missing shell", []string{"--completions"}, "Missing shell", false},
		{"unsupported shell", []string{"--completions", "fish"}, "Unsupported completion shell", false},
		{"unknown shell", []string{"--completions=elvish"}, "Unsupported completion shell", false},
		{"conflicting actions", []string{"--help", "--version"}, "Choose only one", false},
	} {
		t.Run(test.name, func(t *testing.T) {
			parsed = 0
			output, cause := cliTestRun(t, rootFiber(), root, test.args...)
			cliTestError(t, cause, test.message)
			if handled != 0 || output != "" || (parsed > 0) != test.parseRun {
				t.Fatalf("handled=%d parsed=%d output=%q", handled, parsed, output)
			}
		})
	}
}

func TestCliDoubleDashAndNegativeArguments(t *testing.T) {
	var got Value
	handler := Fn(func(_ *Fiber, options Value) Value {
		return Eff(func(*Fiber) Value { got = Field(options, "value"); return Unit })
	})
	words := CliSpecHandle(handler, CliSpecArg(cliTestStringDict(), Unit, Unit, "value", "", CliMultiSome, CliSpecCommand("words")))
	if output, cause := cliTestRun(t, rootFiber(), words, "--", "--help", "--trace", "-x", "--"); cause != nil || output != "" {
		t.Fatalf("double dash: output=%q cause=%v", output, cause)
	}
	if !reflect.DeepEqual(got, []Value{"--help", "--trace", "-x", "--"}) {
		t.Fatalf("double dash parsed %v", got)
	}
	number := CliSpecHandle(handler, CliSpecArg(cliTestIntDict(nil), Unit, Unit, "value", "", CliMultiRequired, CliSpecCommand("number")))
	for _, args := range [][]string{{"-17"}, {"--", "-17"}} {
		if output, cause := cliTestRun(t, rootFiber(), number, args...); cause != nil || output != "" || got != int32(-17) {
			t.Fatalf("negative argument: got=%v output=%q cause=%v", got, output, cause)
		}
	}
}

func TestCliBuiltinsDoNotParseOrHandle(t *testing.T) {
	parsed, handled := 0, 0
	child := CliSpecCommand("plant")
	child = CliSpecDescription("Plant a tree", child)
	child = CliSpecArg(cliTestIntDict(&parsed), Unit, Unit, "tree_count", "How many trees", CliMultiSome, child)
	child = CliSpecHandle(Fn(func(*Fiber, Value) Value {
		return Eff(func(*Fiber) Value { handled++; return Unit })
	}), child)
	root := CliSpecGroup(CliSpecSubcommand(child, "Add trees", CliSpecDescription("Manage the garden", CliSpecCommand("garden"))))
	for _, test := range []struct {
		args []string
		want []string
	}{
		{[]string{"--help"}, []string{"Usage: garden <command>", "Manage the garden", "plant  Add trees", "--completions bash"}},
		{[]string{"plant", "--help"}, []string{"Usage: garden plant <tree-count>...", "Plant a tree", "How many trees", "at least one"}},
		{[]string{"plant", "-h"}, []string{"Usage: garden plant"}},
		{[]string{"--version"}, []string{"garden v2.3.4\n"}},
		{[]string{"plant", "--version"}, []string{"garden v2.3.4\n"}},
		{[]string{"--completions", "bash"}, []string{"complete -o default -F", "'garden'", "plant"}},
		{[]string{"plant", "--completions=bash"}, []string{"complete -o default -F", "'garden'"}},
	} {
		output, cause := cliTestRun(t, rootFiber(), root, test.args...)
		if cause != nil {
			t.Fatalf("%v failed: %v", test.args, cause)
		}
		for _, want := range test.want {
			if !strings.Contains(output, want) {
				t.Errorf("%v output %q lacks %q", test.args, output, want)
			}
		}
		if strings.Contains(output, "--wizard") || strings.Contains(output, "--trace") || strings.Contains(output, "--log-level") {
			t.Errorf("output advertises an unsupported built-in: %q", output)
		}
	}
	output, cause := cliTestRun(t, rootFiber(), root)
	cliTestError(t, cause, "Missing subcommand for garden")
	if !strings.Contains(output, "Usage: garden") || parsed != 0 || handled != 0 {
		t.Fatalf("group invocation: output=%q parsed=%d handled=%d", output, parsed, handled)
	}
}

func TestCliPreservesHandlerFailures(t *testing.T) {
	original := Sequential(&Cause{kind: causeFail, err: NewData(Tag("Test.WriteError.WriteError"), "read only")}, &Cause{kind: causeDie, defect: "finalizer"})
	fb := rootFiber()
	command := CliSpecHandle(Fn(func(current *Fiber, _ Value) Value {
		if current != fb {
			t.Fatal("handler got another fiber")
		}
		return Eff(func(*Fiber) Value { panic(&Failure{original}) })
	}), CliSpecCommand("write"))
	output, cause := cliTestRun(t, fb, command)
	if cause != original || output != "" {
		t.Fatalf("handler cause changed: output=%q cause=%v", output, cause)
	}
}
