//go:build flickcli

package flickrt

import (
	"slices"
	"strings"
)

// Effect's CLI commands (`Command.js`, `internal/command.js`) as `run.ts`
// builds them from a Flick command, and their help documents
// (`internal/help.js`).

// cliConfig is the parameters of a command by the fields they make, in
// order.
type cliConfig struct {
	keys   []string
	params []*cliEParam
}

func (c *cliConfig) add(key string, p *cliEParam) {
	c.keys = append(c.keys, key)
	c.params = append(c.params, p)
}

func (c cliConfig) merge(o cliConfig) cliConfig {
	return cliConfig{append(slices.Clip(c.keys), o.keys...), append(slices.Clip(c.params), o.params...)}
}

func (c cliConfig) of(kind string) []*cliEParam {
	var out []*cliEParam
	for _, p := range c.params {
		if p.single.kind == kind {
			out = append(out, p)
		}
	}
	return out
}

// cliConfigView is a config with its flags and arguments apart.
type cliConfigView struct {
	cliConfig
	flags, args []*cliEParam
}

func cliView(c cliConfig) cliConfigView { return cliConfigView{c, c.of("flag"), c.of("argument")} }

type cliGroup struct {
	group    *string
	commands []*cliCmd
}

type cliExampleDoc struct {
	command     string
	description *string
}

// cliCmd is Effect's command: `base` is what it parses strictly, `shared`
// the flags its subcommands share, parsed beside it.
type cliCmd struct {
	name                                 string
	description, shortDescription, alias *string
	unlisted                             bool
	examples                             []cliExampleDoc
	globalFlags                          []*cliGlobal
	config, contextConfig                cliConfigView
	base                                 cliConfig
	shared                               *cliConfig
	subcommands                          []cliGroup
	handler                              Value
}

func cliText(s string) *string {
	if s == "" {
		return nil
	}
	return &s
}

// cliCommandOf is the command `run.ts` builds of a Flick one: the flags of
// a command with subcommands are flags it shares with them.
func cliCommandOf(spec *cliSpec) *cliCmd {
	c := &cliCmd{name: spec.name, handler: spec.handler}
	var own, shared cliConfig
	for i := range spec.params {
		p := &spec.params[i]
		switch {
		case !p.flag:
			own.add(p.label, cliParamOf("argument", p))
		case len(spec.subcommands) > 0:
			shared.add(p.label, cliParamOf("flag", p))
		default:
			own.add(p.label, cliParamOf("flag", p))
		}
	}
	c.base, c.config, c.contextConfig = own, cliView(own), cliView(cliConfig{})
	c.description, c.shortDescription, c.alias = cliText(spec.description), cliText(spec.shortDescription), cliText(spec.alias)
	c.unlisted = spec.unlisted
	for _, e := range spec.examples {
		c.examples = append(c.examples, cliExampleDoc{e.command, cliText(e.description)})
	}
	for _, a := range spec.actions {
		c.globalFlags = append(c.globalFlags, cliActionFlag(a))
	}
	if len(spec.subcommands) == 0 {
		return c
	}
	c.shared = &shared
	c.config, c.contextConfig = cliView(own.merge(shared)), cliView(shared)
	var subs []*cliCmd
	for _, s := range spec.subcommands {
		sub := cliCommandOf(s.command)
		// the help of `subcommand` lists it, unless it has a short description of its own
		if s.help != "" && s.command.shortDescription == "" {
			help := s.help
			sub.shortDescription = &help
		}
		subs = append(subs, sub)
	}
	parent := map[string]bool{}
	for _, s := range cliFlagSingles(c.contextConfig.flags) {
		parent[s.name] = true
	}
	for _, sub := range subs {
		for _, s := range cliFlagSingles(sub.config.flags) {
			if parent[s.name] {
				panic(Defect{`DuplicateOption: Duplicate flag name "` + s.name + `" in parent command "` + c.name +
					`" and subcommand "` + sub.name + `". Parent will always claim this flag (Mode A semantics). Consider renaming one of them to avoid confusion.`})
			}
		}
	}
	c.subcommands = []cliGroup{{nil, subs}}
	return c
}

