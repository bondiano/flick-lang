//go:build flickcli

package flickrt

import (
	"regexp"
	"slices"
	"strings"
)

// The command line as Effect's CLI reads it (`internal/lexer.js` and
// `internal/parser.js`): tokens, the global flags taken out wherever they
// stand, then each command level's flags, arguments and subcommand.

// cliToken is a word of the command line: a value, `--name[=value]` or
// `-f[=value]`.
type cliToken struct {
	long, short bool
	name        string // the value of a value token, else the flag's name
	raw         string
	value       *string
}

func (t *cliToken) isFlag() bool { return t.long || t.short }

var cliNegativeNumber = regexp.MustCompile(`^-(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?$`)

// cliLex is the tokens of the words before `--` and the words after it.
func cliLex(argv []string) ([]*cliToken, []string) {
	end := slices.Index(argv, "--")
	if end == -1 {
		return cliLexTokens(argv), []string{}
	}
	return cliLexTokens(argv[:end]), argv[end+1:]
}

func cliLexTokens(args []string) []*cliToken {
	tokens := []*cliToken{}
	for _, arg := range args {
		switch {
		case !strings.HasPrefix(arg, "-") || cliNegativeNumber.MatchString(arg):
			tokens = append(tokens, &cliToken{name: arg})
		case strings.HasPrefix(arg, "--"):
			if name, value, ok := strings.Cut(arg[2:], "="); ok {
				tokens = append(tokens, &cliToken{long: true, name: name, raw: arg, value: &value})
			} else {
				tokens = append(tokens, &cliToken{long: true, name: arg[2:], raw: arg})
			}
		case len(arg) > 1:
			if flag, value, ok := strings.Cut(arg[1:], "="); ok {
				tokens = append(tokens, &cliToken{short: true, name: flag, raw: "-" + flag, value: &value})
			} else {
				for _, ch := range arg[1:] {
					tokens = append(tokens, &cliToken{short: true, name: string(ch), raw: "-" + string(ch)})
				}
			}
		default:
			tokens = append(tokens, &cliToken{name: arg})
		}
	}
	return tokens
}

// cliRegistry is the flags of a level by their names and aliases.
type cliRegistry struct {
	params []*cliSingle
	index  map[string]*cliSingle
}

func cliRegistryOf(params []*cliSingle) *cliRegistry {
	r := &cliRegistry{params, map[string]*cliSingle{}}
	for _, p := range params {
		if _, ok := r.index[p.name]; ok {
			panic(Defect{`Error: Duplicate flag name "` + p.name + `" in command definition`})
		}
		r.index[p.name] = p
		for _, a := range p.aliases {
			if other, ok := r.index[a]; ok {
				panic(Defect{`Error: Duplicate flag/alias "` + a + `" in command definition (conflicts with "` + other.name + `")`})
			}
			r.index[a] = p
		}
	}
	return r
}

// cliSubIndex is a level's subcommands by name and alias, in order.
type cliSubIndex struct {
	keys []string
	cmds map[string]*cliCmd
}

func cliSubIndexOf(groups []cliGroup) *cliSubIndex {
	ix := &cliSubIndex{cmds: map[string]*cliCmd{}}
	set := func(key string, c *cliCmd) {
		if existing, ok := ix.cmds[key]; ok {
			if existing != c {
				panic(Defect{`Error: Duplicate subcommand name/alias "` + key + `" in command definition (conflicts with "` + existing.name + `")`})
			}
			return
		}
		ix.keys = append(ix.keys, key)
		ix.cmds[key] = c
	}
	for _, g := range groups {
		for _, c := range g.commands {
			set(c.name, c)
			if c.alias != nil && *c.alias != c.name {
				set(*c.alias, c)
			}
		}
	}
	return ix
}

type cliResolved struct {
	param   *cliSingle
	negated bool
}

