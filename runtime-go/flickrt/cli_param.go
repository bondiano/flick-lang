package flickrt

import (
	"strings"
)

// Effect's Param and Primitive (Param.ts, Primitive.ts) for what a Flick
// parameter becomes: one single flag or argument, wrapped by its
// multiplicity. Parsing returns the arguments left and the value, or the
// CliError that stops it.

type cliPrim struct {
	tag          string // "Boolean", "String", "Path", "FileText", "FileParse" or "Choice"
	pathType     string
	mustExist    bool
	format       string
	choiceKeys   []string
	choiceValues []Value
}

// cliSingle is Param.Single.
type cliSingle struct {
	kind          string // "flag" or "argument"
	name          string
	prim          *cliPrim
	typeName      *string
	description   *string
	aliases       []string
	hidden        bool
	optionalValue *string
}

// typeNameOr is `single.typeName ?? Primitive.getTypeName(single.primitiveType)`.
func (s *cliSingle) typeNameOr() string {
	if s.typeName != nil {
		return *s.typeName
	}
	switch s.prim.tag {
	case "Boolean":
		return "boolean"
	case "String":
		return "string"
	case "Path":
		return "path"
	case "Choice":
		return "choice"
	case "FileText", "FileParse":
		return "file"
	}
	return "value"
}

type cliArgs struct {
	flags     cliFlagMap
	arguments []string
}

type cliParseFn func(fb *Fiber, a cliArgs) ([]string, Value, *cliError)

// cliParam is a Param: its single, getParamMetadata, and its parser.
type cliParam struct {
	single   *cliSingle
	optional bool
	variadic bool
	min, max int // of a variadic, -1 when absent
	parse    cliParseFn
}

func (p *cliPrim) parse(fb *Fiber, text string) (Value, string) {
	switch p.tag {
	case "Boolean":
		// Effect's message of the issue, which names no input
		return decodeBoolean(text)
	case "Choice":
		for i, k := range p.choiceKeys {
			if k == text {
				return p.choiceValues[i], ""
			}
		}
		quoted := make([]string, len(p.choiceKeys))
		for i, k := range p.choiceKeys {
			quoted[i] = QuoteJSON(k)
		}
		return nil, strings.Join(quoted, " | ")
	case "Path":
		return cliPathValue(p.pathType, p.mustExist, text)
	case "FileText":
		return cliFileText(text)
	case "FileParse":
		return cliFileParse(p.format, text)
	}
	return text, ""
}

// cliSingleParse is parseFlag / parsePositional.
func cliSingleParse(s *cliSingle) cliParseFn {
	return func(fb *Fiber, a cliArgs) ([]string, Value, *cliError) {
		var text string
		left := a.arguments
		if s.kind == "flag" {
			values := a.flags[s.name]
			if len(values) == 0 {
				return nil, nil, cliMissingOption(s.name)
			}
			if len(values) > 1 {
				return nil, nil, cliInvalid(s.name, itoa(len(values))+" occurrences", "at most 1 value", "flag")
			}
			text = values[0]
		} else {
			if len(a.arguments) == 0 {
				return nil, nil, cliMissingArgument(s.name)
			}
			text, left = a.arguments[0], a.arguments[1:]
		}
		v, issue := s.prim.parse(fb, text)
		if issue != "" {
			return nil, nil, cliInvalid(s.name, text, issue, s.kind)
		}
		return left, v, nil
	}
}

func cliOptionalParse(inner cliParseFn) cliParseFn {
	return func(fb *Fiber, a cliArgs) ([]string, Value, *cliError) {
		left, v, err := inner(fb, a)
		switch {
		case err == nil:
			return left, Some(v), nil
		case err.missing():
			return a.arguments, None, nil
		}
		return nil, nil, err
	}
}

func cliPlural(n int) string {
	if n == 1 {
		return ""
	}
	return "s"
}

// cliVariadicParse is parsePositionalVariadic / parseOptionVariadic.
func cliVariadicParse(s *cliSingle, inner cliParseFn, min, max int) cliParseFn {
	return func(fb *Fiber, a cliArgs) ([]string, Value, *cliError) {
		out := []Value{}
		if s.kind == "argument" {
			current := a.arguments
			for len(current) > 0 && (max < 0 || len(out) < max) {
				left, v, err := inner(fb, cliArgs{a.flags, current})
				if err != nil {
					return nil, nil, err
				}
				out, current = append(out, v), left
			}
			if len(out) < min {
				if len(out) == 0 {
					return nil, nil, cliMissingArgument(s.name)
				}
				return nil, nil, cliInvalid(s.name, itoa(len(out))+" values", "at least "+itoa(min)+" value"+cliPlural(min), s.kind)
			}
			return current, out, nil
		}
		values := []string{}
		for _, name := range append([]string{s.name}, s.aliases...) {
			values = append(values, a.flags[name]...)
		}
		count := len(values)
		if min >= 0 && count < min {
			if count == 0 {
				return nil, nil, cliMissingOption(s.name)
			}
			return nil, nil, cliInvalid(s.name, itoa(count)+" occurrences", "at least "+itoa(min)+" value"+cliPlural(min), s.kind)
		}
		if max >= 0 && count > max {
			return nil, nil, cliInvalid(s.name, itoa(count)+" occurrences", "at most "+itoa(max)+" value"+cliPlural(max), s.kind)
		}
		for _, text := range values {
			_, v, err := inner(fb, cliArgs{cliFlagMap{s.name: {text}}, nil})
			if err != nil {
				return nil, nil, err
			}
			out = append(out, v)
		}
		return a.arguments, out, nil
	}
}

