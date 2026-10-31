package flickrt

import (
	"fmt"
	"strconv"
	"strings"
)

func cliPath(inputs []cliInput) string {
	var out strings.Builder
	for i, input := range inputs {
		if i > 0 {
			out.WriteByte(' ')
		}
		out.WriteString(input.command.name)
	}
	return out.String()
}

func cliHelp(command *cliSpec, path string) string {
	var out strings.Builder
	out.WriteString("Usage: ")
	out.WriteString(path)
	for _, arg := range command.args {
		out.WriteString(" <" + cliArgName(arg) + ">")
		if arg.multi == cliSome {
			out.WriteString("...")
		}
	}
	if len(command.subcommands) > 0 {
		if command.handler == nil {
			out.WriteString(" <command>")
		} else {
			out.WriteString(" [command]")
		}
	}
	out.WriteString(" [options]\n")
	if command.description != "" {
		out.WriteString("\n" + command.description + "\n")
	}
	if len(command.args) > 0 {
		out.WriteString("\nArguments:\n")
		for _, arg := range command.args {
			out.WriteString("  <" + cliArgName(arg) + ">")
			if arg.multi == cliSome {
				out.WriteString("... (at least one)")
			}
			if arg.help != "" {
				out.WriteString("  " + arg.help)
			}
			out.WriteByte('\n')
		}
	}
	if len(command.subcommands) > 0 {
		out.WriteString("\nCommands:\n")
		for _, sub := range command.subcommands {
			help := sub.help
			if help == "" {
				help = sub.command.description
			}
			out.WriteString("  " + sub.command.name)
			if help != "" {
				out.WriteString("  " + help)
			}
			out.WriteByte('\n')
		}
	}
	out.WriteString("\nOptions:\n  -h, --help           Show this command's help\n  --version            Show the version\n  --completions bash   Write a Bash completion script\n")
	return out.String()
}

func cliShellQuote(text string) string { return "'" + strings.ReplaceAll(text, "'", "'\\''") + "'" }

func cliCompletionFunction(command *cliSpec) string {
	tag := Tag("Cli.Completion." + command.name)
	return "_flick_cli_" + tag[strings.LastIndexByte(tag, '#')+1:]
}

// Bash's native completion protocol needs no running CLI or shell dependency.
func cliBashCompletions(command *cliSpec) string {
	var out strings.Builder
	name := cliCompletionFunction(command)
	out.WriteString("# Source this script in Bash to enable completions.\n" + name + `() {
  local cur="${COMP_WORDS[COMP_CWORD]}" state=0 count=0 ended=0 need_shell=0 word i
  local -a choices
  COMPREPLY=()
  for ((i=1; i<COMP_CWORD; i++)); do
    word="${COMP_WORDS[i]}"
    if (( need_shell )); then need_shell=0; continue; fi
    if (( ended )); then ((count += 1)); continue; fi
    case "$word" in
      --) ended=1; continue ;;
      --completions) need_shell=1; continue ;;
      --completions=*|--help|-h|--version) continue ;;
      -*) [[ "$word" == -[0-9]* || "$word" == -.[0-9]* || "$word" == - ]] || return ;;
    esac
    case "$state:$word" in
`)
	var transitions func(*cliSpec, string)
	transitions = func(spec *cliSpec, state string) {
		for i, sub := range spec.subcommands {
			next := state + "_" + strconv.Itoa(i)
			fmt.Fprintf(&out, "      %s) if (( count >= %d )); then state=%s; count=0; else ((count += 1)); fi ;;\n", cliShellQuote(state+":"+sub.command.name), len(spec.args), next)
			transitions(sub.command, next)
		}
	}
	transitions(command, "0")
	out.WriteString(`      *) ((count += 1)) ;;
    esac
  done
  if (( ended )); then return; fi
  if (( need_shell )); then
    choices=('bash')
  elif [[ "$cur" == --completions=* ]]; then
    choices=('--completions=bash')
  else
    choices=('--help' '--version' '--completions')
    case "$state" in
`)
	var candidates func(*cliSpec, string)
	candidates = func(spec *cliSpec, state string) {
		if len(spec.subcommands) > 0 {
			fmt.Fprintf(&out, "      %s) if (( count >= %d )); then choices+=(", cliShellQuote(state), len(spec.args))
			for _, sub := range spec.subcommands {
				out.WriteString(cliShellQuote(sub.command.name) + " ")
			}
			out.WriteString("); fi ;;\n")
		}
		for i, sub := range spec.subcommands {
			candidates(sub.command, state+"_"+strconv.Itoa(i))
		}
	}
	candidates(command, "0")
	out.WriteString(`    esac
  fi
  for word in "${choices[@]}"; do
    [[ "$word" == "$cur"* ]] && COMPREPLY+=("$word")
  done
  return 0
}
`)
	out.WriteString("complete -o default -F " + name + " -- " + cliShellQuote(command.name) + "\n")
	return out.String()
}
