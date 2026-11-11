package flickrt

import "strconv"

// The `JsonSchema` writers of a draft 2020-12 document as draft-07 and
// draft-04.

func JsonSchemaToDraft07(d Value) Value {
	return convertDocument(d, "draft-07", newConverter(draft07Adapter))
}

func JsonSchemaToDraft04(d Value) Value {
	c := newConverter(draft04Adapter)
	c.booleanAdapter = func(b bool) Value {
		if b {
			return &Record{}
		}
		return Rec("not", &Record{})
	}
	return convertDocument(d, "draft-04", c)
}

// convertDocument converts the schema and its definitions, which move from
// `$defs` to `definitions`; a document of another dialect is a defect.
func convertDocument(d Value, dialect string, c *converter) Value {
	doc := record(d)
	if from := doc.Get("dialect").(string); from != "draft-2020-12" {
		Die("RangeError: expected a draft-2020-12 document, got " + from)
	}
	c.trackIds = true
	schema := c.convert(doc.Get("root"), nil, nil)
	definitions := doc.Get("definitions")
	if m := mapObject(definitions, func(def Value, key string) Value {
		return c.convert(def, []string{"$defs", key}, []string{"definitions", key})
	}); m != nil {
		definitions = m
	}
	c.finish()
	return jsonSchemaDocument(dialect, schema, definitions)
}

var (
	draft07TargetCollisions     = []string{"additionalItems", "definitions", "dependencies"}
	draft04TargetCollisions     = []string{"additionalItems", "definitions", "dependencies", "id"}
	draft04SchemaSingleKeywords = []string{"not", "additionalProperties", "contentSchema"}
)

func convertMetaSchemaKeyword(out *Record, v Value, cx *convContext, targetURI, dialect string) {
	switch {
	case cx.isDocumentRoot():
		if isMetaSchemaURI(v, metaSchemaDraft202012) {
			v = targetURI
		}
		out.put("$schema", v)
	case !isMetaSchemaURI(v, metaSchemaDraft202012):
		unsupportedKeyword("$schema", dialect, "an embedded resource cannot declare a different dialect")
	}
}

func draft07Adapter(source *Record, cx *convContext) *Record {
	rejectKeywordCollisions(source, draft07TargetCollisions, "Draft-07", "Draft 2020-12")
	out := &Record{}
	var reference, prefixItems, items Value
	for i, key := range source.keys {
		v := source.vals[i]
		if convertSubschemaKeyword(out, key, v, cx, jsonSchemaSingleKeywords, schemaMapKeywords) {
			continue
		}
		switch key {
		case "$ref":
			reference = v
		case "$schema":
			convertMetaSchemaKeyword(out, v, cx, metaSchemaDraft07, "Draft-07")
		case "$id", "$anchor", "dependentRequired", "dependentSchemas", "minContains", "maxContains":
		case "$defs":
			out.put("definitions", cx.schemaMap(v, key, "definitions"))
		case "prefixItems":
			prefixItems = v
		case "items":
			items = v
		case "$dynamicRef", "$dynamicAnchor", "$vocabulary", "unevaluatedProperties", "unevaluatedItems":
			unsupportedKeyword(key, "Draft-07", "the target dialect has no equivalent")
		case "required":
			if xs, ok := v.([]Value); ok && len(xs) == 0 {
				break
			}
			out.put(key, v)
		default:
			out.put(key, v)
		}
	}
	convertTuple(out, prefixItems, items, cx)
	if has(source, "contains") {
		checkContainsCardinality(source, "Draft-07")
		if has(source, "minContains") {
			out.put("minContains", lookup(source, "minContains"))
		}
	} else {
		copyContainsBounds(source, out)
	}
	convertDependencies(source, out, cx, false)
	convertLegacyID(source, out, "$id", "Draft-07")
	convertReference(out, reference, cx)
	return out
}

func checkContainsCardinality(source *Record, dialect string) {
	min, max := lookup(source, "minContains"), lookup(source, "maxContains")
	if min != nil && min != float64(1) || max != nil {
		unsupportedKeyword("minContains/maxContains", dialect, "contains cardinality cannot be represented")
	}
}

func copyContainsBounds(source, out *Record) {
	for _, k := range []string{"minContains", "maxContains"} {
		if has(source, k) {
			out.put(k, lookup(source, k))
		}
	}
}

