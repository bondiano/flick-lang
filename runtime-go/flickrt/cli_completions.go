//go:build flickcli

package flickrt

import (
	"regexp"
	"strconv"
	"strings"
)

// Shell completion scripts as Effect's CLI writes them
// (`internal/completions/*.js`), from a descriptor of the command tree.

type cliCompType struct {
	tag      string // "Boolean", "Choice", "Path" or "String"
	values   []string
	pathType string
}

type cliFlagDesc struct {
	name          string
	aliases       []string
	description   *string
	typ           cliCompType
	optionalValue bool
}

type cliArgDesc struct {
	name               string
	description        *string
	required, variadic bool
	typ                cliCompType
}

type cliDescriptor struct {
	name        string
	description *string
	flags       []cliFlagDesc
	arguments   []cliArgDesc
	subcommands []*cliDescriptor
}

func cliCompTypeOf(p *cliPrim, flag bool) cliCompType {
	switch p.tag {
	case "Boolean":
		if flag {
			return cliCompType{tag: "Boolean"}
		}
	case "Choice":
		return cliCompType{tag: "Choice", values: p.choiceKeys()}
	case "Path":
		return cliCompType{tag: "Path", pathType: p.pathType}
	case "FileText", "FileParse":
		return cliCompType{tag: "Path", pathType: "file"}
	}
	return cliCompType{tag: "String"}
}

// cliDescriptorOf describes the command and, below it, its listed
// subcommands, which complete the flags they share with it too.
func cliDescriptorOf(c *cliCmd, inherited []*cliEParam) *cliDescriptor {
	d := &cliDescriptor{name: c.name, description: c.description}
	if c.shortDescription != nil {
		d.description = c.shortDescription
	}
	seen := map[string]bool{}
	for i, p := range append(append([]*cliEParam(nil), c.config.flags...), inherited...) {
		s := p.single
		if s.kind != "flag" || s.hidden || i >= len(c.config.flags) && seen[s.name] {
			continue
		}
		seen[s.name] = true
		d.flags = append(d.flags, cliFlagDesc{s.name, s.aliases, s.description, cliCompTypeOf(s.prim, true), s.optionalValue != nil})
	}
	for _, p := range c.config.args {
		s := p.single
		d.arguments = append(d.arguments, cliArgDesc{s.name, s.description, !p.isOptional(), p.isVariadic(), cliCompTypeOf(s.prim, false)})
	}
	shared := append(append([]*cliEParam(nil), inherited...), c.contextConfig.flags...)
	for _, g := range c.subcommands {
		for _, sub := range g.commands {
			if sub.unlisted {
				continue
			}
			sd := cliDescriptorOf(sub, shared)
			d.subcommands = append(d.subcommands, sd)
			if sub.alias != nil && *sub.alias != sub.name {
				alias := *sd
				alias.name = *sub.alias
				d.subcommands = append(d.subcommands, &alias)
			}
		}
	}
	return d
}

func cliTakesValue(f cliFlagDesc) bool { return f.typ.tag != "Boolean" && !f.optionalValue }

func cliCompletionScript(name, shell string, d *cliDescriptor) string {
	switch shell {
	case "bash":
		return cliBash(name, d)
	case "zsh":
		return cliZsh(name, d)
	case "fish":
		return cliFish(name, d)
	}
	return cliPowerShell(name, d)
}

var cliNonWord = regexp.MustCompile(`[^a-zA-Z0-9_]`)

func cliSanitize(s string) string { return cliNonWord.ReplaceAllString(s, "_") }

// cliFunctionName is the function of the command at the path.
func cliFunctionName(path []string) string {
	parts := make([]string, len(path))
	for i, p := range path {
		parts[i] = cliSanitize(p)
	}
	return "_" + strings.Join(parts, "_")
}

func cliEscapeBash(s string) string { return strings.ReplaceAll(s, "'", `'\''`) }

func cliForm(alias string) string {
	if len(utf16Units(alias)) == 1 {
		return "-" + alias
	}
	return "--" + alias
}

// cliForms is the words of a flag: its name, its aliases and, for a
// switch, its negation.
func cliForms(f cliFlagDesc, negation bool) []string {
	out := []string{"--" + f.name}
	for _, a := range f.aliases {
		out = append(out, cliForm(a))
	}
	if negation && f.typ.tag == "Boolean" {
		out = append(out, "--no-"+f.name)
	}
	return out
}

func cliBashChoices(helper string, values []string) string {
	quoted := make([]string, len(values))
	for i, v := range values {
		quoted[i] = "'" + cliEscapeBash(v) + "'"
	}
	return helper + ` "$cur" "$_comp_word" ` + strings.Join(quoted, " ")
}

