//! PowerShell static shell completion generator matching Effect 4.1.2.

use std::collections::BTreeSet;
use std::fmt::Write as _;

use super::CliMultiplicity;
use super::completions::{Command, footer, header, sanitize};

pub(super) fn powershell(command: &Command<'_>) -> String {
    let name = command.name;
    let safe = sanitize(name);
    let data = format!("$global:_{safe}_Completions");
    let mut output = String::new();
    header(
        &mut output,
        name,
        "PowerShell",
        &format!(
            r"#   {name} --completions powershell > {name}-completion.ps1
#   then add this line to your $PROFILE (use the full path to the file):
#   . <PATH>\{name}-completion.ps1"
        ),
    );
    writeln!(
        output,
        "{data} = [hashtable]::new([System.StringComparer]::Ordinal)"
    )
    .expect("write to String");
    ps_context(&mut output, command, &data, "", &mut BTreeSet::new());
    writeln!(
        output,
        "\nRegister-ArgumentCompleter -Native -CommandName {} -ScriptBlock {{",
        ps_quote(name)
    )
    .expect("write to String");
    output.push_str(&POWERSHELL_COMPLETER.replace("!SAFE!", &safe));
    output.push_str("\n}\n\n");
    footer(&mut output, name);
    output
}

pub(super) fn ps_quote(text: &str) -> String {
    let mut parts = Vec::new();
    let mut literal = String::new();
    for unit in text.encode_utf16() {
        if (0x20..0x80).contains(&unit) {
            let byte = u8::try_from(unit).expect("ASCII unit in 0x20..0x80");
            let ch = char::from(byte);
            literal.push(ch);
            if ch == '\'' {
                literal.push(ch);
            }
        } else {
            if !literal.is_empty() || parts.is_empty() {
                parts.push(format!("'{literal}'"));
            }
            literal.clear();
            parts.push(format!("[char]0x{unit:04X}"));
        }
    }
    if !literal.is_empty() || parts.is_empty() {
        parts.push(format!("'{literal}'"));
    }
    if parts.len() == 1 {
        parts.pop().expect("one part")
    } else {
        format!("({})", parts.join(" + "))
    }
}

fn ps_description(text: &str) -> String {
    if text.is_empty() {
        "$null".to_owned()
    } else {
        ps_quote(text)
    }
}

fn ps_list(output: &mut String, label: &str, entries: impl Iterator<Item = String>) {
    let mut entries = entries.peekable();
    if entries.peek().is_none() {
        writeln!(output, "  {label} = @()").expect("write to String");
    } else {
        writeln!(output, "  {label} = @(").expect("write to String");
        for entry in entries {
            writeln!(output, "    {entry}").expect("write to String");
        }
        output.push_str("  )\n");
    }
}

fn ps_context(
    output: &mut String,
    command: &Command<'_>,
    data: &str,
    path: &str,
    seen: &mut BTreeSet<String>,
) {
    if !seen.insert(path.to_owned()) {
        return;
    }
    writeln!(output, "{data}[{}] = @{{", ps_quote(path)).expect("write to String");
    ps_list(
        output,
        "subcommands",
        command.subcommands.iter().map(|sub| {
            format!(
                "@{{ name = {}; description = {} }}",
                ps_quote(sub.name),
                ps_description(sub.description)
            )
        }),
    );
    ps_list(
        output,
        "flags",
        command.flags.iter().map(|flag| {
            let forms = flag
                .forms
                .iter()
                .map(|form| ps_quote(form))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "@{{ name = {}; forms = @({forms}); takesValue = $true; values = @(); pathType = $null; description = {} }}",
                ps_quote(&flag.name),
                ps_description(&flag.param.help)
            )
        }),
    );
    ps_list(
        output,
        "arguments",
        command.arguments.iter().map(|param| {
            let multi = matches!(param.multi, CliMultiplicity::Many);
            format!(
                "@{{ name = {}; variadic = ${multi}; values = @(); pathType = $null; description = {} }}",
                ps_quote(&param.label.replace('_', "-")),
                ps_description(&param.help)
            )
        }),
    );
    output.push_str("}\n");
    for sub in &command.subcommands {
        let path = if path.is_empty() {
            sub.name.to_owned()
        } else {
            format!("{path} {}", sub.name)
        };
        ps_context(output, sub, data, &path, seen);
    }
}

