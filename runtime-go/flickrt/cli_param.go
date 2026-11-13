//go:build flickcli

package flickrt

import (
	"errors"
	"io/fs"
	"os"
	"path/filepath"
	"strconv"
	"strings"
)

// Effect's CLI parameters (`Param.js`, `Primitive.js`) and errors
// (`CliError.js`), as far as `Cli.run` builds them from a Flick command.

// cliErr is one of Effect's `CliError`s; its fields are those of its tag.
type cliErr struct {
	tag         string
	option      string
	value       string
	expected    string
	kind        string
	argument    string
	subcommand  string
	command     []string
	parent      []string
	suggestions []string
	arguments   []string
	commandPath []string
	errors      []*cliErr
}

func (e *cliErr) message() string {
	did := ""
	if len(e.suggestions) > 0 {
		did = "\n\n  Did you mean this?\n    " + strings.Join(e.suggestions, "\n    ")
	}
	switch e.tag {
	case "UnrecognizedOption":
		return "Unrecognized flag: " + e.option + " in command " + strings.Join(e.command, " ") + did
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
			return "Invalid value for argument <" + e.option + `>: "` + e.value + `". ` + expectation
		case e.value == "":
			return "Missing value for flag --" + e.option + ". " + expectation
		}
		return "Invalid value for flag --" + e.option + `: "` + e.value + `". ` + expectation
	case "UnknownSubcommand":
		return `Unknown subcommand "` + e.subcommand + `" for "` + strings.Join(e.parent, " ") + `"` + did
	}
	return "Help requested"
}

func cliMissing(e *cliErr) bool {
	return e != nil && (e.tag == "MissingOption" || e.tag == "MissingArgument")
}

// cliPrim is Effect's primitive type of a parameter.
type cliPrim struct {
	tag       string // "String", "Boolean", "Path", "FileText", "FileParse" or "Choice"
	pathType  string
	mustExist bool
	format    string
	choices   [][2]string // a choice's key and value
}

func (p *cliPrim) choiceKeys() []string {
	keys := make([]string, len(p.choices))
	for i, c := range p.choices {
		keys[i] = c[0]
	}
	return keys
}

func (p *cliPrim) typeName() string {
	switch p.tag {
	case "Boolean":
		return "boolean"
	case "String":
		return "string"
	case "Path":
		return "path"
	case "Choice":
		return "choice"
	}
	return "file"
}

var (
	cliTrue  = map[string]bool{"true": true, "yes": true, "on": true, "1": true, "y": true}
	cliFalse = map[string]bool{"false": true, "no": true, "off": true, "0": true, "n": true}
)

// parse reads the text as the primitive does, or says what it expected.
func (p *cliPrim) parse(text string) (Value, string) {
	switch p.tag {
	case "Boolean":
		if cliTrue[text] || cliFalse[text] {
			return cliTrue[text], ""
		}
		return nil, `Expected "true" | "yes" | "on" | "1" | "y" | "false" | "no" | "off" | "0" | "n"`
	case "Choice":
		for _, c := range p.choices {
			if c[0] == text {
				return c[1], ""
			}
		}
		quoted := make([]string, len(p.choices))
		for i, c := range p.choices {
			quoted[i] = QuoteJSON(c[0])
		}
		return nil, strings.Join(quoted, " | ")
	case "Path":
		return cliParsePath(text, p.pathType, p.mustExist)
	case "FileText":
		return cliFileText(text)
	case "FileParse":
		return cliFileParse(text, p.format)
	}
	return text, ""
}

// cliAbsolute is the path as Node's `path.resolve` makes it absolute.
func cliAbsolute(path string) string {
	if filepath.IsAbs(path) {
		return path
	}
	abs, err := filepath.Abs(path)
	if err != nil {
		return path
	}
	return abs
}

// cliStat is the kind of the file at the path, "" when there is none, as
// Effect's `exists` and `stat` see it.
func cliStat(path, what string) (string, string) {
	fi, err := os.Stat(path)
	switch {
	case errors.Is(err, fs.ErrNotExist):
		return "", ""
	case err != nil:
		return "", "Failed to check " + what + " existence: " + err.Error()
	case fi.IsDir():
		return "Directory", ""
	case fi.Mode().IsRegular():
		return "File", ""
	}
	return "Other", ""
}

