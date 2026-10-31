package flickrt

// TxChunk and TxDeferred as Effect 4's: a TxRef of a Chunk, and a TxRef of
// an `Option (Result e a)`.

// TxChunk is Effect's TxChunk.
type TxChunk struct{ ref *TxRef }

func txChunkOf(v Value) *TxRef { return v.(*TxChunk).ref }

func TxChunkMake(c Value) Value          { return &TxChunk{newTxRef(c)} }
func TxChunkEmpty() Value                { return &TxChunk{newTxRef(ChunkEmpty)} }
func TxChunkFromIterable(xs Value) Value { return &TxChunk{newTxRef(chunkOf(xs))} }

func TxChunkModify(fb *Fiber, f, c Value) Value { return txModify(fb, txChunkOf(c), txPair(fb, f)) }

func TxChunkUpdate(fb *Fiber, f, c Value) Value {
	return txUpdate(fb, txChunkOf(c), func(v Value) Value { return Apply(fb, f, v) })
}

func TxChunkGet(fb *Fiber, c Value) Value { return txRead(fb, txChunkOf(c), Identity) }

func TxChunkSet(fb *Fiber, x, c Value) Value {
	return txUpdate(fb, txChunkOf(c), func(Value) Value { return x })
}

func TxChunkAppend(fb *Fiber, x, c Value) Value {
	return txUpdate(fb, txChunkOf(c), func(v Value) Value { return ChunkAppend(x, v) })
}

func TxChunkPrepend(fb *Fiber, x, c Value) Value {
	return txUpdate(fb, txChunkOf(c), func(v Value) Value { return ChunkPrepend(x, v) })
}

// Effect reads the size by `modify`, which writes the chunk back; a read
// is the same here but for not making a new version.

func TxChunkSize(fb *Fiber, c Value) Value {
	return txRead(fb, txChunkOf(c), func(v Value) Value { return int32(len(items(v))) })
}

func TxChunkIsEmpty(fb *Fiber, c Value) Value {
	return txRead(fb, txChunkOf(c), func(v Value) Value { return len(items(v)) == 0 })
}

func TxChunkIsNonEmpty(fb *Fiber, c Value) Value {
	return txRead(fb, txChunkOf(c), func(v Value) Value { return len(items(v)) > 0 })
}

func TxChunkTake(fb *Fiber, n, c Value) Value {
	return txUpdate(fb, txChunkOf(c), func(v Value) Value { return ChunkTake(n, v) })
}

func TxChunkDrop(fb *Fiber, n, c Value) Value {
	return txUpdate(fb, txChunkOf(c), func(v Value) Value { return ChunkDrop(n, v) })
}

func TxChunkSlice(fb *Fiber, start, end, c Value) Value {
	return txUpdate(fb, txChunkOf(c), func(v Value) Value {
		return ChunkTake(end.(int32)-start.(int32), ChunkDrop(start, v))
	})
}

func TxChunkMap(fb *Fiber, f, c Value) Value {
	return txUpdate(fb, txChunkOf(c), func(v Value) Value { return ChunkMap(fb, f, v) })
}

func TxChunkFilter(fb *Fiber, f, c Value) Value {
	return txUpdate(fb, txChunkOf(c), func(v Value) Value { return ChunkFilter(fb, f, v) })
}

func TxChunkAppendAll(fb *Fiber, other, c Value) Value {
	return txUpdate(fb, txChunkOf(c), func(v Value) Value { return ChunkAppendAll(other, v) })
}

func TxChunkPrependAll(fb *Fiber, other, c Value) Value {
	return txUpdate(fb, txChunkOf(c), func(v Value) Value { return ChunkConcat(other, v) })
}

// TxChunkConcat appends the chunk of `other` to that of `c`.
func TxChunkConcat(fb *Fiber, other, c Value) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value {
		that := t.get(fb, txChunkOf(other))
		t.update(fb, txChunkOf(c), func(v Value) Value { return ChunkAppendAll(that, v) })
		return Unit
	})
}

// TxDeferred is Effect's TxDeferred.
type TxDeferred struct{ ref *TxRef }

func txDeferredOf(v Value) *TxRef { return v.(*TxDeferred).ref }

func TxDeferredMake() Value { return &TxDeferred{newTxRef(None)} }

// TxDeferredAwait retries until the deferred is done, then ends as it.
func TxDeferredAwait(fb *Fiber, d Value) Value {
	return txDo(fb, func(fb *Fiber, t *tx) Value {
		done, ok := some(t.get(fb, txDeferredOf(d)))
		if !ok {
			return t.retryNow(fb)
		}
		if r := done.(*Data); r.Tag == "Failure" {
			return Fail(r.Fields[0])
		}
		return done.(*Data).Fields[0]
	})
}

func TxDeferredPoll(fb *Fiber, d Value) Value { return txRead(fb, txDeferredOf(d), Identity) }

// TxDeferredDone completes the deferred unless it is done; whether it did.
func TxDeferredDone(fb *Fiber, result, d Value) Value {
	return txModify(fb, txDeferredOf(d), func(v Value) (Value, Value) {
		if _, done := some(v); done {
			return false, v
		}
		return true, Some(result)
	})
}

func TxDeferredSucceed(fb *Fiber, x, d Value) Value { return TxDeferredDone(fb, Ok(x), d) }
func TxDeferredFail(fb *Fiber, err, d Value) Value  { return TxDeferredDone(fb, Err(err), d) }
