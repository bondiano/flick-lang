//go:build flickcli

package flickrt

import (
	"os"
	"slices"
	"strings"
)

// `Cli.run_with` as `run.ts` runs Effect's `Command.runWith`: the global
// flags (`GlobalFlag.js`) first, then the command line, and the handler
// only once it all parsed. Invalid input is `Cli.CliError` with Effect's
// text, after the help of the command it was for.

// cliGlobal is Effect's global flag: an action, which runs instead of the
// handler, or a setting the handler sees.
type cliGlobal struct {
	setting bool
	param   *cliEParam
	run     func(fb *Fiber, value Value, ctx *cliRunCtx)
}

// cliRunCtx is what an action sees, and where the run writes.
type cliRunCtx struct {
	builtIns    []*cliGlobal
	command     *cliCmd
	commandPath []string
	version     string
	formatter   *cliFormatter
	log         func(text string)
}

func cliFlag(name string, prim *cliPrim, description string) *cliSingle {
	return &cliSingle{kind: "flag", name: name, aliases: []string{}, description: &description, prim: prim}
}

// cliSwitch is `Flag.Boolean(name)` with a description, false by default.
func cliSwitch(name, alias, description string) *cliEParam {
	s := cliFlag(name, &cliPrim{tag: "Boolean"}, description)
	if alias != "" {
		s.aliases = []string{alias}
	}
	return &cliEParam{single: s, wrap: "default", def: false, min: -1, max: -1}
}

// cliChoice is an optional `Flag.ChoiceWithValue` with a metavar.
func cliChoice(name, metavar, description string, choices [][2]string) *cliEParam {
	s := cliFlag(name, &cliPrim{tag: "Choice", choices: choices}, description)
	s.typeName = &metavar
	return &cliEParam{single: s, wrap: "optional", min: -1, max: -1}
}

var (
	cliHelp = &cliGlobal{param: cliSwitch("help", "h", "Show help information"),
		run: func(fb *Fiber, _ Value, ctx *cliRunCtx) {
			ctx.log(ctx.formatter.helpDoc(cliHelpOn(ctx.command, ctx.commandPath, ctx.builtIns)))
		}}
	cliVersion = &cliGlobal{param: cliSwitch("version", "v", "Show version information"),
		run: func(fb *Fiber, _ Value, ctx *cliRunCtx) {
			ctx.log(ctx.formatter.version(ctx.command.name, ctx.version))
		}}
	cliWizard      = &cliGlobal{param: cliSwitch("wizard", "", "Start wizard mode for a command")}
	cliCompletions = &cliGlobal{
		param: cliChoice("completions", "<bash|zsh|fish|powershell|sh|pwsh>", "Print shell completion script",
			[][2]string{{"bash", "bash"}, {"zsh", "zsh"}, {"fish", "fish"}, {"powershell", "powershell"}, {"sh", "bash"}, {"pwsh", "powershell"}}),
		run: func(fb *Fiber, shell Value, ctx *cliRunCtx) {
			if s, ok := some(shell); ok {
				ctx.log(cliCompletionScript(ctx.command.name, s.(string), cliDescriptorOf(ctx.command, nil)))
			}
		}}
	cliLogLevel = &cliGlobal{setting: true,
		param: cliChoice("log-level", "<all|trace|debug|info|warn|warning|error|fatal|none>", "Sets the minimum log level",
			[][2]string{{"all", "All"}, {"trace", "Trace"}, {"debug", "Debug"}, {"info", "Info"}, {"warn", "Warn"},
				{"warning", "Warn"}, {"error", "Error"}, {"fatal", "Fatal"}, {"none", "None"}})}
	// cliTrace is Flick's `--trace`, a setting beside Effect's flags.
	cliTrace = &cliGlobal{setting: true,
		param: cliSwitch("trace", "", "Send spans, logs and metrics to OTEL_EXPORTER_OTLP_ENDPOINT")}
	cliBuiltIns = map[string]*cliGlobal{"HelpFlag": cliHelp, "VersionFlag": cliVersion, "WizardFlag": cliWizard,
		"CompletionsFlag": cliCompletions, "LogLevelFlag": cliLogLevel}
	cliAllBuiltIns = []*cliGlobal{cliHelp, cliVersion, cliWizard, cliCompletions, cliLogLevel}
)

