package flickrt

import (
	"math"
	"math/big"
	"regexp"
	"strconv"
	"strings"
)

// What `Config.schema` decodes from a provider, as Effect's string-tree
// codecs over a provider's nodes do, with the messages of Effect's
// `SchemaError`: decoding stops at the first failing element or field,
// and a union that fails reports each of its members.

// A cfgNode is what a provider has at a path: a value, a record of keys
// or an array of a length; a record or an array may have a value too.
type cfgNode struct {
	kind     byte // 'v', 'r' or 'a'
	value    string
	hasValue bool
	keys     []string
	length   int
}

// A cfgIssue is a message about the value at a path below the decoded one.
type cfgIssue struct {
	path    []Value // keys and indices
	message string
}

func cfgIssues(messages ...string) []cfgIssue {
	out := make([]cfgIssue, len(messages))
	for i, m := range messages {
		out[i] = cfgIssue{message: m}
	}
	return out
}

// under is the issues with the segment in front of their paths.
func under(seg Value, issues []cfgIssue) []cfgIssue {
	for i := range issues {
		issues[i].path = append([]Value{seg}, issues[i].path...)
	}
	return issues
}

// formatCfgIssues is Effect's text of the issues: each message, then where
// it is, as `at ["A"][0]`.
func formatCfgIssues(prefix []Value, issues []cfgIssue) string {
	lines := make([]string, len(issues))
	for i, is := range issues {
		path := append(append([]Value{}, prefix...), is.path...)
		lines[i] = is.message
		if len(path) > 0 {
			var at strings.Builder
			for _, seg := range path {
				if n, ok := seg.(int32); ok {
					at.WriteString("[" + strconv.Itoa(int(n)) + "]")
				} else {
					at.WriteString("[" + QuoteJSON(seg.(string)) + "]")
				}
			}
			lines[i] += "\n  at " + at.String()
		}
	}
	return strings.Join(lines, "\n")
}

// A cfgCursor is the node at a path of a provider, read in a fiber.
type cfgCursor struct {
	fb       *Fiber
	provider *configProvider
	path     []Value
	node     *cfgNode
}

func (c cfgCursor) child(seg Value) cfgCursor {
	path := append(append([]Value{}, c.path...), seg)
	return cfgCursor{c.fb, c.provider, path, c.provider.loadOrFail(c.fb, path)}
}

type cfgSchema interface {
	decode(c cfgCursor) (Value, []cfgIssue)
	// decodeText decodes a piece of a value a split gave.
	decodeText(s string) (Value, []cfgIssue)
	// hasInput tells whether the provider has something for the schema, so
	// that its failure is not a missing value.
	hasInput(n *cfgNode) bool
}

// A scalar reads the value of a node, absent when it has none.
type scalar func(text string, present bool) (Value, []cfgIssue)

func (s scalar) decode(c cfgCursor) (Value, []cfgIssue) {
	if c.node == nil || !c.node.hasValue {
		return s("", false)
	}
	return s(c.node.value, true)
}
func (s scalar) decodeText(text string) (Value, []cfgIssue) { return s(text, true) }
func (scalar) hasInput(n *cfgNode) bool                      { return n != nil && n.hasValue }

// literalsText is Effect's expectation of literals: `"a" | "b"`.
func literalsText(literals []string) string {
	quoted := make([]string, len(literals))
	for i, l := range literals {
		quoted[i] = QuoteJSON(l)
	}
	return strings.Join(quoted, " | ")
}

// cfgString is a string, refined by `check` when given.
func cfgString(check func(string) (Value, string)) scalar {
	return func(text string, present bool) (Value, []cfgIssue) {
		if !present {
			return nil, cfgIssues("Expected string")
		}
		if check == nil {
			return text, nil
		}
		v, message := check(text)
		if message != "" {
			return nil, cfgIssues(message)
		}
		return v, nil
	}
}

func cfgLiterals(literals []string, values []Value) scalar {
	return func(text string, present bool) (Value, []cfgIssue) {
		for i, l := range literals {
			if present && text == l {
				return values[i], nil
			}
		}
		return nil, cfgIssues("Expected " + literalsText(literals))
	}
}

var nonFiniteNames = []string{"Infinity", "-Infinity", "NaN"}

