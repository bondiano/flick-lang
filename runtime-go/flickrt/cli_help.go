package flickrt

import "strings"

// Help documents (HelpDoc.ts, internal/help.ts, buildHelpDoc of
// internal/command.ts) and Effect's default formatter (CliOutput.ts).

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

type cliGroupDoc struct {
	group    *string
	commands []cliSubDoc
}

type cliExampleDoc struct {
	command     string
	description *string
}

type cliHelpDoc struct {
	description, usage string
	flags, globalFlags []cliFlagDoc
	args               []cliArgDoc
	subcommands        []cliGroupDoc
	examples           []cliExampleDoc
}

func cliOptional(s string) *string {
	if s == "" {
		return nil
	}
	return &s
}

// cliToFlagDoc is toFlagDoc.
func cliToFlagDoc(p *cliParam) cliFlagDoc {
	s := p.single
	aliases := make([]string, len(s.aliases))
	for i, a := range s.aliases {
		aliases[i] = cliAliasForm(a)
	}
	typ := s.typeNameOr()
	if s.optionalValue != nil {
		typ = "[=" + typ + "]"
	}
	description := s.description
	if keys := s.prim.choiceKeys; len(keys) > 0 {
		suffix := "(choices: " + strings.Join(keys, ", ") + ")"
		if description == nil {
			description = &suffix
		} else {
			d := *description + " " + suffix
			description = &d
		}
	}
	return cliFlagDoc{s.name, aliases, typ, description, s.prim.tag != "Boolean" && !p.optional}
}

// buildHelpDoc of internal/command.ts.
func (c *cliCommand) buildHelpDoc(path []string) *cliHelpDoc {
	doc := &cliHelpDoc{description: c.description}
	for _, p := range c.config.arguments {
		doc.args = append(doc.args, cliArgDoc{
			name: p.single.name, typ: p.single.typeNameOr(), description: p.single.description,
			required: !p.optional && (!p.variadic || p.min > 0), variadic: p.variadic,
		})
	}
	usage := c.name
	if len(path) > 0 {
		usage = strings.Join(path, " ")
	}
	visible := []cliSubDoc{}
	for _, sub := range c.subcommands {
		if !sub.unlisted {
			visible = append(visible, cliSubDoc{sub.name, cliOptional(sub.alias), cliOptional(sub.shortDescription), sub.description})
		}
	}
	if len(visible) > 0 {
		usage += " <subcommand>"
		doc.subcommands = []cliGroupDoc{{nil, visible}}
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
			doc.flags = append(doc.flags, cliToFlagDoc(p))
		}
	}
	for _, e := range c.examples {
		doc.examples = append(doc.examples, cliExampleDoc{e.command, cliOptional(e.description)})
	}
	return doc
}

// cliHelpFor is getHelpForCommandPath.
func cliHelpFor(root *cliCommand, path []string, builtIns []*cliGlobal) *cliHelpDoc {
	commands := cliCommandsOn(root, path)
	doc := commands[len(commands)-1].buildHelpDoc(path)
	seen := map[string]bool{}
	for _, f := range doc.flags {
		seen[f.name] = true
	}
	shared := []cliFlagDoc{}
	if len(commands) > 1 {
		for _, ancestor := range commands[:len(commands)-1] {
			for _, p := range ancestor.context.flags {
				if seen[p.single.name] || p.single.hidden {
					continue
				}
				seen[p.single.name] = true
				shared = append(shared, cliToFlagDoc(p))
			}
		}
	}
	doc.flags = append(shared, doc.flags...)
	for _, g := range cliGlobalsOn(root, path, builtIns) {
		if !g.param.single.hidden {
			d := cliToFlagDoc(g.param)
			d.required = false
			doc.globalFlags = append(doc.globalFlags, d)
		}
	}
	return doc
}

// cliColors is the palette of the default formatter: escapes or nothing.
type cliColors struct{ on bool }

func (c cliColors) wrap(code, text string) string {
	if !c.on {
		return text
	}
	return "\x1b[" + code + "m" + text + "\x1b[0m"
}

func (c cliColors) bold(t string) string  { return c.wrap("1", t) }
func (c cliColors) dim(t string) string   { return c.wrap("2", t) }
func (c cliColors) cyan(t string) string  { return c.wrap("36", t) }
func (c cliColors) green(t string) string { return c.wrap("32", t) }

