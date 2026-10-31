package flickrt

import (
	"math/rand"
	"reflect"
	"sort"
	"testing"
)

// The trie agrees with a sorted map through random inserts and removes, and
// every earlier version stays as it was.
func TestTrieAgreesWithAMap(t *testing.T) {
	rng := rand.New(rand.NewSource(7))
	alphabet := []string{"a", "b", "c", "é", "😀"}
	word := func() string {
		s := ""
		for n := rng.Intn(4); n >= 0; n-- {
			s += alphabet[rng.Intn(len(alphabet))]
		}
		return s
	}
	type version struct {
		trie Value
		want map[string]Value
	}
	trie, want := TrieEmpty, map[string]Value{}
	versions := []version{}
	for i := 0; i < 400; i++ {
		k := word()
		if rng.Intn(3) == 0 {
			trie = TrieRemove(k, trie)
			delete(want, k)
		} else {
			trie = TrieInsert(k, int32(i), trie)
			want[k] = int32(i)
		}
		copied := map[string]Value{}
		for k, v := range want {
			copied[k] = v
		}
		versions = append(versions, version{trie, copied})
	}
	for _, v := range versions {
		keys := make([]string, 0, len(v.want))
		for k := range v.want {
			keys = append(keys, k)
		}
		sort.Slice(keys, func(i, j int) bool { return CompareStrings(keys[i], keys[j]) < 0 })
		got := List(TrieKeys(v.trie))
		if len(got) != len(keys) || TrieSize(v.trie) != int32(len(keys)) {
			t.Fatalf("keys %v, size %v, want %v", got, TrieSize(v.trie), keys)
		}
		for i, k := range keys {
			if got[i] != k || !reflect.DeepEqual(TrieGet(k, v.trie), Some(v.want[k])) {
				t.Fatalf("key %d: %v, want %v", i, got[i], k)
			}
		}
	}
}

func TestTriePrefixes(t *testing.T) {
	trie := TrieFromList([]Value{
		[]Value{"she", 1.0}, []Value{"shells", 2.0}, []Value{"sea", 3.0}, []Value{"sells", 4.0},
	})
	if got := List(TrieKeysWithPrefix("sh", trie)); !reflect.DeepEqual(got, []Value{"she", "shells"}) {
		t.Errorf("keys with prefix: %v", got)
	}
	if got := TrieLongestPrefixOf("shellfish", trie); !reflect.DeepEqual(got, Some([]Value{"she", 1.0})) {
		t.Errorf("longest prefix: %v", got)
	}
	if got := TrieGet("se", trie); got != None {
		t.Errorf("get of an inner key: %v", got)
	}
	if got := TrieSize(TrieRemove("se", trie)); got != int32(4) {
		t.Errorf("removing a missing key: size %v", got)
	}
}
