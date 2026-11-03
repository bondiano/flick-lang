package flickrt

import "regexp"

// The command line as Effect's CLI reads it: internal/lexer.ts and
// internal/parser.ts. Tokens are routed to the commands they name and to
// their flags; values are parsed later, by the parameters.

type cliToken struct {
	kind  string // "long", "short" or "value"
	name  string // long: the name; short: the flag; value: the text
	raw   string
	value *string // an inline `=value`
}

func (t cliToken) flag() bool { return t.kind != "value" }

var cliNegativeNumber = regexp.MustCompile(`^-(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?$`)

// cliLex is lexer.ts's `lex`: the tokens before `--`, and what follows it.
func cliLex(argv []string) ([]cliToken, []string) {
	var trailing []string
	for i, a := range argv {
		if a == "--" {
			argv, trailing = argv[:i], argv[i+1:]
			break
		}
	}
	tokens := []cliToken{}
	for _, arg := range argv {
		switch {
		case len(arg) == 0 || arg[0] != '-' || cliNegativeNumber.MatchString(arg) || arg == "-":
			tokens = append(tokens, cliToken{kind: "value", name: arg})
		case len(arg) > 1 && arg[1] == '-':
			if i := indexByte(arg, '='); i == 2 {
				tokens = append(tokens, cliToken{kind: "value", name: arg}) // `--=x` names no flag
			} else if i >= 0 {
				v := arg[i+1:]
				tokens = append(tokens, cliToken{kind: "long", name: arg[2:i], raw: arg, value: &v})
			} else {
				tokens = append(tokens, cliToken{kind: "long", name: arg[2:], raw: arg})
			}
		default:
			flags := arg[1:]
			if i := indexByte(flags, '='); i >= 0 {
				// `-ab=x` is `-a -b=x`
				chars, v := utf16Chars(flags[:i]), flags[i+1:]
				last := ""
				if len(chars) > 0 {
					chars, last = chars[:len(chars)-1], chars[len(chars)-1]
				}
				for _, ch := range chars {
					tokens = append(tokens, cliToken{kind: "short", name: ch, raw: "-" + ch})
				}
				tokens = append(tokens, cliToken{kind: "short", name: last, raw: "-" + last, value: &v})
				continue
			}
			for _, ch := range utf16Chars(flags) {
				tokens = append(tokens, cliToken{kind: "short", name: ch, raw: "-" + ch})
			}
		}
	}
	return tokens, trailing
}

func indexByte(s string, c byte) int {
	for i := 0; i < len(s); i++ {
		if s[i] == c {
			return i
		}
	}
	return -1
}

// utf16Chars splits as JS's `for (const ch of s)`: by code point.
func utf16Chars(s string) []string {
	out := []string{}
	for _, r := range s {
		out = append(out, string(r))
	}
	return out
}

type cliRegistry struct {
	params []*cliSingle
	index  map[string]*cliSingle
}

func cliNewRegistry(params []*cliSingle) *cliRegistry {
	r := &cliRegistry{params, map[string]*cliSingle{}}
	for _, p := range params {
		if _, ok := r.index[p.name]; ok {
			panic(Defect{`Duplicate flag name "` + p.name + `" in command definition`})
		}
		r.index[p.name] = p
		for _, a := range p.aliases {
			if q, ok := r.index[a]; ok {
				panic(Defect{`Duplicate flag/alias "` + a + `" in command definition (conflicts with "` + q.name + `")`})
			}
			r.index[a] = p
		}
	}
	return r
}

type cliFlagMap map[string][]string

func cliEmptyFlags(params []*cliSingle) cliFlagMap {
	m := cliFlagMap{}
	for _, p := range params {
		m[p.name] = []string{}
	}
	return m
}

type cliResolved struct {
	param   *cliSingle
	negated bool
}

func cliResolve(t cliToken, r *cliRegistry) *cliResolved {
	direct := r.index[t.name]
	if direct != nil && direct.name == t.name {
		return &cliResolved{direct, false}
	}
	if t.kind == "long" && len(t.name) > 3 && t.name[:3] == "no-" {
		canonical := t.name[3:]
		if p := r.index[canonical]; p != nil && p.name == canonical && p.prim.tag == "Boolean" {
			return &cliResolved{p, true}
		}
	}
	if direct != nil {
		return &cliResolved{direct, false}
	}
	return nil
}

func cliResolveFrom(t cliToken, registries []*cliRegistry) *cliResolved {
	for _, r := range registries {
		if found := cliResolve(t, r); found != nil {
			return found
		}
	}
	return nil
}

type cliCursor struct {
	tokens []cliToken
	i      int
}

func (c *cliCursor) peek() *cliToken {
	if c.i < len(c.tokens) {
		return &c.tokens[c.i]
	}
	return nil
}

func (c *cliCursor) take() *cliToken {
	t := c.peek()
	if t != nil {
		c.i++
	}
	return t
}

func (c *cliCursor) rest() []cliToken { return c.tokens[c.i:] }

