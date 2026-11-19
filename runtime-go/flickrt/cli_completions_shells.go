//go:build flickcli

package flickrt

import (
	"regexp"
	"strings"
)

// The Zsh and Fish completion scripts of Effect's CLI.

var cliUnsafeChoice = regexp.MustCompile(`[^A-Za-z0-9_.,/@%+-]`)

func cliEscapeZsh(s string) string {
	s = strings.ReplaceAll(s, `\`, `\\`)
	s = strings.ReplaceAll(s, "'", `'\''`)
	return strings.ReplaceAll(s, ":", `\:`)
}

func cliEscapeZshExplanation(s string) string { return strings.ReplaceAll(cliEscapeZsh(s), "]", `\]`) }

func cliEscapeZshChoice(s string) string {
	return strings.ReplaceAll(cliUnsafeChoice.ReplaceAllString(s, `\$0`), "'", `'\''`)
}

func cliZshChoices(values []string) string {
	out := make([]string, len(values))
	for i, v := range values {
		out[i] = cliEscapeZshChoice(v)
	}
	return "(" + strings.Join(out, " ") + ")"
}

func cliZshValue(t cliCompType) string {
	switch t.tag {
	case "Boolean":
		return ""
	case "Choice":
		return ":value:" + cliZshChoices(t.values)
	case "Path":
		if t.pathType == "directory" {
			return ":directory:_directories"
		}
		return ":file:_files"
	}
	return ":value:"
}

func cliZshFlagSpecs(f cliFlagDesc) []string {
	desc := ""
	if f.description != nil && *f.description != "" {
		desc = "[" + cliEscapeZshExplanation(*f.description) + "]"
	}
	optional := f.optionalValue && f.typ.tag != "Boolean"
	suffix, action := "", cliZshValue(f.typ)
	if optional {
		suffix, action = "=-", ":"+action
	}
	excl := "(" + strings.Join(cliForms(f, true), " ") + ")"
	specs := []string{"'" + excl + "--" + f.name + suffix + desc + action + "'"}
	for _, a := range f.aliases {
		specs = append(specs, "'"+excl+cliForm(a)+suffix+desc+action+"'")
	}
	if f.typ.tag == "Boolean" {
		neg := ""
		if f.description != nil && *f.description != "" {
			neg = "[" + cliEscapeZshExplanation("Disable "+f.name) + "]"
		}
		specs = append(specs, "'"+excl+"--no-"+f.name+neg+"'")
	}
	return specs
}

func cliZshArg(a cliArgDesc) string {
	desc := a.name
	if a.description != nil && *a.description != "" {
		desc = cliEscapeZsh(*a.description)
	}
	action := ""
	switch a.typ.tag {
	case "Choice":
		action = cliZshChoices(a.typ.values)
	case "Path":
		action = "_files"
		if a.typ.pathType == "directory" {
			action = "_directories"
		}
	}
	prefix := ""
	if a.variadic {
		prefix = "*"
	}
	return "'" + prefix + ":" + desc + ":" + action + "'"
}

func cliZshFunction(d *cliDescriptor, parent []string, out *[]string) {
	add := func(lines ...string) { *out = append(*out, lines...) }
	path := append(append([]string(nil), parent...), d.name)
	add(cliFunctionName(path) + "() {")
	if len(d.subcommands) > 0 {
		add("  local context state state_descr line", "  typeset -A opt_args", "", "  local -a commands", "  commands=(")
		for _, sub := range d.subcommands {
			desc := ""
			if sub.description != nil {
				desc = cliEscapeZsh(*sub.description)
			}
			add("    '" + sub.name + ":" + desc + "'")
		}
		add("  )", "", "  local -a specs", "  specs=(")
		for _, f := range d.flags {
			for _, s := range cliZshFlagSpecs(f) {
				add("    " + s)
			}
		}
		if len(d.arguments) > 0 {
			add("    -", "    parent-arguments")
			for _, a := range d.arguments {
				add("    " + cliZshArg(a))
			}
			add("    -", "    subcommands")
		}
		add("    '1:command:->command'", "    '*::arg:->args'", "  )", "", `  _arguments -C "${specs[@]}"`, "",
			`  case "$state" in`, "    command)", "      _describe -t commands 'commands' commands", "      ;;",
			"    args)", `      case "$words[1]" in`)
		for _, sub := range d.subcommands {
			add("        "+sub.name+")", "          "+cliFunctionName(append(path[:len(path):len(path)], sub.name)), "          ;;")
		}
		add("      esac", "      ;;", "  esac")
	} else {
		var specs []string
		for _, f := range d.flags {
			specs = append(specs, cliZshFlagSpecs(f)...)
		}
		for _, a := range d.arguments {
			specs = append(specs, cliZshArg(a))
		}
		if len(specs) > 0 {
			add("  local -a specs", "  specs=(")
			for _, s := range specs {
				add("    " + s)
			}
			add("  )", `  _arguments "${specs[@]}"`)
		}
	}
	add("}", "")
	for _, sub := range d.subcommands {
		cliZshFunction(sub, path, out)
	}
}

func cliZsh(name string, d *cliDescriptor) string {
	safe := cliSanitize(name)
	lines := []string{"#compdef " + name, "###-begin-" + name + "-completions-###", "#", "# Static completion script for Zsh", "#",
		"# Installation:", "#   " + name + " --completions zsh > ~/.zsh/completions/_" + name,
		"#   then add ~/.zsh/completions to your fpath", "#", ""}
	cliZshFunction(d, nil, &lines)
	lines = append(lines, "# Handle both direct invocation and autoload",
		`if [[ "${zsh_eval_context[-1]}" == "loadautofunc" ]]; then`, "  _"+safe+` "$@"`, "else",
		"  compdef _"+safe+" "+name, "fi", "###-end-"+name+"-completions-###")
	return strings.Join(lines, "\n")
}

func cliEscapeFish(s string) string {
	return strings.ReplaceAll(strings.ReplaceAll(s, `\`, `\\`), "'", `\'`)
}

