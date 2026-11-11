package flickrt

import (
	"strings"
)

// `--completions zsh` prints the same static script as the TypeScript CLI.
// Derived from Effect 4 cli/internal/completions/zsh.ts.

var completeZshEscaper = strings.NewReplacer(`\`, `\\`, "'", `'\''`, ":", `\:`)

// completeEscapeZsh is escapeZsh: backslashes, single quotes and the colons
// that separate _arguments fields.
func completeEscapeZsh(s string) string { return completeZshEscaper.Replace(s) }

// completeEscapeZshExplanation also escapes the `]` closing an explanation.
func completeEscapeZshExplanation(s string) string {
	return strings.ReplaceAll(completeEscapeZsh(s), "]", `\]`)
}

// completeEscapeZshChoice escapes choices for the second parse of an
// _arguments action: /[^A-Za-z0-9_.,/@%+-]/gu, per code point.
func completeEscapeZshChoice(s string) string {
	var b strings.Builder
	for _, r := range s {
		if !(r >= 'A' && r <= 'Z' || r >= 'a' && r <= 'z' || r >= '0' && r <= '9' || strings.ContainsRune("_.,/@%+-", r)) {
			b.WriteByte('\\')
		}
		b.WriteRune(r)
	}
	return strings.ReplaceAll(b.String(), "'", `'\''`)
}

func completeZshChoices(values []string) string {
	escaped := make([]string, len(values))
	for i, v := range values {
		escaped[i] = completeEscapeZshChoice(v)
	}
	return "(" + strings.Join(escaped, " ") + ")"
}

func completeZshValueAction(t completeType) string {
	switch t.tag {
	case "Boolean":
		return ""
	case "Choice":
		return ":value:" + completeZshChoices(t.values)
	case "Path":
		if t.pathType == "directory" {
			return ":directory:_directories"
		}
		return ":file:_files"
	case "Int":
		return ":integer:"
	case "Finite":
		return ":float:"
	case "Date":
		return ":date:"
	}
	return ":value:"
}

func completeZshArgAction(t completeType) string {
	switch t.tag {
	case "Choice":
		return completeZshChoices(t.values)
	case "Path":
		if t.pathType == "directory" {
			return "_directories"
		}
		return "_files"
	}
	return ""
}

// completeZshFlagSpecs is flagSpecs: complete single-quoted _arguments
// optspecs, with no brace expansion or line continuations.
func completeZshFlagSpecs(f completeFlag) []string {
	desc := ""
	if f.description != "" {
		desc = "[" + completeEscapeZshExplanation(f.description) + "]"
	}
	// An optional value must follow `=` in the same word, and `::` marks it
	// optional.
	optional := f.optionalValue && f.typ.tag != "Boolean"
	suffix, action := "", completeZshValueAction(f.typ)
	if optional {
		suffix, action = "=-", ":"+action
	}
	excl := "(" + strings.Join(completeFlagForms(f, true), " ") + ")"
	specs := []string{"'" + excl + "--" + f.name + suffix + desc + action + "'"}
	for _, a := range f.aliases {
		specs = append(specs, "'"+excl+completeAliasForm(a)+suffix+desc+action+"'")
	}
	if f.typ.tag == "Boolean" {
		negDesc := ""
		if f.description != "" {
			negDesc = "[" + completeEscapeZshExplanation("Disable "+f.name) + "]"
		}
		specs = append(specs, "'"+excl+"--no-"+f.name+negDesc+"'")
	}
	return specs
}

func completeZshArgSpec(a completeArg) string {
	desc := a.name
	if a.description != "" {
		desc = completeEscapeZsh(a.description)
	}
	prefix := ""
	if a.variadic {
		prefix = "*"
	}
	return "'" + prefix + ":" + desc + ":" + completeZshArgAction(a.typ) + "'"
}

func completeZshFunction(d completeCommand, parent []string, lines []string) []string {
	path := append(append([]string{}, parent...), d.name)
	lines = append(lines, completeFuncName(path)+"() {")
	if len(d.subcommands) > 0 {
		// _arguments -C requires these locals for its state machine.
		lines = append(lines,
			"  local context state state_descr line",
			"  typeset -A opt_args",
			"",
			"  local -a commands",
			"  commands=(")
		for _, sub := range d.subcommands {
			desc := ""
			if sub.description != "" {
				desc = completeEscapeZsh(sub.description)
			}
			lines = append(lines, "    '"+sub.name+":"+desc+"'")
		}
		lines = append(lines, "  )", "", "  local -a specs", "  specs=(")
		for _, f := range d.flags {
			for _, spec := range completeZshFlagSpecs(f) {
				lines = append(lines, "    "+spec)
			}
		}
		if len(d.args) > 0 {
			lines = append(lines, "    -", "    parent-arguments")
			for _, a := range d.args {
				lines = append(lines, "    "+completeZshArgSpec(a))
			}
			lines = append(lines, "    -", "    subcommands")
		}
		lines = append(lines,
			"    '1:command:->command'",
			"    '*::arg:->args'",
			"  )",
			"",
			`  _arguments -C "${specs[@]}"`,
			"",
			`  case "$state" in`,
			"    command)",
			"      _describe -t commands 'commands' commands",
			"      ;;",
			"    args)",
			`      case "$words[1]" in`)
		for _, sub := range d.subcommands {
			lines = append(lines, "        "+sub.name+")",
				"          "+completeFuncName(append(append([]string{}, path...), sub.name)),
				"          ;;")
		}
		lines = append(lines, "      esac", "      ;;", "  esac")
	} else {
		var specs []string
		for _, f := range d.flags {
			specs = append(specs, completeZshFlagSpecs(f)...)
		}
		for _, a := range d.args {
			specs = append(specs, completeZshArgSpec(a))
		}
		if len(specs) > 0 {
			lines = append(lines, "  local -a specs", "  specs=(")
			for _, spec := range specs {
				lines = append(lines, "    "+spec)
			}
			lines = append(lines, "  )", `  _arguments "${specs[@]}"`)
		}
	}
	lines = append(lines, "}", "")
	for _, sub := range d.subcommands {
		lines = completeZshFunction(sub, path, lines)
	}
	return lines
}

// completeZsh is the Zsh script: a self-contained script that never
// re-invokes the CLI.
func completeZsh(executable string, d completeCommand) string {
	safe := completeSanitize(executable)
	lines := []string{
		"#compdef " + executable,
		"###-begin-" + executable + "-completions-###",
		"#",
		"# Static completion script for Zsh",
		"#",
		"# Installation:",
		"#   " + executable + " --completions zsh > ~/.zsh/completions/_" + executable,
		"#   then add ~/.zsh/completions to your fpath",
		"#",
		"",
	}
	lines = completeZshFunction(d, nil, lines)
	lines = append(lines,
		"# Handle both direct invocation and autoload",
		`if [[ "${zsh_eval_context[-1]}" == "loadautofunc" ]]; then`,
		"  _"+safe+` "$@"`,
		"else",
		"  compdef _"+safe+" "+executable,
		"fi",
		"###-end-"+executable+"-completions-###")
	return strings.Join(lines, "\n")
}