func cliBooleanLiteral(t *cliToken) *string {
	if t != nil && t.kind == "value" {
		for _, l := range []string{"true", "yes", "on", "1", "y", "false", "no", "off", "0", "n"} {
			if t.name == l {
				return &t.name
			}
		}
	}
	return nil
}

// cliConsumeValue is consumeFlagValueWithTokens: the value of a flag token,
// or an error, with the tokens it took.
func cliConsumeValue(c *cliCursor, t cliToken, spec *cliSingle, negated bool) (*string, *cliError, []cliToken) {
	taken := func() []cliToken { return []cliToken{*c.take()} }
	if negated {
		invalid := func(v string) *cliError {
			return cliInvalid(spec.name, v, "omit the value and use "+t.raw+" by itself to set --"+spec.name+" to false", "flag")
		}
		if t.value != nil {
			return nil, invalid(*t.value), nil
		}
		if spec.optionalValue == nil {
			if l := cliBooleanLiteral(c.peek()); l != nil {
				v := *l
				return nil, invalid(v), taken()
			}
		}
		f := "false"
		return &f, nil, nil
	}
	if t.value != nil {
		return t.value, nil, nil
	}
	if spec.optionalValue != nil {
		return spec.optionalValue, nil, nil
	}
	if spec.prim.tag == "Boolean" {
		if l := cliBooleanLiteral(c.peek()); l != nil {
			v := *l
			return &v, nil, taken()
		}
		v := "true"
		return &v, nil, nil
	}
	if next := c.peek(); next != nil && next.kind == "value" {
		v := next.name
		return &v, nil, taken()
	}
	expected := spec.typeNameOr()
	if keys := spec.prim.choiceKeys; keys != nil {
		expected = joinStrings(keys, " | ")
	}
	return nil, cliInvalid(spec.name, "", expected, "flag"), nil
}

func cliConsumeKnown(tokens []cliToken, r *cliRegistry) (cliFlagMap, []cliToken, []*cliError) {
	flags, remainder, errors := cliEmptyFlags(r.params), []cliToken{}, []*cliError{}
	c := &cliCursor{tokens: tokens}
	for t := c.take(); t != nil; t = c.take() {
		if !t.flag() {
			remainder = append(remainder, *t)
			continue
		}
		found := cliResolve(*t, r)
		if found == nil {
			remainder = append(remainder, *t)
			continue
		}
		v, err, _ := cliConsumeValue(c, *t, found.param, found.negated)
		if err != nil {
			errors = append(errors, err)
		} else if v != nil {
			flags[found.param.name] = append(flags[found.param.name], *v)
		}
	}
	return flags, remainder, errors
}

func cliSubIndex(cmd *cliCommand) map[string]*cliCommand {
	index := map[string]*cliCommand{}
	set := func(key string, sub *cliCommand) {
		if existing, ok := index[key]; ok && existing != sub {
			panic(Defect{`Duplicate subcommand name/alias "` + key + `" in command definition (conflicts with "` + existing.name + `")`})
		}
		index[key] = sub
	}
	for _, sub := range cmd.subcommands {
		set(sub.name, sub)
		if sub.alias != "" && sub.alias != sub.name {
			set(sub.alias, sub)
		}
	}
	return index
}

func cliPreserve(remainder []cliToken, c *cliCursor, t cliToken, found *cliResolved) []cliToken {
	remainder = append(remainder, t)
	_, _, taken := cliConsumeValue(c, t, found.param, found.negated)
	return append(remainder, taken...)
}

func cliLocalPrecedesSubcommand(t cliToken, rest []cliToken, found *cliResolved, subs map[string]*cliCommand, registries []*cliRegistry) bool {
	c := &cliCursor{tokens: rest}
	cliConsumeValue(c, t, found.param, found.negated)
	for t := c.take(); t != nil; t = c.take() {
		if t.flag() {
			if known := cliResolveFrom(*t, registries); known != nil {
				cliConsumeValue(c, *t, known.param, known.negated)
			}
			continue
		}
		_, ok := subs[t.name]
		return ok
	}
	return false
}

// cliConsumeGlobals is parser.ts's consumeGlobalFlags: the values of the
// global flags, wherever they are, and the tokens left for the commands.
func cliConsumeGlobals(tokens []cliToken, root *cliCommand, global *cliRegistry) (cliFlagMap, []cliToken, []*cliError) {
	flags, errors := cliEmptyFlags(global.params), []*cliError{}
	var level func(tokens []cliToken, cmd *cliCommand, ignored []*cliRegistry) []cliToken
	level = func(tokens []cliToken, cmd *cliCommand, ignored []*cliRegistry) []cliToken {
		local, inherited := cliNewRegistry(cmd.config.flagSingles()), cliNewRegistry(cmd.context.flagSingles())
		subs := cliSubIndex(cmd)
		c := &cliCursor{tokens: tokens}
		remainder := []cliToken{}
		awaiting := true
		for t := c.take(); t != nil; t = c.take() {
			if t.flag() {
				if found := cliResolveFrom(*t, ignored); found != nil {
					remainder = cliPreserve(remainder, c, *t, found)
					continue
				}
				if found := cliResolve(*t, inherited); found != nil {
					remainder = cliPreserve(remainder, c, *t, found)
					continue
				}
				own, g := cliResolve(*t, local), cliResolve(*t, global)
				if own != nil && (g == nil || !awaiting ||
					!cliLocalPrecedesSubcommand(*t, c.rest(), own, subs, []*cliRegistry{local, inherited, global})) {
					remainder = cliPreserve(remainder, c, *t, own)
					continue
				}
				if g != nil {
					v, err, _ := cliConsumeValue(c, *t, g.param, g.negated)
					if err != nil {
						errors = append(errors, err)
					} else if v != nil {
						flags[g.param.name] = append(flags[g.param.name], *v)
					}
					continue
				}
				remainder = append(remainder, *t)
				continue
			}
			if awaiting {
				if sub, ok := subs[t.name]; ok {
					remainder = append(remainder, *t)
					return append(remainder, level(c.rest(), sub, append(ignored[:len(ignored):len(ignored)], inherited))...)
				}
				awaiting = false
			}
			remainder = append(remainder, *t)
		}
		return remainder
	}
	return flags, level(tokens, root, nil), errors
}

