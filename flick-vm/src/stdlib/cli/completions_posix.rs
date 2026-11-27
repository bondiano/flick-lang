//! Bash, Zsh, and Fish static shell completion generators matching Effect 4.1.2.

use std::fmt::Write as _;

use super::completions::{Command, Flag, footer, header, sanitize, short};
use super::{CliMultiplicity, CliParam};

pub(super) fn bash(command: &Command<'_>) -> String {
    let name = command.name;
    let safe = sanitize(name);
    let mut output = String::new();
    header(
        &mut output,
        name,
        "Bash",
        &format!("#   {name} --completions bash >> ~/.bashrc"),
    );
    output.push_str(&BASH_HELPERS.replace("!SAFE!", &safe));
    bash_function(&mut output, command, &format!("_{safe}"), true);
    writeln!(output, "complete -F _{safe} {name}").expect("write to String");
    footer(&mut output, name);
    output
}

fn bash_function(output: &mut String, command: &Command<'_>, function: &str, root: bool) {
    let command_index = if root { "0" } else { r#""$1""# };
    writeln!(
        output,
        "{function}()\n{{\n  local cur prev words cword i\n  local _command_index={command_index}"
    )
    .expect("write to String");
    output.push_str("  _init_completion -n \"$COMP_WORDBREAKS\" || return\n");
    if root {
        output.push_str("  local _comp_word=\"$2\"\n");
    }
    output.push('\n');

    if !command.flags.is_empty() {
        output.push_str("  # Flag value completions\n  case \"$prev\" in\n  esac\n\n");
    }

    if !command.subcommands.is_empty() {
        bash_subcommand_dispatch(output, command, function);
    }

    if !command.flags.is_empty() {
        let declarations = (0..command.flags.len())
            .map(|index| format!(r#"_used_{index}="""#))
            .collect::<Vec<_>>()
            .join(" ");
        writeln!(output, "  local {declarations}").expect("write to String");
        output.push_str(
            r#"  for ((i = 1; i < cword; i++)); do
    case "${words[i]%%=*}" in
"#,
        );
        for (index, flag) in command.flags.iter().enumerate() {
            writeln!(output, "      {}) _used_{index}=1 ;;", flag.forms.join("|"))
                .expect("write to String");
        }
        output.push_str("    esac\n  done\n  local _filtered_flags=\"\"\n");
        for (index, flag) in command.flags.iter().enumerate() {
            writeln!(
                output,
                "  [[ -n \"$_used_{index}\" ]] || _filtered_flags+=\" {}\"",
                flag.forms.join(" ")
            )
            .expect("write to String");
        }
        output.push('\n');
    }
    if !command.flags.is_empty() || !command.subcommands.is_empty() {
        output.push_str(
            "  # Complete flags (filtered) and subcommands\n  if [[ \"$cur\" == -* ]]; then\n",
        );
        if !command.flags.is_empty() {
            output.push_str("    COMPREPLY=( $(compgen -W \"$_filtered_flags\" -- \"$cur\") )\n");
        }
        output.push_str("    return\n  fi\n\n");
    }
    if !command.subcommands.is_empty() {
        let names = command
            .subcommands
            .iter()
            .map(|sub| sub.name)
            .collect::<Vec<_>>()
            .join(" ");
        writeln!(
            output,
            "  COMPREPLY=( $(compgen -W '{names}' -- \"$cur\") )"
        )
        .expect("write to String");
    }
    output.push_str("}\n\n");
    for sub in &command.subcommands {
        bash_function(
            output,
            sub,
            &format!("{function}_{}", sanitize(sub.name)),
            false,
        );
    }
}

fn bash_subcommand_dispatch(output: &mut String, command: &Command<'_>, function: &str) {
    output.push_str(
        r#"  # Subcommand dispatch
  local cmd _skip_next=0
  for ((i = _command_index + 1; i < cword; i++)); do
    if (( _skip_next )); then
      _skip_next=0
      continue
    fi
    case "${words[i]}" in
"#,
    );
    for flag in &command.flags {
        let forms = flag.forms.join("|");
        let assignments = flag
            .forms
            .iter()
            .map(|form| format!("{form}=*"))
            .collect::<Vec<_>>()
            .join("|");
        writeln!(
            output,
            "      {forms}) _skip_next=1 ;;\n      {assignments}) ;;"
        )
        .expect("write to String");
    }
    for sub in &command.subcommands {
        let name = sub.name;
        let child = sanitize(name);
        writeln!(
            output,
            "      {name})\n        {function}_{child} \"$i\"\n        return\n        ;;"
        )
        .expect("write to String");
    }
    output.push_str("    esac\n  done\n\n");
}

pub(super) fn zsh(command: &Command<'_>) -> String {
    let name = command.name;
    let mut output = format!("#compdef {name}\n");
    header(
        &mut output,
        name,
        "Zsh",
        &format!(
            "#   {name} --completions zsh > ~/.zsh/completions/_{name}\n#   then add ~/.zsh/completions to your fpath"
        ),
    );
    let function = format!("_{}", sanitize(name));
    zsh_function(&mut output, command, &function);
    output.push_str(
        r#"# Handle both direct invocation and autoload
if [[ "${zsh_eval_context[-1]}" == "loadautofunc" ]]; then
"#,
    );
    writeln!(
        output,
        "  {function} \"$@\"\nelse\n  compdef {function} {name}\nfi"
    )
    .expect("write to String");
    footer(&mut output, name);
    output
}

fn zsh_escape(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('\'', "'\\''")
        .replace(':', "\\:")
}

fn zsh_flags(output: &mut String, flags: &[Flag<'_>]) {
    for flag in flags {
        let excludes = flag.forms.join(" ");
        let description = if flag.param.help.is_empty() {
            String::new()
        } else {
            format!("[{}]", zsh_escape(&flag.param.help).replace(']', "\\]"))
        };
        for form in &flag.forms {
            writeln!(output, "    '({excludes}){form}{description}:value:'")
                .expect("write to String");
        }
    }
}

fn zsh_arguments(output: &mut String, arguments: &[&CliParam]) {
    for param in arguments {
        let prefix = if matches!(param.multi, CliMultiplicity::Many) {
            "*"
        } else {
            ""
        };
        let description = if param.help.is_empty() {
            param.label.replace('_', "-")
        } else {
            zsh_escape(&param.help)
        };
        writeln!(output, "    '{prefix}:{description}:'").expect("write to String");
    }
}

fn zsh_function(output: &mut String, command: &Command<'_>, function: &str) {
    writeln!(output, "{function}() {{").expect("write to String");
    if command.subcommands.is_empty() {
        if !command.flags.is_empty() || !command.arguments.is_empty() {
            output.push_str("  local -a specs\n  specs=(\n");
            zsh_flags(output, &command.flags);
            zsh_arguments(output, &command.arguments);
            output.push_str("  )\n  _arguments \"${specs[@]}\"\n");
        }
    } else {
        output.push_str(
            "  local context state state_descr line\n  typeset -A opt_args\n\n  local -a commands\n  commands=(\n",
        );
        for sub in &command.subcommands {
            writeln!(output, "    '{}:{}'", sub.name, zsh_escape(sub.description))
                .expect("write to String");
        }
        output.push_str("  )\n\n  local -a specs\n  specs=(\n");
        zsh_flags(output, &command.flags);
        if !command.arguments.is_empty() {
            output.push_str("    -\n    parent-arguments\n");
            zsh_arguments(output, &command.arguments);
            output.push_str("    -\n    subcommands\n");
        }
        output.push_str(
            r#"    '1:command:->command'
    '*::arg:->args'
  )

  _arguments -C "${specs[@]}"

  case "$state" in
    command)
      _describe -t commands 'commands' commands
      ;;
    args)
      case "$words[1]" in
"#,
        );
        for sub in &command.subcommands {
            writeln!(
                output,
                "        {})\n          {function}_{}\n          ;;",
                sub.name,
                sanitize(sub.name)
            )
            .expect("write to String");
        }
        output.push_str("      esac\n      ;;\n  esac\n");
    }
    output.push_str("}\n\n");
    for sub in &command.subcommands {
        zsh_function(output, sub, &format!("{function}_{}", sanitize(sub.name)));
    }
}

pub(super) fn fish(command: &Command<'_>) -> String {
    let name = command.name;
    let mut output = String::new();
    header(
        &mut output,
        name,
        "Fish",
        &format!("#   {name} --completions fish > ~/.config/fish/completions/{name}.fish"),
    );
    fish_context(&mut output, command, name, &mut Vec::new());
    output.push('\n');
    footer(&mut output, name);
    output
}

fn fish_description(help: &str) -> String {
    if help.is_empty() {
        return String::new();
    }
    format!(" -d '{}'", help.replace('\\', "\\\\").replace('\'', "\\'"))
}

fn fish_context<'a>(
    output: &mut String,
    command: &Command<'a>,
    name: &str,
    path: &mut Vec<&'a str>,
) {
    let mut condition = if path.is_empty() {
        if command.subcommands.is_empty() {
            String::new()
        } else {
            "__fish_use_subcommand".to_owned()
        }
    } else {
        path.iter()
            .map(|step| format!("__fish_seen_subcommand_from {step}"))
            .collect::<Vec<_>>()
            .join("; and ")
    };
    if !path.is_empty() && !command.subcommands.is_empty() {
        condition.push_str("; and not __fish_seen_subcommand_from");
        for sub in &command.subcommands {
            write!(condition, " {}", sub.name).expect("write to String");
        }
    }
    let context = if condition.is_empty() {
        String::new()
    } else {
        format!(" -n '{condition}'")
    };
    let joined = |other: &str| {
        if condition.is_empty() {
            other.to_owned()
        } else {
            format!("{condition}; and {other}")
        }
    };
    writeln!(output, "complete -c {name}{context} -f").expect("write to String");
    for sub in &command.subcommands {
        writeln!(
            output,
            "complete -c {name}{context} -f -a '{}'{}",
            sub.name,
            fish_description(sub.description)
        )
        .expect("write to String");
    }
    let mut used_flags = Vec::new();
    for flag in &command.flags {
        let mut declaration = format!("-l {}", flag.name);
        let mut used = "not __fish_contains_opt".to_owned();
        for alias in flag.param.aliases.iter().filter(|alias| short(alias)) {
            write!(used, " -s {alias}").expect("write to String");
        }
        write!(used, " {}", flag.name).expect("write to String");
        for alias in &flag.param.aliases {
            let form = if short(alias) { "s" } else { "l" };
            write!(declaration, " -{form} {alias}").expect("write to String");
            if !short(alias) {
                write!(used, " {alias}").expect("write to String");
            }
        }
        let dedup = joined(&format!(
            "begin; {used}; or contains -- (commandline -poc)[-1] {}; end",
            flag.forms.join(" ")
        ));
        writeln!(
            output,
            "complete -c {name} -n '{dedup}' {declaration}{} -r -f",
            fish_description(&flag.param.help)
        )
        .expect("write to String");
        used_flags.push(used);
    }
    for (flag, used) in command.flags.iter().zip(used_flags) {
        let condition = joined(&format!(
            r#"not string match -q -- "-*" (commandline -ct); and {used}"#
        ));
        writeln!(
            output,
            "complete -c {name} -n '{condition}' -f -a '--{}'{}",
            flag.name,
            fish_description(&flag.param.help)
        )
        .expect("write to String");
    }
    for sub in &command.subcommands {
        path.push(sub.name);
        fish_context(output, sub, name, path);
        path.pop();
    }
}

const BASH_HELPERS: &str = r#"if ! type _init_completion &>/dev/null; then
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

_!SAFE!--choices()
{
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
        _match="${_match//\`/\\\`}"
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

_!SAFE!--paths()
{
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

  _!SAFE!--choices "${_cur%"$_word"}$_rest" "$_rest" "${_names[@]}"
  for ((_i = 0; _i < ${#COMPREPLY[@]}; _i++)); do
    COMPREPLY[_i]="$_typed${COMPREPLY[_i]}"
  done
}

"#;
