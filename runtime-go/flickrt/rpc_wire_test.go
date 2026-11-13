package flickrt

import (
	"bufio"
	"net"
	"strings"
	"sync"
	"testing"
)

// The JSON serializations against what Effect's server wrote for the same
// messages: a batch answered together, a failure's cause as JSON-RPC data,
// a defect, and NDJSON split anywhere across reads.

func decodeAll(t *testing.T, p rpcParser, text string) []Value {
	t.Helper()
	msgs, err := p.decode([]byte(text))
	if err != nil {
		t.Fatal(err)
	}
	return msgs
}

func TestJsonRpcBatchIsAnsweredTogether(t *testing.T) {
	p := serializationOf(RpcJsonRpc).parser()
	msgs := decodeAll(t, p, `[{"jsonrpc":"2.0","method":"double","params":1,"id":1},{"jsonrpc":"2.0","method":"double","params":2,"id":2}]`)
	if len(msgs) != 2 || msgTag(msgs[0]) != "Request" || msgField(msgs[1], "tag") != "double" {
		t.Fatalf("decoded %s", JsonStringify(msgs))
	}
	out := p.encode([]Value{exitMsg(1.0, exitSuccessTree(2.0)), exitMsg(2.0, exitSuccessTree(4.0))}, true)
	want := `[{"jsonrpc":"2.0","id":1,"result":2},{"jsonrpc":"2.0","id":2,"result":4}]`
	if string(out) != want {
		t.Fatalf("got %s\nwant %s", out, want)
	}
}

func TestJsonRpcFailureAndDefect(t *testing.T) {
	p := serializationOf(RpcNdjsonRpc).parser()
	fail := exitMsg(1.0, exitFailureTree(failReason(Rec("_tag", "NotFound", "_0", Rec("id", -1.0)))))
	want := `{"jsonrpc":"2.0","id":1,"error":{"_tag":"Cause","code":0,"message":"[{\"_tag\":\"Fail\",\"error\":{\"_tag\":\"NotFound\",\"_0\":{\"id\":-1}}}]","data":[{"_tag":"Fail","error":{"_tag":"NotFound","_0":{"id":-1}}}]}}` + "\n"
	if got := string(p.encode([]Value{fail}, false)); got != want {
		t.Fatalf("got %s\nwant %s", got, want)
	}
	want = `{"jsonrpc":"2.0","id":-32603,"error":{"_tag":"Defect","code":1,"message":"A defect occurred","data":"kaboom"}}` + "\n"
	if got := string(p.encode([]Value{defectMsg("kaboom")}, false)); got != want {
		t.Fatalf("got %s\nwant %s", got, want)
	}
	back := decodeAll(t, p, want)
	if len(back) != 1 || msgTag(back[0]) != "Defect" || msgField(back[0], "defect") != "kaboom" {
		t.Fatalf("decoded %s", JsonStringify(back))
	}
}

func TestNdjsonSplitAcrossReads(t *testing.T) {
	text := `{"_tag":"Chunk","requestId":5,"values":[1,"é"]}` + "\n\r\n" + `{"_tag":"Pong"}` + "\n"
	for size := 1; size <= len(text); size++ {
		p := serializationOf(RpcNdjson).parser()
		var got []Value
		for i := 0; i < len(text); i += size {
			got = append(got, decodeAll(t, p, text[i:min(i+size, len(text))])...)
		}
		if s := JsonStringify(got).(string); s != `[{"_tag":"Chunk","requestId":5,"values":[1,"é"]},{"_tag":"Pong"}]` {
			t.Fatalf("in reads of %d: %s", size, s)
		}
	}
}

// A message a client masks and fragments reads whole on the server, a
// ping between its fragments answered.
func TestWebsocketFrames(t *testing.T) {
	a, b := net.Pipe()
	var helpers sync.WaitGroup
	helpers.Add(2)
	defer func() { a.Close(); b.Close(); helpers.Wait() }()
	client := &wsConn{conn: a, r: bufio.NewReader(a), client: true}
	server := &wsConn{conn: b, r: bufio.NewReader(b)}
	go func() {
		defer helpers.Done()
		client.writeFrame(1, []byte(strings.Repeat("x", 200)))
		client.writeFrame(9, []byte("hi"))
		// a fragmented message: text, then its continuation
		client.wmu.Lock()
		head := []byte{0x01, 0x80 | 2, 0, 0, 0, 0, 'a', 'b'}
		tail := []byte{0x80, 0x80 | 1, 1, 1, 1, 1, 'c' ^ 1}
		client.conn.Write(append(head, tail...))
		client.wmu.Unlock()
	}()
	go func() {
		defer helpers.Done()
		// the pong of the ping
		client.read()
	}()
	first, err := server.read()
	if err != nil || len(first) != 200 {
		t.Fatalf("first: %d %v", len(first), err)
	}
	second, err := server.read()
	if err != nil || string(second) != "abc" {
		t.Fatalf("second: %q %v", second, err)
	}
}