// cliParsed is Effect's ParsedTokens.
type cliParsed struct {
	flags     cliFlagMap
	arguments []string
	sub       *cliCommand
	subInput  *cliParsed
	errors    []*cliError
}

func cliCommandPath(p *cliParsed) []string {
	if p.sub == nil {
		return nil
	}
	return append([]string{p.sub.name}, cliCommandPath(p.subInput)...)
}

func cliUnrecognizedFlag(t cliToken, params []*cliSingle, path []string) *cliError {
	printable := "-" + t.name
	if t.kind == "long" {
		printable = "--" + t.name
	}
	valid := []string{}
	for _, p := range params {
		if p.hidden {
			continue // a near-miss typo cannot reveal a hidden flag
		}
		valid = append(valid, p.name)
		if p.prim.tag == "Boolean" {
			valid = append(valid, "no-"+p.name)
		}
		valid = append(valid, p.aliases...)
	}
	suggestions := cliSuggest(t.name, valid)
	for i, n := range suggestions {
		suggestions[i] = cliAliasForm(n)
	}
	return cliUnrecognized(printable, path, suggestions)
}

func cliAliasForm(n string) string {
	if len(utf16Units(n)) == 1 {
		return "-" + n
	}
	return "--" + n
}

// cliParseArgs is parser.ts's parseArgs.
func cliParseArgs(tokens []cliToken, trailing []string, cmd *cliCommand, parent []string) *cliParsed {
	path := append(parent[:len(parent):len(parent)], cmd.name)
	singles := cmd.config.flagSingles()
	registry := cliNewRegistry(singles)
	inheritedSingles := cmd.context.flagSingles()
	inherited := cliNewRegistry(inheritedSingles)
	inheritedNames := map[string]bool{}
	for _, p := range inheritedSingles {
		inheritedNames[p.name] = true
	}
	local := []string{}
	for _, p := range singles {
		if !inheritedNames[p.name] {
			local = append(local, p.name)
		}
	}
	flags, arguments, errors := cliEmptyFlags(singles), []string{}, []*cliError{}
	subs := cliSubIndex(cmd)
	awaiting := true
	c := &cliCursor{tokens: tokens}
	for t := c.take(); t != nil; t = c.take() {
		if t.flag() {
			found := cliResolve(*t, registry)
			if found == nil {
				errors = append(errors, cliUnrecognizedFlag(*t, registry.params, path))
				continue
			}
			v, err, _ := cliConsumeValue(c, *t, found.param, found.negated)
			if err != nil {
				errors = append(errors, err)
			} else if v != nil {
				flags[found.param.name] = append(flags[found.param.name], *v)
			}
			continue
		}
		if awaiting {
			if sub, ok := subs[t.name]; ok {
				selected := append(path[:len(path):len(path)], sub.name)
				for _, name := range local {
					if len(flags[name]) > 0 {
						errors = append(errors, cliUnrecognized("--"+name, selected, []string{}))
					}
				}
				tail, rest, tailErrors := cliConsumeKnown(c.rest(), inherited)
				for k, vs := range tail {
					flags[k] = append(flags[k], vs...)
				}
				errors = append(errors, tailErrors...)
				child := cliParseArgs(rest, trailing, sub, path)
				return &cliParsed{flags: flags, arguments: []string{}, sub: sub, subInput: child, errors: append(errors, child.errors...)}
			}
			if len(cmd.config.arguments) == 0 && len(subs) > 0 {
				visible := []string{}
				for _, sub := range cmd.subcommands { // the index's order: name, then alias
					if !sub.unlisted {
						visible = append(visible, sub.name)
						if sub.alias != "" && sub.alias != sub.name {
							visible = append(visible, sub.alias)
						}
					}
				}
				errors = append(errors, &cliError{tag: "UnknownSubcommand", subcommand: t.name, path: path, suggestions: cliSuggest(t.name, visible)})
			}
			awaiting = false
		}
		arguments = append(arguments, t.name)
	}
	return &cliParsed{flags: flags, arguments: append(arguments, trailing...), errors: errors}
}
