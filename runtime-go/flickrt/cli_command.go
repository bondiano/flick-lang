package flickrt

import "slices"

// Effect's Command (Command.ts, internal/command.ts) as run.ts builds it of
// a Flick command: the flags of a command with subcommands are shared
// flags, parsed before the subcommand's name and seen only by its own
// handler.

// cliConfig is internal/config.ts's ConfigInternal; labels are the fields
// of the options record, by orderedParams.
type cliConfig struct {
	arguments, flags, ordered []*cliParam
	labels                    []string
}

func (c *cliConfig) add(label string, p *cliParam) {
	c.ordered, c.labels = append(c.ordered, p), append(c.labels, label)
	if p.single.kind == "flag" {
		c.flags = append(c.flags, p)
	} else {
		c.arguments = append(c.arguments, p)
	}
}

func (c *cliConfig) flagSingles() []*cliSingle {
	out := make([]*cliSingle, len(c.flags))
	for i, f := range c.flags {
		out[i] = f.single
	}
	return out
}

// cliGlobal is GlobalFlag: an Action runs instead of the command, a
// Setting is provided to its handler.
type cliGlobal struct {
	action bool
	id     string
	param  *cliParam
	run    func(fb *Fiber, value Value, ctx *cliContext)
}

type cliCommand struct {
	name, description, shortDescription, alias string // "" when absent
	unlisted                                   bool
	examples                                   []cliExample
	own                                        cliConfig // what `parse` reads before shared flags
	shared                                     cliConfig // withSharedFlags
	config                                     cliConfig // own, then shared: help and the flags of the line
	context                                    cliConfig // the shared flags subcommands see
	globals                                    []*cliGlobal
	subcommands                                []*cliCommand
	handler                                    Value
	hasSubcommands                             bool
}

// cliCommandOf is run.ts's `command`.
func cliCommandOf(spec *cliSpec) *cliCommand {
	c := &cliCommand{
		name: spec.name, description: spec.description, shortDescription: spec.shortDescription,
		alias: spec.alias, unlisted: spec.unlisted, examples: spec.examples, handler: spec.handler,
		hasSubcommands: len(spec.subcommands) > 0,
	}
	for _, p := range spec.params {
		switch {
		case !p.flag:
			c.own.add(p.label, cliParamOf("argument", p))
		case c.hasSubcommands:
			c.shared.add(p.label, cliParamOf("flag", p))
		default:
			c.own.add(p.label, cliParamOf("flag", p))
		}
	}
	c.config = cliConfig{slices.Clip(c.own.arguments), slices.Clip(c.own.flags), slices.Clip(c.own.ordered), slices.Clip(c.own.labels)}
	for i, p := range c.shared.ordered {
		c.config.add(c.shared.labels[i], p)
		c.context.add(c.shared.labels[i], p)
	}
	for _, a := range spec.actions {
		c.globals = append(c.globals, cliActionGlobal(a))
	}
	for _, s := range spec.subcommands {
		sub := cliCommandOf(s.command)
		// the help of `subcommand` lists it, unless it has a short description of its own
		if s.help != "" && sub.shortDescription == "" {
			sub.shortDescription = s.help
		}
		c.subcommands = append(c.subcommands, sub)
	}
	if c.hasSubcommands {
		cliCheckDuplicates(c)
	}
	return c
}

// cliCheckDuplicates is checkForDuplicateFlags: a subcommand's flag must not
// take a name of its parent's shared flags.
func cliCheckDuplicates(c *cliCommand) {
	names := map[string]bool{}
	for _, f := range c.context.flags {
		names[f.single.name] = true
	}
	for _, sub := range c.subcommands {
		for _, f := range sub.config.flags {
			if names[f.single.name] {
				panic(Defect{`Duplicate flag name "` + f.single.name + `" in parent command "` + c.name + `" and subcommand "` + sub.name + `". ` +
					`Parent will always claim this flag (Mode A semantics). Consider renaming one of them to avoid confusion.`})
			}
		}
	}
}

// cliValues parses `params` in order; with `allowLeftovers` false, arguments
// left over are UnexpectedArgument.
func cliValues(fb *Fiber, cfg *cliConfig, a cliArgs, allowLeftovers bool, out *Record) *cliError {
	current := a.arguments
	for i, p := range cfg.ordered {
		left, v, err := p.parse(fb, cliArgs{a.flags, current})
		if err != nil {
			return err
		}
		out.put(cfg.labels[i], v)
		current = left
	}
	if !allowLeftovers && len(current) > 0 {
		return &cliError{tag: "UnexpectedArgument", arguments: current}
	}
	return nil
}

// cliInput is what a command parsed: its options, or the subcommand chosen
// and what that parsed.
type cliInput struct {
	options  *Record
	child    *cliCommand
	childsIn *cliInput
}

func (c *cliCommand) parse(fb *Fiber, raw *cliParsed) (*cliInput, *cliError) {
	a := cliArgs{raw.flags, raw.arguments}
	out := newRecord(len(c.config.ordered))
	if raw.sub != nil {
		if err := cliValues(fb, &c.shared, a, true, out); err != nil {
			return nil, err
		}
		child, err := raw.sub.parse(fb, raw.subInput)
		if err != nil {
			return nil, err
		}
		return &cliInput{options: out, child: raw.sub, childsIn: child}, nil
	}
	if err := cliValues(fb, &c.own, a, false, out); err != nil {
		return nil, err
	}
	if err := cliValues(fb, &c.shared, a, true, out); err != nil {
		return nil, err
	}
	return &cliInput{options: out}, nil
}

// handle runs the handler of the command the input chose, or asks for help
// for a group.
func (c *cliCommand) handle(fb *Fiber, in *cliInput, path []string, trace func(name string, run func() Value) Value) *cliError {
	if in.child != nil {
		return in.child.handle(fb, in.childsIn, append(path[:len(path):len(path)], in.child.name), trace)
	}
	if c.handler == nil {
		return cliShowHelp(path, nil)
	}
	trace(c.name, func() Value { return Run(fb, Apply(fb, c.handler, in.options)) })
	return nil
}

// cliCommandsOn is getCommandsForCommandPath.
func cliCommandsOn(root *cliCommand, path []string) []*cliCommand {
	out := []*cliCommand{root}
	current := root
	for _, name := range path[min(1, len(path)):] {
		var next *cliCommand
		for _, sub := range current.subcommands {
			if sub.name == name {
				next = sub
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
	seen, out := map[*cliGlobal]bool{}, []*cliGlobal{}
	for _, f := range flags {
		if !seen[f] {
			seen[f] = true
			out = append(out, f)
		}
	}
	return out
}

// cliGlobalsOn is getGlobalFlagsForCommandPath.
func cliGlobalsOn(root *cliCommand, path []string, builtIns []*cliGlobal) []*cliGlobal {
	out := append([]*cliGlobal{}, builtIns...)
	for _, c := range cliCommandsOn(root, path) {
		out = append(out, c.globals...)
	}
	return cliDedupe(out)
}

// cliGlobalsOfTree is getGlobalFlagsForCommandTree.
func cliGlobalsOfTree(root *cliCommand, builtIns []*cliGlobal) []*cliGlobal {
	out := append([]*cliGlobal{}, builtIns...)
	var visit func(*cliCommand)
	visit = func(c *cliCommand) {
		out = append(out, c.globals...)
		for _, sub := range c.subcommands {
			visit(sub)
		}
	}
	visit(root)
	return cliDedupe(out)
}