const POWERSHELL_COMPLETER: &str = r#"  param($wordToComplete, $commandAst, $cursorPosition)

  $spec = $global:_!SAFE!_Completions
  $results = [System.Collections.Generic.List[System.Management.Automation.CompletionResult]]::new()
  $booleanLiterals = @('true', 'yes', 'on', '1', 'y', 'false', 'no', 'off', '0', 'n')

  $index = {
    param($context)
    $lookup = [hashtable]::new([System.StringComparer]::Ordinal)
    foreach ($flag in $context.flags) {
      foreach ($form in $flag.forms) { $lookup[$form] = $flag }
    }
    $lookup
  }

  $add = {
    param([string]$value, [string]$type, $tooltip, [string]$typed, [string]$textPrefix)
    if (-not $value.StartsWith($typed, [System.StringComparison]::OrdinalIgnoreCase)) { return }
    $text = $value
    if ($value -notmatch '^[\w./:%+=\\-]+$') { $text = "'" + ($value -replace "['\u2018-\u201b]", '$0$0') + "'" }
    $text = $textPrefix + $text
    $label = $value
    if (-not $label) { $label = $text }
    if (-not $tooltip) { $tooltip = $label }
    $results.Add([System.Management.Automation.CompletionResult]::new($text, $label, $type, $tooltip))
  }

  $addValues = {
    param($entry, [string]$typed, [string]$textPrefix)
    foreach ($value in $entry.values) {
      if ($textPrefix -eq '' -and (& $optionLike $value)) { continue }
      & $add $value 'ParameterValue' $entry.description $typed $textPrefix
    }
    if ($entry.pathType) {
      foreach ($file in [System.Management.Automation.CompletionCompleters]::CompleteFilename($typed)) {
        if ($entry.pathType -eq 'directory' -and $file.ResultType -ne 'ProviderContainer') { continue }
        $results.Add([System.Management.Automation.CompletionResult]::new($textPrefix + $file.CompletionText, $file.ListItemText, $file.ResultType, $file.ToolTip))
      }
    }
  }

  $addFlags = {
    param([string]$typed)
    foreach ($flag in $context.flags) {
      if ($used.ContainsKey($flag.name)) { continue }
      foreach ($form in $flag.forms) {
        & $add $form 'ParameterName' $flag.description $typed ''
      }
      if ($flag.negation) {
        $tooltip = $null
        if ($flag.description) { $tooltip = 'Disable ' + $flag.name }
        & $add $flag.negation 'ParameterName' $tooltip $typed ''
      }
    }
  }

  # As in the CLI lexer, nothing after '--' is an option, and negative numbers
  # and a lone '-' are values.
  $optionLike = {
    param([string]$word)
    -not $endOfOptions -and $word -match '^-.' -and $word -notmatch '^-(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$'
  }

  # A typed value that has no candidates completes to itself, so the engine
  # does not fall back to listing files.
  $keepTyped = {
    if ($results.Count -eq 0 -and $current) {
      $results.Add([System.Management.Automation.CompletionResult]::new($current, $current, 'ParameterValue', $current))
    }
  }

  $path = ''
  $context = $spec[$path]
  $lookup = & $index $context
  $used = [hashtable]::new([System.StringComparer]::Ordinal)
  $pending = $null
  $endOfOptions = $false
  $position = 0

  # The engine strips the quotes from a quoted word.
  $typed = [string]$wordToComplete
  if ($typed -match '^([''"])(.*?)\1?$') { $typed = $Matches[2] }
  $current = $null

  $elements = $commandAst.CommandElements
  for ($i = 1; $i -lt $elements.Count; $i++) {
    $element = $elements[$i]
    $word = $element.Extent.Text
    if ($element -is [System.Management.Automation.Language.StringConstantExpressionAst]) { $word = $element.Value }
    if ($element.Extent.EndOffset -ge $cursorPosition) {
      if ($element -is [System.Management.Automation.Language.ConstantExpressionAst] -and $element.Extent.StartOffset -le $cursorPosition) {
        $current = $element.Extent.Text
        if ($element.Extent.EndOffset -eq $cursorPosition) { $typed = $word }
      }
      break
    }

    # A flag takes the next word as its value unless it is an option or '--';
    # a boolean flag only takes a boolean literal.
    $isOption = & $optionLike $word
    if ($null -ne $pending) {
      $flag = $pending
      $pending = $null
      if ($flag.takesValue) {
        if (-not $isOption) { continue }
      } elseif ($booleanLiterals -ccontains $word) { continue }
    }
    if ($endOfOptions) { $position++; continue }
    if ($word -ceq '--') { $endOfOptions = $true; continue }

    if ($isOption) {
      $hasValue = $word.Contains('=')
      if ($hasValue) {
        $forms = @($word.Substring(0, $word.IndexOf('=')))
      } elseif ($word.StartsWith('--')) {
        $forms = @($word)
      } else {
        $forms = @(foreach ($char in $word.Substring(1).ToCharArray()) { '-' + $char })
      }
      foreach ($form in $forms) {
        $pending = $null
        $flag = $lookup[$form]
        # As in the CLI parser, a negation wins over an alias but not over a
        # canonical name.
        if ($null -eq $flag -or $form -cne ('--' + $flag.name)) {
          $negated = $null
          foreach ($candidate in $context.flags) {
            if ($candidate.negation -ceq $form) { $negated = $candidate }
          }
          if ($null -ne $negated) {
            $used[$negated.name] = $true
            continue
          }
        }
        if ($null -eq $flag) { continue }
        $used[$flag.name] = $true
        # A flag with an optional value only takes a value attached with '='.
        if (-not $hasValue -and -not $flag.optionalValue) { $pending = $flag }
      }
      continue
    }

    if ($position -eq 0 -and $context.subcommands.name -ccontains $word) {
      if ($path) { $path = $path + ' ' + $word } else { $path = $word }
      $context = $spec[$path]
      $lookup = & $index $context
      $used = [hashtable]::new([System.StringComparer]::Ordinal)
      continue
    }
    $position++
  }

  $typedIsOption = & $optionLike $typed
  if ($null -ne $pending -and $pending.takesValue -and -not $typedIsOption) {
    & $addValues $pending $typed ''
    & $keepTyped
    return $results
  }

  if ($typedIsOption) {
    if ($typed -match '^(-[^=]+)=(.*)$') {
      $form = $Matches[1]
      $flag = $lookup[$form]
      if ($null -ne $flag -and $flag.takesValue) {
        & $addValues $flag $Matches[2] ($form + '=')
        & $keepTyped
      }
    } else {
      & $addFlags $typed
    }
    return $results
  }

  if (-not $endOfOptions -and $position -eq 0) {
    foreach ($sub in $context.subcommands) {
      & $add $sub.name 'Command' $sub.description $typed ''
    }
  }

  $arguments = $context.arguments
  if ($position -lt $arguments.Count) {
    & $addValues $arguments[$position] $typed ''
  } elseif ($arguments.Count -gt 0 -and $arguments[-1].variadic) {
    & $addValues $arguments[-1] $typed ''
  }

  if (-not $endOfOptions) {
    & $addFlags $typed
  }
  return $results"#;
