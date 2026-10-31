package flickrt

import (
	"os"
	"strings"
)

// Command.runWith and run.ts: the built-in global flags, the run of a
// command line, and `Cli.run_with`. Invalid input becomes the Flick error
// `Cli.CliError` with Effect's text; the handler runs only after the whole
// command line parsed.

func cliBoolFlag(name, alias, description string) *cliParam {
	s := &cliSingle{kind: "flag", name: name, prim: cliBoolean, description: &description}
	if alias != "" {
		s.aliases = []string{alias}
	}
	inner := cliOptionalParse(cliSingleParse(s))
	return &cliParam{single: s, optional: true, min: -1, parse: func(fb *Fiber, a cliArgs) ([]string, Value, *cliError) {
		left, v, err := inner(fb, a)
		if err != nil {
			return nil, nil, err
		}
		if x, ok := some(v); ok {
			return left, x, nil
		}
		return left, false, nil
	}}
}

func cliChoiceFlag(name string, keys []string, values []string, metavar, description string) *cliParam {
	vs := make([]Value, len(values))
	for i, v := range values {
		vs[i] = v
	}
	s := &cliSingle{kind: "flag", name: name, prim: &cliPrim{tag: "Choice", choiceKeys: keys, choiceValues: vs}, typeName: &metavar, description: &description}
	return &cliParam{single: s, optional: true, min: -1, parse: cliOptionalParse(cliSingleParse(s))}
}

// cliContext is GlobalFlag.HandlerContext, with where the command prints.
type cliContext struct {
	root     *cliCommand
	path     []string
	version  string
	builtIns []*cliGlobal
	env      *cliEnv
}

var (
	cliHelpGlobal = &cliGlobal{action: true, id: "help", param: cliBoolFlag("help", "h", "Show help information"),
		run: func(fb *Fiber, _ Value, ctx *cliContext) {
			ctx.env.log(ctx.env.format.help(fb, cliHelpFor(ctx.root, ctx.path, ctx.builtIns)))
		}}
	cliVersionGlobal = &cliGlobal{action: true, id: "version", param: cliBoolFlag("version", "v", "Show version information"),
		run: func(fb *Fiber, _ Value, ctx *cliContext) {
			ctx.env.log(ctx.env.format.version(fb, ctx.root.name, ctx.version))
		}}
	cliWizardGlobal      = &cliGlobal{action: true, id: "wizard", param: cliBoolFlag("wizard", "", "Start wizard mode for a command")}
	cliCompletionsGlobal = &cliGlobal{action: true, id: "completions",
		param: cliChoiceFlag("completions", []string{"bash", "zsh", "fish", "powershell", "sh", "pwsh"},
			[]string{"bash", "zsh", "fish", "powershell", "bash", "powershell"}, "<bash|zsh|fish|powershell|sh|pwsh>", "Print shell completion script"),
		run: func(fb *Fiber, shell Value, ctx *cliContext) {
			if s, ok := some(shell); ok {
				ctx.env.log(cliGenerateCompletions(ctx.root.name, s.(string), cliDescriptor(ctx.root, nil)))
			}
		}}
	cliLogLevelGlobal = &cliGlobal{id: "log-level",
		param: cliChoiceFlag("log-level", []string{"all", "trace", "debug", "info", "warn", "warning", "error", "fatal", "none"},
			[]string{"All", "Trace", "Debug", "Info", "Warn", "Warn", "Error", "Fatal", "None"},
			"<all|trace|debug|info|warn|warning|error|fatal|none>", "Sets the minimum log level")}
	// cliTraceGlobal is `--trace`, a flag of Flick's rather than Effect's.
	cliTraceGlobal = &cliGlobal{id: "trace", param: cliBoolFlag("trace", "", "Send spans, logs and metrics to OTEL_EXPORTER_OTLP_ENDPOINT")}
)

var cliBuiltIns = map[string]*cliGlobal{
	"HelpFlag": cliHelpGlobal, "VersionFlag": cliVersionGlobal, "WizardFlag": cliWizardGlobal,
	"CompletionsFlag": cliCompletionsGlobal, "LogLevelFlag": cliLogLevelGlobal,
}

var cliAllBuiltIns = []*cliGlobal{cliHelpGlobal, cliVersionGlobal, cliWizardGlobal, cliCompletionsGlobal, cliLogLevelGlobal}

