//! Command-line argument lexing, subcommand dispatch, flag/operand matching,
//! and global built-in flag validation.

use std::collections::VecDeque;
use std::fmt::Write as _;
use std::iter;

use flick_core::opt::json_string;

use crate::stdlib::primitives::parse_float;

use super::help::{param_flag_name, printable_flag, quoted_choices, render_help};
use super::{CliMultiplicity, CliSpec, completions};

pub(super) enum Action {
    Display(String),
    Wizard {
        help: String,
    },
    Invoke {
        path_indices: Vec<usize>,
        values: Vec<Vec<String>>,
        help: String,
    },
}

pub(super) struct ParseError {
    pub(super) message: String,
    pub(super) help: String,
}

const LEVELS: &[&str] = &[
    "all", "trace", "debug", "info", "warn", "warning", "error", "fatal", "none",
];
const SHELLS: &[&str] = &["bash", "zsh", "fish", "powershell", "sh", "pwsh"];
const BOOL_LITERALS: &[&str] = &[
    "true", "yes", "on", "1", "y", "false", "no", "off", "0", "n",
];

#[allow(clippy::too_many_lines)]
pub(super) fn parse(root: &CliSpec, version: &str, argv: &[String]) -> Result<Action, ParseError> {
    let split = argv
        .iter()
        .position(|arg| arg == "--")
        .unwrap_or(argv.len());
    let trailing = argv.get(split + 1..).unwrap_or_default();
    let mut tokens = lex(&argv[..split]);
    let mut spec = root;
    let mut path_indices = Vec::new();
    let mut path = root.name.clone();
    let mut values = empty_params(spec);
    let mut operands = Vec::new();
    let mut scan_errors = Vec::new();
    let mut global = GlobalFlags::default();
    let mut awaiting_subcommand = true;
    while let Some(token) = tokens.pop_front() {
        match token {
            Token::Flag(flag) => {
                if let Some(index) = flag_parameter(spec, flag.name) {
                    let param = &spec.params[index];
                    match flag.value.or_else(|| take_operand(&mut tokens)) {
                        Some(value) => values[index].push(value.to_owned()),
                        None => scan_errors.push(missing_flag_value(&param_flag_name(param))),
                    }
                } else if !global.read(&flag, &mut tokens) {
                    scan_errors.push(unrecognized_flag(spec, &path, &flag));
                }
            }
            Token::Operand(value) => {
                if awaiting_subcommand {
                    if let Some((idx, (subcommand, _))) = spec
                        .subcommands
                        .iter()
                        .enumerate()
                        .find(|(_, (sub, _))| sub.name == value)
                    {
                        spec = subcommand;
                        path_indices.push(idx);
                        write!(path, " {}", spec.name).expect("write to String");
                        values = empty_params(spec);
                        continue;
                    }
                    if !spec.subcommands.is_empty() && spec.params.iter().all(|p| p.flag) {
                        let names = spec.subcommands.iter().map(|(sub, _)| sub.name.as_str());
                        scan_errors.push(format!(
                            "Unknown subcommand {} for {}{}",
                            json_string(value),
                            json_string(&path),
                            suggestions(value, names, str::to_owned),
                        ));
                    }
                    awaiting_subcommand = false;
                }
                operands.push(value);
            }
        }
    }

    let help = render_help(spec, &path);
    let make_err = |message| ParseError {
        message,
        help: help.clone(),
    };
    if !global.errors.is_empty() {
        global.errors.extend(scan_errors);
        return Err(make_err(global.errors.join("\n")));
    }
    for action in [
        GlobalFlag::Help,
        GlobalFlag::Version,
        GlobalFlag::Wizard,
        GlobalFlag::Completion,
    ] {
        if let Some(value) = global.checked(action).map_err(make_err)? {
            return match action {
                GlobalFlag::Help => Ok(Action::Display(help)),
                GlobalFlag::Version => Ok(Action::Display(format!("{} v{version}\n", root.name))),
                GlobalFlag::Completion => completions::render(root, value)
                    .map(Action::Display)
                    .map_err(make_err),
                GlobalFlag::Wizard => Ok(Action::Wizard { help }),
                GlobalFlag::LogLevel | GlobalFlag::Trace => {
                    unreachable!("built-in action order")
                }
            };
        }
    }
    if !scan_errors.is_empty() {
        return Err(make_err(scan_errors.join("\n")));
    }

    operands.extend(trailing.iter().map(String::as_str));
    let mut operands = operands.into_iter();
    let mut errors = Vec::new();
    for (param, selected) in spec.params.iter().zip(&mut values) {
        if param.flag {
            if !matches!(param.multi, CliMultiplicity::Many) && selected.len() > 1 {
                errors.push(format!(
                    "Invalid value for flag --{}: \"{} occurrences\". Expected: at most 1 value",
                    param_flag_name(param),
                    selected.len(),
                ));
            } else if matches!(param.multi, CliMultiplicity::Required) && selected.is_empty() {
                errors.push(format!(
                    "Missing required flag: --{}",
                    param_flag_name(param)
                ));
            }
        } else if matches!(param.multi, CliMultiplicity::Many) {
            selected.extend(operands.by_ref().map(str::to_owned));
        } else if let Some(value) = operands.next() {
            selected.push(value.to_owned());
        } else if matches!(param.multi, CliMultiplicity::Required) {
            errors.push(format!(
                "Missing required argument: {}",
                param_flag_name(param)
            ));
        }
    }
    let leftovers: Vec<_> = operands.map(json_string).collect();
    if !leftovers.is_empty() {
        let suffix = if leftovers.len() == 1 { "" } else { "s" };
        errors.push(format!(
            "Unexpected positional argument{suffix}: {}",
            leftovers.join(", "),
        ));
    }
    if !errors.is_empty() {
        return Err(make_err(errors.join("\n")));
    }
    if spec.handler.is_none() {
        return Err(make_err("Help requested".to_owned()));
    }

    global.checked(GlobalFlag::LogLevel).map_err(make_err)?;
    global.checked(GlobalFlag::Trace).map_err(make_err)?;
    Ok(Action::Invoke {
        path_indices,
        values,
        help,
    })
}

