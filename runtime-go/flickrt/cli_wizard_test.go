package flickrt

import (
	"os"
	"strings"
	"testing"
)

// The wizard on a scripted terminal prints what the wizard of the Effect
// library prints: testdata/cli_wizard.txt is the output of the same
// scenarios run by Effect 4.0.5 on Node, with the command run.ts would build.
func TestCliWizardPrintsAsEffect(t *testing.T) {
	text := NewDict().Set("parse", Fn(func(_ *Fiber, s Value) Value { return Succeed(Ok(s)) }))
	var lines []string
	log := func(s string) { lines = append(lines, s) }
	handler := func(f func(o *Record) string) Value {
		return Fn(func(_ *Fiber, o Value) Value {
			return Eff(func(*Fiber) Value { log(f(record(o))); return Unit })
		})
	}
	greet := CliSpecDescription("Prints a greeting", CliSpecCommand("greet"))
	greet = CliSpecFlag(text, nil, nil, "loud", []Value{}, "Print in capitals", CliMultiSwitch, greet)
	greet = CliSpecFlag(text, nil, nil, "times", []Value{"n"}, "How many times", CliMultiDefault("1"), greet)
	greet = CliSpecFlag(text, nil, nil, "color", []Value{}, "The color of the text", CliMultiOptional, greet)
	greet = CliSpecArg(text, nil, nil, "name", "Who to greet", CliMultiRequired, greet)
	greet = CliSpecHandle(handler(func(o *Record) string {
		color, ok := some(o.Get("color"))
		if !ok {
			color = "-"
		}
		loud := "false"
		if o.Get("loud").(bool) {
			loud = "true"
		}
		return "Hello, " + o.Get("name").(string) + " loud=" + loud + " times=" + o.Get("times").(string) + " color=" + color.(string)
	}), greet)
	sum := CliSpecDescription("Adds numbers", CliSpecCommand("sum"))
	sum = CliSpecHandle(handler(func(o *Record) string {
		return "sum " + strings.Join(cliStrings(o.Get("numbers")), "+")
	}), CliSpecArg(text, nil, nil, "numbers", "", CliMultiSome, sum))
	tool := CliSpecDescription("Project tools", CliSpecCommand("tool"))
	tool = CliSpecFlag(text, nil, nil, "verbose", []Value{"v"}, "Verbose output", CliMultiSwitch, tool)
	tool = CliSpecGroup(CliSpecSubcommand(sum, "", CliSpecSubcommand(greet, "", tool)))

	ret, down, back := promptTestPress("return"), promptTestPress("down"), promptTestPress("backspace")
	for _, scenario := range []struct {
		args []string
		keys []Value
	}{
		{[]string{"--wizard"}, promptTestKeys("y", "y", ret, "n", "y", "-2", ret, "y", "it's red", ret, "Ada", ret, ret)},
		{[]string{"--wizard"}, promptTestKeys("n", down, ret, back, "2", ret, "1", ret, "x", ret, ret)},
		{[]string{"greet", "--wizard"}, promptTestKeys("n")},
	} {
		log("$ " + strings.Join(scenario.args, " "))
		script := &promptScript{keys: scenario.keys}
		env := &cliEnv{version: "1.0.0", builtIns: cliAllBuiltIns, format: cliDefaultFormat(false), log: log,
			tty: script.service().(*Record)}
		if err := cliRun(rootFiber(), tool.(*cliSpec), scenario.args, env); err != nil {
			log("error: " + err.message())
		}
	}
	want, err := os.ReadFile("testdata/cli_wizard.txt")
	if err != nil {
		t.Fatal(err)
	}
	if got := strings.Join(lines, "\n") + "\n"; got != string(want) {
		t.Fatalf("got\n%s\nwant\n%s", got, want)
	}
}
