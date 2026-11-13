//go:build flickcli

package flickrt

import (
	"strings"
)

// Effect's help and version texts (`CliOutput.js`), and `Cli.HelpDoc` and
// `Cli.Formatter` as `output.ts` converts them: absent text is `None`.

// cliFormatter is how help and versions print.
type cliFormatter struct {
	helpDoc func(doc *cliHelpDoc) string
	version func(name, version string) string
}

type cliColors struct{ bold, dim, cyan, green func(string) string }

func cliPaint(on bool, code string) func(string) string {
	if !on {
		return func(s string) string { return s }
	}
	return func(s string) string { return "\x1b[" + code + "m" + s + "\x1b[0m" }
}

// cliDefaultFormatterOf is Effect's `defaultFormatter`.
func cliDefaultFormatterOf(colors bool) *cliFormatter {
	c := cliColors{cliPaint(colors, "1"), cliPaint(colors, "2"), cliPaint(colors, "36"), cliPaint(colors, "32")}
	return &cliFormatter{
		helpDoc: func(doc *cliHelpDoc) string { return cliFormatHelp(doc, c) },
		version: func(name, version string) string { return c.bold(name) + " " + c.dim("v") + c.bold(version) },
	}
}

type cliRow struct{ left, right string }

func cliRenderTable(rows []cliRow, cap int) string {
	widest := 0
	for _, r := range rows {
		widest = max(widest, cliWidth(r.left))
	}
	col := widest + 4
	if cap > 0 {
		col = min(col, cap)
	}
	lines := make([]string, len(rows))
	for i, r := range rows {
		width := max(col, cliWidth(r.left)+1)
		lines[i] = "  " + r.left + strings.Repeat(" ", max(0, width-cliWidth(r.left))) + r.right
	}
	return strings.Join(lines, "\n")
}