// cliSelected is the input of a command whose subcommand was named.
type cliSelected struct {
	cmd    *cliCmd
	result Value
}

// parseConfig reads the fields of a config off the arguments in order;
// arguments left are an error unless `lenient`.
func parseConfig(fb *Fiber, cfg cliConfig, raw *cliParsed, lenient bool, into *Record) *cliErr {
	current := raw.arguments
	for i, p := range cfg.params {
		rest, v, err := p.parse(fb, cliArgs{raw.flags, current})
		if err != nil {
			return err
		}
		into.put(cfg.keys[i], v)
		current = rest
	}
	if !lenient && len(current) > 0 {
		return &cliErr{tag: "UnexpectedArgument", arguments: current}
	}
	return nil
}

func (c *cliCmd) parse(fb *Fiber, raw *cliParsed) (Value, *cliErr) {
	if raw.sub != nil && c.shared != nil {
		if err := parseConfig(fb, *c.shared, raw, true, &Record{}); err != nil {
			return nil, err
		}
		result, err := raw.sub.parse(fb, raw.subParsed)
		if err != nil {
			return nil, err
		}
		return &cliSelected{raw.sub, result}, nil
	}
	out := &Record{}
	if err := parseConfig(fb, c.base, raw, false, out); err != nil {
		return nil, err
	}
	if c.shared != nil {
		if err := parseConfig(fb, *c.shared, raw, true, out); err != nil {
			return nil, err
		}
	}
	return out, nil
}

// handle runs the handler of the command the input names; a group without
// one shows its help.
func (c *cliCmd) handle(fb *Fiber, input Value, path []string) *cliErr {
	if sel, ok := input.(*cliSelected); ok {
		return sel.cmd.handle(fb, sel.result, append(slices.Clip(path), sel.cmd.name))
	}
	if c.handler == nil {
		return &cliErr{tag: "ShowHelp", commandPath: path}
	}
	// shortcut: `--trace` runs the handler without OTLP, which has no go form; trace it when Otlp is ported
	Run(fb, Apply(c.handler, input))
	return nil
}

// cliFlagDoc, cliArgDoc and the others are Effect's `HelpDoc`.
type cliFlagDoc struct {
	name        string
	aliases     []string
	typ         string
	description *string
	required    bool
}

type cliArgDoc struct {
	name, typ          string
	description        *string
	required, variadic bool
}

type cliSubDoc struct {
	name                    string
	alias, shortDescription *string
	description             string
}

type cliSubGroupDoc struct {
	group    *string
	commands []cliSubDoc
}

type cliHelpDoc struct {
	description, usage string
	flags, globalFlags []cliFlagDoc
	args               []cliArgDoc
	subcommands        []cliSubGroupDoc
	examples           []cliExampleDoc
}

func cliFlagDocOf(s *cliSingle, p *cliEParam) cliFlagDoc {
	aliases := []string{}
	for _, a := range s.aliases {
		if len(utf16Units(a)) == 1 {
			aliases = append(aliases, "-"+a)
		} else {
			aliases = append(aliases, "--"+a)
		}
	}
	typ := s.typeLabel()
	if s.optionalValue != nil {
		typ = "[=" + typ + "]"
	}
	description := s.description
	if s.prim.tag == "Choice" {
		suffix := "(choices: " + strings.Join(s.prim.choiceKeys(), ", ") + ")"
		if description != nil {
			suffix = *description + " " + suffix
		}
		description = &suffix
	}
	return cliFlagDoc{s.name, aliases, typ, description, s.prim.tag != "Boolean" && !p.isOptional()}
}

