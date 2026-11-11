package flickrt

import (
	"strconv"
	"strings"
)

// `JsonPointer` (RFC 6901) and `JsonPatch` (RFC 6902's add, remove and
// replace) over JSON values, and the `Differ` of a codec's JSON form.

func EscapeToken(token Value) Value   { return escapeToken(token.(string)) }
func UnescapeToken(token Value) Value { return unescapeToken(token.(string)) }

func escapeToken(s string) string {
	return strings.ReplaceAll(strings.ReplaceAll(s, "~", "~0"), "/", "~1")
}

func unescapeToken(s string) string {
	return strings.ReplaceAll(strings.ReplaceAll(s, "~1", "/"), "~0", "~")
}

// encodeURI is JS's, over a well-formed string.
func encodeURI(s string) string {
	return percentEncode(s, func(c byte) bool {
		return !(c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || c >= '0' && c <= '9' ||
			strings.IndexByte(";,/?:@&=+$-_.!~*'()#", c) >= 0)
	})
}

func formatUriFragment(path []string) string {
	var b strings.Builder
	b.WriteByte('#')
	for _, token := range path {
		b.WriteString("/" + strings.ReplaceAll(encodeURI(escapeToken(token)), "#", "%23"))
	}
	return b.String()
}

// decodeUriFragment is the JSON Pointer of a URI fragment, false when it is
// not one.
func decodeUriFragment(fragment string) (string, bool) {
	if fragment == "" || fragment == "#" {
		return "", true
	}
	if !strings.HasPrefix(fragment, "#") {
		return "", false
	}
	encoded := fragment[1:]
	if wellFormed(encoded) != encoded ||
		strings.ReplaceAll(strings.ReplaceAll(encodeURI(encoded), "%25", "%"), "#", "%23") != encoded {
		return "", false
	}
	pointer, ok := decodeURIComponent(encoded)
	if !ok || !strings.HasPrefix(pointer, "/") {
		return "", false
	}
	for i := 0; i < len(pointer); i++ {
		if pointer[i] == '~' && (i+1 == len(pointer) || pointer[i+1] != '0' && pointer[i+1] != '1') {
			return "", false
		}
	}
	return pointer, true
}

func parseUriFragment(fragment string) ([]string, bool) {
	pointer, ok := decodeUriFragment(fragment)
	if !ok || pointer == "" {
		return []string{}, ok
	}
	tokens := strings.Split(pointer[1:], "/")
	for i, t := range tokens {
		tokens[i] = unescapeToken(t)
	}
	return tokens, true
}

func ParseUriFragment(fragment Value) Value {
	tokens, ok := parseUriFragment(fragment.(string))
	if !ok {
		return None
	}
	out := make([]Value, len(tokens))
	for i, t := range tokens {
		out[i] = t
	}
	return Some(out)
}

func FormatUriFragment(path Value) Value {
	tokens := make([]string, len(List(path)))
	for i, t := range List(path) {
		tokens[i] = t.(string)
	}
	return formatUriFragment(tokens)
}

// patchOp is a JSON Patch operation; `value` is nil for a remove.
type patchOp struct {
	op, path string
	value    Value
}

func (o patchOp) json() Value {
	if o.op == "remove" {
		return Rec("op", o.op, "path", o.path)
	}
	return Rec("op", o.op, "path", o.path, "value", o.value)
}

// jsonPatchGet is `JsonPatch.get`: arrays index by index, objects key by
// sorted key, anything else replaced whole.
func jsonPatchGet(old, new Value, path string, out []patchOp) []patchOp {
	switch o := old.(type) {
	case []Value:
		if n, ok := new.([]Value); ok {
			shared := min(len(o), len(n))
			for i := 0; i < shared; i++ {
				out = jsonPatchGet(o[i], n[i], path+"/"+strconv.Itoa(i), out)
			}
			for i := len(o) - 1; i >= len(n); i-- {
				out = append(out, patchOp{"remove", path + "/" + strconv.Itoa(i), nil})
			}
			for i := len(o); i < len(n); i++ {
				out = append(out, patchOp{"add", path + "/" + strconv.Itoa(i), n[i]})
			}
			return out
		}
	case *Record:
		if n, ok := new.(*Record); ok {
			if o == n {
				return out
			}
			all := &Record{}
			for _, k := range o.keys {
				all.put(k, nil)
			}
			for _, k := range n.keys {
				all.put(k, nil)
			}
			for _, k := range sortedKeys(all) {
				at := path + "/" + escapeToken(k)
				i, j := o.index(k), n.index(k)
				switch {
				case i >= 0 && j >= 0:
					out = jsonPatchGet(o.vals[i], n.vals[j], at, out)
				case j >= 0:
					out = append(out, patchOp{"add", at, n.vals[j]})
				default:
					out = append(out, patchOp{"remove", at, nil})
				}
			}
			return out
		}
	case float64:
		if n, ok := new.(float64); ok && NumberSameValue(o, n).(bool) {
			return out
		}
	case string, bool:
		if old == new {
			return out
		}
	default:
		if old == JsonNull && new == JsonNull {
			return out
		}
	}
	return append(out, patchOp{"replace", path, new})
}