// cliActionFlag is `Cli.action`: the switch runs the Flick action.
func cliActionFlag(a cliAction) *cliGlobal {
	return &cliGlobal{param: cliSwitch(a.name, "", a.help), run: func(fb *Fiber, present Value, ctx *cliRunCtx) {
		if present.(bool) {
			path := make([]Value, len(ctx.commandPath))
			for i, p := range ctx.commandPath {
				path[i] = p
			}
			Run(fb, Apply(a.run, Rec("commandPath", path, "version", ctx.version)))
		}
	}}
}

// cliRoot is the command of a spec, with `--trace` when `trace`.
func cliRoot(spec *cliSpec, trace bool) *cliCmd {
	c := cliCommandOf(spec)
	if trace {
		c.globalFlags = cliDedupe(append(slices.Clip(c.globalFlags), cliTrace))
	}
	return c
}

// cliRun runs the command on the arguments; a `CliError` is returned, a
// failure of the handler fails the fiber.
func cliRun(fb *Fiber, cmd *cliCmd, ctx *cliRunCtx, args []string) *cliErr {
	err := cliRunLine(fb, cmd, ctx, args)
	if err != nil && err.tag == "ShowHelp" {
		ctx.log(ctx.formatter.helpDoc(cliHelpOn(cmd, err.commandPath, ctx.builtIns)))
	}
	return err
}

func cliRunLine(fb *Fiber, cmd *cliCmd, ctx *cliRunCtx, args []string) *cliErr {
	tokens, trailing := cliLex(args)
	all := cliGlobalsOf(cmd, ctx.builtIns)
	var singles []*cliSingle
	for _, g := range all {
		singles = append(singles, g.param.single)
	}
	flagMap, remainder, globalErrs := cliConsumeGlobal(tokens, cmd, cliRegistryOf(singles))
	empty := cliArgs{flagMap, []string{}}
	parsed := cliParseArgs(remainder, trailing, cmd, nil)
	path := append([]string{cmd.name}, cliCommandPath(parsed)...)
	run := *ctx
	run.command, run.commandPath = cmd, path
	active := cliGlobalsOn(cmd, path, ctx.builtIns)
	var outOfScope []*cliErr
	seen := map[string]bool{}
	for _, g := range all {
		name := g.param.single.name
		if slices.Contains(active, g) || len(flagMap[name]) == 0 || seen[name] {
			continue
		}
		seen[name] = true
		outOfScope = append(outOfScope, &cliErr{tag: "UnrecognizedOption", option: "--" + name, suggestions: []string{}, command: path})
	}
	if len(outOfScope) > 0 || len(globalErrs) > 0 {
		errs := append(append(globalErrs, outOfScope...), parsed.errors...)
		return &cliErr{tag: "ShowHelp", commandPath: path, errors: errs}
	}
	for _, g := range active {
		if g.setting || len(flagMap[g.param.single.name]) == 0 {
			continue
		}
		_, value, err := g.param.parse(fb, empty)
		if err != nil {
			return err
		}
		if g == cliWizard {
			return cliRunWizard(fb, cmd, &run, args)
		}
		g.run(fb, value, &run)
		return nil
	}
	if len(parsed.errors) > 0 {
		return &cliErr{tag: "ShowHelp", commandPath: path, errors: parsed.errors}
	}
	input, err := cmd.parse(fb, parsed)
	if err != nil {
		return &cliErr{tag: "ShowHelp", commandPath: path, errors: []*cliErr{err}}
	}
	level := None
	if slices.Contains(active, cliLogLevel) {
		if _, level, err = cliLogLevel.param.parse(fb, empty); err != nil {
			return err
		}
	}
	for _, g := range active {
		if g.setting && g != cliLogLevel {
			if _, _, err := g.param.parse(fb, empty); err != nil {
				return err
			}
		}
	}
	if l, ok := some(level); ok {
		var out *cliErr
		withMinimumLogLevel(fb, l.(string), Eff(func(fb *Fiber) Value {
			out = cmd.handle(fb, input, []string{cmd.name})
			return Unit
		}))
		return out
	}
	return cmd.handle(fb, input, []string{cmd.name})
}

