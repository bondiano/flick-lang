package flickrt

import (
	"strconv"
	"strings"
)

// cliError is Effect's CliError: tag is its `_tag`, the fields those of
// that error, and message() its text.
type cliError struct {
	tag         string
	option      string   // UnrecognizedOption, MissingOption, InvalidValue
	argument    string   // MissingArgument
	arguments   []string // UnexpectedArgument
	value       string   // InvalidValue
	expected    string   // InvalidValue
	kind        string   // InvalidValue: "flag" or "argument"
	subcommand  string   // UnknownSubcommand
	path        []string // UnrecognizedOption command, UnknownSubcommand parent, ShowHelp commandPath
	suggestions []string
	errors      []*cliError // ShowHelp
}

func cliUnrecognized(option string, command []string, suggestions []string) *cliError {
	return &cliError{tag: "UnrecognizedOption", option: option, path: command, suggestions: suggestions}
}

func cliMissingOption(option string) *cliError {
	return &cliError{tag: "MissingOption", option: option}
}

func cliMissingArgument(argument string) *cliError {
	return &cliError{tag: "MissingArgument", argument: argument}
}

func cliInvalid(option, value, expected, kind string) *cliError {
	return &cliError{tag: "InvalidValue", option: option, value: value, expected: expected, kind: kind}
}

func cliShowHelp(path []string, errors []*cliError) *cliError {
	return &cliError{tag: "ShowHelp", path: path, errors: errors}
}

func (e *cliError) missing() bool { return e.tag == "MissingOption" || e.tag == "MissingArgument" }

func cliSuggestionText(suggestions []string) string {
	if len(suggestions) == 0 {
		return ""
	}
	return "\n\n  Did you mean this?\n    " + strings.Join(suggestions, "\n    ")
}

func (e *cliError) message() string {
	switch e.tag {
	case "UnrecognizedOption":
		base := "Unrecognized flag: " + e.option
		if e.path != nil {
			base += " in command " + strings.Join(e.path, " ")
		}
		return base + cliSuggestionText(e.suggestions)
	case "MissingOption":
		return "Missing required flag: --" + e.option
	case "MissingArgument":
		return "Missing required argument: " + e.argument
	case "UnexpectedArgument":
		label := "arguments"
		if len(e.arguments) == 1 {
			label = "argument"
		}
		quoted := make([]string, len(e.arguments))
		for i, a := range e.arguments {
			quoted[i] = QuoteJSON(a)
		}
		return "Unexpected positional " + label + ": " + strings.Join(quoted, ", ")
	case "InvalidValue":
		expectation := e.expected
		if !strings.HasPrefix(expectation, "Expected ") && !strings.HasPrefix(expectation, "Expected:") {
			expectation = "Expected: " + expectation
		}
		switch {
		case e.kind == "argument":
			return "Invalid value for argument <" + e.option + ">: \"" + e.value + "\". " + expectation
		case e.value == "":
			return "Missing value for flag --" + e.option + ". " + expectation
		}
		return "Invalid value for flag --" + e.option + ": \"" + e.value + "\". " + expectation
	case "UnknownSubcommand":
		if e.path != nil {
			return "Unknown subcommand \"" + e.subcommand + "\" for \"" + strings.Join(e.path, " ") + "\"" + cliSuggestionText(e.suggestions)
		}
		return "Unknown subcommand \"" + e.subcommand + "\"" + cliSuggestionText(e.suggestions)
	}
	return "Help requested"
}

// cliLevenshtein counts in UTF-16 code units, as auto-suggest.ts does.
func cliLevenshtein(a, b string) int {
	x, y := utf16Units(a), utf16Units(b)
	prev := make([]int, len(y)+1)
	cur := make([]int, len(y)+1)
	for j := range prev {
		prev[j] = j
	}
	for i := 1; i <= len(x); i++ {
		cur[0] = i
		for j := 1; j <= len(y); j++ {
			cost := 1
			if x[i-1] == y[j-1] {
				cost = 0
			}
			cur[j] = min(prev[j]+1, cur[j-1]+1, prev[j-1]+cost)
		}
		prev, cur = cur, prev
	}
	return prev[len(y)]
}

// cliSuggest is auto-suggest.ts's `suggest`: the candidates nearest to the
// input, within two edits.
func cliSuggest(input string, candidates []string) []string {
	best, out := 3, []string{}
	for _, c := range candidates {
		d := cliLevenshtein(input, c)
		switch {
		case d > 2:
		case d < best:
			best, out = d, []string{c}
		case d == best:
			out = append(out, c)
		}
	}
	return out
}

// cliEscapeControl is CliOutput's escapeControlCharacters.
func cliEscapeControl(text string) string {
	var b strings.Builder
	for _, r := range text {
		if r <= 0x08 || r == 0x0b || r == 0x0c || r >= 0x0e && r <= 0x1f || r == 0x7f {
			h := strconv.FormatInt(int64(r), 16)
			if len(h) < 2 {
				h = "0" + h
			}
			b.WriteString(`\x` + h)
			continue
		}
		b.WriteRune(r)
	}
	return b.String()
}
