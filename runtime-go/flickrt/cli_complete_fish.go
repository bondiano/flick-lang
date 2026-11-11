package flickrt

import (
	"regexp"
	"strings"
)

// The completion descriptor of a command, Fish's script and
// Completions.generate.

func cliCompleteType(p *cliPrim, flag bool) completeType {
	switch p.tag {
	case "Boolean":
		if flag {
			return completeType{tag: "Boolean"}
		}
	case "Choice":
		return completeType{tag: "Choice", values: p.choiceKeys}
	case "Path":
		return completeType{tag: "Path", pathType: p.pathType}
	case "FileText", "FileParse":
		return completeType{tag: "Path", pathType: "file"}
	}
	return completeType{tag: "String"}
}

// cliDescriptor is the completion descriptor of a command.
func cliDescriptor(c *cliCommand, inherited []*cliParam) completeCommand {
	out := completeCommand{name: c.name, description: c.shortDescription}
	if out.description == "" {
		out.description = c.description
	}
	seen := map[string]bool{}
	for i, p := range append(append([]*cliParam{}, c.config.flags...), inherited...) {
		s := p.single
		if s.hidden || i >= len(c.config.flags) && seen[s.name] {
			continue
		}
		seen[s.name] = true
		out.flags = append(out.flags, completeFlag{s.name, s.aliases, cliOr(s.description, ""),
			cliCompleteType(s.prim, true), s.optionalValue != nil})
	}
	for _, p := range c.config.arguments {
		out.args = append(out.args, completeArg{p.single.name, cliOr(p.single.description, ""),
			!p.optional, p.variadic, cliCompleteType(p.single.prim, false)})
	}
	shared := append(append([]*cliParam{}, inherited...), c.context.flags...)
	for _, sub := range c.subcommands {
		if sub.unlisted {
			continue
		}
		d := cliDescriptor(sub, shared)
		out.subcommands = append(out.subcommands, d)
		if sub.alias != "" && sub.alias != sub.name {
			d.name = sub.alias
			out.subcommands = append(out.subcommands, d)
		}
	}
	return out
}

// cliGenerateCompletions is Completions.generate.
func cliGenerateCompletions(executable, shell string, d completeCommand) string {
	switch shell {
	case "bash":
		return completeBash(executable, d)
	case "zsh":
		return completeZsh(executable, d)
	case "fish":
		return completeFish(executable, d)
	}
	return completePowerShell(executable, d)
}

