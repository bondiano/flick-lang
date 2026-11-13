package flickrt

import (
	"errors"
	"strings"
	"testing"
)

func TestRPCJsonRpcPreservesDiscard(t *testing.T) {
	// Exact request emitted by the pinned Effect4.1.2 layerJsonRpc parser.
	const wire = `{"jsonrpc":"2.0","method":"work","params":{"a":1},"id":"7","discard":true}`
	parser := serializationOf(RpcJsonRpc).parser()
	messages, err := parser.decode([]byte(wire))
	if err != nil || len(messages) != 1 || msgField(messages[0], "discard") != true {
		t.Fatalf("discard decoding: %v, %v", messages, err)
	}
	request := Rec("_tag", "Request", "id", "7", "tag", "work", "payload", Rec("a", 1.0), "headers", []Value{}, "discard", true)
	if encoded := string(parser.encode([]Value{request}, false)); encoded != wire {
		t.Fatalf("discard encoding: %s", encoded)
	}
}

func TestRPCNdjsonRejectsMalformedFramesAndResets(t *testing.T) {
	for _, source := range []string{"not json\n", " \n", "{\"_tag\":\"Ping\"}\nbroken\n"} {
		parser := &ndjsonParser{}
		if _, err := parser.decode([]byte(source)); err == nil {
			t.Fatalf("accepted malformed nonempty frame %q", source)
		} else if defect := errorDefect(err); msgField(defect, "name") != "NdjsonDecodeError" || msgField(msgField(defect, "cause"), "name") != "SyntaxError" {
			t.Fatalf("malformed frame defect: %v", defect)
		}
		next, err := parser.decode([]byte("{\"_tag\":\"Pong\"}\n"))
		if err != nil || len(next) != 1 || msgTag(next[0]) != "Pong" {
			t.Fatalf("parser did not reset after error: %v, %v", next, err)
		}
	}
	parser := &ndjsonParser{}
	if messages, err := parser.decode([]byte("\n\r\n")); err != nil || len(messages) != 0 {
		t.Fatalf("empty frames: %v, %v", messages, err)
	}
	if contentType := serializationOf(RpcNdjson).contentType; contentType != "application/x-ndjson" {
		t.Fatalf("content type: %s", contentType)
	}
}

func TestRPCNdjsonLimitCountsUTF16Units(t *testing.T) {
	parser := &ndjsonParser{}
	// More than16MiB of UTF-8, below the unchanged16Mi UTF-16-unit limit.
	source := "\"" + strings.Repeat("é", rpcMaxBuffer/2) + "\"\n"
	if messages, err := parser.decode([]byte(source)); err != nil || len(messages) != 1 {
		t.Fatalf("UTF-16 bound rejected valid frame: %v", err)
	}
	if _, err := parser.decode([]byte(strings.Repeat("x", rpcMaxBuffer+1))); !errors.Is(err, errMaxBuffer) {
		t.Fatalf("limit did not reject oversized ASCII frame: %v", err)
	}
	if messages, err := parser.decode([]byte("null\n")); err != nil || len(messages) != 1 {
		t.Fatalf("limit failure did not reset parser: %v", err)
	}
}
