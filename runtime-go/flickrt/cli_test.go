package flickrt

import (
	"reflect"
	"strings"
	"testing"
)

func TestCliLexSplitsOptionsAndKeepsOperands(t *testing.T) {
	tokens, trailing := cliLex([]string{"--name=x", "-ab", "-n=2", "-3.5", "-", "v", "--", "--help"})
	got := []string{}
	for _, tk := range tokens {
		s := tk.kind + ":" + tk.name
		if tk.value != nil {
			s += "=" + *tk.value
		}
		got = append(got, s)
	}
	want := []string{"long:name=x", "short:a", "short:b", "short:n=2", "value:-3.5", "value:-", "value:v"}
	if !reflect.DeepEqual(got, want) || !reflect.DeepEqual(trailing, []string{"--help"}) {
		t.Fatalf("got %v %v", got, trailing)
	}
}

func TestCliSuggestKeepsTheNearestWithinTwoEdits(t *testing.T) {
	if got := cliSuggest("lod", []string{"loud", "load", "n", "verbose"}); !reflect.DeepEqual(got, []string{"loud", "load"}) {
		t.Fatalf("got %v", got)
	}
	if got := cliSuggest("nope", []string{"loud", "n"}); len(got) != 0 {
		t.Fatalf("got %v", got)
	}
}

func TestCliYamlReadsMappingsSequencesAndFlowCollections(t *testing.T) {
	v, issue := cliYaml("name: flick # the name\nlist: [1, 2]\nnested:\n  - a\n  - {k: 'v w'}\nempty:\n")
	if issue != "" {
		t.Fatal(issue)
	}
	if got := JsonStringify(v); got != `{"name":"flick","list":[1,2],"nested":["a",{"k":"v w"}],"empty":null}` {
		t.Fatalf("got %s", got)
	}
}

func TestCliHelpLaysOutSectionsAsEffect(t *testing.T) {
	spec := CliSpecHandle(Unit, CliSpecArg(nil, nil, nil, "name", "Who", CliMultiRequired,
		CliSpecFlag(nil, nil, nil, "loud", []Value{"l"}, "Shout", CliMultiSwitch,
			CliSpecDescription("Greets", CliSpecCommand("greet")))))
	doc := cliHelpFor(cliCommandOf(spec.(*cliSpec)), []string{"greet"}, []*cliGlobal{cliHelpGlobal})
	want := strings.Join([]string{
		"DESCRIPTION", "  Greets", "",
		"USAGE", "  greet [flags] <name>", "",
		"ARGUMENTS", "  name string    Who", "",
		"FLAGS", "  --loud, -l    Shout", "",
		"GLOBAL FLAGS", "  --help, -h    Show help information",
	}, "\n")
	if got := cliFormatHelp(doc, cliColors{}); got != want {
		t.Fatalf("got\n%s", got)
	}
}

func TestCliErrorsSayWhatEffectSays(t *testing.T) {
	for _, c := range []struct {
		err  *cliError
		want string
	}{
		{cliInvalid("times", "abc", "not a number: abc", "flag"), `Invalid value for flag --times: "abc". Expected: not a number: abc`},
		{cliInvalid("n", "", "string", "flag"), `Missing value for flag --n. Expected: string`},
		{cliInvalid("x", "1", "Expected a", "argument"), `Invalid value for argument <x>: "1". Expected a`},
		{&cliError{tag: "UnexpectedArgument", arguments: []string{"a", "b"}}, `Unexpected positional arguments: "a", "b"`},
		{cliUnrecognized("--lod", []string{"greet"}, []string{"--loud"}), "Unrecognized flag: --lod in command greet\n\n  Did you mean this?\n    --loud"},
	} {
		if got := c.err.message(); got != c.want {
			t.Errorf("got %q, want %q", got, c.want)
		}
	}
}

func TestCliDisplayWidthCountsWideAndZeroWidthText(t *testing.T) {
	for text, want := range map[string]int{"abc": 3, "日本": 4, "é": 1, "\x1b[1mab\x1b[0m": 2, "👍🏽": 2} {
		if got := cliDisplayWidth(text); got != want {
			t.Errorf("%q: got %d, want %d", text, got, want)
		}
	}
}
