//go:build flickcli

package flickrt

// `Cli.Command r e` and its parts as the JS runtime's `cli/spec.ts` has
// them: plain immutable data the combinators of the standard library
// build, turned into Effect's CLI model only when a command runs.

// cliSource is where the text of a value comes from before `Cli.Value`
// parses it: "text", "path", "fileText" or "fileParse".
type cliSource struct {
	kind      string
	pathType  string // "file", "directory" or "either" for a path
	mustExist bool
	format    string // the format of `file_parse`, empty for the extension's
}

// cliMods is what the modifiers of a multiplicity add to its parameter.
type cliMods struct {
	hidden        bool
	metavar       *string
	optionalValue *string
	source        cliSource
}

// cliMulti is `Cli.Multi a t`: "switch", "required", "default", "optional",
// "variadic", "some", "config" or "prompt", with what its kind carries.
type cliMulti struct {
	kind     string
	value    Value // the default
	min, max int   // of a variadic one; -1 when not given
	config   Value
	prompt   Value
	mods     cliMods
}

// cliParam is a flag or a positional argument of a command.
type cliParam struct {
	flag    bool
	label   string // the field of the options record
	aliases []string
	help    string
	multi   *cliMulti
	value   Value // the `Cli.Value` dictionary
}

type cliSubcommand struct {
	command *cliSpec
	help    string
}

type cliExample struct{ command, description string }

// cliAction is `Cli.action`: a switch that runs `run` instead of a handler.
type cliAction struct {
	name, help string
	run        Value
}

// cliSpec is `Cli.Spec r e o`, and `Cli.Command r e` once `handled` (with
// `handler` nil for a group).
type cliSpec struct {
	name, description, shortDescription, alias string
	unlisted                                   bool
	examples                                   []cliExample
	actions                                    []cliAction
	params                                     []cliParam
	subcommands                                []cliSubcommand
	handler                                    Value
}

func cliMultiOf(v Value) *cliMulti { return v.(*cliMulti) }
func cliSpecOf(v Value) *cliSpec   { return v.(*cliSpec) }

func cliMake(kind string) *cliMulti {
	return &cliMulti{kind: kind, min: -1, max: -1, mods: cliMods{source: cliSource{kind: "text"}}}
}

var (
	CliMultiSwitch   Value = cliMake("switch")
	CliMultiRequired Value = cliMake("required")
	CliMultiOptional Value = cliMake("optional")
	CliMultiMany     Value = &cliMulti{kind: "variadic", min: 0, max: -1, mods: cliMods{source: cliSource{kind: "text"}}}
	CliMultiSome     Value = cliMake("some")
)

func CliMultiDefault(value Value) Value {
	m := cliMake("default")
	m.value = value
	return m
}

func cliVariadic(min, max int) Value {
	m := cliMake("variadic")
	m.min, m.max = min, max
	return m
}

func CliMultiAtLeast(min Value) Value      { return cliVariadic(int(min.(int32)), -1) }
func CliMultiAtMost(max Value) Value       { return cliVariadic(-1, int(max.(int32))) }
func CliMultiBetween(min, max Value) Value { return cliVariadic(int(min.(int32)), int(max.(int32))) }

func CliMultiConfig(config Value) Value {
	m := cliMake("config")
	m.config = config
	return m
}

func CliMultiPrompt(prompt Value) Value {
	m := cliMake("prompt")
	m.prompt = prompt
	return m
}

// cliModify is the multiplicity with its modifiers changed by `f`.
func cliModify(m Value, f func(*cliMods)) Value {
	out := *cliMultiOf(m)
	f(&out.mods)
	return &out
}

func CliMultiHidden(m Value) Value { return cliModify(m, func(x *cliMods) { x.hidden = true }) }

func CliMultiMetavar(metavar, m Value) Value {
	s := metavar.(string)
	return cliModify(m, func(x *cliMods) { x.metavar = &s })
}

func CliMultiOptionalValue(text, m Value) Value {
	s := text.(string)
	return cliModify(m, func(x *cliMods) { x.optionalValue = &s })
}