func cliBashValue(t cliCompType, helper string) string {
	switch t.tag {
	case "Choice":
		return cliBashChoices(helper, t.values)
	case "Path":
		if t.pathType == "directory" {
			return `COMPREPLY=( $(compgen -d -- "$cur") )`
		}
		return `COMPREPLY=( $(compgen -f -- "$cur") )`
	}
	return ""
}

func cliBashFunction(d *cliDescriptor, parent []string, out *[]string, helper string) {
	add := func(lines ...string) { *out = append(*out, lines...) }
	path := append(append([]string(nil), parent...), d.name)
	add(cliFunctionName(path)+"()", "{", "  local cur prev words cword i")
	if len(parent) == 0 {
		add("  local _command_index=0")
	} else {
		add(`  local _command_index="$1"`)
	}
	add(`  _init_completion -n "$COMP_WORDBREAKS" || return`)
	if len(parent) == 0 {
		add(`  local _comp_word="$2"`)
	}
	add("")
	var valued []cliFlagDesc
	for _, f := range d.flags {
		if cliTakesValue(f) {
			valued = append(valued, f)
		}
	}
	if len(valued) > 0 {
		add("  # Flag value completions", `  case "$prev" in`)
		for _, f := range valued {
			if completion := cliBashValue(f.typ, helper); completion != "" {
				add("    "+strings.Join(cliForms(f, false), "|")+")", "      "+completion, "      return", "      ;;")
			}
		}
		add("  esac", "")
	}
	if len(d.subcommands) > 0 {
		add("  # Subcommand dispatch", "  local cmd _skip_next=0", "  for ((i = _command_index + 1; i < cword; i++)); do",
			"    if (( _skip_next )); then", "      _skip_next=0", "      continue", "    fi", `    case "${words[i]}" in`)
		for _, f := range valued {
			forms := cliForms(f, true)
			add("      "+strings.Join(forms, "|")+") _skip_next=1 ;;", "      "+cliJoinSuffix(forms, "=*")+") ;;")
		}
		for _, sub := range d.subcommands {
			add("      "+sub.name+")", "        "+cliFunctionName(append(path[:len(path):len(path)], sub.name))+` "$i"`,
				"        return", "        ;;")
		}
		add("    esac", "  done", "")
	}
	if len(d.flags) > 0 {
		used := make([]string, len(d.flags))
		for i := range d.flags {
			used[i] = "_used_" + strconv.Itoa(i) + `=""`
		}
		add("  local "+strings.Join(used, " "), "  for ((i = 1; i < cword; i++)); do", `    case "${words[i]%%=*}" in`)
		for i, f := range d.flags {
			add("      " + strings.Join(cliForms(f, true), "|") + ") _used_" + strconv.Itoa(i) + "=1 ;;")
		}
		add("    esac", "  done", `  local _filtered_flags=""`)
		for i, f := range d.flags {
			add(`  [[ -n "$_used_` + strconv.Itoa(i) + `" ]] || _filtered_flags+=" ` + strings.Join(cliForms(f, true), " ") + `"`)
		}
		add("")
	}
	if len(d.flags) > 0 || len(d.subcommands) > 0 {
		add("  # Complete flags (filtered) and subcommands", `  if [[ "$cur" == -* ]]; then`)
		if len(d.flags) > 0 {
			add(`    COMPREPLY=( $(compgen -W "$_filtered_flags" -- "$cur") )`)
		}
		add("    return", "  fi", "")
	}
	type completed struct {
		arg        cliArgDesc
		completion string
		index      int
	}
	var args []completed
	for i, a := range d.arguments {
		if c := cliBashValue(a.typ, helper); c != "" {
			args = append(args, completed{a, c, i})
		}
	}
	if len(args) > 0 {
		add("  # Positional argument completions", "  local _position=0 _skip_next=0 _end_of_options=0",
			"  for ((i = _command_index + 1; i < cword; i++)); do", "    if (( _skip_next )); then", "      _skip_next=0",
			"      continue", "    fi", "    if (( _end_of_options )); then", "      ((_position += 1))", "      continue",
			"    fi", `    case "${words[i]}" in`, "      --) _end_of_options=1 ;;")
		for _, f := range d.flags {
			forms := cliForms(f, true)
			if !cliTakesValue(f) {
				add("      " + strings.Join(forms, "|") + ") ;;")
			} else {
				add("      "+strings.Join(forms, "|")+") _skip_next=1 ;;", "      "+cliJoinSuffix(forms, "=*")+") ;;")
			}
		}
		add("      -*) ;;", "      *) ((_position += 1)) ;;", "    esac", "  done", `  case "$_position" in`)
		var variadic *completed
		for i := range args {
			if args[i].arg.variadic {
				if variadic == nil {
					variadic = &args[i]
				}
				continue
			}
			add("    "+strconv.Itoa(args[i].index)+")", "      "+args[i].completion, "      return", "      ;;")
		}
		add("  esac")
		if variadic != nil {
			add("  if (( _position >= "+strconv.Itoa(variadic.index)+" )); then", "    "+variadic.completion, "    return", "  fi")
		}
	} else if len(d.subcommands) > 0 {
		names := make([]string, len(d.subcommands))
		for i, s := range d.subcommands {
			names[i] = s.name
		}
		add(`  COMPREPLY=( $(compgen -W '` + strings.Join(names, " ") + `' -- "$cur") )`)
	}
	add("}", "")
	for _, sub := range d.subcommands {
		cliBashFunction(sub, path, out, helper)
	}
}