pub(super) fn invalid_value(is_flag: bool, label: &str, value: &str, expected: &str) -> String {
    let expected = if expected.starts_with("Expected ") || expected.starts_with("Expected:") {
        expected.to_owned()
    } else {
        format!("Expected: {expected}")
    };
    if !is_flag {
        format!("Invalid value for argument <{label}>: \"{value}\". {expected}")
    } else if value.is_empty() {
        format!("Missing value for flag --{label}. {expected}")
    } else {
        format!("Invalid value for flag --{label}: \"{value}\". {expected}")
    }
}

fn empty_params(spec: &CliSpec) -> Vec<Vec<String>> {
    vec![Vec::new(); spec.params.len()]
}

fn flag_parameter(spec: &CliSpec, key: &str) -> Option<usize> {
    spec.params.iter().position(|param| {
        param.flag && (param_flag_name(param) == key || param.aliases.iter().any(|a| a == key))
    })
}

#[derive(Clone, Copy)]
struct Flag<'a> {
    name: &'a str,
    short: bool,
    value: Option<&'a str>,
}

enum Token<'a> {
    Flag(Flag<'a>),
    Operand(&'a str),
}

fn lex(argv: &[String]) -> VecDeque<Token<'_>> {
    let mut out = VecDeque::new();
    for arg in argv {
        let (raw, value) = arg
            .split_once('=')
            .map_or((arg.as_str(), None), |(raw, value)| (raw, Some(value)));
        if let Some(name) = raw.strip_prefix("--")
            && !name.is_empty()
        {
            out.push_back(Token::Flag(Flag {
                name,
                value,
                short: false,
            }));
        } else if raw == "-" && value.is_some() {
            out.push_back(Token::Flag(Flag {
                name: "",
                value,
                short: true,
            }));
        } else if let Some(cluster) = raw
            .strip_prefix('-')
            .filter(|raw| !raw.starts_with('-') && !raw.is_empty())
            && !negative_number(arg)
        {
            let mut chars = cluster.char_indices().peekable();
            while let Some((start, ch)) = chars.next() {
                out.push_back(Token::Flag(Flag {
                    name: &cluster[start..start + ch.len_utf8()],
                    value: if chars.peek().is_none() { value } else { None },
                    short: true,
                }));
            }
        } else {
            out.push_back(Token::Operand(arg));
        }
    }
    out
}

fn negative_number(arg: &str) -> bool {
    arg.as_bytes()
        .get(1)
        .is_some_and(|c| c.is_ascii_digit() || *c == b'.')
        && !arg.chars().any(char::is_whitespace)
        && parse_float(arg).is_some_and(f64::is_finite)
}