func convertTuple(out *Record, prefixItems, items Value, cx *convContext) {
	if prefixItems == nil {
		if items != nil {
			out.put("items", cx.schema(items, "items", "items"))
		}
		return
	}
	out.put("items", cx.schemaArray(prefixItems, "prefixItems", "items"))
	if items != nil {
		out.put("additionalItems", cx.schema(items, "items", "additionalItems"))
	}
}

func convertReference(out *Record, reference Value, cx *convContext) {
	if reference == nil {
		return
	}
	if _, ok := reference.(string); ok && len(out.keys) > 0 {
		ref := &Record{}
		cx.reference(ref, reference)
		appendAllOf(out, ref)
	} else {
		cx.reference(out, reference)
	}
}

func convertDependencies(source, out *Record, cx *convContext, draft04 bool) {
	required, _ := lookup(source, "dependentRequired").(*Record)
	schemas, _ := lookup(source, "dependentSchemas").(*Record)
	if required == nil && schemas == nil {
		return
	}
	keys := &Record{}
	for _, r := range []*Record{required, schemas} {
		if r != nil {
			for _, k := range r.keys {
				keys.put(k, nil)
			}
		}
	}
	dependencies := &Record{}
	for _, key := range keys.keys {
		var req, dependency Value
		if required != nil {
			req = lookup(required, key)
		}
		if schemas != nil {
			dependency = lookup(schemas, key)
		}
		xs, isList := req.([]Value)
		omitRequired := draft04 && isList && len(xs) == 0
		switch {
		case dependency == nil:
			if !omitRequired {
				dependencies.put(key, req)
			}
		case req == nil || omitRequired:
			dependencies.put(key, cx.schemaAt(dependency, []string{"dependentSchemas", key}, []string{"dependencies", key}))
		default:
			dependencies.put(key, Rec("allOf", []Value{
				cx.schemaAt(dependency, []string{"dependentSchemas", key}, []string{"dependencies", key, "allOf", "0"}),
				Rec("required", req),
			}))
		}
	}
	if len(dependencies.keys) > 0 {
		out.put("dependencies", dependencies)
	}
}

func convertLegacyID(source, out *Record, targetKey, dialect string) {
	id, anchor := lookup(source, "$id"), lookup(source, "$anchor")
	if anchor == nil {
		if id != nil {
			out.put(targetKey, id)
		}
		return
	}
	s, ok := anchor.(string)
	if !ok || !anchorPattern.MatchString(s) {
		unsupportedKeyword("$anchor", dialect, "the anchor is not valid")
	}
	if !legacyIDFragment.MatchString(s) {
		unsupportedKeyword("$anchor", dialect, "the anchor cannot be represented as a plain-name fragment identifier")
	}
	if id != nil {
		unsupportedKeyword("$anchor", dialect, "it cannot be combined with the schema $id")
	}
	out.put(targetKey, "#"+s)
}

func draft04Adapter(source *Record, cx *convContext) *Record {
	rejectKeywordCollisions(source, draft04TargetCollisions, "Draft-04", "Draft 2020-12")
	out := &Record{}
	var reference, prefixItems, items, constValue Value
	hasConst := false
	for i, key := range source.keys {
		v := source.vals[i]
		if convertSubschemaKeyword(out, key, v, cx, draft04SchemaSingleKeywords, schemaMapKeywords) {
			continue
		}
		switch key {
		case "$ref":
			reference = v
		case "$schema":
			convertMetaSchemaKeyword(out, v, cx, metaSchemaDraft04, "Draft-04")
		case "$id", "$anchor", "dependentRequired", "dependentSchemas", "contains", "minContains", "maxContains",
			"if", "then", "else", "minimum", "maximum", "exclusiveMinimum", "exclusiveMaximum":
		case "$defs":
			out.put("definitions", cx.schemaMap(v, key, "definitions"))
		case "prefixItems":
			prefixItems = v
		case "items":
			items = v
		case "$dynamicRef", "$dynamicAnchor", "$vocabulary", "unevaluatedProperties", "unevaluatedItems", "propertyNames":
			unsupportedKeyword(key, "Draft-04", "the target dialect has no equivalent")
		case "const":
			constValue, hasConst = v, true
		case "required":
			if xs, ok := v.([]Value); ok && len(xs) == 0 {
				break
			}
			out.put(key, v)
		default:
			out.put(key, v)
		}
	}
	convertTuple(out, prefixItems, items, cx)
	draft04ExclusiveBound(source, out, "minimum", "exclusiveMinimum")
	draft04ExclusiveBound(source, out, "maximum", "exclusiveMaximum")
	convertDependencies(source, out, cx, true)
	convertLegacyID(source, out, "id", "Draft-04")
	draft04Conditionals(source, out, cx)
	draft04Contains(source, out, cx)
	if hasConst {
		if has(source, "enum") {
			appendAllOf(out, Rec("enum", []Value{constValue}))
		} else {
			out.put("enum", []Value{constValue})
		}
	}
	convertReference(out, reference, cx)
	return out
}

