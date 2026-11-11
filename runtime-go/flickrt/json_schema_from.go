package flickrt

import (
	"regexp"
	"slices"
	"sort"
	"strconv"
	"strings"
)

// The `JsonSchema` readers of other dialects into draft 2020-12, the
// writer for OpenAPI 3.1 components, and the Flick `JsonSchema.Document`
// (whose schema is `root`, a keyword in Flick).

var (
	anchorPattern          = regexp.MustCompile(`^[A-Za-z_][-A-Za-z0-9._]*$`)
	legacyIDFragment       = regexp.MustCompile(`^[A-Za-z][-A-Za-z0-9._:]*$`)
	openApiComponentsKey   = regexp.MustCompile(`^[a-zA-Z0-9.\-_]+$`)
	notOpenApiComponentKey = regexp.MustCompile(`[^a-zA-Z0-9._-]`)
)

var (
	pre2020To2020Collisions = []string{"$anchor", "$defs", "$dynamicAnchor", "$dynamicRef", "$vocabulary",
		"contentSchema", "dependentRequired", "dependentSchemas", "maxContains", "minContains", "prefixItems",
		"unevaluatedItems", "unevaluatedProperties"}
	draft07To2020Collisions   = append(slices.Clone(pre2020To2020Collisions), "deprecated")
	openApi30To2020Collisions = append(slices.Clone(pre2020To2020Collisions), "$comment", "$id", "$schema", "const",
		"contains", "contentEncoding", "contentMediaType", "else", "examples", "if", "patternProperties",
		"propertyNames", "then")
	openApi31TargetCollisions = []string{"example", "discriminator", "xml", "externalDocs"}
)

// jsonSchemaDocument is a Flick `JsonSchema.Document`.
func jsonSchemaDocument(dialect string, schema, definitions Value) Value {
	return Rec("dialect", dialect, "root", schema, "definitions", definitions)
}

// fromDraft202012 splits `$defs` off a schema.
//
// shortcut: a schema that is not an object reads as `{}`, where JS spreads a string's characters into one; a string is no schema, so match it only if a program shows one
func fromDraft202012(js Value) Value {
	schema, defs := &Record{}, Value(&Record{})
	if r, ok := js.(*Record); ok {
		schema = r.without("$defs")
		if d, ok := lookup(r, "$defs").(*Record); ok {
			defs = d
		}
	}
	return jsonSchemaDocument("draft-2020-12", schema, defs)
}

func JsonSchemaFromDraft202012(js Value) Value { return fromDraft202012(js) }

func JsonSchemaFromDraft07(js Value) Value {
	c := newConverter(draft07To2020)
	c.trackIds, c.ignoreRefSiblings = true, true
	return fromDraft202012(convertSchema(js, c))
}

func JsonSchemaFromOpenApi30(js Value) Value {
	return fromDraft202012(convertSchema(js, newConverter(openApi30To2020)))
}

func JsonSchemaFromOpenApi31(js Value) Value {
	root := false
	if r, ok := js.(*Record); ok {
		root = createsResource(lookup(r, "$id"))
	}
	return fromDraft202012(transformSchema(js, func(schema *Record, inEmbedded bool) {
		if !root && !inEmbedded {
			rewriteSchemaRef(schema, func(ref, _ string) string { return rewriteOpenApiComponentsReference(ref) })
		}
		if isMetaSchemaURI(lookup(schema, "$schema"), metaSchemaOpenApi31) {
			schema.put("$schema", metaSchemaDraft202012)
		}
		if has(schema, "example") {
			examples := lookup(schema, "examples")
			if xs, ok := examples.([]Value); ok || examples == nil {
				schema.put("examples", append([]Value{lookup(schema, "example")}, xs...))
				schema.remove("example")
			}
		}
	}))
}

func draft07To2020(source *Record, cx *convContext) *Record {
	out := &Record{}
	if ref, ok := lookup(source, "$ref").(string); ok {
		cx.reference(out, ref)
		if has(source, "definitions") {
			out.put("$defs", cx.schemaMap(lookup(source, "definitions"), "definitions", "$defs"))
		}
		return out
	}
	rejectKeywordCollisions(source, draft07To2020Collisions, "Draft 2020-12", "Draft-07")
	var items, additionalItems Value
	for i, key := range source.keys {
		v := source.vals[i]
		if convertSubschemaKeyword(out, key, v, cx, jsonSchemaSingleKeywords, schemaMapKeywords) {
			continue
		}
		switch key {
		case "$schema":
			if isMetaSchemaURI(v, metaSchemaDraft07) {
				v = metaSchemaDraft202012
			}
			out.put(key, v)
		case "$id":
			convertDraft07ID(out, v)
		case "definitions":
			out.put("$defs", cx.schemaMap(v, key, "$defs"))
		case "dependencies":
			deps, ok := v.(*Record)
			if !ok {
				out.put(key, v)
				break
			}
			required, schemas := &Record{}, &Record{}
			for j, dep := range deps.keys {
				if xs, ok := deps.vals[j].([]Value); ok {
					required.put(dep, xs)
				} else {
					schemas.put(dep, cx.schemaAt(deps.vals[j], []string{"dependencies", dep}, []string{"dependentSchemas", dep}))
				}
			}
			if len(required.keys) > 0 {
				out.put("dependentRequired", required)
			}
			if len(schemas.keys) > 0 {
				out.put("dependentSchemas", schemas)
			}
		case "items":
			items = v
		case "additionalItems":
			additionalItems = v
		default:
			out.put(key, v)
		}
	}
	if xs, ok := items.([]Value); ok {
		out.put("prefixItems", cx.schemaArray(xs, "items", "prefixItems"))
		if additionalItems != nil {
			out.put("items", cx.schema(additionalItems, "additionalItems", "items"))
		}
	} else if items != nil {
		out.put("items", cx.schema(items, "items", "items"))
	}
	return out
}