// cliActionGlobal is run.ts's `action`.
func cliActionGlobal(a cliAction) *cliGlobal {
	return &cliGlobal{action: true, id: a.name, param: cliBoolFlag(a.name, "", a.help),
		run: func(fb *Fiber, present Value, ctx *cliContext) {
			if present.(bool) {
				path := make([]Value, len(ctx.path))
				for i, p := range ctx.path {
					path[i] = p
				}
				Run(fb, Apply(fb, a.run, Rec("commandPath", path, "version", ctx.version)))
			}
		}}
}

// cliFormatter is the part of CliOutput.Formatter a run uses.
type cliFormatter struct {
	help    func(fb *Fiber, doc *cliHelpDoc) string
	version func(fb *Fiber, name, version string) string
}

func cliDefaultFormat(colors bool) cliFormatter {
	c := cliColors{colors}
	return cliFormatter{
		func(_ *Fiber, doc *cliHelpDoc) string { return cliFormatHelp(doc, c) },
		func(_ *Fiber, name, version string) string { return cliFormatVersion(name, version, c) },
	}
}

// cliEnv is how a command line runs: its version, built-in flags,
// formatter and Console.log.
type cliEnv struct {
	version  string
	builtIns []*cliGlobal
	trace    bool
	format   cliFormatter
	log      func(string)
}

// cliRun is Command.runWith with `renderErrors: false`: help asked for by
// an error is printed before the error returns.
func cliRun(fb *Fiber, spec *cliSpec, args []string, env *cliEnv) *cliError {
	root := cliCommandOf(spec)
	if env.trace {
		root.globals = cliDedupe(append(root.globals, cliTraceGlobal))
	}
	err := cliRunParsed(fb, root, args, env)
	if err != nil && err.tag == "ShowHelp" {
		env.log(env.format.help(fb, cliHelpFor(root, err.path, env.builtIns)))
	}
	return err
}

func cliRunParsed(fb *Fiber, root *cliCommand, args []string, env *cliEnv) *cliError {
	tokens, trailing := cliLex(args)
	all := cliGlobalsOfTree(root, env.builtIns)
	singles := make([]*cliSingle, len(all))
	for i, g := range all {
		singles[i] = g.param.single
	}
	flagMap, remainder, globalErrors := cliConsumeGlobals(tokens, root, cliNewRegistry(singles))
	empty := cliArgs{flagMap, nil}
	parsed := cliParseArgs(remainder, trailing, root, nil)
	path := append([]string{root.name}, cliCommandPath(parsed)...)
	ctx := &cliContext{root, path, env.version, env.builtIns, env}
	active := cliGlobalsOn(root, path, env.builtIns)

	isActive := map[*cliGlobal]bool{}
	for _, g := range active {
		isActive[g] = true
	}
	outOfScope, seen := []*cliError{}, map[string]bool{}
	for _, g := range all {
		option := "--" + g.param.single.name
		if !isActive[g] && len(flagMap[g.param.single.name]) > 0 && !seen[option] {
			seen[option] = true
			outOfScope = append(outOfScope, cliUnrecognized(option, path, []string{}))
		}
	}
	if len(outOfScope) > 0 || len(globalErrors) > 0 {
		return cliShowHelp(path, append(append(globalErrors, outOfScope...), parsed.errors...))
	}

	for _, g := range active {
		if !g.action || len(flagMap[g.param.single.name]) == 0 {
			continue
		}
		_, v, err := g.param.parse(fb, empty)
		if err != nil {
			return err
		}
		if g == cliWizardGlobal {
			return cliWizard(fb, root, args, ctx)
		}
		g.run(fb, v, ctx)
		return nil
	}

	if len(parsed.errors) > 0 {
		return cliShowHelp(path, parsed.errors)
	}
	input, err := root.parse(fb, parsed)
	if err != nil {
		return cliShowHelp(path, []*cliError{err})
	}

	level, trace := Value(None), false
	for _, g := range active {
		if g.action {
			continue
		}
		_, v, err := g.param.parse(fb, empty)
		if err != nil {
			return err
		}
		switch g {
		case cliLogLevelGlobal:
			level = v
		case cliTraceGlobal:
			trace = v.(bool)
		}
	}
	traced := func(fb *Fiber, name string, run func(*Fiber) Value) Value {
		if trace {
			return cliTraced(fb, name, run)
		}
		return run(fb)
	}
	program := func(fb *Fiber) Value {
		if handleErr := root.handle(fb, input, []string{root.name}, traced); handleErr != nil {
			err = handleErr
		}
		return Unit
	}
	if l, ok := some(level); ok {
		withEnv(fb, minimumLogLevelKey, l, Eff(program))
	} else {
		program(fb)
	}
	return err
}

