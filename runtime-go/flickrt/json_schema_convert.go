package flickrt

import (
	"slices"
	"strconv"
	"strings"
)

// The kernel of the `JsonSchema` dialect conversions: a walk over the
// schemas an adapter names, which records where each source schema lands
// in the target so a `$ref` is relocated once the walk is done. Schemas
// are JSON values; an object being built is a *Record mutated in place
// until it is returned.

const (
	metaSchemaDraft04     = "http://json-schema.org/draft-04/schema#"
	metaSchemaDraft07     = "http://json-schema.org/draft-07/schema#"
	metaSchemaDraft202012 = "https://json-schema.org/draft/2020-12/schema"
	metaSchemaOpenApi31   = "https://spec.openapis.org/oas/3.1/dialect/base"
	rootResourceURI       = "https://effect.invalid/.json-schema/"
)

var (
	MetaSchemaUriDraft04     Value = metaSchemaDraft04
	MetaSchemaUriDraft07     Value = metaSchemaDraft07
	MetaSchemaUriDraft202012 Value = metaSchemaDraft202012
)

// lookup is the value of `k`, nil where JS has undefined.
func lookup(r *Record, k string) Value {
	if i := r.index(k); i >= 0 {
		return r.vals[i]
	}
	return nil
}

func has(r *Record, k string) bool { return r.index(k) >= 0 }

// remove deletes `k` from a record still being built.
func (r *Record) remove(k string) {
	if i := r.index(k); i >= 0 {
		r.keys = append(r.keys[:i:i], r.keys[i+1:]...)
		r.vals = append(r.vals[:i:i], r.vals[i+1:]...)
	}
}

// unsupportedKeyword is the error of a keyword the target cannot hold; it
// is a defect.
func unsupportedKeyword(keyword, dialect, details string) {
	Die("Error: Cannot convert JSON Schema keyword \"" + keyword + "\" to " + dialect + ": " + details)
}

func rejectKeywordCollisions(source *Record, keywords []string, target, from string) {
	for _, k := range keywords {
		if has(source, k) {
			unsupportedKeyword(k, target, "it is not active in "+from+" but would become active in the target")
		}
	}
}

func isMetaSchemaURI(v Value, uri string) bool {
	s, ok := v.(string)
	if !ok {
		return false
	}
	if strings.HasSuffix(uri, "#") {
		return s == uri || s == uri[:len(uri)-1]
	}
	return s == uri || s == uri+"#"
}

func createsResource(id Value) bool {
	s, ok := id.(string)
	return ok && s != "" && s[0] != '#'
}

// mapObject is the object of `f` of each value, nil when `v` is not one.
func mapObject(v Value, f func(item Value, key string) Value) *Record {
	r, ok := v.(*Record)
	if !ok {
		return nil
	}
	out := newRecord(len(r.keys))
	for i, k := range r.keys {
		out.put(k, f(r.vals[i], k))
	}
	return out
}

// appendAllOf adds a schema to `allOf`, which an adapter built itself.
func appendAllOf(out *Record, schema Value) {
	if xs, ok := lookup(out, "allOf").([]Value); ok {
		out.put("allOf", append(xs, schema))
	} else {
		out.put("allOf", []Value{schema})
	}
}

// path is a JSON Pointer as its tokens; extend copies, as paths are shared.
func extend(p []string, tokens ...string) []string {
	return append(append(make([]string, 0, len(p)+len(tokens)), p...), tokens...)
}

// resolveReference is `new URL(ref, base)`, false where JS's `URL.canParse` is.
//
// shortcut: a relative reference against a base without an authority (an opaque or host-less one) does not resolve; the bases in use are http(s) URLs, so handle those if a schema's $id needs them
func resolveReference(ref, base string) (*webURL, bool) {
	if u, ok := parseURL(ref); ok {
		return u, true
	}
	b, ok := parseURL(base)
	if !ok || b.opaque || !b.hasHost {
		return nil, false
	}
	ref = withoutTabs(strings.TrimFunc(ref, func(r rune) bool { return r <= ' ' }))
	origin := *b
	origin.path, origin.hasQuery, origin.query, origin.hasFragment, origin.fragment = "", false, "", false, ""
	noFragment := *b
	noFragment.hasFragment, noFragment.fragment = false, ""
	switch {
	case strings.HasPrefix(ref, "//") || isSpecial(b.scheme) && len(ref) > 1 && isSlash(ref[0]) && isSlash(ref[1]):
		return parseURL(b.scheme + ":" + ref)
	case ref != "" && (ref[0] == '/' || isSpecial(b.scheme) && ref[0] == '\\'):
		return parseURL(origin.href() + ref)
	case ref == "" || ref[0] == '#':
		return parseURL(noFragment.href() + ref)
	case ref[0] == '?':
		noFragment.hasQuery, noFragment.query = false, ""
		return parseURL(noFragment.href() + ref)
	}
	return parseURL(origin.href() + b.path[:strings.LastIndexByte(b.path, '/')+1] + ref)
}

