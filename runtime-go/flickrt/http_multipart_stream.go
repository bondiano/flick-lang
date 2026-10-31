package flickrt

import (
	"io"
)

// Forms read part by part, as Effect's Multipart streams them: a field
// whole, a file as a stream of its bytes that must be read before the next
// part, the rest of it skipped otherwise.

var (
	partFieldTag = Tag("Http.Multipart.Part.Field")
	partFileTag  = Tag("Http.Multipart.Part.File")
)

func fieldPart(key, contentType, value string) Value {
	return NewData(partFieldTag, Rec("key", key, "contentType", contentType, "value", value))
}

// formParts pulls the parts of the form one at a time.
func formParts(f *formReader) pull {
	return func(*Fiber) ([]Value, bool) {
		info, isFile, field, more, reason := f.next()
		if reason != "" {
			Fail(multipartError(reason))
		}
		if !more {
			return nil, false
		}
		if !isFile {
			return []Value{fieldPart(info.name, info.contentType, decodeField(info.charset, field))}, true
		}
		name := info.name
		if info.hasFilename {
			name = info.filename
		}
		return []Value{NewData(partFileTag, Rec("key", info.name, "name", name, "contentType", info.contentType,
			"content", fileContent(f)))}, true
	}
}

// fileContent is the bytes of the file the form reader is at, until it
// moves past it.
func fileContent(f *formReader) *Stream {
	part := f.part
	return streamOpen(func(*Fiber) pull {
		buf := make([]byte, 32*1024)
		return func(*Fiber) ([]Value, bool) {
			for f.part == part && part != nil {
				n, _, reason := f.readFile(buf)
				if reason != "" {
					Fail(multipartError(reason))
				}
				if n > 0 {
					return []Value{byteList(buf[:n])}, true
				}
			}
			return nil, false
		}
	})
}

// formStream reads the form of the body, with the limits of the fiber the
// stream runs in.
func formStream(fb *Fiber, body io.Reader, headers *Record) pull {
	ct, ok := recordField(headers, "content-type").(string)
	f, reason := newFormReader(body, ct, ok, limitsOf(fb))
	if reason != "" {
		Fail(multipartError(reason))
	}
	return formParts(f)
}

// MultipartParts is the parts of the request's form; those of a client's
// form data are its fields.
func MultipartParts(r Value) Value {
	q := requestOf(r)
	if c := q.src.client; c != nil && c.body.kind == bodyForm {
		parts := make([]Value, len(c.body.form))
		for i, pair := range c.body.form {
			k, v := pairOf(pair)
			parts[i] = fieldPart(wellFormed(k), "text/plain", wellFormed(v))
		}
		return chunkStream(parts)
	}
	return streamOpen(func(fb *Fiber) pull {
		body, headers := q.formBody(fb)
		fb.owner.add(fb, func(*Fiber, *Cause) Value { body.Close(); return Unit })
		return formStream(fb, body, headers)
	})
}

// MultipartMakeChannel parses the chunks of bytes it is given as the form
// of the headers.
func MultipartMakeChannel(headers Value) Value {
	h := newRecord(1)
	for _, pair := range List(headers) {
		k, v := pairOf(pair)
		h.put(k, v)
	}
	return fromTransform(func(fb *Fiber, upstream Pull, _ *Owner) Pull {
		return newPull(formStream(fb, &pullReader{fb: fb, next: oldPull(upstream)}, h))
	})
}

// MultipartToPersisted is the form of the parts, its files written to a
// temporary directory the scope removes.
func MultipartToPersisted(fb *Fiber, parts Value) Value {
	p := &persister{fb: fb, form: newRecord(4), used: map[string]bool{}}
	runStream(fb, streamOf(parts), func(chunk []Value) {
		for _, part := range chunk {
			d := part.(*Data)
			r := record(d.Fields[0])
			key, contentType := r.Get("key").(string), r.Get("contentType").(string)
			if d.Tag == partFieldTag {
				p.addField(key, r.Get("value").(string))
				continue
			}
			name := r.Get("name").(string)
			if name == "" {
				continue
			}
			out, ok := p.file(partInfo{name: key, filename: name, hasFilename: true, contentType: contentType})
			if !ok {
				Fail(multipartError(reasonInternal))
			}
			persistFile(fb, out, streamOf(r.Get("content")))
		}
	})
	return p.form
}

func persistFile(fb *Fiber, out io.WriteCloser, content *Stream) {
	defer out.Close()
	runStream(fb, content, func(chunk []Value) {
		for _, bytes := range chunk {
			if _, err := io.WriteString(out, byteString(bytes)); err != nil {
				Fail(multipartError(reasonInternal))
			}
		}
	})
	if out.Close() != nil {
		Fail(multipartError(reasonInternal))
	}
}
