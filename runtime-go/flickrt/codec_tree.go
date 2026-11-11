package flickrt

import (
	"math"
	"math/big"
	"regexp"
	"strconv"
	"strings"
)

// String trees: values whose every leaf is a string, as query strings and
// forms carry them. `Codec.string_tree` reads a schema's values from one,
// and bracket paths (`a[b][0]`) build one.

type stringTree struct{ s *Schema }

// CodecStringTree is the string-tree codec of a schema of numbers, strings,
// booleans, BigInts, lists, tuples and records.
//
// shortcut: Unit, Option, Result, Redacted and variant types are refused; their forms are not ones a query string can carry, port them if a program needs one
func CodecStringTree(s Value) Value {
	checkTree(sch(s), map[*Schema]bool{})
	return &fallible{&stringTree{sch(s)}}
}

func checkTree(s *Schema, seen map[*Schema]bool) {
	if seen[s] {
		return
	}
	seen[s] = true
	s = s.resolved()
	switch s.kind {
	case kindNumber, kindInt, kindString, kindBool, kindBigint:
	case kindList, kindRefined:
		checkTree(s.item, seen)
	case kindTuple, kindRecord:
		for _, item := range s.items {
			checkTree(item, seen)
		}
	default:
		panic(Defect{"Codec.string_tree: this schema has no string-tree form on the go target"})
	}
}

func (t *stringTree) accepts(Value) bool { return true }

var finiteText = regexp.MustCompile(`^[+-]?[0-9]*\.?[0-9]+(?:[Ee][+-]?[0-9]+)?$`)

// numberOfText is the number of a string: a finite number's text, or
// `NaN`, `Infinity` or `-Infinity`.
func numberOfText(in Value) (float64, []issue) {
	s, ok := in.(string)
	if !ok {
		return 0, expected("Union", in)
	}
	switch {
	case finiteText.MatchString(s):
		x, _ := strconv.ParseFloat(s, 64)
		return x, nil
	case s == "NaN":
		return math.NaN(), nil
	case s == "Infinity":
		return math.Inf(1), nil
	case s == "-Infinity":
		return math.Inf(-1), nil
	}
	return 0, append(expected("a string representing a finite number", in), expected("Union", in)...)
}

func (t *stringTree) decode(fb *Fiber, in Value) (Value, []issue) { return decodeTree(fb, t.s, in) }
func (t *stringTree) encode(_ *Fiber, v Value) (Value, []issue)   { return encodeTree(t.s, v), nil }

func decodeTree(fb *Fiber, s *Schema, in Value) (Value, []issue) {
	s = s.resolved()
	switch s.kind {
	case kindRefined:
		v, issues := decodeTree(fb, s.item, in)
		if issues == nil {
			issues = s.refine(fb, v)
		}
		return v, issues
	case kindNumber:
		x, issues := numberOfText(in)
		if issues != nil {
			return nil, issues
		}
		return x, nil
	case kindInt:
		x, issues := numberOfText(in)
		if issues != nil {
			return nil, issues
		}
		if x != math.Trunc(x) || x < math.MinInt32 || x > math.MaxInt32 {
			return nil, fail("expected an Int, an integer from -2147483648 to 2147483647")
		}
		return int32(x), nil
	case kindString:
		if _, ok := in.(string); !ok {
			return nil, expected("a string", in)
		}
		return in, nil
	case kindBool:
		if in != "true" && in != "false" {
			return nil, expected("Union", in)
		}
		return in == "true", nil
	case kindBigint:
		text, ok := in.(string)
		if !ok {
			return nil, expected("a string", in)
		}
		if !digitsPattern.MatchString(text) {
			return nil, expected("a string representing a bigint", in)
		}
		n, _ := new(big.Int).SetString(text, 10)
		return n, nil
	case kindList:
		xs, ok := in.([]Value)
		if !ok {
			return nil, expected("a list", in)
		}
		out := make([]Value, len(xs))
		for i, x := range xs {
			v, issues := decodeTree(fb, s.item, x)
			if issues != nil {
				return nil, under(i, issues)
			}
			out[i] = v
		}
		return out, nil
	case kindTuple:
		xs, ok := in.([]Value)
		if !ok {
			return nil, expected("a tuple of "+strconv.Itoa(len(s.items)), in)
		}
		out := make([]Value, len(s.items))
		for i, item := range s.items {
			if i >= len(xs) {
				return nil, under(i, fail("a missing key"))
			}
			v, issues := decodeTree(fb, item, xs[i])
			if issues != nil {
				return nil, under(i, issues)
			}
			out[i] = v
		}
		if len(xs) > len(s.items) {
			return nil, under(len(s.items), fail("an unknown key"))
		}
		return out, nil
	}
	obj, ok := in.(*Record)
	if !ok {
		return nil, expected("a record", in)
	}
	fields, issues := object(fb, obj, s.labels, make([]*Schema, len(s.labels)))
	if issues != nil {
		return nil, issues
	}
	for i, f := range fields {
		v, issues := decodeTree(fb, s.items[i], f)
		if issues != nil {
			return nil, under(s.labels[i], issues)
		}
		fields[i] = v
	}
	return shaped(s.keys, s.pos, fields), nil
}