func resolveResourceURI(id Value, base string) (string, bool) {
	s, ok := id.(string)
	if !ok {
		return "", false
	}
	u, ok := resolveReference(s, base)
	if !ok {
		return "", false
	}
	u.hasFragment, u.fragment = false, ""
	return u.href(), true
}

func relocateReference(reference string, target []string) string {
	i := strings.IndexByte(reference, '#')
	if i < 0 && len(target) == 0 {
		return reference
	}
	if i >= 0 {
		reference = reference[:i]
	}
	return reference + formatUriFragment(target)
}

func locationKey(resource string, pointer []string) string {
	var b strings.Builder
	b.WriteString(resource)
	for _, t := range pointer {
		b.WriteString("\x00" + strconv.Quote(t))
	}
	return b.String()
}

type resourceScope struct {
	parent                 *resourceScope
	sourceRoot, targetRoot []string
	uri                    string
}

type pendingRef struct {
	out           *Record
	value, source string
}

// converter is a run of a conversion with its adapter.
type converter struct {
	adapter           func(source *Record, cx *convContext) *Record
	booleanAdapter    func(bool) Value
	trackIds          bool
	ignoreRefSiblings bool
	locations         map[string][]string
	refs              []pendingRef
	rootURI           string
}

// convContext is what an adapter converts a schema's subschemas with.
type convContext struct {
	c                      *converter
	sourcePath, targetPath []string
	scope                  *resourceScope
}

func (cx *convContext) isDocumentRoot() bool { return len(cx.sourcePath) == 0 }

func (cx *convContext) schema(v Value, sourceKey, targetKey string) Value {
	return cx.c.loop(v, extend(cx.sourcePath, sourceKey), extend(cx.targetPath, targetKey), cx.scope)
}

func (cx *convContext) schemaAt(v Value, source, target []string) Value {
	return cx.c.loop(v, extend(cx.sourcePath, source...), extend(cx.targetPath, target...), cx.scope)
}

func (cx *convContext) schemaArray(v Value, sourceKey, targetKey string) Value {
	xs, ok := v.([]Value)
	if !ok {
		return v
	}
	out := make([]Value, len(xs))
	for i, x := range xs {
		n := strconv.Itoa(i)
		out[i] = cx.c.loop(x, extend(cx.sourcePath, sourceKey, n), extend(cx.targetPath, targetKey, n), cx.scope)
	}
	return out
}

func (cx *convContext) schemaMap(v Value, sourceKey, targetKey string) Value {
	if _, ok := v.(*Record); !ok {
		return v
	}
	return mapObject(v, func(item Value, key string) Value {
		return cx.c.loop(item, extend(cx.sourcePath, sourceKey, key), extend(cx.targetPath, targetKey, key), cx.scope)
	})
}

// reference sets `$ref` of `out`: a string one once the walk is done and
// its target is known.
func (cx *convContext) reference(out *Record, v Value) {
	if s, ok := v.(string); ok {
		cx.c.refs = append(cx.c.refs, pendingRef{out, s, cx.scope.uri})
	} else {
		out.put("$ref", v)
	}
}

func newConverter(adapter func(*Record, *convContext) *Record) *converter {
	return &converter{adapter: adapter, locations: map[string][]string{}, rootURI: rootResourceURI}
}

func (c *converter) convert(root Value, sourcePath, targetPath []string) Value {
	if len(sourcePath) == 0 && c.trackIds {
		c.rootURI = rootResourceURI
		if r, ok := root.(*Record); ok {
			if uri, ok := resolveResourceURI(c.resourceID(r), rootResourceURI); ok {
				c.rootURI = uri
			}
		}
	}
	return c.loop(root, sourcePath, targetPath, &resourceScope{uri: c.rootURI})
}

// finish relocates the references, once every schema has its target.
func (c *converter) finish() {
	for _, ref := range c.refs {
		reference := ref.value
		if u, ok := resolveReference(ref.value, ref.source); ok {
			hash := ""
			if u.hasFragment && u.fragment != "" {
				hash = "#" + u.fragment
			}
			u.hasFragment, u.fragment = false, ""
			if pointer, ok := parseUriFragment(hash); ok {
				if target, ok := c.locations[locationKey(u.href(), pointer)]; ok {
					reference = relocateReference(ref.value, target)
				}
			}
		}
		ref.out.put("$ref", reference)
	}
}