func cliPathSource(pathType string, mustExist, m Value) Value {
	return cliModify(m, func(x *cliMods) {
		x.source = cliSource{kind: "path", pathType: pathType, mustExist: mustExist.(bool)}
	})
}

func CliMultiFile(mustExist, m Value) Value      { return cliPathSource("file", mustExist, m) }
func CliMultiDirectory(mustExist, m Value) Value { return cliPathSource("directory", mustExist, m) }
func CliMultiPath(mustExist, m Value) Value      { return cliPathSource("either", mustExist, m) }

func CliMultiFileText(m Value) Value {
	return cliModify(m, func(x *cliMods) { x.source = cliSource{kind: "fileText"} })
}

// CliMultiFileParse reads the format of a `Cli.FileFormat`, or `None` for
// the file's extension.
func CliMultiFileParse(format, m Value) Value {
	f := ""
	if v, ok := some(format); ok {
		switch ConstructorName(v.(*Data).Tag) {
		case "Ini":
			f = "ini"
		case "Json":
			f = "json"
		case "Toml":
			f = "toml"
		case "Yaml":
			f = "yaml"
		}
	}
	return cliModify(m, func(x *cliMods) { x.source = cliSource{kind: "fileParse", format: f} })
}

func CliSpecCommand(name Value) Value { return &cliSpec{name: name.(string)} }

// cliWith is a copy of the spec changed by `f`; the slices are copied
// before `f` appends, so specs never share what they add.
func cliWith(spec Value, f func(*cliSpec)) Value {
	out := *cliSpecOf(spec)
	out.examples = append([]cliExample(nil), out.examples...)
	out.actions = append([]cliAction(nil), out.actions...)
	out.params = append([]cliParam(nil), out.params...)
	out.subcommands = append([]cliSubcommand(nil), out.subcommands...)
	f(&out)
	return &out
}

func CliSpecDescription(text, spec Value) Value {
	return cliWith(spec, func(s *cliSpec) { s.description = text.(string) })
}

func CliSpecShortDescription(text, spec Value) Value {
	return cliWith(spec, func(s *cliSpec) { s.shortDescription = text.(string) })
}

func CliSpecAlias(alias, spec Value) Value {
	return cliWith(spec, func(s *cliSpec) { s.alias = alias.(string) })
}

func CliSpecUnlisted(spec Value) Value { return cliWith(spec, func(s *cliSpec) { s.unlisted = true }) }

func CliSpecExample(command, description, spec Value) Value {
	return cliWith(spec, func(s *cliSpec) {
		s.examples = append(s.examples, cliExample{command.(string), description.(string)})
	})
}

func CliSpecAction(name, help, run, spec Value) Value {
	return cliWith(spec, func(s *cliSpec) {
		s.actions = append(s.actions, cliAction{name.(string), help.(string), run})
	})
}

func cliStrings(v Value) []string {
	xs := List(v)
	out := make([]string, len(xs))
	for i, x := range xs {
		out[i] = x.(string)
	}
	return out
}

func CliSpecFlag(value, _, _, label, aliases, help, multi, spec Value) Value {
	return cliWith(spec, func(s *cliSpec) {
		s.params = append(s.params, cliParam{flag: true, label: label.(string), aliases: cliStrings(aliases),
			help: help.(string), multi: cliMultiOf(multi), value: value})
	})
}

func CliSpecArg(value, _, _, label, help, multi, spec Value) Value {
	return cliWith(spec, func(s *cliSpec) {
		s.params = append(s.params, cliParam{label: label.(string), aliases: []string{},
			help: help.(string), multi: cliMultiOf(multi), value: value})
	})
}

func CliSpecSubcommand(command, help, spec Value) Value {
	return cliWith(spec, func(s *cliSpec) {
		s.subcommands = append(s.subcommands, cliSubcommand{cliSpecOf(command), help.(string)})
	})
}

func CliSpecHandle(handler, spec Value) Value {
	return cliWith(spec, func(s *cliSpec) { s.handler = handler })
}

func CliSpecGroup(spec Value) Value { return cliWith(spec, func(s *cliSpec) { s.handler = nil }) }
