package flickrt

import (
	"regexp"
	"strconv"
	"strings"
)

// OpenAPI 3.1 of an HttpApi, as the JS runtime writes it: every schema by
// the strict exporter, an exact wire contract, or an ExportError naming
// `group.endpoint.part -> field`. Nothing is approximated.

// apiDecodeError is the body of a request that does not decode: what the
// server answers with 400.
func apiDecodeError() Value {
	issue := Rec("type", "object",
		"properties", Rec("path", Rec("type", "string"), "message", Rec("type", "string")),
		"required", []Value{"path", "message"}, "additionalProperties", false)
	return Rec("type", "object",
		"properties", Rec("error", Rec("const", "BadRequest"), "issues", Rec("type", "array", "items", issue)),
		"required", []Value{"error", "issues"}, "additionalProperties", false)
}

// openAPI writes the document; its components gather the definitions of
// every schema, the first of each name.
type openAPI struct {
	components *Record
}

// schema is the schema of the codec, its definitions moved to the components.
func (d *openAPI) schema(c codec, root string) *Record {
	top, defs, refused := exportSchema(root, c)
	if refused != "" {
		panic(exportRefused(refused))
	}
	// the definitions keep their own references, as the JS runtime moves them
	for i, name := range defs.keys {
		if d.components.index(name) < 0 {
			d.components.put(name, defs.vals[i])
		}
	}
	return componentRefs(top).(*Record)
}

// componentRefs is the schema with its references to definitions pointing
// to the components.
func componentRefs(v Value) Value {
	const from, to = "#/$defs/", "#/components/schemas/"
	switch x := v.(type) {
	case string:
		if strings.HasPrefix(x, from) {
			return to + x[len(from):]
		}
	case []Value:
		out := make([]Value, len(x))
		for i, e := range x {
			out[i] = componentRefs(e)
		}
		return out
	case *Record:
		out := newRecord(len(x.keys))
		for i, k := range x.keys {
			out.put(componentRefs(k).(string), componentRefs(x.vals[i]))
		}
		return out
	}
	return v
}

// parameters are the path, query or header parameters of a record codec.
func (d *openAPI) parameters(c codec, where, root string) []Value {
	if c == nil {
		return nil
	}
	s := d.schema(c, root)
	props, ok := recordField(s, "properties").(*Record)
	if recordField(s, "type") != "object" || !ok {
		panic(exportRefused(root + " -> OpenAPI: parameters come from a record of fields"))
	}
	required := map[string]bool{}
	if names, ok := recordField(s, "required").([]Value); ok {
		for _, n := range names {
			if name, ok := n.(string); ok {
				required[name] = true
			}
		}
	}
	out := make([]Value, len(props.keys))
	for i, name := range props.keys {
		out[i] = Rec("name", name, "in", where, "required", where == "path" || required[name], "schema", props.vals[i])
	}
	return out
}

func jsonContent(s Value, media string) []Value {
	return []Value{"content", Rec(media, Rec("schema", s))}
}

var payloadMedia = map[string]string{
	encJSON: "application/json", encText: "text/plain", encForm: "application/x-www-form-urlencoded",
	encMultipart: "multipart/form-data", encBytes: "application/octet-stream", encParts: "multipart/form-data",
}

var successMedia = map[string]string{
	encJSON: "application/json", encText: "text/plain", encBytes: "application/octet-stream",
	encStream: "application/octet-stream", encSSE: "text/event-stream", encEvents: "text/event-stream",
}

// uncoded is the schema of a body without a codec: bytes, a form's parts,
// or the text of events.
func uncoded(encoding string) Value {
	switch encoding {
	case encParts:
		return Rec("type", "object")
	case encEvents:
		return Rec("type", "string")
	}
	return Rec("type", "string", "format", "binary")
}

// responseHeaders are the response headers the handler answers with its value.
func (d *openAPI) responseHeaders(e *apiEndpoint, root string) []Value {
	var headers []Value
	for i, extra := range e.extras {
		if extra.headers != nil {
			headers = append(headers, d.parameters(extra.headers, "header", root+".success.headers"+strconv.Itoa(i))...)
		}
	}
	if len(headers) == 0 {
		return nil
	}
	out := newRecord(len(headers))
	for _, h := range headers {
		r := record(h)
		out.put(r.Get("name").(string), Rec("required", r.Get("required"), "schema", r.Get("schema")))
	}
	return []Value{"headers", out}
}

