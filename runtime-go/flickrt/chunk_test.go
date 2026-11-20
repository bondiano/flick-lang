package flickrt

import (
	"math"
	"strconv"
	"testing"
)

func TestPersistentChunkOperations(t *testing.T) {
	const size = 2048
	values := make([]Value, size)
	for i := range values {
		values[i] = int32(i)
	}

	prefix := ChunkEmpty
	for _, x := range values[:size/2] {
		prefix = ChunkAppend(x, prefix)
	}
	snapshot := ChunkToList(prefix)
	left := prefix
	for i := size / 2; i < size; i += 16 {
		left = ChunkConcat(left, ChunkFromList(values[i:i+16]))
	}
	right := ChunkEmpty
	for i := size; i > 0; i -= 16 {
		right = ChunkConcat(ChunkFromList(values[i-16:i]), right)
	}
	assert := func(label string, got, want Value) {
		t.Helper()
		if !Equals(got, want) {
			t.Fatalf("%s: unexpected value", label)
		}
	}
	assert("old chunk", ChunkToList(prefix), values[:size/2])
	assert("old list snapshot", snapshot, values[:size/2])
	assert("left-associated concat", ChunkToList(left), values)
	assert("right-associated concat", ChunkToList(right), values)
	assert("equality across shapes", left, right)
	assert("equality with flat chunk", left, ChunkFromList(values))
	assert("concat with self", ChunkToList(ChunkConcat(prefix, prefix)), append(values[:size/2:size/2], values[:size/2]...))
	assert("concat empty", ChunkConcat(ChunkConcat(ChunkEmpty, left), ChunkEmpty), left)
	assert("non-empty conversion", ChunkNonEmpty(left), Some(left))
	assert("empty conversion", ChunkNonEmpty(ChunkEmpty), None)
	assert("head", ChunkHeadNonEmpty(left), int32(0))
	assert("last", ChunkLastNonEmpty(left), int32(size-1))
	assert("tail", ChunkToList(ChunkTailNonEmpty(left)), values[1:])
	for _, i := range []int32{0, 31, 32, size/2 - 1, size / 2, size - 1} {
		assert("index", ChunkGet(i, left), Some(i))
	}
	assert("negative index", ChunkGet(int32(-1), left), None)
	assert("index past end", ChunkGet(int32(size), left), None)
	assert("take and drop", ChunkToList(ChunkTake(int32(91), ChunkDrop(int32(996), left))), values[996:1087])
	assert("take right", ChunkToList(ChunkTakeRight(int32(57), left)), values[size-57:])
	assert("drop right", ChunkToList(ChunkDropRight(int32(57), left)), values[:size-57])
	assert("non-empty split", ChunkSplitNonEmptyAt(int32(0), left), []Value{ChunkOf(int32(0)), ChunkFromList(values[1:])})
	assert("split across leaves", ChunkSplitAt(int32(1001), left), []Value{ChunkFromList(values[:1001]), ChunkFromList(values[1001:])})

	checkOrder := Curry2(func(_ *Fiber, acc, x Value) Value {
		if got, want := x.(int32), acc.(int32); got != want {
			t.Fatalf("left fold: got %d, want %d", got, want)
		}
		return acc.(int32) + 1
	})
	checkReverseOrder := Curry2(func(_ *Fiber, acc, x Value) Value {
		if got, want := x.(int32), acc.(int32); got != want {
			t.Fatalf("right fold: got %d, want %d", got, want)
		}
		return acc.(int32) - 1
	})
	assert("left fold", ChunkReduce(nil, int32(0), checkOrder, left), int32(size))
	assert("right fold", ChunkReduceRight(nil, int32(size-1), checkReverseOrder, left), int32(-1))
}

