package flickrt

// `Cli.HelpDoc` as Flick records (output.ts): `type` is the field `type_`,
// absent text is `None`.

func cliOptionOf(s *string) Value {
	if s == nil {
		return None
	}
	return Some(*s)
}

func cliStringOf(o Value) *string {
	if v, ok := some(o); ok {
		s := v.(string)
		return &s
	}
	return nil
}

func cliStringList(xs []string) Value {
	out := make([]Value, len(xs))
	for i, x := range xs {
		out[i] = x
	}
	return out
}

func cliFlagDocs(flags []cliFlagDoc) Value {
	out := make([]Value, len(flags))
	for i, f := range flags {
		out[i] = Rec("name", f.name, "aliases", cliStringList(f.aliases), "type_", f.typ,
			"description", cliOptionOf(f.description), "required", f.required)
	}
	return out
}

func cliFlagDocsOf(v Value) []cliFlagDoc {
	out := make([]cliFlagDoc, len(List(v)))
	for i, x := range List(v) {
		r := record(x)
		out[i] = cliFlagDoc{r.Get("name").(string), cliStrings(r.Get("aliases")), r.Get("type_").(string),
			cliStringOf(r.Get("description")), r.Get("required").(bool)}
	}
	return out
}

// cliDocRecord is fromEffectDoc.
func cliDocRecord(doc *cliHelpDoc) Value {
	args := make([]Value, len(doc.args))
	for i, a := range doc.args {
		args[i] = Rec("name", a.name, "type_", a.typ, "description", cliOptionOf(a.description),
			"required", a.required, "variadic", a.variadic)
	}
	groups := make([]Value, len(doc.subcommands))
	for i, g := range doc.subcommands {
		commands := make([]Value, len(g.commands))
		for j, c := range g.commands {
			commands[j] = Rec("name", c.name, "alias", cliOptionOf(c.alias),
				"shortDescription", cliOptionOf(c.shortDescription), "description", c.description)
		}
		groups[i] = Rec("group", cliOptionOf(g.group), "commands", commands)
	}
	examples := make([]Value, len(doc.examples))
	for i, e := range doc.examples {
		examples[i] = Rec("command", e.command, "description", cliOptionOf(e.description))
	}
	return Rec("description", doc.description, "usage", doc.usage, "flags", cliFlagDocs(doc.flags),
		"globalFlags", cliFlagDocs(doc.globalFlags), "args", args, "subcommands", groups, "examples", examples)
}

// cliDocOf is toEffectDoc.
func cliDocOf(v Value) *cliHelpDoc {
	r := record(v)
	doc := &cliHelpDoc{description: r.Get("description").(string), usage: r.Get("usage").(string),
		flags: cliFlagDocsOf(r.Get("flags")), globalFlags: cliFlagDocsOf(r.Get("globalFlags"))}
	for _, x := range List(r.Get("args")) {
		a := record(x)
		doc.args = append(doc.args, cliArgDoc{a.Get("name").(string), a.Get("type_").(string),
			cliStringOf(a.Get("description")), a.Get("required").(bool), a.Get("variadic").(bool)})
	}
	for _, x := range List(r.Get("subcommands")) {
		g := record(x)
		group := cliGroupDoc{group: cliStringOf(g.Get("group"))}
		for _, y := range List(g.Get("commands")) {
			c := record(y)
			group.commands = append(group.commands, cliSubDoc{c.Get("name").(string), cliStringOf(c.Get("alias")),
				cliStringOf(c.Get("shortDescription")), c.Get("description").(string)})
		}
		doc.subcommands = append(doc.subcommands, group)
	}
	for _, x := range List(r.Get("examples")) {
		e := record(x)
		doc.examples = append(doc.examples, cliExampleDoc{e.Get("command").(string), cliStringOf(e.Get("description"))})
	}
	return doc
}
