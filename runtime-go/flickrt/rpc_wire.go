package flickrt

import (
	"errors"
	"strconv"
	"strings"
)

// The serializations of Effect's RpcSerialization: how the messages of
// the protocol, JSON trees in the key order Effect writes them, become
// bytes. Each transport makes a parser per connection (per call over
// HTTP), which keeps what a message split across reads left over.

type rpcSerialization struct {
	contentType string
	framed      bool // whether messages can be told apart in a byte stream
	binary      bool // whether the holes of a message are bytes of their own
	parser      func() rpcParser
}

type rpcParser interface {
	// decode is the messages `data` completes; an error is what Effect throws.
	decode(data []byte) ([]Value, error)
	// encode is the bytes of one message, or of several written as one
	// document (a response over HTTP); nil when there is nothing to send.
	encode(msgs []Value, many bool) []byte
}

func serializationOf(v Value) *rpcSerialization { return v.(*rpcSerialization) }

var (
	RpcNdjson  Value = &rpcSerialization{"application/x-ndjson", true, false, func() rpcParser { return &ndjsonParser{} }}
	RpcJson    Value = &rpcSerialization{"application/json", false, false, func() rpcParser { return jsonDocParser{} }}
	RpcJsonRpc Value = &rpcSerialization{"application/json", false, false, func() rpcParser {
		return &jsonRpcParser{batches: map[string]*jsonRpcBatch{}}
	}}
	RpcNdjsonRpc Value = &rpcSerialization{"application/json-rpc", true, false, func() rpcParser {
		return &jsonRpcParser{framed: &ndjsonParser{}, batches: map[string]*jsonRpcBatch{}}
	}}
	RpcBinary Value = &rpcSerialization{"application/vnd.effect.rpc+schema-binary", true, true, func() rpcParser {
		return &binaryParser{newBinParser()}
	}}
)

// rpcMaxBuffer is the most an NDJSON line may hold, Effect's default.
const rpcMaxBuffer = 16 * 1024 * 1024

var errMaxBuffer = errors.New("RPC serialization buffer exceeded the maximum size of 16777216")

// jsSyntaxError is the error JSON.parse throws, as Effect's defect has it.
type jsSyntaxError struct{ message string }

func (e *jsSyntaxError) Error() string { return "SyntaxError: " + e.message }

type ndjsonDecodeError struct{ cause *jsSyntaxError }

func (*ndjsonDecodeError) Error() string { return "NdjsonDecodeError" }

// ndjsonParser reads one JSON value per nonempty line. Malformed frames
// fail and reset buffering, as the pinned Effect serializer does.
type ndjsonParser struct {
	text   textDecoder
	buffer string
}

func (p *ndjsonParser) decode(data []byte) ([]Value, error) {
	p.buffer += p.text.decode(data)
	var out []Value
	for {
		nl := strings.IndexByte(p.buffer, '\n')
		if nl < 0 {
			break
		}
		if Length(p.buffer[:nl]).(int32) > rpcMaxBuffer {
			p.buffer = ""
			return nil, errMaxBuffer
		}
		line := strings.TrimSuffix(p.buffer[:nl], "\r")
		if line != "" {
			v, reason := parseJSON(line)
			if reason != "" {
				p.buffer = ""
				return nil, &ndjsonDecodeError{&jsSyntaxError{reason}}
			}
			out = append(out, v)
		}
		p.buffer = p.buffer[nl+1:]
	}
	if Length(p.buffer).(int32) > rpcMaxBuffer {
		p.buffer = ""
		return nil, errMaxBuffer
	}
	return out, nil
}

func (p *ndjsonParser) encode(msgs []Value, _ bool) []byte {
	if len(msgs) == 0 {
		return nil
	}
	var b strings.Builder
	for _, m := range msgs {
		stringify(&b, m)
		b.WriteByte('\n')
	}
	return []byte(b.String())
}

// jsonDocParser is one JSON document: a message, or an array of them.
type jsonDocParser struct{}

func parseDocument(data []byte) (Value, error) {
	v, reason := parseJSON(decodeUTF8(data))
	if reason != "" {
		return nil, &jsSyntaxError{reason}
	}
	return v, nil
}

func (jsonDocParser) decode(data []byte) ([]Value, error) {
	v, err := parseDocument(data)
	if err != nil {
		return nil, err
	}
	if xs, ok := v.([]Value); ok {
		return xs, nil
	}
	return []Value{v}, nil
}

