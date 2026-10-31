package flickrt

import (
	"os"
	"strings"
	"testing"
)

// completeTestCommand mirrors the descriptor in the node script that produced
// testdata/complete_*.txt with Effect 4.0.5's own generators.
func completeTestCommand() completeCommand {
	deploy := completeCommand{
		name: "deploy", description: "Deploy it",
		flags: []completeFlag{{name: "env", typ: completeType{tag: "Choice", values: []string{"prod", `it's "qa"`, "a b"}}}},
		args:  []completeArg{{name: "target", required: true, typ: completeType{tag: "Path", pathType: "directory"}}},
	}
	remote := completeCommand{name: "remote", description: "Manage remotes: add]", subcommands: []completeCommand{deploy}}
	alias := remote
	alias.name = "rm"
	return completeCommand{
		name: "my-app", description: "Main",
		flags: []completeFlag{
			{name: "verbose", aliases: []string{"v"}, description: `Be loud: 'very' [x] \ café 😀`, typ: completeType{tag: "Boolean"}},
			{name: "color", aliases: []string{"c", "colour"}, typ: completeType{tag: "Choice", values: []string{"auto", "never", "x:y"}}},
			{name: "config", description: "Config file", typ: completeType{tag: "Path", pathType: "file"}},
			{name: "log", description: "Log level", typ: completeType{tag: "String"}, optionalValue: true},
			{name: "count", typ: completeType{tag: "Int"}},
		},
		args: []completeArg{
			{name: "input", description: "Input path", required: true, typ: completeType{tag: "Path", pathType: "either"}},
			{name: "rest", variadic: true, typ: completeType{tag: "Choice", values: []string{"one", "two"}}},
		},
		subcommands: []completeCommand{remote, alias},
	}
}

func TestCompleteScriptsMatchEffect(t *testing.T) {
	d := completeTestCommand()
	for _, c := range []struct {
		golden string
		got    string
	}{
		{"complete_bash.txt", completeBash("my-app", d)},
		{"complete_zsh.txt", completeZsh("my-app", d)},
		{"complete_powershell.txt", completePowerShell("my-app", d)},
	} {
		want, err := os.ReadFile("testdata/" + c.golden)
		if err != nil {
			t.Fatal(err)
		}
		if c.got != string(want) {
			gl, wl := strings.Split(c.got, "\n"), strings.Split(string(want), "\n")
			for i := 0; i < len(gl) || i < len(wl); i++ {
				if i >= len(gl) || i >= len(wl) || gl[i] != wl[i] {
					g, w := "<eof>", "<eof>"
					if i < len(gl) {
						g = gl[i]
					}
					if i < len(wl) {
						w = wl[i]
					}
					t.Errorf("%s: line %d\n got: %q\nwant: %q", c.golden, i+1, g, w)
					break
				}
			}
		}
	}
}

func TestCompleteSanitizeCountsCodeUnits(t *testing.T) {
	if got := completeSanitize("a-😀é"); got != "a____" {
		t.Errorf("completeSanitize = %q", got)
	}
	if got := completeQuotePs("😀'"); got != "('' + [char]0xD83D + [char]0xDE00 + '''')" {
		t.Errorf("completeQuotePs = %q", got)
	}
}
