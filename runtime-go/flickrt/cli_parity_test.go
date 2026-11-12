package flickrt

import (
	"os"
	"regexp"
	"slices"
	"sort"
	"strings"
	"testing"
)

// cliParityShow is parity.mjs's `show`: an Option, a list or text.
func cliParityShow(v Value) string {
	switch x := v.(type) {
	case []Value:
		parts := make([]string, len(x))
		for i, e := range x {
			parts[i] = cliParityShow(e)
		}
		return "[" + strings.Join(parts, ",") + "]"
	case bool:
		if x {
			return "true"
		}
		return "false"
	case string:
		return x
	}
	if s, ok := some(v); ok {
		return "Some(" + cliParityShow(s) + ")"
	}
	if v == None {
		return "None"
	}
	return cliParityShow(List(v))
}

// cliParityCommand mirrors the command of tools/go-cli/parity.mjs.
func cliParityCommand(log func(string)) *cliSpec {
	text := NewDict().Set("parse", Fn(func(_ *Fiber, s Value) Value { return Succeed(Ok(s)) }))
	digits := regexp.MustCompile(`^-?\d+$`)
	ints := NewDict().Set("parse", Fn(func(_ *Fiber, s Value) Value {
		if digits.MatchString(s.(string)) {
			return Succeed(Ok(s))
		}
		return Succeed(Err("not an Int: " + s.(string)))
	}))
	handler := func(name string) Value {
		return Fn(func(_ *Fiber, o Value) Value {
			return Eff(func(*Fiber) Value {
				r := record(o)
				keys := slices.Clone(r.keys)
				sort.Strings(keys)
				line := name
				for _, k := range keys {
					line += " " + k + "=" + cliParityShow(r.Get(k))
				}
				log(line)
				return Unit
			})
		})
	}
	action := func(name string) Value {
		return Fn(func(_ *Fiber, ctx Value) Value {
			return Eff(func(*Fiber) Value {
				log(name + " at " + strings.Join(cliStrings(record(ctx).Get("commandPath")), " "))
				return Unit
			})
		})
	}
	greet := CliSpecDescription("Prints a greeting", CliSpecCommand("greet"))
	greet = CliSpecFlag(text, nil, nil, "loud", []Value{"l"}, "Print in capitals", CliMultiSwitch, greet)
	greet = CliSpecFlag(ints, nil, nil, "times", []Value{"n"}, "How many times", CliMultiDefault("1"), greet)
	greet = CliSpecFlag(text, nil, nil, "level", []Value{}, "", CliMultiOptionalValue("auto", CliMultiOptional), greet)
	greet = CliSpecArg(text, nil, nil, "name", "Who to greet", CliMultiRequired, greet)
	greet = CliSpecExample("tool greet Ada", "Greet Ada", greet)
	greet = CliSpecHandle(handler("greet"), greet)

	sum := CliSpecShortDescription("Adds", CliSpecCommand("sum"))
	sum = CliSpecFlag(text, nil, nil, "tag", []Value{}, "A tag", CliMultiMany, sum)
	sum = CliSpecArg(ints, nil, nil, "numbers", "", CliMultiSome, sum)
	sum = CliSpecHandle(handler("sum"), sum)

	add := CliSpecArg(text, nil, nil, "url", "", CliMultiRequired, CliSpecCommand("add"))
	add = CliSpecHandle(handler("add"), add)
	remote := CliSpecDescription("Manage remotes", CliSpecCommand("remote"))
	remote = CliSpecAction("force", "Skip checks", action("force"), remote)
	remote = CliSpecFlag(text, nil, nil, "remote_name", []Value{}, "The remote", CliMultiDefault("origin"), remote)
	remote = CliSpecGroup(CliSpecSubcommand(add, "Add one", remote))

	tool := CliSpecDescription("Project tools", CliSpecCommand("tool"))
	tool = CliSpecFlag(text, nil, nil, "verbose", []Value{"v"}, "Verbose output", CliMultiSwitch, tool)
	tool = CliSpecFlag(text, nil, nil, "profile", []Value{}, "Profile name", CliMultiOptional, tool)
	tool = CliSpecAction("dry", "Show the plan", action("dry"), tool)
	tool = CliSpecGroup(CliSpecSubcommand(remote, "", CliSpecSubcommand(sum, "", CliSpecSubcommand(greet, "", tool))))
	return tool.(*cliSpec)
}