func cliParsePath(value, pathType string, mustExist bool) (Value, string) {
	abs := cliAbsolute(value)
	kind, problem := cliStat(abs, "path")
	switch {
	case problem != "":
		return nil, problem
	case mustExist && kind == "":
		return nil, "Path does not exist: " + abs
	case kind != "" && pathType == "file" && kind != "File":
		return nil, "Path is not a file: " + abs
	case kind != "" && pathType == "directory" && kind != "Directory":
		return nil, "Path is not a directory: " + abs
	}
	return abs, ""
}

func cliFileText(path string) (Value, string) {
	abs := cliAbsolute(path)
	kind, problem := cliStat(abs, "file")
	switch {
	case problem != "":
		return nil, problem
	case kind == "":
		return nil, "File does not exist: " + abs
	case kind != "File":
		return nil, "Path is not a file: " + abs
	}
	b, err := os.ReadFile(abs)
	if err != nil {
		return nil, "Failed to read file: " + err.Error()
	}
	return decodeText(b), ""
}

// cliSingle is Effect's `Single` parameter.
type cliSingle struct {
	kind          string // "flag" or "argument"
	name          string
	aliases       []string
	description   *string
	hidden        bool
	typeName      *string
	optionalValue *string
	prim          *cliPrim
}

func (s *cliSingle) typeLabel() string {
	if s.typeName != nil {
		return *s.typeName
	}
	return s.prim.typeName()
}

// cliArgs is what a parameter parses: the values of flags and the
// arguments left.
type cliArgs struct {
	flags     cliFlagMap
	arguments []string
}

// cliEParam is an Effect parameter: a single, the parse of its value into
// a Flick value (`mapEffect`), and how its occurrences make the field.
type cliEParam struct {
	single   *cliSingle
	mapValue func(fb *Fiber, v Value) (Value, *cliErr)
	wrap     string // "", "default", "optional", "variadic", "config" or "prompt"
	def      Value
	min, max int // of a variadic one; -1 when not given
	config   Value
	prompt   Value
}

func (p *cliEParam) isOptional() bool { return p.wrap == "default" || p.wrap == "optional" }

func (p *cliEParam) isVariadic() bool { return p.wrap == "variadic" }

func cliFlagSingles(params []*cliEParam) []*cliSingle {
	var out []*cliSingle
	for _, p := range params {
		if p.single.kind == "flag" {
			out = append(out, p.single)
		}
	}
	return out
}

// parseSingle is the first value of the single, as its primitive reads it.
func (p *cliEParam) parseSingle(fb *Fiber, args cliArgs) ([]string, Value, *cliErr) {
	s := p.single
	var text string
	rest := args.arguments
	if s.kind == "argument" {
		if len(rest) == 0 {
			return nil, nil, &cliErr{tag: "MissingArgument", argument: s.name}
		}
		text, rest = rest[0], rest[1:]
	} else {
		values := args.flags[s.name]
		if len(values) == 0 {
			return nil, nil, &cliErr{tag: "MissingOption", option: s.name}
		}
		text = values[0]
	}
	v, expected := s.prim.parse(text)
	if v == nil {
		return nil, nil, &cliErr{tag: "InvalidValue", option: s.name, value: text, expected: expected, kind: s.kind}
	}
	if p.mapValue != nil {
		mapped, err := p.mapValue(fb, v)
		if err != nil {
			return nil, nil, err
		}
		v = mapped
	}
	return rest, v, nil
}

// parse is the field of the parameter from the arguments, and the
// arguments it left.
func (p *cliEParam) parse(fb *Fiber, args cliArgs) ([]string, Value, *cliErr) {
	switch p.wrap {
	case "default", "optional":
		rest, v, err := p.parseSingle(fb, args)
		switch {
		case cliMissing(err) && p.wrap == "default":
			return args.arguments, p.def, nil
		case cliMissing(err):
			return args.arguments, None, nil
		case err != nil:
			return nil, nil, err
		case p.wrap == "optional":
			return rest, Some(v), nil
		}
		return rest, v, nil
	case "variadic":
		return p.parseVariadic(fb, args)
	case "config", "prompt":
		rest, v, err := p.parseSingle(fb, args)
		if !cliMissing(err) {
			return rest, v, err
		}
		if p.wrap == "prompt" {
			if answer, quit := promptRunValue(fb, p.prompt); !quit {
				return args.arguments, answer, nil
			}
			return nil, nil, err
		}
		value, found, message := configLookup(fb, p.config)
		switch {
		case message != "":
			kind, option := "flag", err.option
			if err.tag == "MissingArgument" {
				kind, option = "argument", err.argument
			}
			return nil, nil, &cliErr{tag: "InvalidValue", option: option, value: "config", expected: message, kind: kind}
		case !found:
			return nil, nil, err
		}
		return args.arguments, value, nil
	}
	return p.parseSingle(fb, args)
}

