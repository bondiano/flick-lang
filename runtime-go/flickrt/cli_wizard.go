package flickrt

import (
	"regexp"
	"strings"
)

// `--wizard`: Effect's internal/wizard.ts and the wizard branch of
// Command.runWith. It asks for each flag and argument of the command, and
// for a subcommand, on the terminal of the process (Effect's Terminal, not
// `Cli.Prompt.Tty`), then offers to run the command line it built.

const ansiMagenta = "\x1b[35m"

func cliAnnotate(colors bool, text string, styles ...string) string {
	if !colors {
		return text
	}
	return strings.Join(styles, "") + text + ansiReset
}

type cliWizardArg struct{ value, display string }

func cliWizardPlain(value string) cliWizardArg { return cliWizardArg{value, value} }

// cliWizardRun is the wizard's state: where it prints and how it asks.
type cliWizardRun struct {
	fb     *Fiber
	env    *cliEnv
	prompt *promptCtx
	colors bool
}

func (w *cliWizardRun) ask(prompt Value) Value { return promptRunOp(w.prompt, prompt) }

// cliWizard is runWith's wizard: the introduction, the questions, the
// command line, and, if confirmed, its run. Quitting cancels it.
func cliWizard(fb *Fiber, root *cliCommand, args []string, ctx *cliContext) *cliError {
	tty := ctx.env.tty
	if tty == nil {
		process := acquirePromptTty()
		defer releasePromptTty()
		tty = promptProcessService(process)
	}
	theme := promptCurrentTheme()
	w := &cliWizardRun{fb, ctx.env, &promptCtx{fb, tty, theme}, theme.colors}
	var desc *string
	if root.description != "" {
		desc = &root.description
	}
	w.env.log(cliWizardIntroduction(root.name, ctx.version, desc, w.colors))
	var err *cliError
	_, cause := catch(func() Value {
		prefix := []string{root.name}
		for _, a := range args {
			if a != "--wizard" && !strings.HasPrefix(a, "--wizard=") {
				prefix = append(prefix, a)
			}
		}
		line, display, wizardErr := w.run(root, ctx.path, prefix)
		if wizardErr != nil {
			err = wizardErr
			return Unit
		}
		w.env.log(cliWizardBlock("Command ready", display, w.colors, ansiCyanBright, ansiGreen))
		yes, no := "yes", "no"
		run := w.ask(PromptToggle(Rec("message", "Run this command?", "initial", Some(true), "active", Some(yes), "inactive", Some(no))))
		if run.(bool) {
			w.env.log("")
			if err = cliRunParsed(fb, root, line[1:], w.env); err != nil && err.tag == "ShowHelp" {
				w.env.log(w.env.format.help(fb, cliHelpFor(root, err.path, w.env.builtIns)))
			}
		}
		return Unit
	})
	if cause != nil {
		if cause.kind != causeFail || cause.err != promptQuit {
			panic(&Failure{cause})
		}
		w.env.log("\n" + cliAnnotate(w.colors, "Wizard cancelled.", ansiRed))
		return nil
	}
	return err
}

// run is wizard.ts's `run`: the command line, and how it is shown.
func (w *cliWizardRun) run(root *cliCommand, path, prefix []string) ([]string, []string, *cliError) {
	selected := root
	for _, name := range path[min(1, len(path)):] {
		var child *cliCommand
		for _, sub := range selected.subcommands {
			if sub.name == name || sub.alias == name {
				child = sub
				break
			}
		}
		if child == nil {
			break
		}
		selected = child
	}
	line := make([]cliWizardArg, len(prefix))
	for i, p := range prefix {
		line[i] = cliWizardPlain(p)
	}
	w.logCommand(line)
	section := selected.name
	if selected == root {
		section = "ROOT"
	}
	line, err := w.command(selected, line, section)
	if err != nil {
		return nil, nil, err
	}
	values, display := make([]string, len(line)), make([]string, len(line))
	for i, a := range line {
		values[i], display[i] = a.value, a.display
	}
	return values, display, nil
}