func convertDraft07ID(out *Record, v Value) {
	s, ok := v.(string)
	if !ok || !strings.Contains(s, "#") {
		out.put("$id", v)
		return
	}
	id, anchor, _ := strings.Cut(s, "#")
	if anchor != "" && !anchorPattern.MatchString(anchor) {
		unsupportedKeyword("$id", "Draft 2020-12", "fragment \"#"+anchor+"\" is not a valid $anchor")
	}
	if id != "" {
		out.put("$id", id)
	}
	if anchor != "" {
		out.put("$anchor", anchor)
	}
}

func openApi30To2020(source *Record, cx *convContext) *Record {
	out := &Record{}
	if ref, ok := lookup(source, "$ref").(string); ok {
		cx.reference(out, rewriteOpenApiComponentsReference(ref))
		return out
	}
	rejectKeywordCollisions(source, openApi30To2020Collisions, "Draft 2020-12", "OpenAPI 3.0")
	for i, key := range source.keys {
		v := source.vals[i]
		if convertSubschemaKeyword(out, key, v, cx, []string{"not", "items", "additionalProperties"}, []string{"properties"}) {
			continue
		}
		switch key {
		case "example":
			out.put("examples", []Value{v})
		case "nullable", "exclusiveMinimum", "exclusiveMaximum":
		default:
			out.put(key, v)
		}
	}
	openApiExclusiveBound(source, out, "minimum", "exclusiveMinimum")
	openApiExclusiveBound(source, out, "maximum", "exclusiveMaximum")
	if t, ok := lookup(source, "type").(string); ok && lookup(source, "nullable") == true {
		out.put("type", []Value{t, "null"})
	}
	return out
}

func openApiExclusiveBound(source, out *Record, bound, exclusiveKey string) {
	exclusive, ok := lookup(source, exclusiveKey).(bool)
	if !ok {
		if v := lookup(source, exclusiveKey); v != nil {
			out.put(exclusiveKey, v)
		}
		return
	}
	if n, isNumber := lookup(source, bound).(float64); exclusive && isNumber {
		out.put(exclusiveKey, n)
		out.remove(bound)
	}
}

func sanitizeOpenApiComponentsKey(s string) string {
	if s == "" {
		return "_"
	}
	return notOpenApiComponentKey.ReplaceAllString(s, "_")
}

// JsonSchemaToOpenApi31 is `toMultiDocumentOpenApi3_1`: definitions under
// `#/components/schemas`, their keys made valid component names.
func JsonSchemaToOpenApi31(multi Value) Value {
	m := record(multi)
	shared, _ := m.Get("definitions").(*Record)
	if shared == nil {
		shared = &Record{}
	}
	keyMap := map[string]string{}
	used := map[string]bool{}
	var invalid []string
	for _, k := range shared.keys {
		if openApiComponentsKey.MatchString(k) {
			used[k] = true
		} else {
			invalid = append(invalid, k)
		}
	}
	sort.Slice(invalid, func(i, j int) bool { return CompareStrings(invalid[i], invalid[j]) < 0 })
	for _, k := range invalid {
		if base := sanitizeOpenApiComponentsKey(k); !used[base] {
			used[base] = true
			keyMap[k] = base
		}
	}
	for _, k := range invalid {
		if _, ok := keyMap[k]; ok {
			continue
		}
		base := sanitizeOpenApiComponentsKey(k)
		candidate := ""
		for suffix := 1; candidate == "" || used[candidate]; suffix++ {
			candidate = base + "_" + strconv.Itoa(suffix)
		}
		used[candidate] = true
		keyMap[k] = candidate
	}
	component := func(k string) string {
		if v, ok := keyMap[k]; ok {
			return v
		}
		return k
	}
	rewrite := func(schema Value, rejectShared bool) Value {
		var local *Record
		isRoot := false
		if r, ok := schema.(*Record); ok {
			isRoot = createsResource(lookup(r, "$id"))
			local, _ = lookup(r, "$defs").(*Record)
		}
		return transformSchema(schema, func(s *Record, inEmbedded bool) {
			rejectKeywordCollisions(s, openApi31TargetCollisions, "OpenAPI 3.1", "Draft 2020-12")
			rewriteSchemaRef(s, func(ref, keyword string) string {
				if !strings.HasPrefix(ref, "#") {
					return ref
				}
				path, ok := parseUriFragment(ref)
				if !ok || len(path) < 2 || path[0] != "$defs" {
					return ref
				}
				key := path[1]
				if isRoot {
					if rejectShared && !inEmbedded && has(shared, key) && (local == nil || !has(local, key)) {
						unsupportedKeyword(keyword, "OpenAPI 3.1", "a schema resource cannot reference the shared definitions pool")
					}
					return ref
				}
				if inEmbedded {
					return ref
				}
				return formatUriFragment(append([]string{"components", "schemas", component(key)}, path[2:]...))
			})
		})
	}
	schemas := List(m.Get("schemas"))
	out := make([]Value, len(schemas))
	for i, s := range schemas {
		out[i] = rewrite(s, true)
	}
	definitions := &Record{}
	for i, k := range shared.keys {
		definitions.put(component(k), rewrite(shared.vals[i], false))
	}
	return Rec("dialect", "openapi-3.1", "schemas", out, "definitions", definitions)
}
