package flickrt

import (
	"bytes"
	"io"
	"mime/multipart"
	"net/http"
	"net/textproto"
	"regexp"
	"sort"
	"strings"
)

// HttpApi clients as the JS runtime's: a call encodes the request's parts
// with the endpoint's codecs and decodes the response by its status: the
// success, a declared error as the typed failure, anything else a
// ClientError.

func clientError(status int32, message string) Value {
	return NewData(apiClientErrorTag, Rec("status", status, "message", message))
}

// issueText is the issues of a SchemaError as one line.
func issueText(issues []issue) string {
	parts := make([]string, len(issues))
	for i, is := range issues {
		path := is.path
		if path == "" {
			path = "(root)"
		}
		parts[i] = path + ": " + is.message
	}
	return strings.Join(parts, "; ")
}

// clientEncode is the encoded part, or the ClientError naming it.
func clientEncode(c codec, v Value, part string) (Value, Value) {
	if c == nil {
		return Unit, nil
	}
	encoded, issues := c.encode(v)
	if issues != nil {
		return nil, clientError(0, part+": "+issueText(issues))
	}
	return encoded, nil
}

func clientPairs(c codec, v Value, part string) []Value {
	encoded, err := clientEncode(c, v, part)
	if err != nil {
		Fail(err)
	}
	return apiStrings(encoded)
}

// apiURL is the endpoint's path with the parameters, under `base`, and the query.
func apiURL(base string, e *apiEndpoint, params, query []Value) string {
	values := map[string]string{}
	for _, p := range params {
		k, v := pairOf(p)
		values[k] = v
	}
	resolved := pathParam.ReplaceAllStringFunc(e.path, func(m string) string {
		encoded, ok := encodeURIComponent(values[m[1:]])
		if !ok {
			panic(Defect{"URIError: URI malformed"})
		}
		return encoded
	})
	if !strings.HasSuffix(base, "/") {
		base += "/"
	}
	b, ok := parseURL(base)
	if !ok || b.opaque {
		panic(Defect{"TypeError: Invalid URL"})
	}
	// the path is relative to the base's: it replaces its last segment
	b.query, b.hasQuery, b.fragment, b.hasFragment = "", false, "", false
	href := b.href()
	dir := b.path[:strings.LastIndexByte(b.path, '/')+1]
	u, ok := parseURL(href[:len(href)-len(b.path)] + dir + strings.TrimPrefix(resolved, "/"))
	if !ok {
		panic(Defect{"TypeError: Invalid URL"})
	}
	if len(query) > 0 {
		u.setQueryParams(append(u.searchParams(), query...))
	}
	return u.href()
}

// HttpApiUrl is `HttpApi.url base endpoint path query`: the URL a call would request.
func HttpApiUrl(base, endpoint, path, query Value) Value {
	e := endpointOf(endpoint)
	params, err := clientEncode(e.params, path, "path")
	if err != nil {
		return Err(err)
	}
	search, err := clientEncode(e.query, query, "query")
	if err != nil {
		return Err(err)
	}
	return Ok(apiURL(base.(string), e, apiStrings(params), apiStrings(search)))
}

// formEscape escapes a name in a part's header as fetch's FormData does.
var formEscape = strings.NewReplacer("\n", "%0A", "\r", "%0D", `"`, "%22")

type formWriter struct {
	buf bytes.Buffer
	w   *multipart.Writer
}

func newFormWriter() *formWriter {
	f := &formWriter{}
	f.w = multipart.NewWriter(&f.buf)
	return f
}

func (f *formWriter) field(key, value string) {
	h := textproto.MIMEHeader{}
	h.Set("Content-Disposition", `form-data; name="`+formEscape.Replace(wellFormed(key))+`"`)
	p, _ := f.w.CreatePart(h)
	io.WriteString(p, wellFormed(value))
}

func (f *formWriter) file(key, name, contentType string, data []byte) {
	if contentType == "" {
		contentType = "application/octet-stream"
	}
	h := textproto.MIMEHeader{}
	h.Set("Content-Disposition", `form-data; name="`+formEscape.Replace(wellFormed(key))+`"; filename="`+
		formEscape.Replace(wellFormed(name))+`"`)
	h.Set("Content-Type", contentType)
	p, _ := f.w.CreatePart(h)
	p.Write(data)
}

func (f *formWriter) done() ([]byte, string) {
	f.w.Close()
	return f.buf.Bytes(), f.w.FormDataContentType()
}