// cliFlickError is run.ts's `cliError`: help asked for with the errors that
// caused it says them.
func cliFlickError(err *cliError) Value {
	text := err.message()
	if err.tag == "ShowHelp" && len(err.errors) > 0 {
		messages := make([]string, len(err.errors))
		for i, e := range err.errors {
			messages[i] = e.message()
		}
		text = strings.Join(messages, "\n")
	}
	return NewData(cliErrorTag, text)
}

func cliStrings(v Value) []string {
	out := make([]string, len(List(v)))
	for i, x := range List(v) {
		out[i] = x.(string)
	}
	return out
}

func cliStdoutLog(text string) { StdoutLine(text) }

// CliRunWith is `Cli.run_with settings command args`.
func CliRunWith(fb *Fiber, settings, spec, args Value) Value {
	s := record(settings)
	env := &cliEnv{version: s.Get("version").(string), log: cliStdoutLog}
	for _, b := range List(s.Get("builtIns")) {
		name := ConstructorName(b.(*Data).Tag)
		if g, ok := cliBuiltIns[name]; ok {
			env.builtIns = append(env.builtIns, g)
		} else if name == "TraceFlag" {
			env.trace = true
		}
	}
	env.format = cliDefaultFormat(fileIsTerminal(os.Stdout) && os.Getenv("NO_COLOR") == "")
	if f, ok := some(s.Get("formatter")); ok {
		formatHelp, formatVersion := record(f).Get("formatHelpDoc"), record(f).Get("formatVersion")
		env.format = cliFormatter{
			func(fb *Fiber, doc *cliHelpDoc) string { return Apply(fb, formatHelp, cliDocRecord(doc)).(string) },
			func(fb *Fiber, name, version string) string {
				return Apply(fb, Apply(fb, formatVersion, name), version).(string)
			},
		}
	}
	if err := cliRun(fb, spec.(*cliSpec), cliStrings(args), env); err != nil {
		return Fail(cliFlickError(err))
	}
	return Unit
}

// CliRunVersion is `Cli.run_version version command args`: every built-in flag.
func CliRunVersion(fb *Fiber, version, spec, args Value) Value {
	env := &cliEnv{version: version.(string), builtIns: cliAllBuiltIns, trace: true, log: cliStdoutLog,
		format: cliDefaultFormat(fileIsTerminal(os.Stdout) && os.Getenv("NO_COLOR") == "")}
	if err := cliRun(fb, spec.(*cliSpec), cliStrings(args), env); err != nil {
		return Fail(cliFlickError(err))
	}
	return Unit
}

// cliPrinted is run.ts's `printed`: what the command prints for `args`
// with every built-in flag; failures are ignored.
func cliPrinted(fb *Fiber, spec *cliSpec, args []string, format cliFormatter) string {
	lines := []string{}
	env := &cliEnv{version: "0.0.0", builtIns: cliAllBuiltIns, trace: true, format: format,
		log: func(s string) { lines = append(lines, s) }}
	cliRun(fb, spec, args, env)
	return strings.Join(lines, "\n")
}

// CliCompletions is `Cli.completions shell command`.
func CliCompletions(fb *Fiber, shell, spec Value) Value {
	name := strings.ToLower(ConstructorName(shell.(*Data).Tag))
	return cliPrinted(fb, spec.(*cliSpec), []string{"--completions", name}, cliDefaultFormat(false))
}

// CliHelpDoc is `Cli.help_doc path command`: an unknown subcommand gives
// the help of the command above it.
func CliHelpDoc(fb *Fiber, path, spec Value) Value {
	var captured *cliHelpDoc
	plain := cliDefaultFormat(false)
	capture := cliFormatter{func(_ *Fiber, doc *cliHelpDoc) string { captured = doc; return "" }, plain.version}
	cliPrinted(fb, spec.(*cliSpec), append(cliStrings(path), "--help"), capture)
	if captured == nil {
		return Die("Effect's CLI printed no help")
	}
	return cliDocRecord(captured)
}

// CliDefaultFormatter is `Cli.default_formatter colors`.
func CliDefaultFormatter(colors Value) Value {
	c := cliColors{colors.(bool)}
	return Rec(
		"formatHelpDoc", Fn(func(_ *Fiber, doc Value) Value { return cliFormatHelp(cliDocOf(doc), c) }),
		"formatVersion", Curry2(func(_ *Fiber, name, version Value) Value {
			return cliFormatVersion(name.(string), version.(string), c)
		}),
	)
}
