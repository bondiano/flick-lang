package flickrt

import (
	"encoding/json"
	"math"
	"os"
	"strings"
	"testing"
)

// testdata/json_errors.txt is Node's JSON.parse message for each input.
func TestJsonParseErrorsReadAsNodes(t *testing.T) {
	data, err := os.ReadFile("testdata/json_errors.txt")
	if err != nil {
		t.Fatal(err)
	}
	for _, line := range strings.Split(strings.TrimSpace(string(data)), "\n") {
		quoted, want, _ := strings.Cut(line, "\t")
		var input string
		if err := json.Unmarshal([]byte(quoted), &input); err != nil {
			t.Fatal(err)
		}
		got := "ok"
		if r := JsonParse(input); !isOk(r) {
			got = strings.TrimPrefix(List(value(r).(*Data).Fields[0])[0].(*Record).Get("message").(string), "not JSON: ")
		}
		if got != want {
			t.Errorf("%s: got %q, want %q", quoted, got, want)
		}
	}
}

func TestJsonRoundTrips(t *testing.T) {
	text := `{"b":[true,null,-0,2.5,1e21,"\ud83d\ude00\ud83d"],"2":{},"1":"x\n"}`
	got := JsonStringify(value(JsonParse(text)))
	want := `{"1":"x\n","2":{},"b":[true,null,0,2.5,1e+21,"😀\ud83d"]}`
	if got != want {
		t.Fatalf("got %s, want %s", got, want)
	}
}

// `Json.json` is Effect's `Schema.Json`: a tree with a non-finite number
// anywhere is no JSON value, and the whole input is what it names.
func TestJsonCodecRefusesNonFiniteNumbers(t *testing.T) {
	codec := codecOf(JsonCodec)
	if _, issues := codec.decode([]Value{1.0, Rec("a", JsonNull, "b", "x")}); issues != nil {
		t.Errorf("a JSON tree: %v", issues)
	}
	for message, input := range map[string]Value{
		"expected JSON value, got an array":  []Value{1.0, math.Inf(1)},
		"expected JSON value, got an object": Rec("a", []Value{math.NaN()}),
		"expected JSON value, got NaN":       math.NaN(),
	} {
		if _, issues := codec.decode(input); len(issues) != 1 || issues[0].message != message {
			t.Errorf("%v: %v, want %q", input, issues, message)
		}
	}
}
