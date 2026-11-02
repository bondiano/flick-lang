package flickrt

import (
	"hash/fnv"
	"math/rand/v2"
	"os"
	"runtime"
	"sync"
	"time"
)

// Stress (`just stress`): with FLICK_STRESS set to a seed, each
// checkpoint and each new fiber yields or sleeps at random and turns are
// off (startup.go), so the runs of a program take interleavings a plain
// run rarely does; the monitor of λRT checks the trace of each. Go's own
// scheduler is not seeded: a seed picks the delays, not the interleaving.
var stressing = os.Getenv("FLICK_STRESS") != ""

var stressRand = struct {
	sync.Mutex
	*rand.Rand
}{Rand: rand.New(rand.NewPCG(stressSeed(), 0))}

func stressSeed() uint64 {
	h := fnv.New64a()
	_, _ = h.Write([]byte(os.Getenv("FLICK_STRESS")))
	return h.Sum64()
}

// stress yields a fifth of the time and sleeps up to half a millisecond a
// twentieth of it.
func stress() {
	stressRand.Lock()
	n, d := stressRand.IntN(100), stressRand.IntN(500)
	stressRand.Unlock()
	switch {
	case n < 20:
		runtime.Gosched()
	case n < 25:
		time.Sleep(time.Duration(d) * time.Microsecond)
	}
}
