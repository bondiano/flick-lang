package flickrt

import (
	"os"
	"os/exec"
	"reflect"
	"strconv"
	"strings"
	"testing"
)

func cliTestBashCandidates(t *testing.T, script, function string, words ...string) []string {
	t.Helper()
	bash, err := exec.LookPath("bash")
	if err != nil {
		t.Skip("Bash is not installed")
	}
	var call strings.Builder
	call.WriteString(script)
	call.WriteString("\nCOMP_WORDS=(")
	env := append(os.Environ(), "BASH_ENV=")
	for i, word := range words {
		key := "FLICK_CLI_WORD_" + strconv.Itoa(i)
		env = append(env, key+"="+word)
		call.WriteString("\"${" + key + "}\" ")
	}
	call.WriteString(")\nCOMP_CWORD=" + strconv.Itoa(len(words)-1) + "\n")
	call.WriteString("[[ \"$(complete -p -- \"${COMP_WORDS[0]}\")\" == *" + function + "* ]] || exit 9\n")
	call.WriteString(function + "\nfor candidate in \"${COMPREPLY[@]}\"; do printf '%s\\n' \"$candidate\"; done\n")
	command := exec.Command(bash, "--noprofile", "--norc", "-c", call.String())
	command.Env = env
	command.Dir = t.TempDir()
	output, err := command.CombinedOutput()
	if err != nil {
		t.Fatalf("Bash completion failed: %v\n%s", err, output)
	}
	if len(output) == 0 {
		return nil
	}
	return strings.Split(strings.TrimSuffix(string(output), "\n"), "\n")
}

func TestCliBashCompletionCompletesTheCommandTree(t *testing.T) {
	handled := 0
	handler := Fn(func(*Fiber, Value) Value {
		return Eff(func(*Fiber) Value { handled++; return Unit })
	})
	plant := CliSpecHandle(handler, CliSpecCommand("plant"))
	list := CliSpecHandle(handler, CliSpecCommand("list"))
	project := CliSpecCommand("project")
	project = CliSpecArg(cliTestStringDict(), Unit, Unit, "project_name", "", CliMultiRequired, project)
	project = CliSpecGroup(CliSpecSubcommand(list, "", project))
	root := CliSpecCommand("garden-tool")
	for _, child := range []Value{plant, project, CliSpecHandle(handler, CliSpecCommand("grow'up"))} {
		root = CliSpecSubcommand(child, "", root)
	}
	root = CliSpecGroup(root)
	script, cause := cliTestRun(t, rootFiber(), root, "--completions", "bash")
	if cause != nil || handled != 0 {
		t.Fatalf("generating completions ran the handler: %v", cause)
	}
	function := cliCompletionFunction(root.(*cliSpec))
	for _, test := range []struct {
		name  string
		words []string
		want  []string
	}{
		{"root", []string{"garden-tool", "p"}, []string{"plant", "project"}},
		{"global flags", []string{"garden-tool", "--"}, []string{"--help", "--version", "--completions"}},
		{"shell", []string{"garden-tool", "--completions", "b"}, []string{"bash"}},
		{"shell equals", []string{"garden-tool", "--completions=ba"}, []string{"--completions=bash"}},
		{"unsupported shell", []string{"garden-tool", "--completions", "z"}, nil},
		{"leaf", []string{"garden-tool", "plant", "p"}, nil},
		{"child flags", []string{"garden-tool", "plant", "--v"}, []string{"--version"}},
		{"parent argument missing", []string{"garden-tool", "project", "l"}, nil},
		{"nested command", []string{"garden-tool", "project", "orchard", "l"}, []string{"list"}},
		{"negative parent argument", []string{"garden-tool", "project", "-4", "l"}, []string{"list"}},
		{"nested leaf", []string{"garden-tool", "project", "orchard", "list", "l"}, nil},
		{"after terminator", []string{"garden-tool", "--", "p"}, nil},
		{"unknown flag", []string{"garden-tool", "--unknown", "p"}, nil},
		{"literal name", []string{"garden-tool", "grow"}, []string{"grow'up"}},
	} {
		t.Run(test.name, func(t *testing.T) {
			got := cliTestBashCandidates(t, script, function, test.words...)
			if !reflect.DeepEqual(got, test.want) {
				t.Fatalf("complete %v = %v, want %v", test.words, got, test.want)
			}
		})
	}
}

func TestCliBashCompletionQuotesCommandNames(t *testing.T) {
	command := CliSpecGroup(CliSpecSubcommand(CliSpecGroup(CliSpecCommand("child' $literal")), "", CliSpecCommand("root' $literal")))
	script, cause := cliTestRun(t, rootFiber(), command, "--completions", "bash")
	if cause != nil {
		t.Fatal(cause)
	}
	got := cliTestBashCandidates(t, script, cliCompletionFunction(command.(*cliSpec)), "root' $literal", "child")
	if !reflect.DeepEqual(got, []string{"child' $literal"}) {
		t.Fatalf("literal names were interpreted by Bash: %v", got)
	}
}
