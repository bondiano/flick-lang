package flickrt

import (
	"io"
	"math/big"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"unicode/utf8"
)

// Http.Multipart as Effect's Multipart: the whole form of a request, its
// fields and its files, which are written to a temporary directory the
// scope of the call removes. A request reads its form once; later reads
// get the same form. The form of a request made of a client request's
// form data is that data, without the parser or its limits.

const (
	maxPartsKey       = "effect/http/Multipart/MaxParts"
	maxFieldSizeKey   = "effect/http/Multipart/MaxFieldSize"
	maxFileSizeKey    = "effect/http/Multipart/MaxFileSize"
	fieldMimeTypesKey = "effect/http/Multipart/FieldMimeTypes"
)

var (
	multipartErrorTag = Tag("Http.Multipart.MultipartError.MultipartError")
	multipartStatus   = map[string]int32{
		Tag("Http.Multipart.MultipartErrorReason.FileTooLarge"):  413,
		Tag("Http.Multipart.MultipartErrorReason.FieldTooLarge"): 413,
		Tag("Http.Multipart.MultipartErrorReason.BodyTooLarge"):  413,
		Tag("Http.Multipart.MultipartErrorReason.TooManyParts"):  413,
		Tag("Http.Multipart.MultipartErrorReason.InternalError"): 500,
		Tag("Http.Multipart.MultipartErrorReason.Parse"):         400,
	}
)

func multipartError(reason string) Value {
	return NewData(multipartErrorTag, Nullary(Tag("Http.Multipart.MultipartErrorReason."+reason)))
}

// limitsOf are the limits `with_limits` set for the fiber, or Effect's
// defaults: fields of at most 10 MiB, JSON parts read as fields.
func limitsOf(fb *Fiber) *formLimits {
	l := &formLimits{maxParts: -1, maxFieldSize: 10 << 20, maxPartSize: -1, maxTotalSize: -1, fieldTypes: []string{"application/json"}}
	if v, ok := fb.env.get(maxPartsKey); ok {
		l.maxParts = int64(v.(int32))
	}
	for key, into := range map[string]*int64{maxFieldSizeKey: &l.maxFieldSize, maxFileSizeKey: &l.maxPartSize, maxBodySizeKey: &l.maxTotalSize} {
		if v, ok := fb.env.get(key); ok && v.(*big.Int).IsInt64() {
			*into = v.(*big.Int).Int64()
		}
	}
	if v, ok := fb.env.get(fieldMimeTypesKey); ok {
		l.fieldTypes = stringsOf(v)
	}
	return l
}

// MultipartWithLimits runs the task with the limits the record sets; the
// others stay as they are.
func MultipartWithLimits(fb *Fiber, limits, task Value) Value {
	l := record(limits)
	inner := *fb
	for _, k := range [][2]string{{"maxParts", maxPartsKey}, {"maxFieldSize", maxFieldSizeKey}, {"maxFileSize", maxFileSizeKey}, {"maxTotalSize", maxBodySizeKey}} {
		if v, ok := some(l.Get(k[0])); ok {
			inner.env = inner.env.with(k[1], v)
		}
	}
	if types := List(l.Get("fieldMimeTypes")); len(types) > 0 {
		inner.env = inner.env.with(fieldMimeTypesKey, types)
	}
	return Run(&inner, task)
}

// persister builds the form, as Effect's `toPersisted`: a field's values
// a string or a list of them, a file's a list of PersistedFile records.
type persister struct {
	fb   *Fiber
	form *Record
	dir  string
	used map[string]bool
	n    int
}

func (p *persister) field(info partInfo, value []byte) {
	p.addField(info.name, decodeField(info.charset, value))
}

func (p *persister) addField(key, value string) {
	switch old := recordField(p.form, key).(type) {
	case nil:
		p.form.put(key, value)
	case string:
		p.form.put(key, []Value{old, value})
	default:
		p.form.put(key, append(List(old), value))
	}
}

func (p *persister) file(info partInfo) (io.WriteCloser, bool) {
	name := info.name
	if info.hasFilename {
		name = info.filename
	}
	if name == "" {
		return nil, true
	}
	if p.dir == "" {
		dir, err := os.MkdirTemp("", "")
		if err != nil {
			return nil, false
		}
		p.dir = dir
		removeWhenClosed(p.fb, dir)
	}
	base := persistedFileName(name)
	path := filepath.Join(p.dir, base)
	for p.used[path] {
		path = filepath.Join(p.dir, strconv.Itoa(p.n)+"-"+base)
		p.n++
	}
	p.used[path] = true
	file := Rec("key", info.name, "name", name, "contentType", info.contentType, "path", path)
	if old, ok := recordField(p.form, info.name).([]Value); ok {
		p.form.put(info.name, append(old, file))
	} else {
		p.form.put(info.name, []Value{file})
	}
	f, err := os.Create(path)
	return f, err == nil
}

