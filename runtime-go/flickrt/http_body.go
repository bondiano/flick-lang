package flickrt

import (
	"bytes"
	"io"
	"mime/multipart"
	"strconv"
)

// Http.Body as Effect's HttpBody: nothing, bytes (a text body keeps the
// text it was made of), the fields of a form, or bytes read when the body
// is sent. A content type and length go with the bytes.

const (
	bodyEmpty = iota
	bodyBytes
	bodyForm
	bodyStream
)

type Body struct {
	kind        uint8
	text        bool // data is the text as given, lone surrogates and all
	hasType     bool
	contentType string
	length      int64 // -1: unknown
	data        string
	form        []Value // the pairs of a form
	open        func(fb *Fiber) (io.ReadCloser, error)
}

var (
	emptyBody          = &Body{kind: bodyEmpty, length: -1}
	BodyEmpty    Value = emptyBody
	bodyErrorTag       = Tag("Http.Body.HttpBodyError.HttpBodyError")
)

func bodyOf(v Value) *Body { return v.(*Body) }

// textBody is the text as UTF-8, its byte length that of the text as Node
// writes it, each lone surrogate a U+FFFD of as many bytes.
func textBody(text, contentType string) *Body {
	return &Body{kind: bodyBytes, text: true, hasType: true, contentType: contentType, length: int64(len(text)), data: text}
}

func bytesBody(data, contentType string) *Body {
	return &Body{kind: bodyBytes, hasType: true, contentType: contentType, length: int64(len(data)), data: data}
}

func orType(contentType Value, otherwise string) string {
	if t, ok := some(contentType); ok {
		return t.(string)
	}
	return otherwise
}

// byteString is the bytes of a `List Int`, each number taken modulo 256 as
// a Uint8Array takes it.
func byteString(list Value) string {
	xs := List(list)
	b := make([]byte, len(xs))
	for i, x := range xs {
		b[i] = byte(x.(int32))
	}
	return string(b)
}

func byteList(b []byte) Value {
	out := make([]Value, len(b))
	for i, x := range b {
		out[i] = int32(x)
	}
	return out
}

func BodyUint8Array(list, contentType Value) Value {
	return bytesBody(byteString(list), orType(contentType, "application/octet-stream"))
}

func BodyText(text, contentType Value) Value {
	return textBody(text.(string), orType(contentType, "text/plain"))
}

func jsonBody(json Value, contentType string) *Body {
	return textBody(JsonStringify(json).(string), contentType)
}

func BodyJson(_ *Fiber, json Value) Value { return jsonBody(json, "application/json") }

// BodyJsonSchema is the JSON of the value as the codec encodes it.
func BodyJsonSchema(_ *Fiber, c, v Value) Value {
	e, issues := codecOf(c).encode(v)
	if issues != nil {
		return Fail(schemaErrorOf(issues))
	}
	return jsonBody(e, "application/json")
}

func BodyUrlParams(params, contentType Value) Value {
	return textBody(formEncode(paramsOf(params)), orType(contentType, "application/x-www-form-urlencoded"))
}

// BodyFormData is a form body: its content type, with the boundary, and its
// length are the platform's to give when it is sent.
func BodyFormData(pairs Value) Value { return &Body{kind: bodyForm, length: -1, form: List(pairs)} }

func BodyContentType(b Value) Value {
	if body := bodyOf(b); body.hasType {
		return Some(body.contentType)
	}
	return None
}

func BodyContentLength(b Value) Value {
	if body := bodyOf(b); body.length >= 0 {
		return Some(float64(body.length))
	}
	return None
}

// BodyToText is the text of a body of bytes, the text it was made of if
// it was.
func BodyToText(b Value) Value {
	body := bodyOf(b)
	switch {
	case body.kind != bodyBytes:
		return None
	case body.text:
		return Some(body.data)
	}
	return Some(decodeUTF8([]byte(body.data)))
}

// bytes is what the body sends, and the content type a form body then has;
// a stream body runs in the fiber.
func (b *Body) bytes(fb *Fiber) ([]byte, string, error) {
	switch b.kind {
	case bodyBytes:
		if b.text {
			return []byte(wellFormed(b.data)), b.contentType, nil
		}
		return []byte(b.data), b.contentType, nil
	case bodyForm:
		var buf bytes.Buffer
		w := multipart.NewWriter(&buf)
		for _, p := range b.form {
			k, v := pairOf(p)
			w.WriteField(wellFormed(k), wellFormed(v))
		}
		w.Close()
		return buf.Bytes(), w.FormDataContentType(), nil
	case bodyStream:
		r, err := b.open(fb)
		if err != nil {
			return nil, b.contentType, err
		}
		defer r.Close()
		data, err := io.ReadAll(r)
		return data, b.contentType, err
	}
	return nil, "", nil
}

// reader is what the body sends, as it is read, and its content type.
func (b *Body) reader(fb *Fiber) (io.ReadCloser, string, error) {
	if b.kind == bodyStream {
		r, err := b.open(fb)
		return r, b.contentType, err
	}
	data, contentType, err := b.bytes(fb)
	return io.NopCloser(bytes.NewReader(data)), contentType, err
}

// parseContentLength is Effect's length of a `Content-Length` header: its
// digits as a safe integer, or -1.
func parseContentLength(h Value) int64 {
	s, ok := h.(string)
	if !ok || s == "" {
		return -1
	}
	for i := 0; i < len(s); i++ {
		if !isDigit(s[i]) {
			return -1
		}
	}
	n, err := strconv.ParseInt(s, 10, 64)
	if err != nil || n > 1<<53-1 {
		return -1
	}
	return n
}

// withBodyHeaders is Effect's headers of a request given the body: its
// content type and length, or neither for an empty or a form body.
func withBodyHeaders(h *Record, b *Body) *Record {
	if b.kind == bodyEmpty || b.kind == bodyForm {
		return h.without("content-type").without("content-length")
	}
	if b.hasType {
		h = h.set("content-type", b.contentType)
	} else {
		h = h.without("content-type")
	}
	if b.length >= 0 {
		return h.set("content-length", strconv.FormatInt(b.length, 10))
	}
	return h.without("content-length")
}
