package flickrt

import (
	"math/big"
	"strconv"
	"strings"
)

// The GraphViz and Mermaid texts of a graph.

// labelText is JS `String(value)` of a Flick value, a label's default.
func labelText(v Value) string {
	switch x := v.(type) {
	case *big.Int:
		return x.String()
	case UnitType:
		return "undefined"
	case *Data, *Record:
		return "[object Object]"
	case []Value:
		parts := make([]string, len(x))
		for i, e := range x {
			parts[i] = labelText(e)
		}
		return strings.Join(parts, ",")
	}
	return jsString(v)
}

// label is the text of an optional label function, `String` by default.
func labelOf(fb *Fiber, f Value) func(Value) string {
	if l, ok := some(f); ok {
		return func(x Value) string { return Apply(fb, l, x).(string) }
	}
	return labelText
}

var graphVizEscape = strings.NewReplacer(`\`, `\\`, `"`, `\"`, "\r\n", `\n`, "\r", `\n`, "\n", `\n`)

func GraphToGraphViz(fb *Fiber, options, v Value) Value {
	g, o := graphOf(v), record(options)
	nodeLabel, edgeLabel := labelOf(fb, o.Get("nodeLabel")), labelOf(fb, o.Get("edgeLabel"))
	name := "G"
	if n, ok := some(o.Get("graphName")); ok {
		name = n.(string)
	}
	kind, operator := "graph", "--"
	if g.kind == "directed" {
		kind, operator = "digraph", "->"
	}
	lines := []string{kind + ` "` + graphVizEscape.Replace(name) + `" {`}
	for _, n := range g.nodes {
		lines = append(lines, `  "`+strconv.Itoa(n.index)+`" [label="`+graphVizEscape.Replace(nodeLabel(n.data))+`"];`)
	}
	for _, e := range g.edges {
		lines = append(lines, `  "`+strconv.Itoa(e.source)+`" `+operator+` "`+strconv.Itoa(e.target)+
			`" [label="`+graphVizEscape.Replace(edgeLabel(e.data))+`"];`)
	}
	return strings.Join(append(lines, "}"), "\n")
}

// mermaidEscape escapes a Mermaid label: its replacements run one after
// another, so the `#` of an earlier one is not escaped again.
func mermaidEscape(s string) string {
	for _, r := range [][2]string{
		{"#", "#35;"}, {`"`, "#quot;"}, {"<", "#lt;"}, {">", "#gt;"}, {"&", "#amp;"},
		{"[", "#91;"}, {"]", "#93;"}, {"{", "#123;"}, {"}", "#125;"}, {"(", "#40;"},
		{")", "#41;"}, {"|", "#124;"}, {`\`, "#92;"},
	} {
		s = strings.ReplaceAll(s, r[0], r[1])
	}
	return strings.NewReplacer("\r\n", "<br/>", "\r", "<br/>", "\n", "<br/>").Replace(s)
}

var mermaidShapes = map[string][2]string{
	"rounded":     {"(", ")"},
	"circle":      {"((", "))"},
	"diamond":     {"{", "}"},
	"hexagon":     {"{{", "}}"},
	"stadium":     {"([", "])"},
	"subroutine":  {"[[", "]]"},
	"cylindrical": {"[(", ")]"},
}

func GraphToMermaid(fb *Fiber, options, v Value) Value {
	g, o := graphOf(v), record(options)
	nodeLabel, edgeLabel := labelOf(fb, o.Get("nodeLabel")), labelOf(fb, o.Get("edgeLabel"))
	diagram := "graph"
	if g.kind == "directed" {
		diagram = "flowchart"
	}
	if d, ok := some(o.Get("diagramType")); ok {
		diagram = d.(string)
	}
	direction := "TD"
	if d, ok := some(o.Get("direction")); ok {
		direction = d.(string)
	}
	shape := func(Value) string { return "rectangle" }
	if f, ok := some(o.Get("nodeShape")); ok {
		shape = func(x Value) string { return Apply(fb, f, x).(string) }
	}
	lines := []string{diagram + " " + direction}
	for _, n := range g.nodes {
		text := mermaidEscape(nodeLabel(n.data))
		brackets, ok := mermaidShapes[shape(n.data)]
		if !ok {
			brackets = [2]string{"[", "]"}
		}
		lines = append(lines, "  "+strconv.Itoa(n.index)+brackets[0]+`"`+text+`"`+brackets[1])
	}
	operator := "---"
	if diagram == "flowchart" {
		operator = "-->"
	}
	for _, e := range g.edges {
		source, target := strconv.Itoa(e.source), strconv.Itoa(e.target)
		if text := mermaidEscape(edgeLabel(e.data)); text != "" {
			lines = append(lines, "  "+source+" "+operator+`|"`+text+`"| `+target)
		} else {
			lines = append(lines, "  "+source+" "+operator+" "+target)
		}
	}
	return strings.Join(lines, "\n")
}