// persistedFileName is the last 128 UTF-8 bytes of the base name, whole
// characters; `file` for an empty or a dot name.
func persistedFileName(name string) string {
	base := filepath.Base(name)
	if base == "/" || base == string(filepath.Separator) {
		base = ""
	}
	cut := len(base)
	for cut > 0 {
		_, size := utf8.DecodeLastRuneInString(base[:cut])
		if len(base)-(cut-size) > 128 {
			break
		}
		cut -= size
	}
	base = base[cut:]
	if base == "" || base == "." || base == ".." {
		return "file"
	}
	return base
}

// decodeField is the text of a field's bytes in its charset.
//
// shortcut: every charset reads as UTF-8; decode the others TextDecoder knows when a program sends them
func decodeField(charset string, value []byte) string { return decodeText(value) }

// formBody is the body of the request a form is read from, and its
// headers.
func (q *ServerRequest) formBody(fb *Fiber) (io.ReadCloser, *Record) {
	s := q.src
	if s.client == nil {
		return s.r.Body, s.headers()
	}
	body, _, err := s.client.body.reader(fb)
	if err != nil {
		rethrow(err)
		Fail(multipartError(reasonInternal))
	}
	return body, q.headerRecord()
}

// readForm reads the request's form.
func (q *ServerRequest) readForm(fb *Fiber) Value {
	s := q.src
	p := &persister{fb: fb, form: newRecord(4), used: map[string]bool{}}
	if s.client != nil && s.client.body.kind == bodyForm {
		for _, pair := range s.client.body.form {
			k, v := pairOf(pair)
			p.addField(wellFormed(k), wellFormed(v))
		}
		return p.form
	}
	body, headers := q.formBody(fb)
	defer body.Close()
	ct, ok := recordField(headers, "content-type").(string)
	if reason := parseForm(body, ct, ok, limitsOf(fb), p); reason != "" {
		return Fail(multipartError(reason))
	}
	return p.form
}

// persisted is the request's form, read once.
func (q *ServerRequest) persisted(fb *Fiber) Value {
	s := q.src
	s.formOnce.Do(func() { s.form, s.formCause = catch(func() Value { return q.readForm(fb) }) })
	if s.formCause != nil {
		panic(&Failure{s.formCause})
	}
	return s.form
}

func MultipartPersisted(fb *Fiber, r Value) Value { return requestOf(r).persisted(fb) }

// isFiles is whether a value of the form is that of files.
func isFiles(v Value) bool {
	xs, ok := v.([]Value)
	if !ok || len(xs) == 0 {
		return false
	}
	_, ok = xs[0].(*Record)
	return ok
}

func MultipartFields(persisted Value) Value {
	form := record(persisted)
	out := make([]Value, 0, len(form.keys))
	for i, k := range form.keys {
		switch v := form.vals[i].(type) {
		case string:
			out = append(out, []Value{k, []Value{v}})
		default:
			if !isFiles(v) {
				out = append(out, []Value{k, v})
			}
		}
	}
	return out
}

func MultipartFiles(persisted Value) Value {
	form := record(persisted)
	out := []Value{}
	for i, k := range form.keys {
		if isFiles(form.vals[i]) {
			out = append(out, []Value{k, form.vals[i]})
		}
	}
	return out
}

func MultipartSchemaPersisted(fb *Fiber, c, persisted Value) Value {
	return CodecDecode(fb, c, persisted)
}

// MultipartSchemaJson decodes the JSON text of the field, "" unless it is
// one string.
func MultipartSchemaJson(fb *Fiber, c, field, persisted Value) Value {
	text, _ := recordField(record(persisted), field.(string)).(string)
	return CodecDecode(fb, c, FromResult(JsonParse(text)))
}

func MultipartSchemaBodyMultipart(fb *Fiber, c, r Value) Value {
	return CodecDecode(fb, c, requestOf(r).persisted(fb))
}

// MultipartSchemaBodyForm decodes a multipart form, or any other body as
// URL parameters.
func MultipartSchemaBodyForm(fb *Fiber, c, r Value) Value {
	if ct, _ := requestOf(r).header("content-type"); strings.Contains(strings.ToLower(ct), "multipart/form-data") {
		return MultipartSchemaBodyMultipart(fb, c, r)
	}
	return RequestSchemaBodyUrlParams(fb, c, r)
}

func MultipartSchemaBodyFormJson(fb *Fiber, c, field, r Value) Value {
	return MultipartSchemaJson(fb, c, field, requestOf(r).persisted(fb))
}

// partInfoOf is the parser's info of a Flick `PartInfo`.
func partInfoOf(v Value) partInfo {
	r := record(v)
	info := partInfo{name: r.Get("name").(string), contentType: r.Get("contentType").(string)}
	if f, ok := some(r.Get("filename")); ok {
		info.filename, info.hasFilename = f.(string), true
	}
	for _, p := range List(r.Get("contentTypeParameters")) {
		if k, v := pairOf(p); k == "charset" {
			info.charset = v
		}
	}
	return info
}

func MultipartDefaultIsFile(info Value) Value { return defaultIsFile(partInfoOf(info)) }

func MultipartDecodeField(info, bytes Value) Value {
	return decodeField(partInfoOf(info).charset, []byte(byteString(bytes)))
}
