package flickrt

import (
	"bufio"
	"net"
	"net/http"
	"sync"
)

// Connections a server's responses take over, a WebSocket's: Shutdown
// does not track them, so the server tells them it stops, closes them and
// waits for what serves them.

type upgrades struct {
	mu       sync.Mutex
	conns    map[net.Conn]struct{}
	stopping chan struct{}
	closed   bool
	wg       sync.WaitGroup
}

func newUpgrades() *upgrades {
	return &upgrades{conns: map[net.Conn]struct{}{}, stopping: make(chan struct{})}
}

// take hijacks the response's connection and runs `run` on it until it
// returns, unless the server is stopping.
func (u *upgrades) take(w http.ResponseWriter, run func(conn net.Conn, rw *bufio.ReadWriter, stopping <-chan struct{})) {
	conn, rw, err := http.NewResponseController(w).Hijack()
	if err != nil {
		return
	}
	u.mu.Lock()
	if u.closed {
		u.mu.Unlock()
		conn.Close()
		return
	}
	u.conns[conn] = struct{}{}
	u.wg.Add(1)
	u.mu.Unlock()
	defer func() {
		u.mu.Lock()
		delete(u.conns, conn)
		u.mu.Unlock()
		conn.Close()
		u.wg.Done()
	}()
	run(conn, rw, u.stopping)
}

// close stops the connections taken over and waits for them.
func (u *upgrades) close() {
	u.mu.Lock()
	if !u.closed {
		u.closed = true
		close(u.stopping)
	}
	u.mu.Unlock()
	u.wg.Wait()
}