// cfgNumber is Effect's `Number` from text: a finite number's text, or the
// names of the others; `int32` refines it as Flick's `Int` does.
func cfgNumber(int32Check bool) scalar {
	return func(text string, present bool) (Value, []cfgIssue) {
		if !present {
			return nil, cfgIssues("Expected string | " + literalsText(nonFiniteNames))
		}
		var x float64
		switch {
		case finiteText.MatchString(text):
			x, _ = strconv.ParseFloat(text, 64)
		case text == "NaN":
			x = math.NaN()
		case text == "Infinity":
			x = math.Inf(1)
		case text == "-Infinity":
			x = math.Inf(-1)
		default:
			return nil, cfgIssues("Expected a string representing a finite number", "Expected "+literalsText(nonFiniteNames))
		}
		if int32Check {
			if x != math.Trunc(x) || x < math.MinInt32 || x > math.MaxInt32 {
				return nil, cfgIssues("expected an Int, an integer from -2147483648 to 2147483647")
			}
			return int32(x), nil
		}
		return x, nil
	}
}

// cfgFinite is Effect's `Finite` from text, refined by `check`.
func cfgFinite(check func(x float64) (Value, string)) scalar {
	return cfgString(func(text string) (Value, string) {
		if !finiteText.MatchString(text) {
			return nil, "Expected a string representing a finite number"
		}
		x, _ := strconv.ParseFloat(text, 64)
		if math.IsInf(x, 0) {
			return nil, "Expected a finite number"
		}
		if check == nil {
			return x, ""
		}
		return check(x)
	})
}

// cfgInt is Effect's `Int`, within the bounds.
func cfgInt(lo, hi float64, asInt32 bool) scalar {
	return cfgFinite(func(x float64) (Value, string) {
		if x != math.Trunc(x) {
			return nil, "Expected an integer"
		}
		if x < lo || x > hi {
			return nil, "Expected a value between " + ShowNumber(lo) + " and " + ShowNumber(hi)
		}
		if asInt32 {
			return int32(x), ""
		}
		return x, ""
	})
}

var cfgBigint = cfgString(func(text string) (Value, string) {
	if !digitsPattern.MatchString(text) {
		return nil, "Expected a string representing a bigint"
	}
	n, _ := new(big.Int).SetString(text, 10)
	return n, ""
})

// An array of a provider's array node.
type cfgArray struct{ item cfgSchema }

func (a cfgArray) decode(c cfgCursor) (Value, []cfgIssue) {
	if c.node == nil || c.node.kind != 'a' {
		return nil, cfgIssues("Expected array")
	}
	out := make([]Value, c.node.length)
	for i := range out {
		v, issues := a.item.decode(c.child(int32(i)))
		if issues != nil {
			return nil, under(int32(i), issues)
		}
		out[i] = v
	}
	return out, nil
}
func (cfgArray) decodeText(string) (Value, []cfgIssue) { return nil, cfgIssues("Expected array") }
func (cfgArray) hasInput(n *cfgNode) bool               { return n != nil && n.kind == 'a' }

// A tuple of a provider's array node: no more elements than it has.
type cfgTuple struct{ items []cfgSchema }

func (t cfgTuple) decode(c cfgCursor) (Value, []cfgIssue) {
	if c.node == nil || c.node.kind != 'a' {
		return nil, cfgIssues("Expected array")
	}
	out := make([]Value, len(t.items))
	for i, item := range t.items {
		child := c.child(int32(i))
		if child.node == nil {
			return nil, under(int32(i), cfgIssues("Missing key"))
		}
		v, issues := item.decode(child)
		if issues != nil {
			return nil, under(int32(i), issues)
		}
		out[i] = v
	}
	if c.node.length > len(t.items) {
		return nil, under(int32(len(t.items)), cfgIssues("Expected no excess property"))
	}
	return out, nil
}
func (cfgTuple) decodeText(string) (Value, []cfgIssue) { return nil, cfgIssues("Expected array") }
func (cfgTuple) hasInput(n *cfgNode) bool               { return n != nil && n.kind == 'a' }

// A struct of a provider's record node: its fields by label.
type cfgStruct struct {
	labels []string
	fields []cfgSchema
}

func (s cfgStruct) decode(c cfgCursor) (Value, []cfgIssue) {
	if c.node == nil || c.node.kind != 'r' {
		return nil, cfgIssues("Expected object")
	}
	out := newRecord(len(s.labels))
	for i, label := range s.labels {
		child := c.child(label)
		if child.node == nil {
			return nil, under(label, cfgIssues("Missing key"))
		}
		v, issues := s.fields[i].decode(child)
		if issues != nil {
			return nil, under(label, issues)
		}
		out.put(label, v)
	}
	return out, nil
}
func (cfgStruct) decodeText(string) (Value, []cfgIssue) { return nil, cfgIssues("Expected object") }
func (cfgStruct) hasInput(n *cfgNode) bool               { return n != nil && n.kind == 'r' }

// A record of string keys of a provider's record node, as pairs.
type cfgPairs struct{ value cfgSchema }