// The command lines of parity.mjs print and fail as the CLI of the Effect
// library does through run.ts: testdata/cli_parity.txt.
func TestCliParityAsEffect(t *testing.T) {
	var out []string
	log := func(s string) { out = append(out, s) }
	tool := cliParityCommand(log)
	corpus := [][]string{
		{},
		{"--help"},
		{"greet", "--help"},
		{"remote", "add", "--help"},
		{"sum", "-h"},
		{"--version"},
		{"greet", "Ada"},
		{"-v", "--profile", "p", "greet", "-l", "--times", "3", "--level", "Ada"},
		{"greet", "--level=x", "Ada"},
		{"greet", "--loud", "--loud", "Ada"},
		{"greet", "-n", "2", "--times", "3", "Ada"},
		{"greet", "--level", "--level=b", "Ada"},
		{"--verbose", "-v", "greet", "Ada"},
		{"--profile", "a", "greet", "--profile", "b", "Ada"},
		{"sum", "--tag", "a", "--tag", "b", "1", "2"},
		{"sum", "1", "x"},
		{"greet", "--times", "x", "Ada"},
		{"greet", "--=x"},
		{"--=x"},
		{"greet", "-ln=3", "Ada"},
		{"greet", "--no-loud", "Ada"},
		{"greet", "--no-loud=true", "Ada"},
		{"greet", "--no-loud", "yes", "Ada"},
		{"greet", "--no-times", "Ada"},
		{"greet", "--lod", "Ada"},
		{"greet", "--times"},
		{"greet"},
		{"greet", "Ada", "Bob"},
		{"gret"},
		{"remote"},
		{"remote", "add", "u"},
		{"remote", "--remote-name", "up", "add", "u"},
		{"remote", "--force", "add", "u"},
		{"greet", "--force", "Ada"},
		{"--dry", "greet", "Ada"},
		{"--dry", "--dry", "greet", "Ada"},
		{"--dry=maybe", "greet", "Ada"},
		{"--log-level", "debug", "greet", "Ada"},
		{"--log-level", "bogus", "greet", "Ada"},
		{"--log-level", "info", "--log-level", "warn", "greet", "Ada"},
		{"--log-level"},
		{"--completions", "nope"},
		{"--trace=nah", "greet", "Ada"},
		{"--trace", "--trace", "greet", "Ada"},
		{"--wizard=maybe", "greet", "Ada"},
	}
	blocks, printed := []string{}, map[string]string{}
	for _, args := range corpus {
		out = nil
		env := &cliEnv{version: "1.0.0", builtIns: cliAllBuiltIns, trace: true, format: cliDefaultFormat(false), log: log}
		err := cliRun(rootFiber(), tool, args, env)
		title := strings.TrimRight("$ tool "+strings.Join(args, " "), " ")
		stdout, lines := strings.Join(out, "\n"), []string{title}
		if first, ok := printed[stdout]; ok {
			lines = append(lines, "(stdout of "+first+")")
		} else if len(out) > 0 {
			lines, printed[stdout] = append(lines, stdout), title
		}
		if err != nil {
			lines = append(lines, "error: "+cliFlickError(err).(*Data).Fields()[0].(string))
		}
		blocks = append(blocks, strings.Join(lines, "\n"))
	}
	plain := record(CliDefaultFormatter(false)).Get("formatHelpDoc")
	for _, path := range [][]string{{}, {"greet"}, {"remote", "add"}} {
		fb := rootFiber()
		doc := CliHelpDoc(fb, cliStringList(path), tool)
		blocks = append(blocks, "help ["+strings.Join(path, " ")+"]\n"+Apply(fb, plain, doc).(string))
	}
	want, err := os.ReadFile("testdata/cli_parity.txt")
	if err != nil {
		t.Fatal(err)
	}
	got := strings.Join(blocks, "\n\n") + "\n"
	if got == string(want) {
		return
	}
	gb, wb := strings.Split(got, "\n\n$"), strings.Split(string(want), "\n\n$")
	for i := 0; i < len(gb) && i < len(wb); i++ {
		if gb[i] != wb[i] {
			t.Fatalf("block %d\n got:\n%s\nwant:\n%s", i, gb[i], wb[i])
		}
	}
	t.Fatalf("got %d blocks, want %d", len(gb), len(wb))
}