// formOf is a form of the fields of an encoded record; a list is a field of each value.
func formOf(encoded Value) ([]byte, string) {
	f := newFormWriter()
	if r, ok := encoded.(*Record); ok {
		for i, k := range r.keys {
			values, ok := r.vals[i].([]Value)
			if !ok {
				values = []Value{r.vals[i]}
			}
			for _, v := range values {
				switch x := v.(type) {
				case string:
					f.field(k, x)
				default:
					if v != Unit && v != JsonNull {
						f.field(k, JsonStringify(v).(string))
					}
				}
			}
		}
	}
	return f.done()
}

// partsForm is a form of the parts of the stream; each file's bytes are read.
func partsForm(fb *Fiber, parts Value) ([]byte, string) {
	f := newFormWriter()
	_, c := catch(func() Value {
		runStream(fb, streamOf(parts), func(chunk []Value) {
			for _, part := range chunk {
				d := part.(*Data)
				r := record(d.Fields[0])
				if d.Tag == partFieldTag {
					f.field(r.Get("key").(string), r.Get("value").(string))
					continue
				}
				var data []byte
				for _, b := range List(StreamRunCollect(fb, r.Get("content"))) {
					data = append(data, byteString(b)...)
				}
				f.file(r.Get("key").(string), r.Get("name").(string), r.Get("contentType").(string), data)
			}
		})
		return Unit
	})
	if c != nil {
		if _, ok := typedFailure(c); ok {
			Fail(clientError(0, "payload: [object Object]"))
		}
		panic(&Failure{c})
	}
	return f.done()
}

var clientMedia = map[string]string{
	encJSON: "application/json", encText: "text/plain",
	encForm: "application/x-www-form-urlencoded", encBytes: "application/octet-stream",
}

// sentBody is the body a call sends, and the content type of a form's own boundary.
func sentBody(fb *Fiber, e *apiEndpoint, payload Value) ([]byte, string) {
	switch e.payloadEncoding {
	case encJSON:
		return []byte(wellFormed(JsonStringify(payload).(string))), ""
	case encText:
		return []byte(wellFormed(jsString(payload))), ""
	case encForm:
		return []byte(formEncode(apiStrings(payload))), ""
	case encMultipart:
		return formOf(payload)
	case encBytes:
		return []byte(byteString(payload)), ""
	}
	return partsForm(fb, payload)
}

type received struct {
	status  int32
	headers http.Header
	body    []byte
}

// lowerHeaders is the response's headers as fetch iterates them: lower-case
// names in order, the values of one joined.
func lowerHeaders(h http.Header) *Record {
	names := make([]string, 0, len(h))
	for k := range h {
		names = append(names, k)
	}
	sort.Slice(names, func(i, j int) bool { return strings.ToLower(names[i]) < strings.ToLower(names[j]) })
	out := newRecord(len(names))
	for _, k := range names {
		out.put(strings.ToLower(k), strings.Join(h[k], ", "))
	}
	return out
}

// HttpApiCall is `HttpApi.call base endpoint request`.
func HttpApiCall(fb *Fiber, base, endpoint, request Value) Value {
	e, r := endpointOf(endpoint), record(request)
	params := clientPairs(e.params, r.Get("path"), "path")
	query := clientPairs(e.query, r.Get("query"), "query")
	url := apiURL(base.(string), e, params, query)
	headers := clientPairs(e.headers, r.Get("headers"), "headers")
	payload := r.Get("payload")
	if e.payload != nil {
		encoded, err := clientEncode(e.payload, payload, "payload")
		if err != nil {
			Fail(err)
		}
		payload = encoded
	}
	var body []byte
	contentType := clientMedia[e.payloadEncoding]
	if e.hasPayload() {
		var own string
		if body, own = sentBody(fb, e, payload); own != "" {
			contentType = own
		}
	}
	got := ForeignAsync(fb, func(ctx Context) Value {
		var reader io.Reader
		if e.hasPayload() {
			reader = bytes.NewReader(body)
		}
		req, err := http.NewRequestWithContext(ctx, e.method, url, reader)
		if err != nil {
			return nil
		}
		for _, p := range headers {
			k, v := pairOf(p)
			req.Header.Set(k, wellFormed(v))
		}
		if e.hasPayload() && contentType != "" {
			req.Header.Set("Content-Type", contentType)
		}
		res, err := fetchClient.Do(req)
		if err != nil {
			return nil
		}
		defer res.Body.Close()
		data, err := io.ReadAll(res.Body)
		if err != nil {
			return nil
		}
		return &received{int32(res.StatusCode), res.Header, data}
	})
	res, ok := got.(*received)
	if !ok {
		return Fail(clientError(0, "fetch failed"))
	}
	text := decodeUTF8(res.body)
	if res.status != e.status {
		return clientDeclared(e, res.status, jsonOrText(text))
	}
	return clientExtras(e, res, clientSuccess(e, res, text))
}

