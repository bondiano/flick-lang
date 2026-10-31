package flickrt

import (
	"fmt"
	"strconv"
	"strings"
	"unicode/utf16"
)

// Ported line for line from Effect 4's cli/internal/completions/powershell.ts
// so `--completions powershell` prints the same static script as the
// TypeScript CLI. The script sticks to Windows PowerShell 5.1.

// completeQuotePs is quotePs: non-ASCII and control characters become
// `[char]` UTF-16 code units, since Windows PowerShell 5.1 can misread
// BOM-less UTF-8 as ANSI and turn some bytes into quote delimiters.
func completeQuotePs(s string) string {
	var parts []string
	var literal strings.Builder
	for _, code := range utf16.Encode([]rune(s)) {
		if code >= 0x20 && code < 0x80 {
			if code == '\'' {
				literal.WriteString("''")
			} else {
				literal.WriteByte(byte(code))
			}
			continue
		}
		if literal.Len() > 0 || len(parts) == 0 {
			parts = append(parts, "'"+literal.String()+"'")
		}
		literal.Reset()
		parts = append(parts, fmt.Sprintf("[char]0x%04X", code))
	}
	if literal.Len() > 0 || len(parts) == 0 {
		parts = append(parts, "'"+literal.String()+"'")
	}
	if len(parts) == 1 {
		return parts[0]
	}
	return "(" + strings.Join(parts, " + ") + ")"
}

func completePsArray(items []string) string {
	quoted := make([]string, len(items))
	for i, item := range items {
		quoted[i] = completeQuotePs(item)
	}
	return "@(" + strings.Join(quoted, ", ") + ")"
}

func completePsOptional(s string) string {
	if s == "" {
		return "$null"
	}
	return completeQuotePs(s)
}

func completePsValueFields(t completeType) string {
	var values []string
	if t.tag == "Choice" {
		values = t.values
	}
	pathType := "$null"
	if t.tag == "Path" {
		pathType = completeQuotePs(t.pathType)
	}
	return "values = " + completePsArray(values) + "; pathType = " + pathType
}

func completePsFlagEntry(f completeFlag) string {
	optional := ""
	if f.optionalValue {
		optional = "optionalValue = $true; "
	}
	return "@{ name = " + completeQuotePs(f.name) + "; forms = " + completePsArray(completeFlagForms(f, false)) + "; " +
		"takesValue = $" + strconv.FormatBool(f.typ.tag != "Boolean") + "; " +
		optional + completePsValueFields(f.typ) + "; " +
		"description = " + completePsOptional(f.description) + " }"
}

func completePsArgEntry(a completeArg) string {
	return "@{ name = " + completeQuotePs(a.name) + "; variadic = $" + strconv.FormatBool(a.variadic) + "; " +
		completePsValueFields(a.typ) + "; description = " + completePsOptional(a.description) + " }"
}

func completePsList(lines []string, field string, entries []string) []string {
	if len(entries) == 0 {
		return append(lines, "  "+field+" = @()")
	}
	lines = append(lines, "  "+field+" = @(")
	for _, e := range entries {
		lines = append(lines, "    "+e)
	}
	return append(lines, "  )")
}

// completePsContexts emits one context per subcommand path into an ordinal
// hashtable, since hash literal keys ignore case and subcommands do not.
func completePsContexts(data string, d completeCommand, path []string, lines []string, seen map[string]bool) []string {
	key := strings.Join(path, " ")
	if seen[key] {
		return lines
	}
	seen[key] = true
	lines = append(lines, data+"["+completeQuotePs(key)+"] = @{")
	subs := make([]string, len(d.subcommands))
	for i, sub := range d.subcommands {
		subs[i] = "@{ name = " + completeQuotePs(sub.name) + "; description = " + completePsOptional(sub.description) + " }"
	}
	flags := make([]string, len(d.flags))
	for i, f := range d.flags {
		flags[i] = completePsFlagEntry(f)
	}
	args := make([]string, len(d.args))
	for i, a := range d.args {
		args[i] = completePsArgEntry(a)
	}
	lines = completePsList(lines, "subcommands", subs)
	lines = completePsList(lines, "flags", flags)
	lines = completePsList(lines, "arguments", args)
	lines = append(lines, "}")
	for _, sub := range d.subcommands {
		lines = completePsContexts(data, sub, append(append([]string{}, path...), sub.name), lines, seen)
	}
	return lines
}

// completePowerShell is powershell.ts's generate: a self-contained script
// that never re-invokes the CLI.
func completePowerShell(executable string, d completeCommand) string {
	data := "$global:_" + completeSanitize(executable) + "_Completions"
	lines := []string{
		"###-begin-" + executable + "-completions-###",
		"#",
		"# Static completion script for PowerShell",
		"#",
		"# Installation:",
		// `>>` on Windows PowerShell 5.1 appends UTF-16LE to the profile, so
		// the script goes in its own file.
		"#   " + executable + " --completions powershell > " + executable + "-completion.ps1",
		"#   then add this line to your $PROFILE (use the full path to the file):",
		`#   . <PATH>\` + executable + "-completion.ps1",
		"#",
		"",
		data + " = [hashtable]::new([System.StringComparer]::Ordinal)",
	}
	lines = completePsContexts(data, d, nil, lines, map[string]bool{})
	lines = append(lines,
		"",
		"Register-ArgumentCompleter -Native -CommandName "+completeQuotePs(executable)+" -ScriptBlock {",
		completePowerShellHead+data+completePowerShellTail,
		"}",
		"",
		"###-end-"+executable+"-completions-###")
	return strings.Join(lines, "\n")
}

// completePowerShellHead and completePowerShellTail surround the data
// variable in the completer script block, which replays the committed words
// with the CLI lexer and parser rules.
const completePowerShellHead = `  param($wordToComplete, $commandAst, $cursorPosition)

  $spec = `

const completePowerShellTail = `
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
      if (-not $flag.takesValue) {
        $tooltip = $null
        if ($flag.description) { $tooltip = 'Disable ' + $flag.name }
        & $add ('--no-' + $flag.name) 'ParameterName' $tooltip $typed ''
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
        if ($null -eq $flag) {
          if ($form.StartsWith('--no-')) {
            $name = $form.Substring(5)
            $flag = $lookup['--' + $name]
            if ($null -ne $flag -and -not $flag.takesValue -and $flag.name -ceq $name) { $used[$name] = $true }
          }
          continue
        }
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
  return $results`