func TestPersistentChunkTreeBalance(t *testing.T) {
	check := func(chunk *Chunk) {
		t.Helper()
		var heightOf func(*Chunk) int
		heightOf = func(node *Chunk) int {
			if node.left == nil {
				if node.right != nil || node.height != 0 {
					t.Fatalf("invalid leaf: %+v", node)
				}
				return 0
			}
			if node.right == nil || node.leaf != nil || node.left.length() == 0 || node.right.length() == 0 {
				t.Fatalf("invalid branch: %+v", node)
			}
			left, right := heightOf(node.left), heightOf(node.right)
			height := max(left, right) + 1
			if left > right+1 || right > left+1 || node.height != height {
				t.Fatalf("invalid height: cached %d, left %d, right %d", node.height, left, right)
			}
			if node.size != node.left.length()+node.right.length() {
				t.Fatalf("invalid size: %d", node.size)
			}
			return height
		}
		heightOf(chunk)
	}

	x := chunkOf(make([]Value, 100))
	y := chunkOf(make([]Value, 200))
	joined := joinChunks(x, y)
	if joined.left != x || joined.right != y {
		t.Fatal("joining large leaves must share both inputs")
	}
	if grown := ChunkAppend(int32(42), joined).(*Chunk); grown.left != joined {
		t.Fatal("appending must share the existing tree")
	}

	const size = 16384
	ascending, descending := ChunkEmpty, ChunkEmpty
	for i := range size {
		ascending = ChunkAppend(int32(i), ascending)
		descending = ChunkPrepend(int32(size-i-1), descending)
	}
	check(ascending.(*Chunk))
	check(descending.(*Chunk))
	for i := 1; i < size; i += 113 {
		prefix := ascending.(*Chunk).slice(0, i)
		suffix := descending.(*Chunk).slice(i, size)
		joined := joinChunks(prefix, suffix)
		check(joined)
		if joined.length() != size || joined.at(i-1) != int32(i-1) || joined.at(i) != int32(i) {
			t.Fatalf("join across split at %d lost element order", i)
		}
	}
}

func TestPersistentChunkConsumers(t *testing.T) {
	const size = 96
	xs := make([]Value, size)
	c := ChunkEmpty
	for i := range xs {
		xs[i] = int32(i)
		c = ChunkAppend(xs[i], c)
	}

	fb := rootFiber()
	s := SchemaChunk(SchemaInt)
	if result := SchemaValidate(fb, s, c); !isOk(result) || !Equals(value(result), c) {
		t.Fatal("schema validation lost elements")
	}
	codec := codecOf(s.(*Schema).jsonOf())
	encoded, issues := codec.encode(fb, c)
	if issues != nil || len(List(encoded)) != size {
		t.Fatalf("codec encoded an invalid chunk: %v", issues)
	}
	decoded, issues := codec.decode(fb, encoded)
	if issues != nil || !Equals(decoded, c) {
		t.Fatalf("codec round trip changed the chunk: %v", issues)
	}
	channel := ChannelFromChunk(c)
	if !Equals(ChannelRunHead(fb, channel), Some(int32(0))) {
		t.Fatal("channel did not emit its first element")
	}
	for range 2 {
		if !Equals(ChannelRunCollect(fb, channel), xs) {
			t.Fatal("channel replay lost elements")
		}
	}

	stored := TxChunkMake(c)
	TxChunkAppend(fb, int32(size), stored)
	if !Equals(TxChunkSize(fb, stored), int32(size+1)) || !Equals(ChunkToList(c), xs) {
		t.Fatal("transaction changed a prior chunk")
	}

	queue := TxQueueUnbounded()
	TxQueueOfferAll(fb, xs, queue)
	if !Equals(TxQueueSize(fb, queue), int32(size)) ||
		!Equals(TxQueuePeek(fb, queue), int32(0)) ||
		!Equals(TxQueueTakeN(fb, int32(45), queue), xs[:45]) ||
		!Equals(TxQueueTake(fb, queue), int32(45)) ||
		!Equals(TxQueueTakeAll(fb, queue), xs[46:]) {
		t.Fatal("transactional queue lost element order")
	}
}

func TestPersistentChunkLengthOverflow(t *testing.T) {
	chunk := ChunkOf(int32(42))
	for i := 1; i < strconv.IntSize-1; i++ {
		chunk = ChunkConcat(chunk, chunk)
	}
	maximum := ChunkConcat(chunk, ChunkDrop(int32(1), chunk)).(*Chunk)
	if maximum.length() != math.MaxInt || !Equals(ChunkLast(maximum), Some(int32(42))) {
		t.Fatal("largest representable length did not preserve its last element")
	}
	defer func() {
		if got := recover(); got != "Chunk length overflow" {
			t.Fatalf("concat beyond host int limit panicked with %v", got)
		}
	}()
	ChunkConcat(chunk, chunk)
}