fn take_operand<'a>(tokens: &mut VecDeque<Token<'a>>) -> Option<&'a str> {
    if let Some(Token::Operand(value)) = tokens.front() {
        let value = *value;
        tokens.pop_front();
        Some(value)
    } else {
        None
    }
}

fn missing_flag_value(name: &str) -> String {
    format!("Missing value for flag --{name}. Expected: string")
}

fn unrecognized_flag(spec: &CliSpec, path: &str, flag: &Flag<'_>) -> String {
    let prefix = if flag.short { "-" } else { "--" };
    let names = spec
        .params
        .iter()
        .filter(|param| param.flag)
        .flat_map(|param| iter::once(param_flag_name(param)).chain(param.aliases.iter().cloned()))
        .collect::<Vec<_>>();
    format!(
        "Unrecognized flag: {prefix}{} in command {path}{}",
        flag.name,
        suggestions(flag.name, names.iter().map(String::as_str), printable_flag),
    )
}

fn suggestions<'a>(
    input: &str,
    candidates: impl IntoIterator<Item = &'a str>,
    display: impl Fn(&str) -> String,
) -> String {
    let mut best = Vec::new();
    let mut distance = 2;
    for candidate in candidates {
        let score = edit_distance(input, candidate);
        if score < distance {
            best.clear();
            distance = score;
        }
        if score == distance {
            best.push(display(candidate));
        }
    }
    if best.is_empty() {
        String::new()
    } else {
        format!("\n\n  Did you mean this?\n    {}", best.join("\n    "))
    }
}

fn edit_distance(left: &str, right: &str) -> usize {
    let right: Vec<_> = right.encode_utf16().collect();
    let mut row: Vec<_> = (0..=right.len()).collect();
    for (i, left) in left.encode_utf16().enumerate() {
        let mut diagonal = row[0];
        row[0] = i + 1;
        for (j, right) in right.iter().enumerate() {
            let old = row[j + 1];
            row[j + 1] = (diagonal + usize::from(left != *right))
                .min(row[j] + 1)
                .min(old + 1);
            diagonal = old;
        }
    }
    row[right.len()]
}

#[derive(Clone, Copy)]
enum GlobalFlag {
    Help,
    Version,
    Wizard,
    Completion,
    LogLevel,
    Trace,
}

impl GlobalFlag {
    const COUNT: usize = Self::Trace as usize + 1;

    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "help" => Self::Help,
            "version" => Self::Version,
            "wizard" => Self::Wizard,
            "completions" => Self::Completion,
            "log-level" => Self::LogLevel,
            "trace" => Self::Trace,
            _ => return None,
        })
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Help => "help",
            Self::Version => "version",
            Self::Wizard => "wizard",
            Self::Completion => "completions",
            Self::LogLevel => "log-level",
            Self::Trace => "trace",
        }
    }

    const fn choices(self) -> Option<&'static [&'static str]> {
        match self {
            Self::Completion => Some(SHELLS),
            Self::LogLevel => Some(LEVELS),
            Self::Help | Self::Version | Self::Wizard | Self::Trace => None,
        }
    }
}

#[derive(Default)]
struct GlobalFlags<'a> {
    values: [Vec<&'a str>; GlobalFlag::COUNT],
    errors: Vec<String>,
}

impl<'a> GlobalFlags<'a> {
    fn read(&mut self, flag: &Flag<'a>, tokens: &mut VecDeque<Token<'a>>) -> bool {
        let positive = if flag.short {
            match flag.name {
                "h" => "help",
                "v" => "version",
                _ => return false,
            }
        } else {
            flag.name
        };
        let (key, negated) = positive
            .strip_prefix("no-")
            .map_or((positive, false), |name| (name, true));
        let Some(kind) = GlobalFlag::from_name(key) else {
            return false;
        };
        if let Some(choices) = kind.choices() {
            if negated {
                return false;
            }
            if let Some(value) = flag.value.or_else(|| take_operand(tokens)) {
                self.values[kind as usize].push(value);
            } else {
                self.errors.push(format!(
                    "Missing value for flag --{key}. Expected: {}",
                    quoted_choices(choices)
                ));
            }
        } else {
            let value = flag.value.or_else(|| {
                if let Some(Token::Operand(value)) = tokens.front()
                    && bool_value(value).is_some()
                {
                    return take_operand(tokens);
                }
                None
            });
            if negated && let Some(value) = value {
                self.errors.push(format!(
                    "Invalid value for flag --{key}: \"{value}\". Expected: omit the value and use --no-{key} by itself to set --{key} to false",
                ));
            } else {
                self.values[kind as usize].push(value.unwrap_or(if negated {
                    "false"
                } else {
                    "true"
                }));
            }
        }
        true
    }