var cliErrorTag = Tag("Cli.CliError.CliError")

// cliFail fails with the Flick `CliError` of Effect's error: the errors
// that asked for help, one per line.
func cliFail(err *cliErr) Value {
	message := err.message()
	if err.tag == "ShowHelp" && len(err.errors) > 0 {
		lines := make([]string, len(err.errors))
		for i, e := range err.errors {
			lines[i] = e.message()
		}
		message = strings.Join(lines, "\n")
	}
	return Fail(NewData(cliErrorTag, message))
}

// cliDetectColors is Effect's: stdout is a terminal and NO_COLOR is unset.
func cliDetectColors() bool { return isTTY(os.Stdout.Fd()) && os.Getenv("NO_COLOR") == "" }

func CliRunWith(fb *Fiber, settings, spec, args Value) Value {
	s := record(settings)
	var builtIns []*cliGlobal
	trace := false
	for _, b := range List(s.Get("builtIns")) {
		name := ConstructorName(b.(*Data).Tag)
		if g, ok := cliBuiltIns[name]; ok {
			builtIns = append(builtIns, g)
		}
		trace = trace || name == "TraceFlag"
	}
	formatter := cliDefaultFormatterOf(cliDetectColors())
	if f, ok := some(s.Get("formatter")); ok {
		formatter = cliFlickFormatter(f)
	}
	console := currentConsole(fb)
	ctx := &cliRunCtx{builtIns: builtIns, version: s.Get("version").(string), formatter: formatter, log: console.log}
	if err := cliRun(fb, cliRoot(cliSpecOf(spec), trace), ctx, cliStrings(args)); err != nil {
		return cliFail(err)
	}
	return Unit
}

func CliRunVersion(fb *Fiber, version, spec, args Value) Value {
	ctx := &cliRunCtx{builtIns: cliAllBuiltIns, version: version.(string),
		formatter: cliDefaultFormatterOf(cliDetectColors()), log: currentConsole(fb).log}
	if err := cliRun(fb, cliRoot(cliSpecOf(spec), true), ctx, cliStrings(args)); err != nil {
		return cliFail(err)
	}
	return Unit
}

// cliPrinted is what the command prints for the arguments, every built-in
// flag on and failures ignored, as `run.ts` captures it.
func cliPrinted(fb *Fiber, spec Value, args []string, formatter *cliFormatter) []string {
	var lines []string
	ctx := &cliRunCtx{builtIns: cliAllBuiltIns, version: "0.0.0", formatter: formatter,
		log: func(text string) { lines = append(lines, text) }}
	cliRun(fb, cliRoot(cliSpecOf(spec), true), ctx, args)
	return lines
}

func CliCompletions(fb *Fiber, shell, spec Value) Value {
	name := strings.ToLower(ConstructorName(shell.(*Data).Tag))
	return strings.Join(cliPrinted(fb, spec, []string{"--completions", name}, cliDefaultFormatterOf(false)), "\n")
}

// CliHelpDoc is the help `--help` gives after the path: an unknown
// subcommand gives the help of the command above it.
func CliHelpDoc(fb *Fiber, path, spec Value) Value {
	var doc *cliHelpDoc
	capture := *cliDefaultFormatterOf(false)
	capture.helpDoc = func(d *cliHelpDoc) string {
		doc = d
		return ""
	}
	cliPrinted(fb, spec, append(cliStrings(path), "--help"), &capture)
	if doc == nil {
		panic(Defect{"Error: Effect's CLI printed no help"})
	}
	return cliHelpDocValue(doc)
}