func (jsonDocParser) encode(msgs []Value, many bool) []byte {
	if many {
		return []byte(JsonStringify(msgs).(string))
	}
	return []byte(JsonStringify(msgs[0]).(string))
}

// binaryParser is Effect's SchemaBinary frames, each message's holes
// written by the layouts of its procedure (rpc_binary.go).
type binaryParser struct{ p *binParser }

func (b *binaryParser) decode(data []byte) ([]Value, error) { return b.p.feed(data) }

func (b *binaryParser) encode(msgs []Value, _ bool) []byte {
	if len(msgs) == 0 {
		return nil
	}
	return binEncodeMessages(msgs)
}

// The messages, in the key order of Effect's encoded messages.

func msgTag(m Value) string {
	if r, ok := m.(*Record); ok {
		if t, ok := recordField(r, "_tag").(string); ok {
			return t
		}
	}
	return ""
}

func msgField(m Value, k string) Value {
	if r, ok := m.(*Record); ok {
		return recordField(r, k)
	}
	return nil
}

// idKey is a request id as a key: `1` and `"1"` are different ids.
func idKey(id Value) string { return JsonStringify(id).(string) }

func exitMsg(id, exit Value) Value    { return Rec("_tag", "Exit", "requestId", id, "exit", exit) }
func chunkMsg(id, values Value) Value { return Rec("_tag", "Chunk", "requestId", id, "values", values) }
func defectMsg(defect Value) Value    { return Rec("_tag", "Defect", "defect", defect) }

var (
	pongMsg = Rec("_tag", "Pong")
	pingMsg = Rec("_tag", "Ping")
	eofMsg  = Rec("_tag", "Eof")
)

// The JSON trees of Effect's Schema.Exit and Schema.Defect.

func exitSuccessTree(v Value) Value { return Rec("_tag", "Success", "value", v) }

func exitFailureTree(reasons ...Value) Value { return Rec("_tag", "Failure", "cause", reasons) }

func failReason(err Value) Value { return Rec("_tag", "Fail", "error", err) }

func dieReason(defect Value) Value { return Rec("_tag", "Die", "defect", defect) }

func interruptReason(fiber int64) Value {
	return Rec("_tag", "Interrupt", "fiberId", float64(fiber))
}

// defectTree is a defect of the go target as Effect writes the error the
// JS target dies with: `{ name, message }` for an error, the text itself
// for a message.
func defectTree(defect string) Value {
	text, _, _ := strings.Cut(defect, "\n    at ")
	text = strings.TrimSuffix(text, "\n")
	if name, message, ok := strings.Cut(text, ": "); ok {
		for _, n := range jsErrorNames {
			if n == name {
				return Rec("name", name, "message", message)
			}
		}
	}
	return text
}

// wireDefectText is the defect of a tree another side sent.
func wireDefectText(tree Value) string {
	switch x := tree.(type) {
	case string:
		return x
	case *Record:
		name, _ := recordField(x, "name").(string)
		message, _ := recordField(x, "message").(string)
		if name == "" {
			return message
		}
		return name + ": " + message
	}
	return JsonStringify(tree).(string)
}

// errorDefect is what a thrown error of a parser is as a defect.
func errorDefect(err error) Value {
	var ndjson *ndjsonDecodeError
	if errors.As(err, &ndjson) {
		return Rec("name", "NdjsonDecodeError", "message", "", "cause", errorDefect(ndjson.cause))
	}
	if errors.Is(err, errMaxBuffer) {
		return Rec("name", "MaxBufferSizeExceeded", "message", err.Error())
	}
	var syntax *jsSyntaxError
	if errors.As(err, &syntax) {
		return Rec("name", "SyntaxError", "message", syntax.message)
	}
	return Rec("name", "Error", "message", err.Error())
}

// rpcNumberOf is JS's Number of a JSON value.
func rpcNumberOf(v Value) Value {
	switch x := v.(type) {
	case float64:
		return x
	case bool:
		if x {
			return 1.0
		}
		return 0.0
	case *jsonNull:
		return 0.0
	case string:
		t := strings.TrimSpace(x)
		if t == "" {
			return 0.0
		}
		if n, err := strconv.ParseFloat(t, 64); err == nil {
			return n
		}
	}
	return nil // NaN, written null
}