func (p *cliEParam) parseVariadic(fb *Fiber, args cliArgs) ([]string, Value, *cliErr) {
	s := p.single
	results := []Value{}
	plural := func(n int) string {
		if n == 1 {
			return ""
		}
		return "s"
	}
	if s.kind == "argument" {
		count, current := 0, args.arguments
		for len(current) > 0 && (p.max < 0 || count < p.max) {
			rest, v, err := p.parseSingle(fb, cliArgs{args.flags, current})
			if err != nil {
				return nil, nil, err
			}
			results, current = append(results, v), rest
			count++
		}
		if count < max(p.min, 0) {
			if count == 0 {
				return nil, nil, &cliErr{tag: "MissingArgument", argument: s.name}
			}
			return nil, nil, &cliErr{tag: "InvalidValue", option: s.name, value: strconv.Itoa(count) + " values",
				expected: "at least " + strconv.Itoa(p.min) + " value" + plural(p.min), kind: s.kind}
		}
		return current, results, nil
	}
	var values []string
	for _, name := range append([]string{s.name}, s.aliases...) {
		values = append(values, args.flags[name]...)
	}
	count := len(values)
	if p.min >= 0 && count < p.min {
		if count == 0 {
			return nil, nil, &cliErr{tag: "MissingOption", option: s.name}
		}
		return nil, nil, &cliErr{tag: "InvalidValue", option: s.name, value: strconv.Itoa(count) + " occurrences",
			expected: "at least " + strconv.Itoa(p.min) + " value" + plural(p.min), kind: s.kind}
	}
	if p.max >= 0 && count > p.max {
		return nil, nil, &cliErr{tag: "InvalidValue", option: s.name, value: strconv.Itoa(count) + " occurrences",
			expected: "at most " + strconv.Itoa(p.max) + " value" + plural(p.max), kind: s.kind}
	}
	for _, value := range values {
		_, v, err := p.parseSingle(fb, cliArgs{cliFlagMap{s.name: {value}}, []string{}})
		if err != nil {
			return nil, nil, err
		}
		results = append(results, v)
	}
	return args.arguments, results, nil
}

// cliParamOf is Effect's parameter of a Flick one, as `run.ts` builds it.
func cliParamOf(kind string, p *cliParam) *cliEParam {
	m := p.multi
	s := &cliSingle{kind: kind, name: strings.ReplaceAll(p.label, "_", "-"), aliases: []string{}}
	for _, a := range p.aliases {
		s.aliases = append(s.aliases, strings.TrimLeft(a, "-"))
	}
	if p.help != "" {
		help := p.help
		s.description = &help
	}
	s.hidden = m.mods.hidden
	s.typeName = m.mods.metavar
	if kind == "flag" {
		s.optionalValue = m.mods.optionalValue
	}
	out := &cliEParam{single: s, min: -1, max: -1}
	if m.kind == "switch" {
		s.prim = &cliPrim{tag: "Boolean"}
		out.wrap, out.def = "default", false
		return out
	}
	src := m.mods.source
	switch src.kind {
	case "path":
		s.prim = &cliPrim{tag: "Path", pathType: src.pathType, mustExist: src.mustExist}
	case "fileText":
		s.prim = &cliPrim{tag: "FileText"}
	case "fileParse":
		s.prim = &cliPrim{tag: "FileParse", format: src.format}
	default:
		s.prim = &cliPrim{tag: "String"}
	}
	out.mapValue = func(fb *Fiber, v Value) (Value, *cliErr) {
		// a primitive's value is text here: a parsed file comes as its JSON
		text := v.(string)
		result := Run(fb, Apply(Method(p.value, "parse"), text)).(*Data)
		if result.Tag == "Success" {
			return result.Fields[0], nil
		}
		return nil, &cliErr{tag: "InvalidValue", option: p.label, value: text, expected: result.Fields[0].(string), kind: kind}
	}
	switch m.kind {
	case "default":
		out.wrap, out.def = "default", m.value
	case "optional":
		out.wrap = "optional"
	case "variadic":
		out.wrap, out.min, out.max = "variadic", m.min, m.max
	case "some":
		out.wrap, out.min = "variadic", 1
	case "config":
		out.wrap, out.config = "config", m.config
	case "prompt":
		out.wrap, out.prompt = "prompt", m.prompt
	}
	return out
}