func (w *cliWizardRun) logCommand(line []cliWizardArg) {
	shown := make([]string, len(line))
	for i, a := range line {
		shown[i] = a.display
	}
	w.env.log(cliWizardBlock("Current command", shown, w.colors, ansiMagenta, ansiWhite))
}

func (w *cliWizardRun) section(name, section string) {
	w.env.log(cliAnnotate(w.colors, strings.ToUpper(name), ansiBold, ansiCyanBright) + " " +
		cliAnnotate(w.colors, "·", ansiBlackBright) + " " + cliAnnotate(w.colors, section, ansiBold, ansiWhite))
}

// command is wizard.ts's `promptCommand`.
func (w *cliWizardRun) command(c *cliCommand, line []cliWizardArg, section string) ([]cliWizardArg, *cliError) {
	visible := cliVisible(c)
	config := &c.config
	if len(visible) > 0 {
		config = &c.context
	}
	if len(config.flags) > 0 {
		w.section(section, "FLAGS")
		for _, p := range config.flags {
			args, err := w.param(p)
			if err != nil {
				return nil, err
			}
			line = append(line, args...)
		}
		if len(config.arguments) > 0 || len(visible) > 0 {
			w.logCommand(line)
		}
	}
	if len(config.arguments) > 0 {
		w.section(section, "ARGUMENTS")
		for _, p := range config.arguments {
			args, err := w.param(p)
			if err != nil {
				return nil, err
			}
			line = append(line, args...)
		}
		if len(visible) > 0 {
			w.logCommand(line)
		}
	}
	if len(visible) == 0 {
		return line, nil
	}
	choices := make([]Value, len(visible))
	for i, sub := range visible {
		description := Value(None)
		if sub.shortDescription != "" {
			description = Some(sub.shortDescription)
		} else if sub.description != "" {
			description = Some(sub.description)
		}
		choices[i] = Rec("title", sub.name, "value", int32(i), "description", description, "disabled", false, "selected", false)
	}
	child := visible[w.ask(PromptSelect(Rec("message", Some("Command"), "choices", choices, "maxPerPage", None))).(int32)]
	w.env.log("")
	line = append(line, cliWizardPlain(child.name))
	if len(cliVisible(child)) > 0 || len(child.config.ordered) > 0 { // hasWizardSteps
		w.logCommand(line)
	}
	return w.command(child, line, child.name)
}

func cliVisible(c *cliCommand) []*cliCommand {
	out := []*cliCommand{}
	for _, sub := range c.subcommands {
		if !sub.unlisted {
			out = append(out, sub)
		}
	}
	return out
}

// param is wizard.ts's `promptParam`.
func (w *cliWizardRun) param(p *cliParam) ([]cliWizardArg, *cliError) {
	s := p.single
	if p.optional {
		include := w.ask(PromptConfirm(Rec("message", "Set "+cliWizardLabel(s)+"?", "initial", Some(false),
			"label", None, "placeholder", None)))
		if !include.(bool) {
			w.env.log("")
			return nil, nil
		}
	}
	count := 1
	if p.variadic {
		min := max(p.min, 0)
		maxValue := Value(None)
		if p.max >= 0 {
			maxValue = Some(int32(p.max))
		}
		count = int(w.ask(PromptInt(Rec("message", cliWizardLabel(s)+" count", "default", Some(int32(min)),
			"min", Some(int32(min)), "max", maxValue, "incrementBy", None, "decrementBy", None, "validate", None))).(int32))
	}
	values := make([]cliWizardArg, count)
	texts := make([]string, count)
	for i := range values {
		values[i] = w.single(s)
		texts[i] = values[i].value
	}
	parsed := cliArgs{cliFlagMap{}, texts}
	if s.kind == "flag" {
		parsed = cliArgs{cliFlagMap{s.name: texts}, nil}
	}
	if _, _, err := p.parse(w.fb, parsed); err != nil {
		return nil, err
	}
	w.env.log("")
	if s.kind == "argument" {
		return values, nil
	}
	out := []cliWizardArg{}
	for _, v := range values {
		// option-looking values are inlined, so the lexer cannot read them as flags or `--`
		if s.optionalValue != nil || strings.HasPrefix(v.value, "-") && len(v.value) > 1 {
			out = append(out, cliWizardArg{"--" + s.name + "=" + v.value, "--" + s.name + "=" + v.display})
		} else {
			out = append(out, cliWizardPlain("--"+s.name), v)
		}
	}
	return out, nil
}