var fishEscape = strings.NewReplacer(`\`, `\\`, `'`, `\'`)

var fishChoiceSpecial = regexp.MustCompile(`[^A-Za-z0-9_.,/@%+-]`)

func fishChoice(s string) string {
	return fishEscape.Replace(fishChoiceSpecial.ReplaceAllStringFunc(s, func(m string) string { return `\` + m }))
}

func fishSubcommandCondition(parent, children []string) string {
	if len(parent) == 0 {
		if len(children) > 0 {
			return "__fish_use_subcommand"
		}
		return ""
	}
	conds := make([]string, len(parent))
	for i, p := range parent {
		conds[i] = "__fish_seen_subcommand_from " + p
	}
	cond := strings.Join(conds, "; and ")
	if len(children) > 0 {
		return cond + "; and not __fish_seen_subcommand_from " + strings.Join(children, " ")
	}
	return cond
}

func fishContainsOpt(f completeFlag) string {
	args := []string{}
	for _, a := range f.aliases {
		if completeAliasForm(a) == "-"+a {
			args = append(args, "-s "+a)
		}
	}
	args = append(args, f.name)
	for _, a := range f.aliases {
		if completeAliasForm(a) != "-"+a {
			args = append(args, a)
		}
	}
	if f.typ.tag == "Boolean" {
		args = append(args, "no-"+f.name)
	}
	return "not __fish_contains_opt " + strings.Join(args, " ")
}

func fishValueDedup(f completeFlag) string {
	forms := []string{"--" + f.name}
	for _, a := range f.aliases {
		forms = append(forms, completeAliasForm(a))
	}
	return "begin; " + fishContainsOpt(f) + "; or contains -- (commandline -poc)[-1] " + strings.Join(forms, " ") + "; end"
}

func fishFlagArgs(f completeFlag) []string {
	args := []string{"-l " + f.name}
	for _, a := range f.aliases {
		if completeAliasForm(a) == "-"+a {
			args = append(args, "-s "+a)
		} else {
			args = append(args, "-l "+a)
		}
	}
	if f.description != "" {
		args = append(args, "-d '"+fishEscape.Replace(f.description)+"'")
	}
	if completeTakesSeparateValue(f) {
		if v := fishValueArgs(f.typ, true); v != "" {
			args = append(args, v)
		}
	}
	return args
}

func fishValueArgs(t completeType, flag bool) string {
	switch t.tag {
	case "Choice":
		values := make([]string, len(t.values))
		for i, v := range t.values {
			values[i] = fishChoice(v)
		}
		return "-r -f -a '" + strings.Join(values, " ") + "'"
	case "Path":
		return "-r -F"
	case "Boolean":
		return ""
	}
	if flag {
		return "-r -f" // requires a value, without falling back to files
	}
	return ""
}

func fishCommand(executable string, d completeCommand, parent []string, lines []string) []string {
	names := make([]string, len(d.subcommands))
	for i, s := range d.subcommands {
		names[i] = s.name
	}
	condition := fishSubcommandCondition(parent, names)
	head := func(cond string) []string {
		parts := []string{"complete -c " + executable}
		if cond != "" {
			parts = append(parts, "-n '"+cond+"'")
		}
		return parts
	}
	hasPathArgs := false
	for _, a := range d.args {
		hasPathArgs = hasPathArgs || a.typ.tag == "Path"
	}
	if !hasPathArgs {
		lines = append(lines, strings.Join(append(head(condition), "-f"), " "))
	}
	for _, s := range d.subcommands {
		parts := append(head(condition), "-f -a '"+fishEscape.Replace(s.name)+"'")
		if s.description != "" {
			parts = append(parts, "-d '"+fishEscape.Replace(s.description)+"'")
		}
		lines = append(lines, strings.Join(parts, " "))
	}
	and := func(cond string) string {
		if condition != "" {
			return condition + "; and " + cond
		}
		return cond
	}
	for _, f := range d.flags {
		dedup := fishContainsOpt(f)
		if completeTakesSeparateValue(f) {
			dedup = fishValueDedup(f)
		}
		cond := and(dedup)
		lines = append(lines, strings.Join(append(head(cond), fishFlagArgs(f)...), " "))
		if f.typ.tag == "Boolean" {
			parts := append(head(cond), "-l no-"+f.name)
			if f.description != "" {
				parts = append(parts, "-d '"+fishEscape.Replace("Disable "+f.name)+"'")
			}
			lines = append(lines, strings.Join(parts, " "))
		}
	}
	// flags as -a entries, so a bare <TAB> shows them too
	bare := and(`not string match -q -- "-*" (commandline -ct)`)
	for _, f := range d.flags {
		cond := "-n '" + bare + "; and " + fishContainsOpt(f) + "'"
		parts := []string{"complete -c " + executable, cond, "-f -a '--" + f.name + "'"}
		if f.description != "" {
			parts = append(parts, "-d '"+fishEscape.Replace(f.description)+"'")
		}
		lines = append(lines, strings.Join(parts, " "))
		if f.typ.tag == "Boolean" {
			parts := []string{"complete -c " + executable, cond, "-f -a '--no-" + f.name + "'"}
			if f.description != "" {
				parts = append(parts, "-d '"+fishEscape.Replace("Disable "+f.name)+"'")
			}
			lines = append(lines, strings.Join(parts, " "))
		}
	}
	for _, a := range d.args {
		if v := fishValueArgs(a.typ, false); v != "" {
			parts := append(head(condition), v)
			if a.description != "" {
				parts = append(parts, "-d '"+fishEscape.Replace(a.description)+"'")
			}
			lines = append(lines, strings.Join(parts, " "))
		}
	}
	for _, s := range d.subcommands {
		lines = fishCommand(executable, s, append(parent[:len(parent):len(parent)], s.name), lines)
	}
	return lines
}

// completeFish is the Fish script.
func completeFish(executable string, d completeCommand) string {
	lines := []string{
		"###-begin-" + executable + "-completions-###", "#", "# Static completion script for Fish", "#",
		"# Installation:", "#   " + executable + " --completions fish > ~/.config/fish/completions/" + executable + ".fish",
		"#", "",
	}
	lines = fishCommand(executable, d, nil, lines)
	return strings.Join(append(lines, "", "###-end-"+executable+"-completions-###"), "\n")
}