func cliOr(s *string, otherwise string) string {
	if s == nil {
		return otherwise
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

func cliSubRows(g cliSubGroupDoc, c cliColors) []cliRow {
	rows := make([]cliRow, len(g.commands))
	for i, s := range g.commands {
		name := s.name
		if s.alias != nil && *s.alias != "" {
			name += ", " + *s.alias
		}
		rows[i] = cliRow{c.cyan(name), cliOr(s.shortDescription, s.description)}
	}
	return rows
}

func cliFormatHelp(doc *cliHelpDoc, c cliColors) string {
	var out []string
	if doc.description != "" {
		out = append(out, c.bold("DESCRIPTION"), "  "+doc.description, "")
	}
	out = append(out, c.bold("USAGE"), "  "+c.cyan(doc.usage), "")
	if len(doc.args) > 0 {
		rows := make([]cliRow, len(doc.args))
		for i, a := range doc.args {
			name := a.name
			if a.variadic {
				name += "..."
			}
			optional := ""
			if !a.required {
				optional = c.dim(" (optional)")
			}
			rows[i] = cliRow{c.green(name) + " " + c.dim(a.typ), cliOr(a.description, "") + optional}
		}
		out = append(out, c.bold("ARGUMENTS"), cliRenderTable(rows, 25), "")
	}
	if len(doc.flags) > 0 {
		out = append(out, c.bold("FLAGS"), cliRenderTable(cliFlagRows(doc.flags, c), 0), "")
	}
	if len(doc.globalFlags) > 0 {
		out = append(out, c.bold("GLOBAL FLAGS"), cliRenderTable(cliFlagRows(doc.globalFlags, c), 0), "")
	}
	if len(doc.subcommands) > 0 {
		for _, g := range doc.subcommands {
			if g.group == nil {
				out = append(out, c.bold("SUBCOMMANDS"), cliRenderTable(cliSubRows(g, c), 20))
				if len(doc.subcommands) > 1 {
					out = append(out, "")
				}
				break
			}
		}
		for _, g := range doc.subcommands {
			if g.group != nil {
				out = append(out, c.bold(*g.group+":"), cliRenderTable(cliSubRows(g, c), 20), "")
			}
		}
	}
	if len(doc.examples) > 0 {
		out = append(out, c.bold("EXAMPLES"))
		first, previousHad := true, false
		for _, e := range doc.examples {
			has := e.description != nil && *e.description != ""
			if has {
				if !first {
					out = append(out, "")
				}
				out = append(out, "  "+c.dim("# "+*e.description))
			} else if previousHad {
				out = append(out, "")
			}
			out = append(out, "  "+c.cyan(e.command))
			first, previousHad = false, has
		}
		out = append(out, "")
	}
	if len(out) > 0 && out[len(out)-1] == "" {
		out = out[:len(out)-1]
	}
	return strings.Join(out, "\n")
}

func cliOption(s *string) Value {
	if s == nil {
		return None
	}
	return Some(*s)
}

func cliOptional(v Value) *string {
	if s, ok := some(v); ok {
		text := s.(string)
		return &text
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

func cliFlagDocValues(flags []cliFlagDoc) Value {
	out := make([]Value, len(flags))
	for i, f := range flags {
		out[i] = Rec("name", f.name, "aliases", cliStringList(f.aliases), "type_", f.typ,
			"description", cliOption(f.description), "required", f.required)
	}
	return out
}

// cliHelpDocValue is the `Cli.HelpDoc` of Effect's document.
func cliHelpDocValue(doc *cliHelpDoc) Value {
	args := make([]Value, len(doc.args))
	for i, a := range doc.args {
		args[i] = Rec("name", a.name, "type_", a.typ, "description", cliOption(a.description),
			"required", a.required, "variadic", a.variadic)
	}
	groups := make([]Value, len(doc.subcommands))
	for i, g := range doc.subcommands {
		commands := make([]Value, len(g.commands))
		for j, s := range g.commands {
			commands[j] = Rec("name", s.name, "alias", cliOption(s.alias),
				"shortDescription", cliOption(s.shortDescription), "description", s.description)
		}
		groups[i] = Rec("group", cliOption(g.group), "commands", commands)
	}
	examples := make([]Value, len(doc.examples))
	for i, e := range doc.examples {
		examples[i] = Rec("command", e.command, "description", cliOption(e.description))
	}
	return Rec("description", doc.description, "usage", doc.usage, "flags", cliFlagDocValues(doc.flags),
		"globalFlags", cliFlagDocValues(doc.globalFlags), "args", args, "subcommands", groups, "examples", examples)
}

func cliFlagDocsOf(v Value) []cliFlagDoc {
	var out []cliFlagDoc
	for _, f := range List(v) {
		r := record(f)
		out = append(out, cliFlagDoc{r.Get("name").(string), cliStrings(r.Get("aliases")), r.Get("type_").(string),
			cliOptional(r.Get("description")), r.Get("required").(bool)})
	}
	return out
}

// cliHelpDocOf is Effect's document of a `Cli.HelpDoc`.
func cliHelpDocOf(v Value) *cliHelpDoc {
	r := record(v)
	doc := &cliHelpDoc{description: r.Get("description").(string), usage: r.Get("usage").(string),
		flags: cliFlagDocsOf(r.Get("flags")), globalFlags: cliFlagDocsOf(r.Get("globalFlags"))}
	for _, a := range List(r.Get("args")) {
		x := record(a)
		doc.args = append(doc.args, cliArgDoc{x.Get("name").(string), x.Get("type_").(string),
			cliOptional(x.Get("description")), x.Get("required").(bool), x.Get("variadic").(bool)})
	}
	for _, g := range List(r.Get("subcommands")) {
		x := record(g)
		group := cliSubGroupDoc{group: cliOptional(x.Get("group"))}
		for _, s := range List(x.Get("commands")) {
			y := record(s)
			group.commands = append(group.commands, cliSubDoc{y.Get("name").(string), cliOptional(y.Get("alias")),
				cliOptional(y.Get("shortDescription")), y.Get("description").(string)})
		}
		doc.subcommands = append(doc.subcommands, group)
	}
	for _, e := range List(r.Get("examples")) {
		x := record(e)
		doc.examples = append(doc.examples, cliExampleDoc{x.Get("command").(string), cliOptional(x.Get("description"))})
	}
	return doc
}

// cliFlickFormatter is Effect's formatter of a `Cli.Formatter`.
func cliFlickFormatter(f Value) *cliFormatter {
	r := record(f)
	return &cliFormatter{
		helpDoc: func(doc *cliHelpDoc) string { return Apply(r.Get("formatHelpDoc"), cliHelpDocValue(doc)).(string) },
		version: func(name, version string) string {
			return Apply(Apply(r.Get("formatVersion"), name), version).(string)
		},
	}
}

func CliDefaultFormatter(colors Value) Value {
	f := cliDefaultFormatterOf(colors.(bool))
	return Rec(
		"formatHelpDoc", Fn(func(doc Value) Value { return f.helpDoc(cliHelpDocOf(doc)) }),
		"formatVersion", Fn(func(name Value) Value {
			return Fn(func(version Value) Value { return f.version(name.(string), version.(string)) })
		}),
	)
}

func CliDisplayWidth(text Value) Value { return int32(cliWidth(text.(string))) }
