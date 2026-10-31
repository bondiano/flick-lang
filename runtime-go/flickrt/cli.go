package flickrt

import "strings"

// `Cli.Command r e` and its parts: plain data the combinators of the
// standard library build, turned into Effect's CLI model only when a
// command runs (`cli_command.go`). Descriptions retain callbacks, not their
// construction-time fiber.

// cliSource is where the text of a value comes from before `Cli.Value`
// parses it: "text", "path", "fileText" or "fileParse".
type cliSource struct {
	kind      string
	pathType  string // path: "file", "directory" or "either"
	mustExist bool
	format    string // fileParse: "ini", "json", "toml", "yaml" or "" (by extension)
}

// cliMods is what the modifiers of a multiplicity add to its parameter.
type cliMods struct {
	hidden        bool
	metavar       *string
	optionalValue *string
	source        cliSource
}

// cliMulti is `Cli.Multi a t`: how the values of a parameter make its field.
// kind: "switch", "required", "default", "optional", "variadic", "some",
// "config" or "prompt"; min and max of a variadic are -1 when absent.
type cliMulti struct {
	kind     string
	value    Value // default; config; prompt
	min, max int
	mods     cliMods
}

type cliSpecParam struct {
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

// cliAction is `Cli.action`: a flag that runs `run` instead of the command.
type cliAction struct {
	name, help string
	run        Value
}

// cliSpec is `Cli.Spec r e o`, and `Cli.Command r e` once `isCommand`;
// a command without a handler is a group.
type cliSpec struct {
	name, description, shortDescription, alias string
	unlisted                                   bool
	examples                                   []cliExample
	actions                                    []cliAction
	params                                     []cliSpecParam
	subcommands                                []cliSubcommand
	handler                                    Value
}

var cliErrorTag = Tag("Cli.CliError.CliError")

func cliMultiOf(kind string) *cliMulti {
	return &cliMulti{kind: kind, min: -1, max: -1, mods: cliMods{source: cliSource{kind: "text"}}}
}

var (
	CliMultiSwitch   Value = cliMultiOf("switch")
	CliMultiRequired Value = cliMultiOf("required")
	CliMultiOptional Value = cliMultiOf("optional")
	CliMultiMany     Value = &cliMulti{kind: "variadic", min: 0, max: -1, mods: cliMods{source: cliSource{kind: "text"}}}
	CliMultiSome     Value = cliMultiOf("some")
)

func CliMultiDefault(value Value) Value {
	m := cliMultiOf("default")
	m.value = value
	return m
}

func cliVariadic(min, max int) Value {
	m := cliMultiOf("variadic")
	m.min, m.max = min, max
	return m
}

func CliMultiAtLeast(min Value) Value      { return cliVariadic(int(min.(int32)), -1) }
func CliMultiAtMost(max Value) Value       { return cliVariadic(-1, int(max.(int32))) }
func CliMultiBetween(min, max Value) Value { return cliVariadic(int(min.(int32)), int(max.(int32))) }
func CliMultiConfig(config Value) Value    { m := cliMultiOf("config"); m.value = config; return m }
func CliMultiPrompt(prompt Value) Value    { m := cliMultiOf("prompt"); m.value = prompt; return m }
func cliModify(m Value, f func(*cliMods)) Value {
	out := *m.(*cliMulti)
	f(&out.mods)
	return &out
}

func CliMultiHidden(m Value) Value { return cliModify(m, func(o *cliMods) { o.hidden = true }) }

func CliMultiMetavar(metavar, m Value) Value {
	s := metavar.(string)
	return cliModify(m, func(o *cliMods) { o.metavar = &s })
}

func CliMultiOptionalValue(text, m Value) Value {
	s := text.(string)
	return cliModify(m, func(o *cliMods) { o.optionalValue = &s })
}

func cliPathSource(pathType string, mustExist, m Value) Value {
	return cliModify(m, func(o *cliMods) { o.source = cliSource{kind: "path", pathType: pathType, mustExist: Bool(mustExist)} })
}

func CliMultiFile(mustExist, m Value) Value      { return cliPathSource("file", mustExist, m) }
func CliMultiDirectory(mustExist, m Value) Value { return cliPathSource("directory", mustExist, m) }
func CliMultiPath(mustExist, m Value) Value      { return cliPathSource("either", mustExist, m) }

func CliMultiFileText(m Value) Value {
	return cliModify(m, func(o *cliMods) { o.source = cliSource{kind: "fileText"} })
}

// CliMultiFileParse is `Cli.file_parse`: a `Cli.FileFormat`, or `None` for
// the file's extension.
func CliMultiFileParse(format, m Value) Value {
	name := ""
	if f, ok := some(format); ok {
		name = strings.ToLower(ConstructorName(f.(*Data).Tag))
	}
	return cliModify(m, func(o *cliMods) { o.source = cliSource{kind: "fileParse", format: name} })
}

func cliWith(spec Value, f func(*cliSpec)) Value {
	out := *spec.(*cliSpec)
	f(&out)
	return &out
}

func CliSpecCommand(name Value) Value { return &cliSpec{name: name.(string)} }

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
		s.examples = append(s.examples[:len(s.examples):len(s.examples)], cliExample{command.(string), description.(string)})
	})
}

func CliSpecAction(name, help, run, spec Value) Value {
	return cliWith(spec, func(s *cliSpec) {
		s.actions = append(s.actions[:len(s.actions):len(s.actions)], cliAction{name.(string), help.(string), run})
	})
}

func cliAddParam(spec Value, p cliSpecParam) Value {
	return cliWith(spec, func(s *cliSpec) { s.params = append(s.params[:len(s.params):len(s.params)], p) })
}

func CliSpecFlag(dict, _lacks, _union, label, aliases, help, multi, spec Value) Value {
	names := make([]string, 0, len(List(aliases)))
	for _, a := range List(aliases) {
		names = append(names, a.(string))
	}
	return cliAddParam(spec, cliSpecParam{true, label.(string), names, help.(string), multi.(*cliMulti), dict})
}

func CliSpecArg(dict, _lacks, _union, label, help, multi, spec Value) Value {
	return cliAddParam(spec, cliSpecParam{false, label.(string), nil, help.(string), multi.(*cliMulti), dict})
}

func CliSpecSubcommand(command, help, spec Value) Value {
	return cliWith(spec, func(s *cliSpec) {
		s.subcommands = append(s.subcommands[:len(s.subcommands):len(s.subcommands)], cliSubcommand{command.(*cliSpec), help.(string)})
	})
}

func CliSpecHandle(handler, spec Value) Value {
	return cliWith(spec, func(s *cliSpec) { s.handler = handler })
}

func CliSpecGroup(spec Value) Value { return cliWith(spec, func(s *cliSpec) { s.handler = nil }) }
