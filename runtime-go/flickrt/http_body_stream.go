package flickrt

import (
	"errors"
	"io"
	"math/big"
	"os"
)

// Bodies read as they are sent: a Flick stream of chunks of bytes, run in
// the fiber that sends the body, or a part of a file.

// causeError is the failure of a Flick stream read as an io.Reader, which
// a reader in a fiber fails with again.
type causeError struct{ cause *Cause }

func (e *causeError) Error() string { return "the stream failed" }

// rethrow fails with the failure of a Flick stream `err` carries, if any.
func rethrow(err error) {
	var c *causeError
	if errors.As(err, &c) {
		panic(&Failure{c.cause})
	}
}

// streamReader reads a Flick stream opened in a scope of its own, which
// closes at its end, at its failure, or when the reader is closed.
type streamReader struct {
	pullReader
	owner  *Owner
	outer  *Fiber
	closed bool
}

func (r *streamReader) finish(cause *Cause) *Cause {
	if r.closed {
		return cause
	}
	r.closed = true
	return r.owner.close(r.outer, cause)
}

func (r *streamReader) Read(p []byte) (n int, err error) {
	if r.closed {
		return 0, io.EOF
	}
	_, cause := catch(func() Value {
		Check(r.fb)
		n, err = r.pullReader.Read(p)
		return nil
	})
	if cause != nil {
		return 0, &causeError{r.finish(cause)}
	}
	if err == io.EOF {
		if cause := r.finish(nil); cause != nil {
			return n, &causeError{cause}
		}
	}
	return n, err
}

func (r *streamReader) Close() error {
	if cause := r.finish(nil); cause != nil {
		return &causeError{cause}
	}
	return nil
}

// streamOpener opens the stream of chunks of bytes in the fiber that reads
// the body.
func streamOpener(s *Stream) func(fb *Fiber) (io.ReadCloser, error) {
	return func(fb *Fiber) (io.ReadCloser, error) {
		r := &streamReader{owner: newOwner(), outer: fb}
		inner := *fb
		inner.owner = r.owner
		r.fb = &inner
		if _, cause := catch(func() Value { r.next = s.open(&inner); return nil }); cause != nil {
			return nil, &causeError{r.finish(cause)}
		}
		return r, nil
	}
}

// fileOpener opens the file at `offset`, at most `limit` bytes of it (-1:
// to its end) to be read.
func fileOpener(path string, offset, limit int64) func(*Fiber) (io.ReadCloser, error) {
	return func(*Fiber) (io.ReadCloser, error) {
		f, err := os.Open(path)
		if err != nil {
			return nil, err
		}
		if offset > 0 {
			if _, err := f.Seek(offset, io.SeekStart); err != nil {
				f.Close()
				return nil, err
			}
		}
		if limit < 0 {
			return f, nil
		}
		return struct {
			io.Reader
			io.Closer
		}{io.LimitReader(f, limit), f}, nil
	}
}

func streamBody(s Value, contentType string, length int64) *Body {
	return &Body{kind: bodyStream, hasType: true, contentType: contentType, length: length, open: streamOpener(streamOf(s))}
}

func optionalLength(v Value) int64 {
	if n, ok := some(v); ok {
		return int64(n.(float64))
	}
	return -1
}

func BodyStream(s, contentType, contentLength Value) Value {
	return streamBody(s, orType(contentType, "application/octet-stream"), optionalLength(contentLength))
}

func ClientRequestBodyStream(s, contentType, contentLength, self Value) Value {
	return ClientRequestSetBody(BodyStream(s, contentType, contentLength), self)
}

// BodyFileFromInfo is the part of the file the options choose, of the
// size the info gives; the file is opened when the body is sent.
//
// shortcut: the file is read with Go's os, not through the FileSystem service, and in chunks of Go's size, not `chunkSize`; read through the service when a program provides another one
func BodyFileFromInfo(_ *Fiber, options, path, info Value) Value {
	o := record(options)
	offset, bytesToRead := max(optionalSize(o.Get("offset")), 0), optionalSize(o.Get("bytesToRead"))
	length := max(record(info).Get("size").(*big.Int).Int64()-offset, 0)
	if bytesToRead >= 0 {
		length = min(length, bytesToRead)
	}
	return &Body{kind: bodyStream, hasType: true, contentType: orType(o.Get("contentType"), "application/octet-stream"),
		length: length, open: fileOpener(path.(string), offset, bytesToRead)}
}

// ResponseStream sends the stream's chunks as they come.
func ResponseStream(s, options Value) Value {
	o := record(options)
	h := optionHeaders(o)
	return bodyResponse(options, 200, h, *streamBody(s, contentTypeOf(o, h, "application/octet-stream"), -1))
}

func ResponseStreamText(s, options Value) Value {
	o := record(options)
	h := optionHeaders(o)
	return bodyResponse(options, 200, h, *streamBody(StreamEncodeText(s), contentTypeOf(o, h, "text/plain"), -1))
}