// single is wizard.ts's `promptSingle` for the primitives a Flick command
// has: a switch, a choice, and text for the others.
func (w *cliWizardRun) single(s *cliSingle) cliWizardArg {
	message := cliWizardLabel(s)
	switch s.prim.tag {
	case "Boolean":
		v := w.ask(PromptConfirm(Rec("message", message, "initial", None,
			"label", Some(Rec("confirm", "true", "deny", "false")),
			"placeholder", Some(Rec("defaultConfirm", Some("(T/f)"), "defaultDeny", Some("(t/F)"))))))
		if v.(bool) {
			return cliWizardPlain("true")
		}
		return cliWizardPlain("false")
	case "Choice":
		choices := make([]Value, len(s.prim.choiceKeys))
		for i, k := range s.prim.choiceKeys {
			choices[i] = Rec("title", k, "value", k, "description", None, "disabled", false, "selected", false)
		}
		return cliWizardPlain(w.ask(PromptSelect(Rec("message", Some(message), "choices", choices, "maxPerPage", None))).(string))
	}
	return cliWizardPlain(w.ask(PromptText(Rec("message", message, "default", None, "validate", None))).(string))
}

var cliWizardWords = regexp.MustCompile(`[-_]+`)

func cliWizardLabel(s *cliSingle) string {
	label := cliHumanize(s.name)
	if s.kind == "flag" && s.description != nil {
		if d := strings.TrimSpace(*s.description); len(utf16Units(d)) <= 32 {
			label = d
		}
	}
	if s.kind == "flag" {
		return label + " (--" + s.name + ")"
	}
	return label
}

func cliHumanize(name string) string {
	words := []string{}
	for _, w := range cliWizardWords.Split(name, -1) {
		if w != "" {
			words = append(words, w)
		}
	}
	if len(words) == 0 {
		return name
	}
	first := []rune(words[0])
	words[0] = strings.ToUpper(string(first[0])) + string(first[1:])
	return strings.Join(words, " ")
}

// cliWizardIntroduction is renderIntroduction.
func cliWizardIntroduction(name, version string, summary *string, colors bool) string {
	lines := []string{cliAnnotate(colors, name, ansiBold, ansiCyanBright) + " " + cliAnnotate(colors, "v"+version, ansiWhite) +
		" " + cliAnnotate(colors, "· Command wizard", ansiBold, ansiWhite)}
	if summary != nil && *summary != "" {
		lines = append(lines, *summary)
	}
	return strings.Join(append(lines, cliAnnotate(colors, "Build a command interactively. Press Ctrl+C to cancel.", ansiBlackBright), ""), "\n")
}

// cliWizardBlock is renderCommandBlock: the command line wrapped at 88
// columns, shell-quoted.
func cliWizardBlock(label string, line []string, colors bool, commandColor, labelColor string) string {
	out := []string{cliAnnotate(colors, label, ansiBold, labelColor)}
	for _, l := range cliWrapCommand(line) {
		out = append(out, cliAnnotate(colors, l, commandColor))
	}
	return strings.Join(append(out, ""), "\n")
}

var cliShellSafe = regexp.MustCompile(`^[A-Za-z0-9_./:@%+=,-]+$`)

func cliWrapCommand(line []string) []string {
	const width, first, continuation = 88, "  $ ", "    "
	lines, current := []string{}, first
	for _, arg := range line {
		if !cliShellSafe.MatchString(arg) {
			arg = "'" + strings.ReplaceAll(arg, "'", `'"'"'`) + "'"
		}
		separator := " "
		if current == first || current == continuation {
			separator = ""
		}
		if len(utf16Units(current))+len(separator)+len(utf16Units(arg)) > width && current != first {
			lines = append(lines, current+" \\")
			current = continuation + arg
		} else {
			current += separator + arg
		}
	}
	if current != first {
		lines = append(lines, current)
	}
	return lines
}
