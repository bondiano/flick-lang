package flickrt

import (
	"strconv"
	"testing"
)

var benchmarkChunkResult Value

func BenchmarkPersistentChunk(b *testing.B) {
	for _, size := range []int{1024, 8192} {
		b.Run(strconv.Itoa(size), func(b *testing.B) {
			values := make([]Value, size)
			for i := range values {
				values[i] = int32(i)
			}
			parts := make([]Value, size/16)
			for i := range parts {
				parts[i] = ChunkFromList(values[i*16 : (i+1)*16])
			}
			// Build a chunk through repeated concatenation for all read workloads.
			input := ChunkEmpty
			for _, part := range parts {
				input = ChunkConcat(input, part)
			}
			keepElement := Curry2(func(_ *Fiber, _, x Value) Value { return x })
			b.Run("Append", func(b *testing.B) {
				for range b.N {
					chunk := ChunkEmpty
					for _, x := range values {
						chunk = ChunkAppend(x, chunk)
					}
					benchmarkChunkResult = chunk
				}
			})
			b.Run("Concat16", func(b *testing.B) {
				for range b.N {
					chunk := ChunkEmpty
					for _, part := range parts {
						chunk = ChunkConcat(chunk, part)
					}
					benchmarkChunkResult = chunk
				}
			})
			b.Run("Get", func(b *testing.B) {
				for i := range b.N {
					benchmarkChunkResult = ChunkGet(int32(i&(size-1)), input)
				}
			})
			b.Run("Reduce", func(b *testing.B) {
				for range b.N {
					benchmarkChunkResult = ChunkReduce(nil, Unit, keepElement, input)
				}
			})
			b.Run("ToList", func(b *testing.B) {
				for range b.N {
					benchmarkChunkResult = ChunkToList(input)
				}
			})
		})
	}
}