// cliResolve finds the flag of a token: its name, `--no-` and a boolean
// flag's name, or an alias.
func cliResolve(t *cliToken, r *cliRegistry) *cliResolved {
	direct := r.index[t.name]
	if direct != nil && direct.name == t.name {
		return &cliResolved{direct, false}
	}
	if t.long && strings.HasPrefix(t.name, "no-") {
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

func cliResolveIn(t *cliToken, registries []*cliRegistry) *cliResolved {
	for _, r := range registries {
		if found := cliResolve(t, r); found != nil {
			return found
		}
	}
	return nil
}

// cliCursor walks tokens.
type cliCursor struct {
	tokens []*cliToken
	i      int
}

func (c *cliCursor) peek() *cliToken {
	if c.i < len(c.tokens) {
		return c.tokens[c.i]
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

func (c *cliCursor) rest() []*cliToken { return c.tokens[c.i:] }

func cliBooleanLiteral(t *cliToken) (string, bool) {
	if t != nil && !t.isFlag() && (cliTrue[t.name] || cliFalse[t.name]) {
		return t.name, true
	}
	return "", false
}

// cliFlagValue takes the value of a flag token: attached, its optional
// value, a boolean literal after a boolean flag, or the next value; with
// the tokens it took.
func cliFlagValue(c *cliCursor, t *cliToken, spec *cliSingle, negated bool) (string, *cliErr, []*cliToken) {
	if negated {
		if t.value != nil {
			return "", cliNegatedValue(t, spec, *t.value), nil
		}
		if spec.optionalValue == nil {
			if lit, ok := cliBooleanLiteral(c.peek()); ok {
				return "", cliNegatedValue(t, spec, lit), []*cliToken{c.take()}
			}
		}
		return "false", nil, nil
	}
	if t.value != nil {
		return *t.value, nil, nil
	}
	if spec.optionalValue != nil {
		return *spec.optionalValue, nil, nil
	}
	if spec.prim.tag == "Boolean" {
		if lit, ok := cliBooleanLiteral(c.peek()); ok {
			return lit, nil, []*cliToken{c.take()}
		}
		return "true", nil, nil
	}
	if next := c.peek(); next != nil && !next.isFlag() {
		return next.name, nil, []*cliToken{c.take()}
	}
	expected := spec.typeLabel()
	if spec.prim.tag == "Choice" {
		expected = strings.Join(spec.prim.choiceKeys(), " | ")
	}
	return "", &cliErr{tag: "InvalidValue", option: spec.name, expected: expected, kind: "flag"}, nil
}

func cliNegatedValue(t *cliToken, spec *cliSingle, value string) *cliErr {
	return &cliErr{tag: "InvalidValue", option: spec.name, value: value, kind: "flag",
		expected: "omit the value and use " + t.raw + " by itself to set --" + spec.name + " to false"}
}

// cliFlagMap is the values given for each flag, by its name.
type cliFlagMap map[string][]string

func cliEmptyFlags(params []*cliSingle) cliFlagMap {
	m := cliFlagMap{}
	for _, p := range params {
		m[p.name] = []string{}
	}
	return m
}

// cliConsumeKnown takes the flags of the registry out of the tokens.
func cliConsumeKnown(tokens []*cliToken, r *cliRegistry) (cliFlagMap, []*cliToken, []*cliErr) {
	flags := cliEmptyFlags(r.params)
	remainder := []*cliToken{}
	var errs []*cliErr
	c := &cliCursor{tokens: tokens}
	for t := c.take(); t != nil; t = c.take() {
		found := (*cliResolved)(nil)
		if t.isFlag() {
			found = cliResolve(t, r)
		}
		if found == nil {
			remainder = append(remainder, t)
			continue
		}
		value, err, _ := cliFlagValue(c, t, found.param, found.negated)
		if err != nil {
			errs = append(errs, err)
			continue
		}
		flags[found.param.name] = append(flags[found.param.name], value)
	}
	return flags, remainder, errs
}

// cliConsumeGlobal takes the global flags out of the tokens, but where a
// level's own flag of the same name stands before its subcommand.
func cliConsumeGlobal(tokens []*cliToken, cmd *cliCmd, global *cliRegistry) (cliFlagMap, []*cliToken, []*cliErr) {
	flags := cliEmptyFlags(global.params)
	var errs []*cliErr
	var level func(tokens []*cliToken, cmd *cliCmd, ignored []*cliRegistry) []*cliToken
	level = func(tokens []*cliToken, cmd *cliCmd, ignored []*cliRegistry) []*cliToken {
		local := cliRegistryOf(cliFlagSingles(cmd.config.flags))
		inherited := cliRegistryOf(cliFlagSingles(cmd.contextConfig.flags))
		subs := cliSubIndexOf(cmd.subcommands)
		c := &cliCursor{tokens: tokens}
		remainder := []*cliToken{}
		preserve := func(t *cliToken, r *cliResolved) {
			remainder = append(remainder, t)
			_, _, taken := cliFlagValue(c, t, r.param, r.negated)
			remainder = append(remainder, taken...)
		}
		awaitingFirst := true
		for t := c.take(); t != nil; t = c.take() {
			if t.isFlag() {
				if r := cliResolveIn(t, ignored); r != nil {
					preserve(t, r)
					continue
				}
				if r := cliResolve(t, inherited); r != nil {
					preserve(t, r)
					continue
				}
				l, g := cliResolve(t, local), cliResolve(t, global)
				if l != nil && (g == nil || !awaitingFirst ||
					!cliPrecedesSubcommand(t, c.rest(), l, subs, []*cliRegistry{local, inherited, global})) {
					preserve(t, l)
					continue
				}
				if g != nil {
					value, err, _ := cliFlagValue(c, t, g.param, g.negated)
					if err != nil {
						errs = append(errs, err)
					} else {
						flags[g.param.name] = append(flags[g.param.name], value)
					}
					continue
				}
				remainder = append(remainder, t)
				continue
			}
			if awaitingFirst {
				if sub := subs.cmds[t.name]; sub != nil {
					remainder = append(remainder, t)
					return append(remainder, level(c.rest(), sub, append(slices.Clip(ignored), inherited))...)
				}
				awaitingFirst = false
			}
			remainder = append(remainder, t)
		}
		return remainder
	}
	return flags, level(tokens, cmd, nil), errs
}

// cliPrecedesSubcommand tells whether a level's flag stands before the name
// of one of its subcommands, so the subcommand's line follows.
func cliPrecedesSubcommand(t *cliToken, rest []*cliToken, r *cliResolved, subs *cliSubIndex, registries []*cliRegistry) bool {
	c := &cliCursor{tokens: rest}
	cliFlagValue(c, t, r.param, r.negated)
	for t := c.take(); t != nil; t = c.take() {
		if t.isFlag() {
			if known := cliResolveIn(t, registries); known != nil {
				cliFlagValue(c, t, known.param, known.negated)
			}
			continue
		}
		_, ok := subs.cmds[t.name]
		return ok
	}
	return false
}

// cliParsed is the command line of a level: its flags, its arguments or
// its subcommand's line, and what was wrong with it.
type cliParsed struct {
	flags     cliFlagMap
	arguments []string
	sub       *cliCmd
	subParsed *cliParsed
	errors    []*cliErr
}

// cliCommandPath is the names of the subcommands the line names.
func cliCommandPath(p *cliParsed) []string {
	if p.sub == nil {
		return nil
	}
	return append([]string{p.sub.name}, cliCommandPath(p.subParsed)...)
}

func cliParseArgs(tokens []*cliToken, trailing []string, cmd *cliCmd, path []string) *cliParsed {
	path = append(slices.Clip(path), cmd.name)
	singles := cliFlagSingles(cmd.config.flags)
	registry := cliRegistryOf(singles)
	inheritedSingles := cliFlagSingles(cmd.contextConfig.flags)
	inherited := cliRegistryOf(inheritedSingles)
	var localNames []string
	for _, p := range singles {
		if !slices.ContainsFunc(inheritedSingles, func(q *cliSingle) bool { return q.name == p.name }) {
			localNames = append(localNames, p.name)
		}
	}
	subs := cliSubIndexOf(cmd.subcommands)
	flags := cliEmptyFlags(registry.params)
	var arguments []string
	var errs []*cliErr
	awaitingFirst := true
	c := &cliCursor{tokens: tokens}
	for t := c.take(); t != nil; t = c.take() {
		if t.isFlag() {
			r := cliResolve(t, registry)
			if r == nil {
				errs = append(errs, cliUnrecognized(t, registry.params, path))
				continue
			}
			value, err, _ := cliFlagValue(c, t, r.param, r.negated)
			if err != nil {
				errs = append(errs, err)
				continue
			}
			flags[r.param.name] = append(flags[r.param.name], value)
			continue
		}
		if awaitingFirst {
			if sub := subs.cmds[t.name]; sub != nil {
				selected := append(slices.Clip(path), sub.name)
				for _, name := range localNames {
					if len(flags[name]) > 0 {
						errs = append(errs, &cliErr{tag: "UnrecognizedOption", option: "--" + name, command: selected})
					}
				}
				tail, rest, tailErrs := cliConsumeKnown(c.rest(), inherited)
				for k, vs := range tail {
					flags[k] = append(flags[k], vs...)
				}
				errs = append(errs, tailErrs...)
				subParsed := cliParseArgs(rest, trailing, sub, path)
				return &cliParsed{flags: flags, arguments: []string{}, sub: sub, subParsed: subParsed,
					errors: append(errs, subParsed.errors...)}
			}
			if len(cmd.config.args) == 0 && len(subs.keys) > 0 {
				var visible []string
				for _, k := range subs.keys {
					if !subs.cmds[k].unlisted {
						visible = append(visible, k)
					}
				}
				errs = append(errs, &cliErr{tag: "UnknownSubcommand", subcommand: t.name, parent: path,
					suggestions: cliSuggest(t.name, visible)})
			}
			awaitingFirst = false
		}
		arguments = append(arguments, t.name)
	}
	return &cliParsed{flags: flags, arguments: append(arguments, trailing...), errors: errs}
}

func cliUnrecognized(t *cliToken, params []*cliSingle, path []string) *cliErr {
	printable := "-" + t.name
	if t.long {
		printable = "--" + t.name
	}
	var valid []string
	for _, p := range params {
		if p.hidden {
			continue
		}
		valid = append(valid, p.name)
		if p.prim.tag == "Boolean" {
			valid = append(valid, "no-"+p.name)
		}
		valid = append(valid, p.aliases...)
	}
	suggestions := []string{}
	for _, n := range cliSuggest(t.name, valid) {
		if len(utf16Units(n)) == 1 {
			suggestions = append(suggestions, "-"+n)
		} else {
			suggestions = append(suggestions, "--"+n)
		}
	}
	return &cliErr{tag: "UnrecognizedOption", option: printable, suggestions: suggestions, command: path}
}

// cliSuggest is the candidates nearest the input, within two edits.
func cliSuggest(input string, candidates []string) []string {
	type scored struct {
		d int
		c string
	}
	var near []scored
	for _, c := range candidates {
		if d := cliLevenshtein(utf16Units(input), utf16Units(c)); d <= 2 {
			near = append(near, scored{d, c})
		}
	}
	slices.SortStableFunc(near, func(a, b scored) int { return a.d - b.d })
	out := []string{}
	for _, s := range near {
		if s.d == near[0].d {
			out = append(out, s.c)
		}
	}
	return out
}

func cliLevenshtein(a, b []uint16) int {
	prev := make([]int, len(b)+1)
	for j := range prev {
		prev[j] = j
	}
	for i := 1; i <= len(a); i++ {
		cur := make([]int, len(b)+1)
		cur[0] = i
		for j := 1; j <= len(b); j++ {
			cost := 1
			if a[i-1] == b[j-1] {
				cost = 0
			}
			cur[j] = min(prev[j]+1, cur[j-1]+1, prev[j-1]+cost)
		}
		prev = cur
	}
	return prev[len(b)]
}