func (c *cliCmd) helpDoc(path []string) *cliHelpDoc {
	doc := &cliHelpDoc{description: "", flags: []cliFlagDoc{}}
	if c.description != nil {
		doc.description = *c.description
	}
	for _, p := range c.config.args {
		s := p.single
		doc.args = append(doc.args, cliArgDoc{s.name, s.typeLabel(), s.description,
			!p.isOptional() && (!p.isVariadic() || p.min > 0), p.isVariadic()})
	}
	usage := c.name
	if len(path) > 0 {
		usage = strings.Join(path, " ")
	}
	var subDocs []cliSubGroupDoc
	for _, g := range c.subcommands {
		var visible []cliSubDoc
		for _, sub := range g.commands {
			if !sub.unlisted {
				description := ""
				if sub.description != nil {
					description = *sub.description
				}
				visible = append(visible, cliSubDoc{sub.name, sub.alias, sub.shortDescription, description})
			}
		}
		if len(visible) > 0 {
			subDocs = append(subDocs, cliSubGroupDoc{g.group, visible})
		}
	}
	if len(subDocs) > 0 {
		usage += " <subcommand>"
	}
	usage += " [flags]"
	for _, a := range doc.args {
		name := "<" + a.name + ">"
		if a.variadic {
			name = "<" + a.name + "...>"
		}
		if !a.required {
			name = "[" + name + "]"
		}
		usage += " " + name
	}
	doc.usage = usage
	for _, p := range c.config.flags {
		if !p.single.hidden {
			doc.flags = append(doc.flags, cliFlagDocOf(p.single, p))
		}
	}
	doc.subcommands, doc.examples = subDocs, c.examples
	return doc
}

// cliCommandsOn is the commands the path names, from the root.
func cliCommandsOn(root *cliCmd, path []string) []*cliCmd {
	out := []*cliCmd{root}
	current := root
	for _, name := range path[min(1, len(path)):] {
		var next *cliCmd
		for _, g := range current.subcommands {
			if i := slices.IndexFunc(g.commands, func(c *cliCmd) bool { return c.name == name }); i >= 0 {
				next = g.commands[i]
				break
			}
		}
		if next == nil {
			break
		}
		out, current = append(out, next), next
	}
	return out
}

func cliDedupe(flags []*cliGlobal) []*cliGlobal {
	var out []*cliGlobal
	for _, f := range flags {
		if !slices.Contains(out, f) {
			out = append(out, f)
		}
	}
	return out
}

// cliGlobalsOn is the global flags of the commands of the path.
func cliGlobalsOn(root *cliCmd, path []string, builtIns []*cliGlobal) []*cliGlobal {
	all := slices.Clone(builtIns)
	for _, c := range cliCommandsOn(root, path) {
		all = append(all, c.globalFlags...)
	}
	return cliDedupe(all)
}

// cliGlobalsOf is the global flags of every command of the tree.
func cliGlobalsOf(root *cliCmd, builtIns []*cliGlobal) []*cliGlobal {
	all := slices.Clone(builtIns)
	var visit func(c *cliCmd)
	visit = func(c *cliCmd) {
		all = append(all, c.globalFlags...)
		for _, g := range c.subcommands {
			for _, sub := range g.commands {
				visit(sub)
			}
		}
	}
	visit(root)
	return cliDedupe(all)
}

// cliHelpOn is the help document of the command the path names.
func cliHelpOn(root *cliCmd, path []string, builtIns []*cliGlobal) *cliHelpDoc {
	commands := cliCommandsOn(root, path)
	doc := commands[len(commands)-1].helpDoc(path)
	seen := map[string]bool{}
	for _, f := range doc.flags {
		seen[f.name] = true
	}
	var shared []cliFlagDoc
	for _, ancestor := range commands[:len(commands)-1] {
		for _, p := range ancestor.contextConfig.flags {
			if !seen[p.single.name] && !p.single.hidden {
				seen[p.single.name] = true
				shared = append(shared, cliFlagDocOf(p.single, p))
			}
		}
	}
	doc.flags = append(shared, doc.flags...)
	doc.globalFlags = []cliFlagDoc{}
	for _, g := range cliGlobalsOn(root, path, builtIns) {
		if !g.param.single.hidden {
			f := cliFlagDocOf(g.param.single, g.param)
			f.required = false
			doc.globalFlags = append(doc.globalFlags, f)
		}
	}
	return doc
}