func encodeTree(s *Schema, v Value) Value {
	s = s.resolved()
	switch s.kind {
	case kindRefined:
		return encodeTree(s.item, v)
	case kindNumber:
		return jsString(v)
	case kindInt:
		return strconv.Itoa(int(v.(int32)))
	case kindBool:
		return strconv.FormatBool(v.(bool))
	case kindBigint:
		return v.(*big.Int).String()
	case kindList, kindTuple:
		xs := List(v)
		out := make([]Value, len(xs))
		for i, x := range xs {
			item := s.item
			if s.kind == kindTuple {
				item = s.items[i]
			}
			out[i] = encodeTree(item, x)
		}
		return out
	case kindRecord:
		r := v.(*Record)
		out := make([]Value, len(s.labels))
		for i, l := range s.labels {
			out[i] = encodeTree(s.items[i], r.Get(l))
		}
		return shaped(s.keys, s.pos, out)
	}
	return v
}

// A tree being built from bracket paths: objects are records filled in
// place, a key given twice holds a growing list, and an object reached by
// indices becomes an array once every path is read.
type treeList struct{ vals []Value }

type treeIndex struct {
	parent *Record
	key    string
	length int64
}

// bracketTokens is the keys of a bracket path: `a[b][0]` is a, b and the
// index 0; `a[]` ends in an empty key, the next index.
func bracketTokens(path string) []any {
	if path == "" {
		return []any{""}
	}
	replaced := bracketPart.ReplaceAllString(path, ".$1")
	parts := strings.Split(replaced, ".")
	if strings.HasPrefix(replaced, ".") {
		parts = parts[1:]
	}
	out := make([]any, len(parts))
	for i, p := range parts {
		out[i] = p
		if n, err := strconv.ParseInt(p, 10, 64); err == nil && n >= 0 && n < 1<<32-1 && strconv.FormatInt(n, 10) == p {
			out[i] = n
		}
	}
	return out
}

var bracketPart = regexp.MustCompile(`\[(.*?)\]`)

// maxArrayHoles is how many missing indices an array may have; past that
// it stays an object.
const maxArrayHoles = 20

// treeRecord is the tree of the pairs of bracket path and value.
func treeRecord(pairs []Value) Value {
	root := &Record{}
	containers := map[*Record]bool{}
	indexed := map[*Record]*treeIndex{}
	var order []*Record
	for _, p := range pairs {
		path, val := pairOf(p)
		tokens := bracketTokens(path)
		cur := root
		for i, t := range tokens {
			state := indexed[cur]
			token := t
			if t == "" && state != nil {
				token = state.length
			}
			if n, ok := token.(int64); ok && state != nil {
				state.length = max(state.length, n+1)
			}
			key := keyOf(token)
			at := cur.index(key)
			var current Value
			if at >= 0 {
				current = cur.vals[at]
			}
			if i == len(tokens)-1 {
				child, _ := current.(*Record)
				switch list, isList := current.(*treeList); {
				case child != nil && indexed[child] != nil:
					child.put(strconv.FormatInt(indexed[child].length, 10), val)
					indexed[child].length++
				case isList:
					list.vals = append(list.vals, val)
				case at >= 0:
					cur.put(key, &treeList{[]Value{current, val}})
				default:
					cur.put(key, val)
				}
				continue
			}
			next := tokens[i+1]
			_, nextIndex := next.(int64)
			array := nextIndex || next == ""
			if child, ok := current.(*Record); ok && containers[child] && (indexed[child] != nil) == array {
				cur = child
				continue
			}
			container := &Record{}
			containers[container] = true
			if array {
				indexed[container] = &treeIndex{cur, key, 0}
				order = append(order, container)
			}
			cur.put(key, container)
			cur = container
		}
	}
	arrays := map[*Record]int64{}
	for _, b := range order {
		st := indexed[b]
		if i := st.parent.index(st.key); i >= 0 && st.parent.vals[i] == Value(b) && st.length-int64(len(b.keys)) <= maxArrayHoles {
			arrays[b] = st.length
		}
	}
	return finishTree(root, arrays)
}

func keyOf(token any) string {
	if n, ok := token.(int64); ok {
		return strconv.FormatInt(n, 10)
	}
	return token.(string)
}

// finishTree is the built tree as values: a missing index is `undefined`.
func finishTree(v Value, arrays map[*Record]int64) Value {
	switch x := v.(type) {
	case *treeList:
		out := make([]Value, len(x.vals))
		for i, e := range x.vals {
			out[i] = finishTree(e, arrays)
		}
		return out
	case *Record:
		if n, ok := arrays[x]; ok {
			out := make([]Value, n)
			for i := range out {
				out[i] = Unit
			}
			for i, k := range x.keys {
				if at, err := strconv.ParseInt(k, 10, 64); err == nil && at < n {
					out[at] = finishTree(x.vals[i], arrays)
				}
			}
			return out
		}
		out := &Record{x.keys, make([]Value, len(x.vals))}
		for i, e := range x.vals {
			out.vals[i] = finishTree(e, arrays)
		}
		return out
	}
	return v
}

// bracketEntries is the pairs of bracket path and string of a tree: an
// array of strings repeats its key, other arrays and objects nest.
func bracketEntries(tree Value) []Value {
	out := []Value{}
	var add func(key string, v Value)
	add = func(key string, v Value) {
		switch x := v.(type) {
		case string:
			out = append(out, []Value{key, x})
		case []Value:
			leaves := true
			for _, e := range x {
				_, ok := e.(string)
				leaves = leaves && ok
			}
			for i, e := range x {
				if leaves {
					out = append(out, []Value{key, e})
				} else {
					add(key+"["+strconv.Itoa(i)+"]", e)
				}
			}
		case *Record:
			for i, k := range x.keys {
				add(key+"["+k+"]", x.vals[i])
			}
		}
	}
	if r, ok := tree.(*Record); ok {
		for i, k := range r.keys {
			add(k, r.vals[i])
		}
	}
	return out
}