func (p cfgPairs) decode(c cfgCursor) (Value, []cfgIssue) {
	if c.node == nil || c.node.kind != 'r' {
		return nil, cfgIssues("Expected object")
	}
	out := []Value{}
	for _, k := range c.node.keys {
		child := c.child(k)
		if child.node == nil {
			continue
		}
		v, issues := p.value.decode(child)
		if issues != nil {
			return nil, under(k, issues)
		}
		out = append(out, []Value{k, v})
	}
	return out, nil
}
func (cfgPairs) decodeText(string) (Value, []cfgIssue) { return nil, cfgIssues("Expected object") }
func (cfgPairs) hasInput(n *cfgNode) bool               { return n != nil && n.kind == 'r' }

// A union is the first of its members that decodes the input.
type cfgUnion []cfgSchema

func (u cfgUnion) decode(c cfgCursor) (Value, []cfgIssue) {
	var all []cfgIssue
	for _, m := range u {
		v, issues := m.decode(c)
		if issues == nil {
			return v, nil
		}
		all = append(all, issues...)
	}
	return nil, all
}
func (u cfgUnion) decodeText(s string) (Value, []cfgIssue) {
	var all []cfgIssue
	for _, m := range u {
		v, issues := m.decodeText(s)
		if issues == nil {
			return v, nil
		}
		all = append(all, issues...)
	}
	return nil, all
}
func (u cfgUnion) hasInput(n *cfgNode) bool {
	for _, m := range u {
		if m.hasInput(n) {
			return true
		}
	}
	return false
}

// cfgSplit is a value split into the elements of an array.
type cfgSplit struct {
	separator string
	item      cfgSchema
}

func (s cfgSplit) decode(c cfgCursor) (Value, []cfgIssue) {
	if c.node == nil || !c.node.hasValue {
		return nil, cfgIssues("Expected string")
	}
	return s.decodeText(c.node.value)
}
func (s cfgSplit) decodeText(text string) (Value, []cfgIssue) {
	parts := []string{}
	if text != "" {
		parts = strings.Split(text, s.separator)
	}
	out := make([]Value, len(parts))
	for i, p := range parts {
		v, issues := s.item.decodeText(p)
		if issues != nil {
			return nil, under(int32(i), issues)
		}
		out[i] = v
	}
	return out, nil
}
func (cfgSplit) hasInput(n *cfgNode) bool { return n != nil && n.hasValue }

// cfgKeyValues is a value split into `key=value` pairs.
type cfgKeyValues struct {
	separator, keyValueSeparator string
	value                        cfgSchema
}

func (s cfgKeyValues) decode(c cfgCursor) (Value, []cfgIssue) {
	if c.node == nil || !c.node.hasValue {
		return nil, cfgIssues("Expected string")
	}
	return s.decodeText(c.node.value)
}
func (s cfgKeyValues) decodeText(text string) (Value, []cfgIssue) {
	out := []Value{}
	seen := map[string]int{}
	for _, pair := range strings.Split(text, s.separator) {
		k, v, ok := strings.Cut(pair, s.keyValueSeparator)
		if !ok {
			continue
		}
		k, v = strings.TrimSpace(k), strings.TrimSpace(v)
		decoded, issues := s.value.decodeText(v)
		if issues != nil {
			return nil, under(k, issues)
		}
		if i, dup := seen[k]; dup {
			out[i] = []Value{k, decoded}
			continue
		}
		seen[k] = len(out)
		out = append(out, []Value{k, decoded})
	}
	return out, nil
}
func (cfgKeyValues) hasInput(n *cfgNode) bool { return n != nil && n.hasValue }

// cfgSchemaOf is the decoder of a Flick schema's string-tree form.
func cfgSchemaOf(s *Schema) cfgSchema {
	s = s.resolved()
	switch s.kind {
	case kindNumber:
		return cfgNumber(false)
	case kindInt:
		return cfgNumber(true)
	case kindString:
		return cfgString(nil)
	case kindBool:
		return cfgLiterals([]string{"true", "false"}, []Value{true, false})
	case kindBigint:
		return cfgBigint
	case kindList:
		return cfgArray{cfgSchemaOf(s.item)}
	case kindTuple:
		items := make([]cfgSchema, len(s.items))
		for i, item := range s.items {
			items[i] = cfgSchemaOf(item)
		}
		return cfgTuple{items}
	case kindRecord:
		fields := make([]cfgSchema, len(s.labels))
		for i := range s.labels {
			fields[i] = cfgSchemaOf(s.items[i])
		}
		return cfgStruct{s.labels, fields}
	}
	// shortcut: options, results, variants and the other opaque schemas are refused, as Effect refuses its declarations; read the forms Effect gives variant types if a program configures one
	panic(Defect{"Error: Config.schema does not support opaque StringTree encodings"})
}

var _ = regexp.MustCompile
