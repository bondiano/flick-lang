package flickrt

import (
	"sync"
	"testing"
)

func TestToPullIsSharedOnePullAtATime(t *testing.T) {
	owned(rootFiber(), func(fb *Fiber, _ *Owner) Value {
		p := toPull(fb, channelFromArray([]Value{int32(1), int32(2), int32(3)}))
		var mu sync.Mutex
		var got []Value
		var wg sync.WaitGroup
		for range 3 {
			wg.Add(1)
			go func() {
				defer wg.Done()
				v := p(rootFiber())
				mu.Lock()
				got = append(got, v)
				mu.Unlock()
			}()
		}
		wg.Wait()
		if len(got) != 3 {
			t.Fatalf("pulled %v", got)
		}
		if ended := orDone(fb, p); ended != None {
			t.Errorf("a fourth pull gives %v, not the end", ended)
		}
		return Unit
	})
}

func TestEmbedInputStopsItsReaderWithTheChannel(t *testing.T) {
	stopped := false
	var read Value
	ready := make(chan struct{})
	// emits "x" once the reader has read its upstream
	self := fromPull(func(*Fiber) Pull {
		sent := false
		return func(fb *Fiber) Value {
			if sent {
				return halt(Unit)
			}
			wait(fb, ready)
			sent = true
			return "x"
		}
	})
	embedded := embedInput(self, func(fb *Fiber, upstream Pull) {
		defer func() { stopped = true }()
		read = upstream(fb)
		close(ready)
		wait(fb, nil)
	})
	got := runCollect(rootFiber(), channelPipeTo(channelFromArray([]Value{int32(7)}), embedded))
	if len(got) != 1 || got[0] != "x" || !stopped || read != int32(7) {
		t.Errorf("emitted %v, read %v, reader stopped %v", got, read, stopped)
	}
}

func TestTransformPullBracketClosesItsScopeAtTheEnd(t *testing.T) {
	var closed []*Cause
	s := streamTransformPullBracket(chunkStream([]Value{int32(1)}), func(fb *Fiber, pull Pull, _, forked *Owner) Pull {
		forked.add(fb, func(_ *Fiber, exit *Cause) Value { closed = append(closed, exit); return Unit })
		return pull
	})
	if got := StreamRunCollect(rootFiber(), s); len(List(got)) != 1 || len(closed) != 1 || closed[0] != nil {
		t.Errorf("collected %v, closed with %v", got, closed)
	}
}
