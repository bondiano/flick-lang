package flickrt

import (
	"strconv"
	"strings"
)

// Ported line for line from Effect 4's cli/internal/completions/bash.ts so
// `--completions bash` prints the same static script as the TypeScript CLI.

// completeEscapeBash is escapeForBash: close, escape and reopen a single quote.
func completeEscapeBash(s string) string {
	return strings.ReplaceAll(s, "'", `'\''`)
}

// completeBashFallback stands in for bash-completion's _init_completion while
// keeping word-break characters inside values.
const completeBashFallback = `if ! type _init_completion &>/dev/null; then
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

// completeBashChoicesBody completes choices without `compgen -W`, which
// reparses quoted values, escaping only what readline replaces for the
// active quote context.
const completeBashChoicesBody = `{
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

// completeBashPathsBody completes paths through the choice helper ({choices}),
// one `compgen` line per candidate, keeping the typed directory as written.
const completeBashPathsBody = `{
  local _kind="$1" _cur="$2" _word="$3"

  local _prefix="$_cur" _committed="${_cur%"$_word"}"
  _prefix=${_prefix//\\/}; _prefix=${_prefix//\"/}; _prefix=${_prefix//\'/}
  _committed=${_committed//\\/}; _committed=${_committed//\"/}; _committed=${_committed//\'/}

  local _typed="" _dir="$_committed" _open="" _i
  if [[ "$_word" == */* ]]; then
    _typed="${_word%/*}/"
    _dir="${_prefix%/*}/"
    if [[ "$_cur" == "$_word" ]]; then
      for ((_i = 0; _i < ${#_typed}; _i++)); do
        case "$_open${_typed:_i:1}" in
          \\|\"\\) ((_i += 1)) ;;
          \"|\') _open="${_typed:_i:1}" ;;
          \"\"|\'\') _open="" ;;
        esac
      done
    fi
  fi

  local _path _names=() _rest="${_word#"$_typed"}"
  while IFS= read -r _path; do
    if [[ "$_path" == "$_dir"* ]]; then
      _names+=("$_committed${_path#"$_dir"}")
    elif [[ -z "$_committed" ]]; then
      _names+=("$_path")
      _typed="" _rest="" _open=""
    fi
  done < <(compgen "$_kind" -- "$_prefix")
  if [[ -n "$_open" ]]; then
    _typed+="$_open"
    _rest="${_prefix##*/}"
  fi

  {choices} "${_cur%"$_word"}$_rest" "$_rest" "${_names[@]}"
  for ((_i = 0; _i < ${#COMPREPLY[@]}; _i++)); do
    COMPREPLY[_i]="$_typed${COMPREPLY[_i]}"
  done
}
`

// completeBashValue is flagValueCompletion / argCompletion; "" when the type
// has no completion.
func completeBashValue(t completeType, helper string) string {
	switch t.tag {
	case "Choice":
		quoted := make([]string, len(t.values))
		for i, v := range t.values {
			quoted[i] = "'" + completeEscapeBash(v) + "'"
		}
		return helper + ` "$cur" "$_comp_word" ` + strings.Join(quoted, " ")
	case "Path":
		paths := strings.TrimSuffix(helper, "choices") + "paths"
		if t.pathType == "directory" {
			return paths + ` -d "$cur" "$_comp_word"`
		}
		return paths + ` -f "$cur" "$_comp_word"`
	}
	return ""
}

// completeBashValueCases emits the case arms that skip a flag's value.
func completeBashValueCases(forms []string, lines []string) []string {
	eq := make([]string, len(forms))
	for i, f := range forms {
		eq[i] = f + "=*"
	}
	return append(lines,
		"      "+strings.Join(forms, "|")+") _skip_next=1 ;;",
		"      "+strings.Join(eq, "|")+") ;;")
}

func completeBashFunction(d completeCommand, parent []string, lines []string, helper string) []string {
	path := append(append([]string{}, parent...), d.name)
	lines = append(lines, completeFuncName(path)+"()", "{", "  local cur prev words cword i")
	if len(parent) == 0 {
		lines = append(lines, "  local _command_index=0")
	} else {
		lines = append(lines, `  local _command_index="$1"`)
	}
	// Keep values containing COMP_WORDBREAKS characters in one word.
	lines = append(lines, `  _init_completion -n "$COMP_WORDBREAKS" || return`)
	if len(parent) == 0 {
		// Subcommand functions inherit this through Bash's dynamic scope.
		lines = append(lines, `  local _comp_word="$2"`)
	}
	lines = append(lines, "")

	hasValues := false
	for _, f := range d.flags {
		hasValues = hasValues || completeTakesSeparateValue(f)
	}
	if hasValues {
		lines = append(lines, "  # Flag value completions", `  case "$prev" in`)
		for _, f := range d.flags {
			if !completeTakesSeparateValue(f) {
				continue
			}
			if c := completeBashValue(f.typ, helper); c != "" {
				lines = append(lines, "    "+strings.Join(completeFlagForms(f, false), "|")+")",
					"      "+c, "      return", "      ;;")
			}
		}
		lines = append(lines, "  esac", "")
	}

	if len(d.subcommands) > 0 {
		lines = append(lines,
			"  # Subcommand dispatch",
			"  local cmd _skip_next=0",
			"  for ((i = _command_index + 1; i < cword; i++)); do",
			"    if (( _skip_next )); then",
			"      _skip_next=0",
			"      continue",
			"    fi",
			`    case "${words[i]}" in`)
		for _, f := range d.flags {
			if completeTakesSeparateValue(f) {
				lines = completeBashValueCases(completeFlagForms(f, true), lines)
			}
		}
		for _, sub := range d.subcommands {
			lines = append(lines, "      "+sub.name+")",
				"        "+completeFuncName(append(append([]string{}, path...), sub.name))+` "$i"`,
				"        return", "        ;;")
		}
		lines = append(lines, "    esac", "  done", "")
	}

	// Filter already-used flags: Bash 3.2 has no associative arrays.
	if len(d.flags) > 0 {
		groups := make([][]string, len(d.flags))
		used := make([]string, len(d.flags))
		for i, f := range d.flags {
			groups[i] = completeFlagForms(f, true)
			used[i] = "_used_" + strconv.Itoa(i) + `=""`
		}
		lines = append(lines, "  local "+strings.Join(used, " "),
			"  for ((i = 1; i < cword; i++)); do",
			`    case "${words[i]%%=*}" in`)
		for i, forms := range groups {
			lines = append(lines, "      "+strings.Join(forms, "|")+") _used_"+strconv.Itoa(i)+"=1 ;;")
		}
		lines = append(lines, "    esac", "  done", `  local _filtered_flags=""`)
		for i, forms := range groups {
			lines = append(lines, `  [[ -n "$_used_`+strconv.Itoa(i)+`" ]] || _filtered_flags+=" `+strings.Join(forms, " ")+`"`)
		}
		lines = append(lines, "")
	}

	if len(d.flags) > 0 || len(d.subcommands) > 0 {
		lines = append(lines, "  # Complete flags (filtered) and subcommands", `  if [[ "$cur" == -* ]]; then`)
		if len(d.flags) > 0 {
			lines = append(lines, `    COMPREPLY=( $(compgen -W "$_filtered_flags" -- "$cur") )`)
		}
		lines = append(lines, "    return", "  fi", "")
	}

	type argCompletion struct {
		variadic   bool
		completion string
		index      int
	}
	var withCompletions []argCompletion
	for i, a := range d.args {
		if c := completeBashValue(a.typ, helper); c != "" {
			withCompletions = append(withCompletions, argCompletion{a.variadic, c, i})
		}
	}
	if len(withCompletions) > 0 {
		lines = append(lines,
			"  # Positional argument completions",
			"  local _position=0 _skip_next=0 _end_of_options=0",
			"  for ((i = _command_index + 1; i < cword; i++)); do",
			"    if (( _skip_next )); then",
			"      _skip_next=0",
			"      continue",
			"    fi",
			"    if (( _end_of_options )); then",
			"      ((_position += 1))",
			"      continue",
			"    fi",
			`    case "${words[i]}" in`,
			"      --) _end_of_options=1 ;;")
		for _, f := range d.flags {
			forms := completeFlagForms(f, true)
			if completeTakesSeparateValue(f) {
				lines = completeBashValueCases(forms, lines)
			} else {
				lines = append(lines, "      "+strings.Join(forms, "|")+") ;;")
			}
		}
		lines = append(lines, "      -*) ;;", "      *) ((_position += 1)) ;;", "    esac", "  done", `  case "$_position" in`)
		for _, a := range withCompletions {
			if !a.variadic {
				lines = append(lines, "    "+strconv.Itoa(a.index)+")", "      "+a.completion, "      return", "      ;;")
			}
		}
		lines = append(lines, "  esac")
		for _, a := range withCompletions {
			if a.variadic {
				lines = append(lines, "  if (( _position >= "+strconv.Itoa(a.index)+" )); then",
					"    "+a.completion, "    return", "  fi")
				break
			}
		}
	} else if len(d.subcommands) > 0 {
		names := make([]string, len(d.subcommands))
		for i, sub := range d.subcommands {
			names[i] = sub.name
		}
		lines = append(lines, "  COMPREPLY=( $(compgen -W '"+strings.Join(names, " ")+`' -- "$cur") )`)
	}

	lines = append(lines, "}", "")
	for _, sub := range d.subcommands {
		lines = completeBashFunction(sub, path, lines, helper)
	}
	return lines
}

// completeBash is bash.ts's generate: a self-contained script that never
// re-invokes the CLI.
func completeBash(executable string, d completeCommand) string {
	safe := completeSanitize(executable)
	// Sanitized command names cannot contain `-`.
	helper := "_" + safe + "--choices"
	exe := completeEscapeBash(executable)
	lines := []string{
		"###-begin-" + exe + "-completions-###",
		"#",
		"# Static completion script for Bash",
		"#",
		"# Installation:",
		"#   " + exe + " --completions bash >> ~/.bashrc",
		"#",
		"",
		completeBashFallback,
		helper + "()",
		completeBashChoicesBody,
		"_" + safe + "--paths()",
		strings.ReplaceAll(completeBashPathsBody, "{choices}", helper),
	}
	lines = completeBashFunction(d, nil, lines, helper)
	lines = append(lines, "complete -F _"+safe+" "+exe, "###-end-"+exe+"-completions-###")
	return strings.Join(lines, "\n")
}