func (c *converter) resourceID(schema *Record) Value {
	if c.ignoreRefSiblings {
		if _, ok := lookup(schema, "$ref").(string); ok {
			return nil
		}
	}
	return lookup(schema, "$id")
}

func (c *converter) record(sourcePath, targetPath []string, scope *resourceScope) {
	// a JSON Pointer can address an embedded schema from any containing resource
	for ; scope != nil; scope = scope.parent {
		c.locations[locationKey(scope.uri, sourcePath[len(scope.sourceRoot):])] = targetPath[len(scope.targetRoot):]
	}
}

func (c *converter) loop(node Value, sourcePath, targetPath []string, scope *resourceScope) Value {
	if b, ok := node.(bool); ok {
		c.record(sourcePath, targetPath, scope)
		if c.booleanAdapter != nil {
			return c.booleanAdapter(b)
		}
		return b
	}
	r, ok := node.(*Record)
	if !ok {
		return node
	}
	if id := c.resourceID(r); len(sourcePath) > 0 && c.trackIds && createsResource(id) {
		if uri, ok := resolveResourceURI(id, scope.uri); ok {
			scope = &resourceScope{scope, sourcePath, targetPath, uri}
		}
	}
	c.record(sourcePath, targetPath, scope)
	return c.adapter(r, &convContext{c, sourcePath, targetPath, scope})
}

// convertSchema converts one schema.
func convertSchema(root Value, c *converter) Value {
	out := c.convert(root, nil, nil)
	c.finish()
	return out
}

var (
	schemaMapKeywords        = []string{"properties", "patternProperties"}
	schemaArrayKeywords      = []string{"allOf", "anyOf", "oneOf"}
	jsonSchemaSingleKeywords = []string{"not", "additionalProperties", "propertyNames", "contains", "if", "then", "else", "contentSchema"}
)

// convertSubschemaKeyword converts a keyword whose value is schemas, false
// for another.
func convertSubschemaKeyword(out *Record, key string, v Value, cx *convContext, single, maps []string) bool {
	switch {
	case slices.Contains(maps, key):
		out.put(key, cx.schemaMap(v, key, key))
	case slices.Contains(schemaArrayKeywords, key):
		out.put(key, cx.schemaArray(v, key, key))
	case slices.Contains(single, key):
		out.put(key, cx.schema(v, key, key))
	default:
		return false
	}
	return true
}

// transformSchema copies a schema, calling `f` on each schema object (its
// subschemas already done) and whether it is inside an embedded resource.
func transformSchema(node Value, f func(schema *Record, inEmbedded bool)) Value {
	var walk func(node Value, inherited, isRoot bool) Value
	walk = func(node Value, inherited, isRoot bool) Value {
		r, ok := node.(*Record)
		if !ok {
			return node
		}
		inEmbedded := inherited || !isRoot && createsResource(lookup(r, "$id"))
		out := newRecord(len(r.keys))
		for i, key := range r.keys {
			v := r.vals[i]
			switch key {
			case "$defs", "properties", "patternProperties", "dependentSchemas":
				if m := mapObject(v, func(x Value, _ string) Value { return walk(x, inEmbedded, false) }); m != nil {
					v = m
				}
			case "allOf", "anyOf", "oneOf", "prefixItems":
				if xs, ok := v.([]Value); ok {
					ys := make([]Value, len(xs))
					for j, x := range xs {
						ys[j] = walk(x, inEmbedded, false)
					}
					v = ys
				}
			case "not", "additionalProperties", "propertyNames", "unevaluatedProperties", "items", "contains",
				"unevaluatedItems", "if", "then", "else", "contentSchema":
				v = walk(v, inEmbedded, false)
			}
			out.put(key, v)
		}
		f(out, inEmbedded)
		return out
	}
	return walk(node, false, true)
}

func rewriteSchemaRef(schema *Record, rewrite func(ref, keyword string) string) {
	if s, ok := lookup(schema, "$ref").(string); ok {
		schema.put("$ref", rewrite(s, "$ref"))
	}
	if s, ok := lookup(schema, "$dynamicRef").(string); ok {
		schema.put("$dynamicRef", rewrite(s, "$dynamicRef"))
	}
}

func rewriteOpenApiComponentsReference(ref string) string {
	if !strings.HasPrefix(ref, "#") {
		return ref
	}
	path, ok := parseUriFragment(ref)
	if ok && len(path) >= 2 && path[0] == "components" && path[1] == "schemas" {
		return formatUriFragment(append([]string{"$defs"}, path[2:]...))
	}
	return ref
}
