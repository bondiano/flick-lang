package flickrt

import "strings"

// Http.Headers: a record of lower-case names, in the order JS keeps an
// object's keys; `show` masks the values of secret names.

var (
	plainTag            = Tag("Http.Headers.Redaction.Plain")
	secretTag           = Tag("Http.Headers.Redaction.Secret")
	redactedNamesKey    = "Http.Headers.CurrentRedactedNames"
	defaultRedactedName = []string{"authorization", "cookie", "set-cookie", "x-api-key"}
)

func lowerName(name Value) string { return ToLowerCase(name).(string) }

func headersOf(v Value) *Record { return v.(*Record) }

var HeadersEmpty Value = &Record{}

// HeadersFromInput is headers of pairs, each name lower-cased; a later
// pair wins.
func HeadersFromInput(input Value) Value {
	pairs := List(input)
	out := newRecord(len(pairs))
	for _, p := range pairs {
		kv := p.([]Value)
		out.put(lowerName(kv[0]), kv[1])
	}
	return out
}

func HeadersHas(name, h Value) Value { return headersOf(h).index(lowerName(name)) >= 0 }

func HeadersGet(name, h Value) Value {
	r := headersOf(h)
	if i := r.index(lowerName(name)); i >= 0 {
		return Some(r.vals[i])
	}
	return None
}

func HeadersSet(name, value, h Value) Value { return headersOf(h).set(lowerName(name), value) }

func HeadersSetAll(input, h Value) Value { return HeadersMerge(HeadersFromInput(input), h) }

// HeadersMerge is `h` with the headers of `with` over it.
func HeadersMerge(with, h Value) Value {
	out := headersOf(h)
	w := headersOf(with)
	for i, k := range w.keys {
		out = out.set(k, w.vals[i])
	}
	return out
}

func HeadersRemove(name, h Value) Value { return headersOf(h).without(lowerName(name)) }

func HeadersRemoveMany(names, h Value) Value {
	out := headersOf(h)
	for _, n := range List(names) {
		out = out.without(lowerName(n))
	}
	return out
}

// redacted is whether the header `name` is one of `names`, any case.
func redacted(name string, names []string) bool {
	for _, n := range names {
		if ToLowerCase(n).(string) == name {
			return true
		}
	}
	return false
}

func stringsOf(list Value) []string {
	vs := List(list)
	out := make([]string, len(vs))
	for i, v := range vs {
		out[i] = v.(string)
	}
	return out
}

// HeadersRedact is the pairs of the headers, the values of `names` secret.
func HeadersRedact(names, h Value) Value {
	r, ns := headersOf(h), stringsOf(names)
	out := make([]Value, len(r.keys))
	for i, k := range r.keys {
		v := Value(NewData(plainTag, r.vals[i]))
		if redacted(k, ns) {
			v = NewData(secretTag, RedactedMake(r.vals[i]))
		}
		out[i] = []Value{k, v}
	}
	return out
}

func HeadersIsRedactedName(name, names Value) Value {
	return redacted(lowerName(name), stringsOf(names))
}

// HeadersWithRedactedNames runs `task` with `names` as the secret ones.
func HeadersWithRedactedNames(fb *Fiber, names, task Value) Value {
	return withEnv(fb, redactedNamesKey, names, task)
}

func HeadersToList(h Value) Value {
	r := headersOf(h)
	out := make([]Value, len(r.keys))
	for i, k := range r.keys {
		out[i] = []Value{k, r.vals[i]}
	}
	return out
}

func HeadersToUnknown(h Value) Value { return h }

// HeadersEquivalence is whether both have the same names with the same
// values, in any order.
func HeadersEquivalence(a, b Value) Value {
	x, y := headersOf(a), headersOf(b)
	if len(x.keys) != len(y.keys) {
		return false
	}
	for i, k := range x.keys {
		j := y.index(k)
		if j < 0 || y.vals[j] != x.vals[i] {
			return false
		}
	}
	return true
}

// HeadersShow is `show` of headers, the default secret names masked.
func HeadersShow(h Value) Value {
	return showPairs("Headers", headersOf(h), defaultRedactedName)
}

// showPairs is `Name [("k", "v"), …]` with the values of `secret` masked.
func showPairs(name string, r *Record, secret []string) string {
	var b strings.Builder
	b.WriteString(name + " [")
	for i, k := range r.keys {
		if i > 0 {
			b.WriteString(", ")
		}
		v := r.vals[i].(string)
		if redacted(k, secret) {
			v = "<redacted>"
		}
		b.WriteString("(" + QuoteJSON(k) + ", " + QuoteJSON(v) + ")")
	}
	return b.String() + "]"
}
