package flickrt

import (
	"encoding/hex"
	"testing"
)

func TestRPCUnknownUnionMembersFollowRequiredness(t *testing.T) {
	// Independent Effect 4.1.2 frames: writer union A|B, reader union A|C.
	member := func(tag string) *binNode {
		return binStructOf([]binSentinel{{"_tag", tag}}, []*binField{binFieldOf("n", false, binLeaf(binNumber))})
	}
	union := binUnionOf([]*binNode{member("A"), member("C")})
	for _, optional := range []bool{true, false} {
		layout := binStructOf(nil, []*binField{binFieldOf("maybe", optional, union)})
		data, _ := hex.DecodeString(rpcBinaryOracle(t, "unknown union field"))
		out, err := binDecode(layout, data)
		if optional {
			if err != nil || len(record(out).keys) != 0 {
				t.Fatalf("optional member: %v, %v", out, err)
			}
		} else if err == nil {
			t.Fatal("required unknown member was accepted")
		}
	}
	data, _ := hex.DecodeString(rpcBinaryOracle(t, "unknown union element"))
	if _, err := binDecode(&binNode{kind: binArray, rest: union}, data); err == nil {
		t.Fatal("unknown required array element was accepted")
	}
}