func cliFishChoices(values []string) string {
	out := make([]string, len(values))
	for i, v := range values {
		out[i] = cliEscapeFish(cliUnsafeChoice.ReplaceAllString(v, `\$0`))
	}
	return "-r -f -a '" + strings.Join(out, " ") + "'"
}

func cliFishCondition(parent, children []string) string {
	if len(parent) == 0 {
		if len(children) > 0 {
			return "__fish_use_subcommand"
		}
		return ""
	}
	seen := make([]string, len(parent))
	for i, p := range parent {
		seen[i] = "__fish_seen_subcommand_from " + p
	}
	condition := strings.Join(seen, "; and ")
	if len(children) > 0 {
		return condition + "; and not __fish_seen_subcommand_from " + strings.Join(children, " ")
	}
	return condition
}

func cliFishContains(f cliFlagDesc) string {
	var args []string
	for _, a := range f.aliases {
		if len(utf16Units(a)) == 1 {
			args = append(args, "-s "+a)
		}
	}
	args = append(args, f.name)
	for _, a := range f.aliases {
		if len(utf16Units(a)) > 1 {
			args = append(args, a)
		}
	}
	if f.typ.tag == "Boolean" {
		args = append(args, "no-"+f.name)
	}
	return "not __fish_contains_opt " + strings.Join(args, " ")
}

func cliFishDescribed(parts []string, description *string) []string {
	if description != nil && *description != "" {
		return append(parts, "-d '"+cliEscapeFish(*description)+"'")
	}
	return parts
}

func cliFishDisable(parts []string, f cliFlagDesc) []string {
	if f.description != nil && *f.description != "" {
		return append(parts, "-d '"+cliEscapeFish("Disable "+f.name)+"'")
	}
	return parts
}

func cliFishCompletions(name string, d *cliDescriptor, parent []string, out *[]string) {
	add := func(parts []string) { *out = append(*out, strings.Join(parts, " ")) }
	children := make([]string, len(d.subcommands))
	for i, s := range d.subcommands {
		children[i] = s.name
	}
	condition := cliFishCondition(parent, children)
	head := []string{"complete -c " + name}
	if condition != "" {
		head = append(head, "-n '"+condition+"'")
	}
	with := func(parts ...string) []string { return append(append([]string(nil), head...), parts...) }
	pathArgs := false
	for _, a := range d.arguments {
		pathArgs = pathArgs || a.typ.tag == "Path"
	}
	if !pathArgs {
		add(with("-f"))
	}
	for _, sub := range d.subcommands {
		add(cliFishDescribed(with("-f -a '"+cliEscapeFish(sub.name)+"'"), sub.description))
	}
	for _, f := range d.flags {
		dedup := cliFishContains(f)
		if cliTakesValue(f) {
			dedup = "begin; " + cliFishContains(f) + "; or contains -- (commandline -poc)[-1] " + strings.Join(cliForms(f, false), " ") + "; end"
		}
		if condition != "" {
			dedup = condition + "; and " + dedup
		}
		parts := []string{"complete -c " + name, "-n '" + dedup + "'", "-l " + f.name}
		for _, a := range f.aliases {
			if len(utf16Units(a)) == 1 {
				parts = append(parts, "-s "+a)
			} else {
				parts = append(parts, "-l "+a)
			}
		}
		parts = cliFishDescribed(parts, f.description)
		if cliTakesValue(f) {
			switch f.typ.tag {
			case "Choice":
				parts = append(parts, cliFishChoices(f.typ.values))
			case "Path":
				parts = append(parts, "-r -F")
			default:
				parts = append(parts, "-r -f")
			}
		}
		add(parts)
		if f.typ.tag == "Boolean" {
			add(cliFishDisable([]string{"complete -c " + name, "-n '" + dedup + "'", "-l no-" + f.name}, f))
		}
	}
	if len(d.flags) > 0 {
		bare := `not string match -q -- "-*" (commandline -ct)`
		if condition != "" {
			bare = condition + "; and " + bare
		}
		for _, f := range d.flags {
			when := "-n '" + bare + "; and " + cliFishContains(f) + "'"
			add(cliFishDescribed([]string{"complete -c " + name, when, "-f -a '--" + f.name + "'"}, f.description))
			if f.typ.tag == "Boolean" {
				add(cliFishDisable([]string{"complete -c " + name, when, "-f -a '--no-" + f.name + "'"}, f))
			}
		}
	}
	for _, a := range d.arguments {
		switch a.typ.tag {
		case "Choice":
			add(cliFishDescribed(with(cliFishChoices(a.typ.values)), a.description))
		case "Path":
			add(cliFishDescribed(with("-r -F"), a.description))
		}
	}
	for _, sub := range d.subcommands {
		cliFishCompletions(name, sub, append(append([]string(nil), parent...), sub.name), out)
	}
}

func cliFish(name string, d *cliDescriptor) string {
	lines := []string{"###-begin-" + name + "-completions-###", "#", "# Static completion script for Fish", "#", "# Installation:",
		"#   " + name + " --completions fish > ~/.config/fish/completions/" + name + ".fish", "#", ""}
	cliFishCompletions(name, d, nil, &lines)
	lines = append(lines, "", "###-end-"+name+"-completions-###")
	return strings.Join(lines, "\n")
}