// patchError is the message of an operation that cannot apply.
type patchError string

func patchFail(format string, args ...string) {
	for _, a := range args {
		format = strings.Replace(format, "%s", a, 1)
	}
	panic(patchError(format))
}

func arrayIndexOf(token string) int {
	if token == "0" || token != "" && token[0] >= '1' && token[0] <= '9' && strings.Trim(token, "0123456789") == "" {
		if n, err := strconv.Atoi(token); err == nil {
			return n
		}
		return int(^uint(0) >> 1)
	}
	patchFail(`Invalid array index: "%s"`, token)
	return 0
}

// jsonPatchApply is `JsonPatch.apply`; an operation that cannot apply
// panics with a patchError.
func jsonPatchApply(ops []patchOp, doc Value) Value {
	for _, op := range ops {
		doc = applyPatchOp(doc, op)
	}
	return doc
}

func applyPatchOp(doc Value, op patchOp) Value {
	if op.path == "" {
		if op.op == "remove" {
			patchFail("Unsupported operation at the root")
		}
		return op.value
	}
	if op.path[0] != '/' {
		patchFail(`Invalid JSON Pointer, it must start with "/": %s`, JsonStringify(op.path).(string))
	}
	tokens := strings.Split(op.path[1:], "/")
	for i, t := range tokens {
		tokens[i] = unescapeToken(t)
	}
	notFound := `Cannot %s at "%s" (parent not found or not a container).`
	last := tokens[len(tokens)-1]
	var stack []Value
	cur := doc
	for _, token := range tokens[:len(tokens)-1] {
		switch c := cur.(type) {
		case []Value:
			i := arrayIndexOf(token)
			if i >= len(c) {
				patchFail(notFound, op.op, op.path)
			}
			stack, cur = append(stack, c, i), c[i]
			continue
		case *Record:
			if i := c.index(token); i >= 0 {
				stack, cur = append(stack, c, token), c.vals[i]
				continue
			}
		}
		patchFail(notFound, op.op, op.path)
	}
	var updated Value
	switch parent := cur.(type) {
	case []Value:
		if last == "-" && op.op != "add" {
			patchFail(`"-" is not valid for %s at "%s".`, op.op, op.path)
		}
		i := len(parent)
		if last != "-" {
			i = arrayIndexOf(last)
		}
		limit := len(parent) - 1
		if op.op == "add" {
			limit = len(parent)
		}
		if i > limit {
			patchFail(`Array index out of bounds at "%s".`, op.path)
		}
		out := append([]Value{}, parent[:i]...)
		switch op.op {
		case "add":
			out = append(append(out, op.value), parent[i:]...)
		case "remove":
			out = append(out, parent[i+1:]...)
		default:
			out = append(append(out, op.value), parent[i+1:]...)
		}
		updated = out
	case *Record:
		if op.op != "add" && parent.index(last) < 0 {
			patchFail(`Property "%s" does not exist at "%s".`, last, op.path)
		}
		if op.op == "remove" {
			updated = parent.without(last)
		} else {
			updated = parent.set(last, op.value)
		}
	default:
		patchFail(notFound, op.op, op.path)
	}
	for i := len(stack) - 2; i >= 0; i -= 2 {
		if c, ok := stack[i].([]Value); ok {
			c = append([]Value{}, c...)
			c[stack[i+1].(int)] = updated
			updated = c
		} else {
			updated = stack[i].(*Record).set(stack[i+1].(string), updated)
		}
	}
	return updated
}