func (d *openAPI) operation(g *apiGroup, e *apiEndpoint) Value {
	root := g.name + "." + e.name
	success := append([]Value{"description", "Success"}, d.responseHeaders(e, root)...)
	if e.hasSuccessBody() {
		var s Value
		if e.success == nil {
			s = uncoded(e.successEncoding)
		} else {
			s = d.schema(e.success, root+".success")
		}
		success = append(success, jsonContent(s, successMedia[e.successEncoding])...)
	}
	responses := newRecord(4)
	responses.put(strconv.Itoa(int(e.status)), Rec(success...))
	responses.put("400", Rec(append([]Value{"description", "The request does not decode"},
		jsonContent(apiDecodeError(), "application/json")...)...))
	var statuses []int32
	byStatus := map[int32][]Value{}
	for i, c := range e.errors {
		if _, seen := byStatus[c.status]; !seen {
			statuses = append(statuses, c.status)
		}
		byStatus[c.status] = append(byStatus[c.status], d.schema(c.codec, root+".error"+strconv.Itoa(i)))
	}
	for _, status := range statuses {
		var one Value = Rec("oneOf", byStatus[status])
		if len(byStatus[status]) == 1 {
			one = byStatus[status][0]
		}
		responses.put(strconv.Itoa(int(status)), Rec(append([]Value{"description", "Error"}, jsonContent(one, "application/json")...)...))
	}
	op := []Value{"operationId", root, "tags", []Value{g.name}}
	if e.description != "" {
		op = append(op, "summary", e.description)
	}
	if e.deprecated {
		op = append(op, "deprecated", true)
	}
	if len(e.security) > 0 {
		security := make([]Value, len(e.security))
		for i, s := range e.security {
			security[i] = Rec(s.name, []Value{})
		}
		op = append(op, "security", security)
	}
	params := append(append(d.parameters(e.params, "path", root+".path"),
		d.parameters(e.query, "query", root+".query")...),
		d.parameters(e.headers, "header", root+".headers")...)
	if params == nil {
		params = []Value{}
	}
	op = append(op, "parameters", params)
	if e.hasPayload() {
		var s Value
		if e.payload == nil {
			s = uncoded(e.payloadEncoding)
		} else {
			s = d.schema(e.payload, root+".payload")
		}
		op = append(op, "requestBody", Rec(append([]Value{"required", true}, jsonContent(s, payloadMedia[e.payloadEncoding])...)...))
	}
	return Rec(append(op, "responses", responses)...)
}

var pathParam = regexp.MustCompile(`:([A-Za-z_]\w*)`)

func securityScheme(s *apiSecurity) Value {
	switch s.kind {
	case "bearer":
		return Rec("type", "http", "scheme", "bearer")
	case "basic":
		return Rec("type", "http", "scheme", "basic")
	}
	return Rec("type", "apiKey", "name", s.key, "in", s.in)
}

func (d *openAPI) document(a *apiDefinition) Value {
	schemes := newRecord(0)
	paths := newRecord(0)
	for _, g := range a.groups {
		for _, e := range g.endpoints {
			if e.hidden {
				continue
			}
			for _, s := range e.security {
				schemes.put(s.name, securityScheme(s))
			}
			path := pathParam.ReplaceAllString(e.path, "{$1}")
			methods, ok := recordField(paths, path).(*Record)
			if !ok {
				methods = newRecord(1)
			}
			paths.put(path, methods.set(strings.ToLower(e.method), d.operation(g, e)))
		}
	}
	info := []Value{"title", a.name, "version", a.info.version}
	if a.info.description != "" {
		info = append(info, "description", a.info.description)
	}
	doc := []Value{"openapi", "3.1.0", "info", Rec(info...)}
	if len(a.info.servers) > 0 {
		servers := make([]Value, len(a.info.servers))
		for i, url := range a.info.servers {
			servers[i] = Rec("url", url)
		}
		doc = append(doc, "servers", servers)
	}
	tags := make([]Value, len(a.groups))
	for i, g := range a.groups {
		tags[i] = Rec("name", g.name)
	}
	doc = append(doc, "tags", tags, "paths", paths)
	var component []Value
	if len(d.components.keys) > 0 {
		component = append(component, "schemas", d.components)
	}
	if len(schemes.keys) > 0 {
		component = append(component, "securitySchemes", schemes)
	}
	if component != nil {
		doc = append(doc, "components", Rec(component...))
	}
	return Rec(doc...)
}

// HttpApiOpenApi is `HttpApi.openapi`: the document as JSON text.
func HttpApiOpenApi(api Value) (out Value) {
	defer func() {
		if r := recover(); r != nil {
			msg, ok := r.(exportRefused)
			if !ok {
				panic(r)
			}
			out = exportError(string(msg))
		}
	}()
	d := &openAPI{components: newRecord(0)}
	return Ok(stringifyIndented(d.document(api.(*apiDefinition))))
}