// cliFallbackParse runs `fallback` for a missing value, as
// Param.withFallbackConfig / withFallbackPrompt do.
func cliFallbackParse(inner cliParseFn, fallback func(fb *Fiber, err *cliError) (Value, *cliError)) cliParseFn {
	return func(fb *Fiber, a cliArgs) ([]string, Value, *cliError) {
		left, v, err := inner(fb, a)
		if err == nil || !err.missing() {
			return left, v, err
		}
		v, err = fallback(fb, err)
		if err != nil {
			return nil, nil, err
		}
		return a.arguments, v, nil
	}
}

func cliMissingName(err *cliError) (string, string) {
	if err.tag == "MissingOption" {
		return err.option, "flag"
	}
	return err.argument, "argument"
}

// cliConfigFallback reads `Config.option(config)`: absent keeps the missing
// error, another config error is an invalid value.
func cliConfigFallback(config Value) func(*Fiber, *cliError) (Value, *cliError) {
	return func(fb *Fiber, missingErr *cliError) (Value, *cliError) {
		var v, missing Value
		_, cause := catch(func() Value { v, missing = cfg(config).eval(fb, currentProvider(fb), nil); return Unit })
		switch {
		case cause != nil && cause.kind == causeFail:
			name, kind := cliMissingName(missingErr)
			return nil, cliInvalid(name, "config", cause.err.(*Data).Fields[0].(string), kind)
		case cause != nil:
			panic(&Failure{cause})
		case missing != nil:
			return nil, missingErr
		}
		return v, nil
	}
}

func cliPromptFallback(prompt Value) func(*Fiber, *cliError) (Value, *cliError) {
	return func(fb *Fiber, missingErr *cliError) (Value, *cliError) {
		// skipped when the terminal cannot prompt (Effect 4.0.7)
		if !cliTerminal().canPrompt {
			return nil, missingErr
		}
		if v, ok := promptFallback(fb, prompt); ok {
			return v, nil
		}
		return nil, missingErr
	}
}

func cliPrimOf(source cliSource) *cliPrim {
	switch source.kind {
	case "path":
		return &cliPrim{tag: "Path", pathType: source.pathType, mustExist: source.mustExist}
	case "fileText":
		return &cliPrim{tag: "FileText"}
	case "fileParse":
		return &cliPrim{tag: "FileParse", format: source.format}
	}
	return &cliPrim{tag: "String"}
}

var cliBoolean = &cliPrim{tag: "Boolean"}

// cliParamOf is run.ts's `param` and `flagParam`: Effect's parameter of a
// Flick one, its multiplicity and modifiers applied.
func cliParamOf(kind string, p cliSpecParam) *cliParam {
	m := p.multi
	s := &cliSingle{kind: kind, name: strings.ReplaceAll(p.label, "_", "-"), prim: cliBoolean, hidden: m.mods.hidden, typeName: m.mods.metavar}
	if m.kind != "switch" {
		s.prim = cliPrimOf(m.mods.source)
	}
	for _, a := range p.aliases {
		s.aliases = append(s.aliases, strings.TrimLeft(a, "-"))
	}
	if p.help != "" {
		help := p.help
		s.description = &help
	}
	if kind == "flag" {
		s.optionalValue = m.mods.optionalValue
	}
	parse := cliSingleParse(s)
	if m.kind != "switch" {
		base, source, dict := parse, m.mods.source.kind, p.value
		parse = func(fb *Fiber, a cliArgs) ([]string, Value, *cliError) {
			left, v, err := base(fb, a)
			if err != nil {
				return nil, nil, err
			}
			text, ok := v.(string)
			if source == "fileParse" || !ok {
				text = JsonStringify(v).(string)
			}
			parsed := Run(fb, Apply(fb, Method(fb, dict, "parse"), text))
			if !isOk(parsed) {
				return nil, nil, cliInvalid(p.label, text, value(parsed).(string), kind)
			}
			return left, value(parsed), nil
		}
	}
	out := &cliParam{single: s, min: -1, max: -1}
	switch m.kind {
	case "switch", "default":
		fallback := m.value
		if m.kind == "switch" {
			fallback = false
		}
		opt := cliOptionalParse(parse)
		out.optional, out.parse = true, func(fb *Fiber, a cliArgs) ([]string, Value, *cliError) {
			left, v, err := opt(fb, a)
			if err != nil {
				return nil, nil, err
			}
			if x, ok := some(v); ok {
				return left, x, nil
			}
			return left, fallback, nil
		}
	case "optional":
		out.optional, out.parse = true, cliOptionalParse(parse)
	case "variadic", "some":
		min, max := m.min, m.max
		if m.kind == "some" {
			min = 1
		}
		out.variadic, out.min, out.max, out.parse = true, min, max, cliVariadicParse(s, parse, min, max)
	case "config":
		out.parse = cliFallbackParse(parse, cliConfigFallback(m.value))
	case "prompt":
		out.parse = cliFallbackParse(parse, cliPromptFallback(m.value))
	default:
		out.parse = parse
	}
	return out
}

func itoa(n int) string { return ShowNumber(float64(n)) }
