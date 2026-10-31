package flickrt

import (
	"strconv"
	"strings"
)

// CLI descriptions retain callbacks, not their construction-time fiber.
type cliMulti uint8

const (
	cliRequired cliMulti = iota
	cliSome
)

var (
	CliMultiRequired Value = cliRequired
	CliMultiSome     Value = cliSome
	cliErrorTag            = Tag("Cli.CliError.CliError")
)

type cliArg struct {
	label, help string
	multi       cliMulti
	value       Value // the Cli.Value dictionary
}

type cliSubcommand struct {
	command *cliSpec
	help    string
}

type cliSpec struct {
	name, description string
	args              []cliArg
	subcommands       []cliSubcommand
	handler           Value // nil for a group
}

func CliSpecCommand(name Value) Value { return &cliSpec{name: name.(string)} }

func CliSpecDescription(description, spec Value) Value {
	out := *spec.(*cliSpec)
	out.description = description.(string)
	return &out
}

func CliSpecArg(dict, _lacks, _union, label, help, multi, spec Value) Value {
	out := *spec.(*cliSpec)
	out.args = make([]cliArg, len(out.args)+1)
	copy(out.args, spec.(*cliSpec).args)
	out.args[len(out.args)-1] = cliArg{label.(string), help.(string), multi.(cliMulti), dict}
	return &out
}

func CliSpecSubcommand(command, help, spec Value) Value {
	out := *spec.(*cliSpec)
	out.subcommands = make([]cliSubcommand, len(out.subcommands)+1)
	copy(out.subcommands, spec.(*cliSpec).subcommands)
	out.subcommands[len(out.subcommands)-1] = cliSubcommand{command.(*cliSpec), help.(string)}
	return &out
}

func CliSpecHandle(handler, spec Value) Value {
	out := *spec.(*cliSpec)
	out.handler = handler
	return &out
}

func CliSpecGroup(spec Value) Value {
	out := *spec.(*cliSpec)
	out.handler = nil
	return &out
}

func cliFail(message string) Value { return Fail(NewData(cliErrorTag, message)) }

func cliArgName(arg cliArg) string { return strings.ReplaceAll(arg.label, "_", "-") }

type cliInput struct {
	command *cliSpec
	texts   []string
}

// Negative numeric arguments are values, not unrecognized short flags.
func cliNegativeNumber(text string) bool {
	if !strings.HasPrefix(text, "-") || len(text) < 2 {
		return false
	}
	_, err := strconv.ParseFloat(text, 64)
	if err == nil {
		return true
	}
	if e, ok := err.(*strconv.NumError); ok && e.Err == strconv.ErrRange {
		return true
	}
	return false
}

// Routing and arity validation finish before any value parser or handler runs.
func cliRoute(root *cliSpec, args []Value) ([]cliInput, string, string) {
	inputs := []cliInput{{command: root}}
	builtin, shell := "", ""
	ended := false
	for at := 0; at < len(args); at++ {
		text := args[at].(string)
		if !ended {
			if text == "--" {
				ended = true
				continue
			}
			action := ""
			switch {
			case text == "--help" || text == "-h":
				action = "help"
			case text == "--version":
				action = "version"
			case text == "--completions":
				action = "completions"
				at++
				if at == len(args) {
					cliFail("Missing shell for --completions (supported: bash)")
				}
				shell = args[at].(string)
			case strings.HasPrefix(text, "--completions="):
				action = "completions"
				shell = strings.TrimPrefix(text, "--completions=")
			case text == "--wizard" || text == "--trace" || text == "--log-level" || strings.HasPrefix(text, "--log-level="):
				cliFail("Unsupported built-in flag on the go target: " + text)
			case strings.HasPrefix(text, "-") && text != "-" && !cliNegativeNumber(text):
				cliFail("Unknown flag: " + text)
			}
			if action != "" {
				if builtin != "" && builtin != action {
					cliFail("Choose only one of --help, --version and --completions")
				}
				builtin = action
				continue
			}
		}
		input := &inputs[len(inputs)-1]
		found := false
		if !ended && len(input.texts) >= len(input.command.args) {
			for _, sub := range input.command.subcommands {
				if sub.command.name == text {
					inputs = append(inputs, cliInput{command: sub.command})
					found = true
					break
				}
			}
		}
		if found {
			continue
		}
		if len(input.command.args) == 0 && len(input.command.subcommands) > 0 {
			cliFail("Unknown command " + strconv.Quote(text) + " for " + input.command.name)
		}
		input.texts = append(input.texts, text)
	}
	return inputs, builtin, shell
}

func cliValidate(input cliInput) {
	args, texts := input.command.args, input.texts
	if len(texts) < len(args) {
		cliFail("Missing required argument <" + cliArgName(args[len(texts)]) + "> for " + input.command.name)
	}
	for _, arg := range args {
		if arg.multi == cliSome {
			return
		}
	}
	if len(texts) > len(args) {
		cliFail("Unexpected argument " + strconv.Quote(texts[len(args)]) + " for " + input.command.name)
	}
}

func cliParseValue(fb *Fiber, parser Value, arg cliArg, text string) Value {
	parsed := Run(fb, Apply(fb, parser, text))
	if !isOk(parsed) {
		return cliFail("Invalid value " + strconv.Quote(text) + " for <" + cliArgName(arg) + ">: " + value(parsed).(string))
	}
	return value(parsed)
}

// CliRunVersion runs only the selected handler, after the entire line parses.
func CliRunVersion(fb *Fiber, version, spec, args Value) Value {
	inputs, builtin, shell := cliRoute(spec.(*cliSpec), List(args))
	selected := inputs[len(inputs)-1].command
	switch builtin {
	case "help":
		return StdoutWrite(cliHelp(selected, cliPath(inputs)))
	case "version":
		return StdoutLine(inputs[0].command.name + " v" + version.(string))
	case "completions":
		if shell != "bash" {
			return cliFail("Unsupported completion shell " + strconv.Quote(shell) + " on the go target (supported: bash)")
		}
		return StdoutWrite(cliBashCompletions(inputs[0].command))
	}
	for _, input := range inputs {
		cliValidate(input)
	}
	if selected.handler == nil {
		path := cliPath(inputs)
		StdoutWrite(cliHelp(selected, path))
		return cliFail("Missing subcommand for " + path)
	}
	var options *Record
	for i, input := range inputs {
		if i == len(inputs)-1 {
			options = newRecord(len(input.command.args))
		}
		at := 0
		for j, arg := range input.command.args {
			parser := Method(fb, arg.value, "parse")
			var parsed Value
			if arg.multi == cliSome {
				count := len(input.texts) - at - (len(input.command.args) - j - 1)
				values := make([]Value, count)
				for k := range values {
					values[k] = cliParseValue(fb, parser, arg, input.texts[at+k])
				}
				at += count
				parsed = values // NonEmpty is represented by its list.
			} else {
				parsed = cliParseValue(fb, parser, arg, input.texts[at])
				at++
			}
			if i == len(inputs)-1 {
				options.put(arg.label, parsed)
			}
		}
	}
	Run(fb, Apply(fb, selected.handler, options))
	return Unit
}