// jsonOrText is the JSON value of the text, or the text.
func jsonOrText(text string) Value {
	if v, reason := parseJSON(text); reason == "" {
		return v
	}
	return text
}

// clientDeclared fails with the first declared error of the status that
// decodes, or a ClientError of the body.
func clientDeclared(e *apiEndpoint, status int32, body Value) Value {
	for _, c := range e.errors {
		if c.status != status {
			continue
		}
		if v, issues := c.codec.decode(body); issues == nil {
			return Fail(v)
		}
	}
	text, ok := body.(string)
	if !ok {
		text = JsonStringify(body).(string)
	}
	return Fail(clientError(status, text))
}

func clientDecode(c codec, status int32, in Value) Value {
	v, issues := c.decode(in)
	if issues != nil {
		return Fail(clientError(status, "the response does not decode: "+issueText(issues)))
	}
	return v
}

// clientSuccess is the success value of the response, before its extras.
func clientSuccess(e *apiEndpoint, r *received, text string) Value {
	switch e.successEncoding {
	case encBytes:
		return byteList(r.body)
	case encStream:
		return StreamFromList([]Value{byteList(r.body)})
	case encEvents:
		return StreamFromList(sseEvents(text))
	case encSSE:
		if e.success == nil {
			return Unit
		}
		events := sseEvents(text)
		out := make([]Value, len(events))
		for i, ev := range events {
			out[i] = clientDecode(e.success, r.status, jsonOrText(record(ev).Get("data").(string)))
		}
		return StreamFromList(out)
	}
	if e.success == nil {
		return Unit
	}
	if e.successEncoding == encText {
		return clientDecode(e.success, r.status, text)
	}
	return clientDecode(e.success, r.status, jsonOrText(text))
}

// clientExtras is the value with each extra of the response: its decoded
// headers, the value of its cookie.
func clientExtras(e *apiEndpoint, r *received, v Value) Value {
	for _, extra := range e.extras {
		if extra.headers != nil {
			v = []Value{v, clientDecode(&ignoring{extra.headers}, r.status, lowerHeaders(r.headers))}
			continue
		}
		// a browser hides `Set-Cookie`, and an HttpOnly cookie, from scripts: the call still succeeds
		shown := None
		for _, c := range r.headers.Values("Set-Cookie") {
			first, _, _ := strings.Cut(c, ";")
			if value, ok := strings.CutPrefix(first, extra.cookie+"="); ok {
				decoded, ok := decodeURIComponent(value)
				if !ok {
					panic(Defect{"URIError: URI malformed"})
				}
				shown = Some(RedactedMake(decoded))
				break
			}
		}
		v = []Value{v, shown}
	}
	return v
}

// ignoring decodes an object without the keys its record codec does not name.
type ignoring struct{ codec }

func (i *ignoring) decode(in Value) (Value, []issue) {
	r, ok := in.(*Record)
	if !ok {
		return i.codec.decode(in)
	}
	c := i.codec
	if b, ok := c.(*block); ok {
		c = &block{b.name, b.fields, true}
	}
	return c.decode(withoutExtra(c, r))
}

var (
	blankLine = regexp.MustCompile(`\r?\n\r?\n`)
	lineBreak = regexp.MustCompile(`\r?\n`)
)

// sseEvents is the events of the text: their id, name and data lines.
func sseEvents(text string) []Value {
	out := []Value{}
	for _, block := range blankLine.Split(text, -1) {
		var data []string
		id, event := None, "message"
		haveID, haveEvent := false, false
		for _, line := range lineBreak.Split(block, -1) {
			var k, v string
			if i := strings.IndexByte(line, ':'); i >= 0 {
				k, v = line[:i], strings.TrimPrefix(line[i+1:], " ")
			} else {
				k, v = strings.TrimSuffix(line, lastRune(line)), line
			}
			switch {
			case k == "data":
				data = append(data, v)
			case k == "id" && !haveID:
				id, haveID = Some(v), true
			case k == "event" && !haveEvent:
				event, haveEvent = v, true
			}
		}
		if len(data) > 0 {
			out = append(out, Rec("id", id, "event", event, "data", strings.Join(data, "\n")))
		}
	}
	return out
}

func lastRune(s string) string {
	r := []rune(s)
	if len(r) == 0 {
		return ""
	}
	return string(r[len(r)-1])
}