    fn checked(&self, flag: GlobalFlag) -> Result<Option<&'a str>, String> {
        let key = flag.name();
        let value = match self.values[flag as usize].as_slice() {
            [] => return Ok(None),
            [value] => *value,
            values => return Err(duplicate_flag(key, values.len())),
        };
        if let Some(choices) = flag.choices() {
            if !choices.contains(&value) {
                let expected = quoted_choices(choices);
                return Err(if value.is_empty() {
                    format!("Missing value for flag --{key}. Expected: {expected}")
                } else {
                    format!("Invalid value for flag --{key}: \"{value}\". Expected: {expected}")
                });
            }
        } else if bool_value(value).is_none() {
            return Err(format!(
                "Invalid value for flag --{key}: \"{value}\". Expected {}",
                quoted_choices(BOOL_LITERALS),
            ));
        }
        Ok(Some(value))
    }
}

fn duplicate_flag(name: &str, count: usize) -> String {
    format!("Invalid value for flag --{name}: \"{count} occurrences\". Expected: at most 1 value")
}

pub(super) fn bool_value(value: &str) -> Option<bool> {
    BOOL_LITERALS
        .iter()
        .position(|word| word.eq_ignore_ascii_case(value))
        .map(|index| index < 5)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stdlib::cli::CliParam;
    use crate::tier0::Value;

    fn spec() -> CliSpec {
        CliSpec {
            name: "sample".into(),
            description: "Sample application".into(),
            params: Vec::new(),
            subcommands: vec![(
                CliSpec {
                    name: "select".into(),
                    description: "Select inputs".into(),
                    params: vec![
                        CliParam {
                            flag: true,
                            label: "item_count".into(),
                            aliases: vec!["n".into()],
                            help: "Number of items".into(),
                            multi: CliMultiplicity::Required,
                            dictionary: Value::Unit,
                        },
                        CliParam {
                            flag: true,
                            label: "limit".into(),
                            aliases: Vec::new(),
                            help: String::new(),
                            multi: CliMultiplicity::Optional,
                            dictionary: Value::Unit,
                        },
                        CliParam {
                            flag: false,
                            label: "items".into(),
                            aliases: Vec::new(),
                            help: String::new(),
                            multi: CliMultiplicity::Many,
                            dictionary: Value::Unit,
                        },
                    ],
                    subcommands: Vec::new(),
                    handler: Some(Value::Unit),
                },
                String::new(),
            )],
            handler: None,
        }
    }

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|arg| (*arg).to_owned()).collect()
    }

    #[test]
    fn preserves_empty_optional_and_trailing_operands_for_vm_callbacks() {
        let root = spec();
        let args = strings(&["select", "-n", "-1", "one", "--", "--help", "-x"]);
        let Ok(Action::Invoke {
            path_indices,
            values,
            ..
        }) = parse(&root, "0.0.0", &args)
        else {
            panic!("expected command invocation");
        };
        assert_eq!(path_indices, vec![0]);
        assert_eq!(
            values,
            vec![
                strings(&["-1"]),
                Vec::new(),
                strings(&["one", "--help", "-x"]),
            ]
        );
    }

    #[test]
    fn rejects_repeated_flag_values_across_aliases() {
        let root = spec();
        let args = strings(&["select", "-n=1", "--item-count=2"]);
        let Err(error) = parse(&root, "0.0.0", &args) else {
            panic!("expected a duplicate flag error");
        };
        assert_eq!(
            error.message,
            "Invalid value for flag --item-count: \"2 occurrences\". Expected: at most 1 value"
        );
        assert!(error.help.contains("  sample select [flags] [<items...>]"));
    }

    #[test]
    fn help_uses_selected_command_before_validating_settings_and_params() {
        let root = spec();
        let args = strings(&["--help", "select", "--log-level=unknown"]);
        let Ok(Action::Display(help)) = parse(&root, "0.0.0", &args) else {
            panic!("expected help");
        };
        assert!(help.starts_with("DESCRIPTION\n  Select inputs\n"));
        assert!(help.contains("--item-count, -n string"));
    }
}