// tryPatch is the patched value, or the message of the operation that
// cannot apply.
func tryPatch(ops []patchOp, doc Value) (out Value, failure string) {
	defer func() {
		if r := recover(); r != nil {
			e, ok := r.(patchError)
			if !ok {
				panic(r)
			}
			failure = string(e)
		}
	}()
	return jsonPatchApply(ops, doc), ""
}

var (
	patchAddTag     = Tag("JsonPatch.Operation.Add")
	patchRemoveTag  = Tag("JsonPatch.Operation.Remove")
	patchReplaceTag = Tag("JsonPatch.Operation.Replace")
	patchErrorTag   = Tag("JsonPatch.PatchError.PatchError")
)

// JsonPatchGet is `JsonPatch.get` as `JsonPatch.Operation`s.
func JsonPatchGet(old, new Value) Value {
	ops := jsonPatchGet(old, new, "", nil)
	out := make([]Value, len(ops))
	for i, op := range ops {
		switch op.op {
		case "remove":
			out[i] = NewData(patchRemoveTag, op.path)
		case "add":
			out[i] = NewData(patchAddTag, op.path, op.value)
		default:
			out[i] = NewData(patchReplaceTag, op.path, op.value)
		}
	}
	return out
}

// JsonPatchApply is `JsonPatch.apply` of `JsonPatch.Operation`s, a
// PatchError where one cannot apply.
func JsonPatchApply(patch, doc Value) Value {
	ops := make([]patchOp, len(List(patch)))
	for i, v := range List(patch) {
		d := v.(*Data)
		switch d.Tag {
		case patchRemoveTag:
			ops[i] = patchOp{"remove", d.Fields[0].(string), nil}
		case patchAddTag:
			ops[i] = patchOp{"add", d.Fields[0].(string), d.Fields[1]}
		default:
			ops[i] = patchOp{"replace", d.Fields[0].(string), d.Fields[1]}
		}
	}
	out, failure := tryPatch(ops, doc)
	if failure != "" {
		return Err(NewData(patchErrorTag, failure))
	}
	return Ok(out)
}

// differ is a `Differ`: patches `p` of values `a`.
type differ struct {
	empty   Value
	diff    func(fb *Fiber, old, new Value) Value
	combine func(first, second Value) Value
	patch   func(fb *Fiber, old, patch Value) Value
}

func DifferEmpty(d Value) Value                        { return d.(*differ).empty }
func DifferDiff(fb *Fiber, old, new, d Value) Value    { return d.(*differ).diff(fb, old, new) }
func DifferCombine(first, second, d Value) Value       { return d.(*differ).combine(first, second) }
func DifferPatch(fb *Fiber, old, patch, d Value) Value { return d.(*differ).patch(fb, old, patch) }

// DifferJsonPatch is `Schema.toDifferJsonPatch`: JSON Patches of the
// codec's JSON form; a value the codec refuses is a defect.
func DifferJsonPatch(c Value) Value {
	cd := codecOf(c)
	encode := func(fb *Fiber, v Value) Value {
		out, issues := cd.encode(fb, v)
		if issues != nil {
			Die(issueText(issues))
		}
		return out
	}
	return &differ{
		empty: []Value{},
		diff: func(fb *Fiber, old, new Value) Value {
			ops := jsonPatchGet(encode(fb, old), encode(fb, new), "", nil)
			out := make([]Value, len(ops))
			for i, op := range ops {
				out[i] = op.json()
			}
			return out
		},
		combine: func(first, second Value) Value { return ListConcat(first, second) },
		patch: func(fb *Fiber, old, patch Value) Value {
			if len(List(patch)) == 0 {
				return old
			}
			ops := make([]patchOp, len(List(patch)))
			for i, v := range List(patch) {
				r := record(v)
				op := patchOp{r.Get("op").(string), r.Get("path").(string), JsonNull}
				if j := r.index("value"); j >= 0 {
					op.value = r.vals[j]
				}
				ops[i] = op
			}
			patched, failure := tryPatch(ops, encode(fb, old))
			if failure != "" {
				Die("Error: " + failure)
			}
			out, issues := cd.decode(fb, patched)
			if issues != nil {
				Die(issueText(issues))
			}
			return out
		},
	}
}