func allOfLength(out *Record) string {
	xs, _ := lookup(out, "allOf").([]Value)
	return strconv.Itoa(len(xs))
}

func draft04Conditionals(source, out *Record, cx *convContext) {
	hasIf, hasThen, hasElse := has(source, "if"), has(source, "then"), has(source, "else")
	if !hasIf || !hasThen && !hasElse {
		for _, k := range []string{"if", "then", "else"} {
			if has(source, k) {
				out.put(k, cx.schema(lookup(source, k), k, k))
			}
		}
		return
	}
	base := []string{"allOf", allOfLength(out), "anyOf"}
	branch := func(key string, target ...string) Value {
		return cx.schemaAt(lookup(source, key), []string{key}, extend(base, target...))
	}
	var conditional Value
	switch {
	case hasThen && hasElse:
		if hasSchemaIdentifier(lookup(source, "if")) {
			unsupportedKeyword("if", "Draft-04", "lowering both branches would duplicate a schema identifier")
		}
		// (if AND then) OR ((NOT if) AND else)
		conditional = Rec("anyOf", []Value{
			Rec("allOf", []Value{branch("if", "0", "allOf", "0"), branch("then", "0", "allOf", "1")}),
			Rec("allOf", []Value{
				Rec("not", branch("if", "1", "allOf", "0", "not")),
				branch("else", "1", "allOf", "1"),
			}),
		})
	case hasThen:
		// (NOT if) OR then
		conditional = Rec("anyOf", []Value{Rec("not", branch("if", "0", "not")), branch("then", "1")})
	default:
		// if OR else
		conditional = Rec("anyOf", []Value{branch("if", "0"), branch("else", "1")})
	}
	appendAllOf(out, conditional)
}

func hasSchemaIdentifier(node Value) bool {
	r, ok := node.(*Record)
	if !ok {
		return false
	}
	if has(r, "$id") || has(r, "$anchor") {
		return true
	}
	for i, key := range r.keys {
		switch v := r.vals[i]; key {
		case "$defs", "properties", "patternProperties", "dependentSchemas":
			if m, ok := v.(*Record); ok {
				for _, x := range m.vals {
					if hasSchemaIdentifier(x) {
						return true
					}
				}
			}
		case "allOf", "anyOf", "oneOf", "prefixItems":
			if xs, ok := v.([]Value); ok {
				for _, x := range xs {
					if hasSchemaIdentifier(x) {
						return true
					}
				}
			}
		case "not", "additionalProperties", "propertyNames", "unevaluatedProperties", "items", "contains",
			"unevaluatedItems", "if", "then", "else", "contentSchema":
			if hasSchemaIdentifier(v) {
				return true
			}
		}
	}
	return false
}

func draft04Contains(source, out *Record, cx *convContext) {
	if !has(source, "contains") {
		copyContainsBounds(source, out)
		return
	}
	checkContainsCardinality(source, "Draft-04")
	contains := cx.schemaAt(lookup(source, "contains"), []string{"contains"},
		[]string{"allOf", allOfLength(out), "anyOf", "1", "not", "items", "not"})
	appendAllOf(out, Rec("anyOf", []Value{
		Rec("not", Rec("type", "array")),
		Rec("not", Rec("items", Rec("not", contains))),
	}))
}

func draft04ExclusiveBound(source, out *Record, bound, exclusiveKey string) {
	b := lookup(source, bound)
	if exclusive, ok := lookup(source, exclusiveKey).(float64); ok {
		n, isNumber := b.(float64)
		if isNumber && (bound == "minimum" && n > exclusive || bound == "maximum" && n < exclusive) {
			out.put(bound, n)
		} else {
			out.put(bound, exclusive)
			out.put(exclusiveKey, true)
		}
	} else if b != nil {
		out.put(bound, b)
	}
}