func cliJoinSuffix(forms []string, suffix string) string {
	out := make([]string, len(forms))
	for i, f := range forms {
		out[i] = f + suffix
	}
	return strings.Join(out, "|")
}

// cliBashInit is the stand-in of bash-completion's `_init_completion`,
// keeping word-break characters inside values.
const cliBashInit = `if ! type _init_completion &>/dev/null; then
  _init_completion()
  {
    COMPREPLY=()
    local _i _j=0 _piece _line="$COMP_LINE"
    words=("${COMP_WORDS[0]}")
    cword=0
    _line="${_line#*"${COMP_WORDS[0]}"}"
    for ((_i = 1; _i < ${#COMP_WORDS[@]}; _i++)); do
      _piece="${COMP_WORDS[_i]}"
      if [[ "$_line" == [[:blank:]]* ]]; then
        ((_j++))
        words[_j]="$_piece"
      else
        words[_j]="${words[_j]}$_piece"
      fi
      ((_i == COMP_CWORD)) && cword=$_j
      _line="${_line#*"$_piece"}"
    done
    cur="${words[cword]}"
    prev=""
    ((cword > 0)) && prev="${words[cword-1]}"
    return 0
  }
fi
`

// cliBashChoicesHelper completes choices inside quotes as readline expects.
const cliBashChoicesHelper = `{
  local _cur="$1" _word="$2"; shift 2

  local _head="${_cur%"$_word"}"
  local _open=""
  case "$_head" in
    *\') _open="'" ;;
    *\") _open='"' ;;
  esac

  local _prefix="$_cur" _committed="$_head"
  _prefix=${_prefix//\\/}; _prefix=${_prefix//\"/}; _prefix=${_prefix//\'/}
  _committed=${_committed//\\/}; _committed=${_committed//\"/}; _committed=${_committed//\'/}

  COMPREPLY=()
  local _choice _rest _match
  for _choice in "$@"; do
    [[ "$_choice" == "$_prefix"* ]] || continue
    _rest="${_choice#"$_committed"}"
    case "$_open" in
      "'")
        if [[ "$_head" == "'" ]]; then
          _match=${_rest//\'/\'\\\'\'}
        else
          [[ "$_rest" == *\'* ]] && continue
          _match="$_rest"
        fi
        ;;
      '"')
        _match="${_rest//\\/\\\\}"
        _match="${_match//\$/\\$}"
        _match="${_match//\` + "`" + `/\\\` + "`" + `}"
        _match="${_match//\"/\\\"}"
        ;;
      *)
        printf -v _match '%q' "$_rest"
        [[ -z "$_head" && "$_match" == '~'* ]] && _match="\\$_match"
        ;;
    esac
    [[ -n "$_open" && "$_match" == *"$_open" ]] && _match+="$_open"
    COMPREPLY+=("$_match")
  done
}
`

func cliBash(name string, d *cliDescriptor) string {
	safe := cliSanitize(name)
	helper := "_" + safe + "--choices"
	lines := []string{"###-begin-" + cliEscapeBash(name) + "-completions-###", "#", "# Static completion script for Bash", "#",
		"# Installation:", "#   " + cliEscapeBash(name) + " --completions bash >> ~/.bashrc", "#", "",
		cliBashInit + "\n" + helper + "()\n" + cliBashChoicesHelper}
	cliBashFunction(d, nil, &lines, helper)
	lines = append(lines, "complete -F _"+safe+" "+cliEscapeBash(name), "###-end-"+cliEscapeBash(name)+"-completions-###")
	return strings.Join(lines, "\n")
}
