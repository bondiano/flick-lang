//! Static shell completions for the text flags and arguments represented by
//! the VM CLI. Script templates follow the pinned Effect 4.1.2 CLI generators.

/*
MIT License

Copyright (c) 2023 Effectful Technologies Inc

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
*/

use std::fmt::Write as _;

use super::completions_posix::{bash, fish, zsh};
use super::completions_ps::powershell;
use super::{CliParam, CliSpec};

pub(super) fn render(spec: &CliSpec, shell: &str) -> Result<String, String> {
    let command = Command::from_spec(spec, "", &[])?;
    Ok(match shell {
        "bash" | "sh" => bash(&command),
        "zsh" => zsh(&command),
        "fish" => fish(&command),
        "powershell" | "pwsh" => powershell(&command),
        _ => return Err(format!("Unsupported completion shell {shell:?}")),
    })
}

pub(super) struct Command<'a> {
    pub(super) name: &'a str,
    pub(super) description: &'a str,
    pub(super) flags: Vec<Flag<'a>>,
    pub(super) arguments: Vec<&'a CliParam>,
    pub(super) subcommands: Vec<Self>,
}

pub(super) struct Flag<'a> {
    pub(super) name: String,
    pub(super) forms: Vec<String>,
    pub(super) param: &'a CliParam,
}

impl<'a> Command<'a> {
    fn from_spec(
        spec: &'a CliSpec,
        help: &'a str,
        inherited: &[&'a CliParam],
    ) -> Result<Self, String> {
        check_name(&spec.name)?;
        let mut arguments = Vec::new();
        let mut local = Vec::new();
        for param in &spec.params {
            check_name(&param.label)?;
            for alias in &param.aliases {
                check_name(alias)?;
            }
            if param.flag {
                local.push(param);
            } else {
                arguments.push(param);
            }
        }

        // Parent flags are shared with descendants. A local flag takes priority
        // over all inherited flags with that same normalized name.
        let mut flags: Vec<_> = local.iter().copied().map(Flag::new).collect();
        for param in inherited {
            let flag = Flag::new(param);
            if !flags.iter().any(|entry| entry.name == flag.name) {
                flags.push(flag);
            }
        }
        let shared: Vec<_> = inherited.iter().copied().chain(local).collect();
        let subcommands = spec
            .subcommands
            .iter()
            .map(|(child, help)| Self::from_spec(child, help, &shared))
            .collect::<Result<_, _>>()?;
        let description = if help.is_empty() {
            &spec.description
        } else {
            help
        };
        Ok(Self {
            name: &spec.name,
            description,
            flags,
            arguments,
            subcommands,
        })
    }
}

impl<'a> Flag<'a> {
    fn new(param: &'a CliParam) -> Self {
        let name = param.label.replace('_', "-");
        let mut forms = vec![format!("--{name}")];
        forms.extend(param.aliases.iter().map(|alias| {
            let prefix = if short(alias) { "-" } else { "--" };
            format!("{prefix}{alias}")
        }));
        Self { name, forms, param }
    }
}

fn check_name(name: &str) -> Result<(), String> {
    if !name.is_empty()
        && !name.starts_with('-')
        && name.chars().all(|ch| {
            ch.is_alphanumeric() || matches!(ch, '-' | '_' | '.' | '/' | ':' | '+' | '@' | '%')
        })
    {
        Ok(())
    } else {
        Err(format!("Unsupported completion name {name:?}"))
    }
}

pub(super) fn short(alias: &str) -> bool {
    alias.encode_utf16().count() == 1
}

pub(super) fn sanitize(name: &str) -> String {
    name.encode_utf16()
        .map(|unit| {
            let ch = char::from_u32(u32::from(unit)).unwrap_or('_');
            if ch.is_ascii_alphanumeric() { ch } else { '_' }
        })
        .collect()
}

pub(super) fn header(output: &mut String, name: &str, shell: &str, installation: &str) {
    writeln!(
        output,
        "###-begin-{name}-completions-###\n#\n# Static completion script for {shell}\n#\n# Installation:\n{installation}\n#\n"
    )
    .expect("write to String");
}

pub(super) fn footer(output: &mut String, name: &str) {
    writeln!(output, "###-end-{name}-completions-###").expect("write to String");
}

#[cfg(test)]
mod tests {
    use super::super::completions_ps::ps_quote;
    use super::super::{CliMultiplicity, CliParam, CliSpec};
    use super::*;
    use crate::tier0::Value;

    fn command(name: &str) -> CliSpec {
        CliSpec {
            name: name.into(),
            description: String::new(),
            params: vec![],
            subcommands: vec![],
            handler: None,
        }
    }

    fn flag(label: &str, alias: &str) -> CliParam {
        CliParam {
            flag: true,
            label: label.into(),
            aliases: vec![alias.into()],
            help: String::new(),
            multi: CliMultiplicity::Optional,
            dictionary: Value::Unit,
        }
    }

    #[test]
    fn shell_aliases_emit_identical_scripts() {
        let spec = command("tool");
        for (alias, canonical) in [("sh", "bash"), ("pwsh", "powershell")] {
            assert_eq!(
                render(&spec, alias).expect("alias completion renders"),
                render(&spec, canonical).expect("canonical completion renders")
            );
        }
    }

    #[test]
    fn shared_flags_reach_descendants_and_local_flags_shadow_them() {
        let mut root = command("tool");
        let mut middle = command("middle");
        let mut leaf = command("leaf");
        root.params.push(flag("config_file", "c"));
        middle.params.push(flag("region", "r"));
        leaf.params.push(flag("config_file", "f"));
        middle.subcommands.push((leaf, "Leaf summary".into()));
        root.subcommands.push((middle, "Middle summary".into()));

        let root = Command::from_spec(&root, "", &[]).expect("command tree builds");
        let [middle] = root.subcommands.as_slice() else {
            panic!("middle command")
        };
        let [leaf] = middle.subcommands.as_slice() else {
            panic!("leaf command")
        };
        assert_eq!(root.flags[0].forms, ["--config-file", "-c"]);
        assert_eq!(middle.flags[0].forms, ["--region", "-r"]);
        assert_eq!(middle.flags[1].forms, ["--config-file", "-c"]);
        assert_eq!(leaf.flags[0].forms, ["--config-file", "-f"]);
        assert_eq!(leaf.flags[1].forms, ["--region", "-r"]);
        assert_eq!(leaf.description, "Leaf summary");
    }

    #[test]
    fn unsafe_unquoted_names_are_rejected_and_powershell_quotes_utf16() {
        assert!(render(&command("with a space"), "bash").is_err());
        assert_eq!(
            ps_quote("it’s '🙂'"),
            "('it' + [char]0x2019 + 's ''' + [char]0xD83D + [char]0xDE42 + '''')",
        );
    }
}
