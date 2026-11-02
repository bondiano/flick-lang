package flickrt

import (
	"math"
	"strings"
)

// URLs of a type that derives `Url`, as the JS runtime's route.ts: a
// constructor is a path segment, its fields the segments after it and a
// final record field the query. A parsed value is checked by the type's own
// schema, so refined fields stay valid.

// QueryField is a query parameter: its label, its schema, and whether it
// is an `Option`.
type QueryField struct {
	Label    string
	Schema   Value
	Optional bool
}

// Route is a constructor: its tag, its path segment ("" for the root), its
// segment fields, its query, nil without a record field, and whether its
// last segment is an `Option`, absent from the URL for `None`. A segment is
// a scalar field's schema or the *routeWords of a type's constructors.
type Route struct {
	Tag      string
	Path     string
	Segments []Value
	Query    []QueryField
	Optional bool
}

// routeWords is a segment of constructors without fields: each word with
// its tag.
type routeWords struct{ words, tags []string }

// RouteWords is the segment of the constructors with these words and tags,
// in pairs.
func RouteWords(pairs []string) Value {
	w := &routeWords{}
	for i := 0; i+1 < len(pairs); i += 2 {
		w.words, w.tags = append(w.words, pairs[i]), append(w.tags, pairs[i+1])
	}
	return w
}

// routeSegmentText is the text of a segment's field.
func routeSegmentText(segment, field Value) string {
	w, ok := segment.(*routeWords)
	if !ok {
		return jsString(field)
	}
	for i, tag := range w.tags {
		if tag == field.(*Data).Tag {
			return w.words[i]
		}
	}
	panic(Defect{"TypeError: a constructor of another type"})
}

// routeSegmentField is the field of a segment's text.
func routeSegmentField(fb *Fiber, segment Value, text string) (Value, bool) {
	w, ok := segment.(*routeWords)
	if !ok {
		return coerce(fb, segment, text)
	}
	for i, word := range w.words {
		if word == text {
			return NewData(w.tags[i]), true
		}
	}
	return nil, false
}

// urlCodec is `Url a`, what `Route.print` and `Route.parse` read.
type urlCodec struct {
	schema Value
	routes []Route
}

// RouteCodec is `T$url` of a type that derives `Url`.
func RouteCodec(schema Value, routes []Route) Value { return &urlCodec{schema, routes} }

// coerce is text as a value of `schema`: the text itself, else a number or
// a boolean.
func coerce(fb *Fiber, schema Value, text string) (Value, bool) {
	candidates := []Value{text}
	if Trim(text) != "" {
		if n := jsNumber(text).(float64); !math.IsNaN(n) || text == "NaN" {
			candidates = append(candidates, n)
		}
	}
	if text == "true" || text == "false" {
		candidates = append(candidates, text == "true")
	}
	for _, c := range candidates {
		if v, issues := sch(schema).validate(fb, c); issues == nil {
			return v, true
		}
	}
	return nil, false
}

// RoutePrint is `Route.print url value`: the path and query of `value`. A
// lone surrogate, which has no UTF-8 form, prints as U+FFFD.
func RoutePrint(codec, value Value) Value {
	c, d := codec.(*urlCodec), value.(*Data)
	var route *Route
	for i := range c.routes {
		if c.routes[i].Tag == d.Tag {
			route = &c.routes[i]
		}
	}
	if route == nil {
		panic(Defect{"TypeError: a value of another type"})
	}
	var b strings.Builder
	if route.Path != "" {
		b.WriteString("/" + route.Path)
	}
	for i, s := range route.Segments {
		field := d.Fields[i]
		if route.Optional && i == len(route.Segments)-1 {
			v, ok := some(field)
			if !ok {
				continue
			}
			field = v
		}
		segment, _ := encodeURIComponent(wellFormed(routeSegmentText(s, field)))
		b.WriteString("/" + segment)
	}
	if b.Len() == 0 {
		b.WriteByte('/')
	}
	if route.Query == nil {
		return b.String()
	}
	r := d.Fields[len(route.Segments)].(*Record)
	var pairs []Value
	for _, q := range route.Query {
		field := recordField(r, q.Label)
		if q.Optional {
			v, ok := some(field)
			if !ok {
				continue
			}
			field = v
		}
		pairs = append(pairs, []Value{q.Label, jsString(field)})
	}
	if len(pairs) > 0 {
		b.WriteString("?" + formEncode(pairs))
	}
	return b.String()
}

// RouteParse is `Route.parse url text`: the value whose URL is `text`,
// ignoring its origin and fragment.
func RouteParse(fb *Fiber, codec, text Value) Value {
	c := codec.(*urlCodec)
	u, ok := some(resolveURL(text.(string), "http", "localhost"))
	if !ok {
		return None
	}
	url := u.(webURL)
	// "/" is the root; otherwise each "/" starts a segment, an empty one included
	var parts []string
	if url.path != "/" {
		parts = strings.Split(url.path, "/")[1:]
	}
	for i, p := range parts {
		if s, ok := decodeURIComponent(p); ok {
			parts[i] = s
		}
	}
	params := url.searchParams()
	param := func(label string) (string, bool) {
		for _, kv := range params {
			if k, v := pairOf(kv); k == label {
				return v, true
			}
		}
		return "", false
	}
	for _, route := range c.routes {
		rest := parts
		if route.Path != "" {
			if len(parts) == 0 || parts[0] != route.Path {
				continue
			}
			rest = parts[1:]
		}
		if v, ok := route.parse(fb, rest, param); ok {
			if checked, issues := sch(c.schema).validate(fb, v); issues == nil {
				return Some(checked)
			}
		}
	}
	return None
}

// parse is the value of the route of these segments and query, unchecked.
func (route *Route) parse(fb *Fiber, rest []string, param func(string) (string, bool)) (Value, bool) {
	n := len(route.Segments)
	missing := route.Optional && len(rest) == n-1
	if len(rest) != n && !missing {
		return nil, false
	}
	fields := make([]Value, n, n+1)
	for i, s := range route.Segments {
		last := route.Optional && i == n-1
		if last && missing {
			fields[i] = None
			continue
		}
		v, ok := routeSegmentField(fb, s, rest[i])
		if !ok {
			return nil, false
		}
		if last {
			v = Some(v)
		}
		fields[i] = v
	}
	if route.Query != nil {
		record := make([]Value, 0, 2*len(route.Query))
		for _, q := range route.Query {
			var field Value
			text, ok := param(q.Label)
			if ok {
				field, ok = coerce(fb, q.Schema, text)
			}
			switch {
			case q.Optional && ok:
				field = Some(field)
			case q.Optional:
				field = None
			case !ok:
				return nil, false
			}
			record = append(record, q.Label, field)
		}
		fields = append(fields, Rec(record...))
	}
	return NewData(route.Tag, fields...), true
}
