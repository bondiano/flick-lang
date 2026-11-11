package flickrt

import (
	"strings"
	"unicode/utf16"
)

// completeType is the type of a flag or of an argument, for completions.
type completeType struct {
	tag      string   // "Boolean", "String", "Int", "Finite", "Date", "Choice", "Path"
	values   []string // Choice keys
	pathType string   // Path: "file", "directory" or "either"
}

type completeFlag struct {
	name          string
	aliases       []string
	description   string // "" when absent
	typ           completeType
	optionalValue bool
}

type completeArg struct {
	name, description  string
	required, variadic bool
	typ                completeType
}

// completeCommand is the descriptor of a command, for completions.
type completeCommand struct {
	name, description string // description "" when absent
	flags             []completeFlag
	args              []completeArg
	subcommands       []completeCommand
}

// completeTakesSeparateValue is whether a flag's value is a word of its
// own: not for a boolean, nor for a flag whose value is optional.
func completeTakesSeparateValue(f completeFlag) bool {
	return f.typ.tag != "Boolean" && !f.optionalValue
}

// completeSanitize is the generators' s.replace(/[^a-zA-Z0-9_]/g, "_"). The
// regex has no u flag, so it replaces UTF-16 code units: an astral rune
// becomes two underscores.
func completeSanitize(s string) string {
	var b strings.Builder
	for _, r := range s {
		switch {
		case r >= 'a' && r <= 'z', r >= 'A' && r <= 'Z', r >= '0' && r <= '9', r == '_':
			b.WriteRune(r)
		case r > 0xFFFF:
			b.WriteString("__")
		default:
			b.WriteByte('_')
		}
	}
	return b.String()
}

// completeAliasForm is `alias.length === 1 ? -alias : --alias`; JS length
// counts UTF-16 code units.
func completeAliasForm(alias string) string {
	if len(utf16.Encode([]rune(alias))) == 1 {
		return "-" + alias
	}
	return "--" + alias
}

// completeFlagForms is the flag's long name and aliases, plus --no-name for
// booleans when negate is set (bash's flagNamesForWordlist, zsh's allForms).
func completeFlagForms(f completeFlag, negate bool) []string {
	forms := []string{"--" + f.name}
	for _, a := range f.aliases {
		forms = append(forms, completeAliasForm(a))
	}
	if negate && f.typ.tag == "Boolean" {
		forms = append(forms, "--no-"+f.name)
	}
	return forms
}

// completeNegatedName is the long name that sets a boolean to false,
// `no-name`.
func completeNegatedName(f completeFlag) (string, bool) {
	if f.typ.tag != "Boolean" {
		return "", false
	}
	return "no-" + f.name, true
}

// completeFuncName is `_${path.map(sanitize).join("_")}`.
func completeFuncName(path []string) string {
	parts := make([]string, len(path))
	for i, p := range path {
		parts[i] = completeSanitize(p)
	}
	return "_" + strings.Join(parts, "_")
}