type cliRow struct{ left, right string }

// cliTable is renderTable; widthCap < 0 means none.
func cliTable(rows []cliRow, widthCap int) string {
	widest := 0
	for _, r := range rows {
		widest = max(widest, cliDisplayWidth(r.left))
	}
	col := widest + 4
	if widthCap >= 0 {
		col = min(col, widthCap)
	}
	lines := make([]string, len(rows))
	for i, r := range rows {
		w := cliDisplayWidth(r.left)
		lines[i] = "  " + r.left + strings.Repeat(" ", max(0, max(col, w+1)-w)) + r.right
	}
	return strings.Join(lines, "\n")
}

func cliOr(s *string, fallback string) string {
	if s == nil {
		return fallback
	}
	return *s
}

func cliFlagRows(flags []cliFlagDoc, c cliColors) []cliRow {
	rows := make([]cliRow, len(flags))
	for i, f := range flags {
		names := []string{c.green("--" + f.name)}
		for _, a := range f.aliases {
			names = append(names, c.green(a))
		}
		left := strings.Join(names, ", ")
		if f.typ != "boolean" {
			left += " " + c.dim(f.typ)
		}
		rows[i] = cliRow{left, cliOr(f.description, "")}
	}
	return rows
}

func cliSubRows(commands []cliSubDoc, c cliColors) []cliRow {
	rows := make([]cliRow, len(commands))
	for i, s := range commands {
		name := s.name
		if s.alias != nil && *s.alias != "" {
			name += ", " + *s.alias
		}
		rows[i] = cliRow{c.cyan(name), cliOr(s.shortDescription, s.description)}
	}
	return rows
}

// cliFormatHelp is formatHelpDocImpl.
func cliFormatHelp(doc *cliHelpDoc, c cliColors) string {
	s := []string{}
	if doc.description != "" {
		s = append(s, c.bold("DESCRIPTION"), "  "+doc.description, "")
	}
	s = append(s, c.bold("USAGE"), "  "+c.cyan(doc.usage), "")
	if len(doc.args) > 0 {
		rows := make([]cliRow, len(doc.args))
		for i, a := range doc.args {
			name := a.name
			if a.variadic {
				name += "..."
			}
			suffix := ""
			if !a.required {
				suffix = c.dim(" (optional)")
			}
			rows[i] = cliRow{c.green(name) + " " + c.dim(a.typ), cliOr(a.description, "") + suffix}
		}
		s = append(s, c.bold("ARGUMENTS"), cliTable(rows, 25), "")
	}
	if len(doc.flags) > 0 {
		s = append(s, c.bold("FLAGS"), cliTable(cliFlagRows(doc.flags, c), -1), "")
	}
	if len(doc.globalFlags) > 0 {
		s = append(s, c.bold("GLOBAL FLAGS"), cliTable(cliFlagRows(doc.globalFlags, c), -1), "")
	}
	if len(doc.subcommands) > 0 {
		for _, g := range doc.subcommands {
			if g.group == nil {
				s = append(s, c.bold("SUBCOMMANDS"), cliTable(cliSubRows(g.commands, c), 20))
				if len(doc.subcommands) > 1 {
					s = append(s, "")
				}
				break
			}
		}
		for _, g := range doc.subcommands {
			if g.group != nil {
				s = append(s, c.bold(*g.group+":"), cliTable(cliSubRows(g.commands, c), 20), "")
			}
		}
	}
	if len(doc.examples) > 0 {
		s = append(s, c.bold("EXAMPLES"))
		first, previous := true, false
		for _, e := range doc.examples {
			described := e.description != nil && *e.description != ""
			if described {
				if !first {
					s = append(s, "")
				}
				s = append(s, "  "+c.dim("# "+*e.description))
			} else if previous {
				s = append(s, "")
			}
			s = append(s, "  "+c.cyan(e.command))
			first, previous = false, described
		}
		s = append(s, "")
	}
	if s[len(s)-1] == "" {
		s = s[:len(s)-1]
	}
	return strings.Join(s, "\n")
}

func cliFormatVersion(name, version string, c cliColors) string {
	return c.bold(name) + " " + c.dim("v") + c.bold(version)
}
