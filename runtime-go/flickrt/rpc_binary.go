package flickrt

import (
	"encoding/binary"
	"errors"
)

// The frames of the `binary` RPC serialization: each is its length as a
// uvarint, an envelope byte, then the value. RPC messages are written in
// fingerprint mode, their holes (payload, chunk, exit, defect) as frames of
// their own without one, as RpcSerialization.layerSchemaBinary() does.

const (
	binPlain       = 0x20     // version 2, no flags
	binFingerprint = 0x21     // version 2, a layout fingerprint follows
	binMaxFrame    = 16 << 20 // Effect's default frame limit
)

func (w *binWriter) frame(n *binNode, v Value, envelope byte, fingerprint []byte) error {
	mark := w.begin()
	w.buf = append(append(w.buf, envelope), fingerprint...)
	if err := w.value(n, v); err != nil {
		return err
	}
	w.end(mark, 1)
	return nil
}

// binEncode writes a hole: one frame of `v`, a JSON tree as the JSON
// serializations carry it.
func binEncode(n *binNode, v Value) ([]byte, error) {
	w := &binWriter{}
	if err := w.frame(n, v, binPlain, nil); err != nil {
		return nil, err
	}
	return w.buf, nil
}

// binDecode reads a hole written by binEncode or a JS peer.
func binDecode(n *binNode, b []byte) (Value, error) {
	size, k := binary.Uvarint(b)
	if k <= 0 || size == 0 || size != uint64(len(b)-k) {
		return nil, binInvalidData("frame length")
	}
	return binFrame(n, b[k:], false)
}

func binFrame(n *binNode, body []byte, positional bool) (Value, error) {
	r := &binReader{b: body, end: len(body), positional: positional}
	envelope, fingerprint := byte(binPlain), []byte(nil)
	if positional {
		envelope, fingerprint = binFingerprint, binEnvelopeFingerprint
	}
	if b, err := r.byte(); err != nil || b != envelope {
		return nil, binInvalidData("envelope byte")
	}
	if got, err := r.take(uint64(len(fingerprint))); err != nil || string(got) != string(fingerprint) {
		return nil, binInvalidData("layout fingerprint")
	}
	return r.checked(n)
}

// binEncodeMessages writes RPC messages, records shaped as the JSON
// serialization writes them with each hole as bytes, frame after frame.
func binEncodeMessages(msgs []Value) []byte {
	w := &binWriter{positional: true}
	for _, m := range msgs {
		if err := w.frame(binEnvelope, m, binFingerprint, binEnvelopeFingerprint); err != nil {
			panic(Defect{"an RPC message the binary serialization cannot write: " + err.Error()})
		}
	}
	return w.buf
}

// binParser reads RPC messages from bytes that may split frames anywhere.
// After an error it only gives that error.
type binParser struct {
	buf []byte
	err error
}

func newBinParser() *binParser { return &binParser{} }

// feed is the messages of the frames `b` completes, in order; on an error,
// those before it with the error.
func (p *binParser) feed(b []byte) ([]Value, error) {
	if p.err != nil {
		return nil, p.err
	}
	p.buf = append(p.buf, b...)
	var out []Value
	pos := 0
	for {
		size, k := binary.Uvarint(p.buf[pos:])
		if k == 0 {
			break // the length is not complete yet
		}
		if k < 0 || size == 0 || size > binMaxFrame {
			p.err = errors.New("binary: invalid frame length")
			break
		}
		if size > uint64(len(p.buf)-pos-k) {
			break
		}
		start := pos + k
		pos = start + int(size)
		v, err := binFrame(binEnvelope, p.buf[start:pos], true)
		if err != nil {
			p.err = err
			break
		}
		out = append(out, v)
	}
	p.buf = append(p.buf[:0], p.buf[pos:]...)
	return out, p.err
}
