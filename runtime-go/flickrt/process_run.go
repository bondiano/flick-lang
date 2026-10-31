package flickrt

// A spawned `ChildProcess.Process`: its commands started in process groups
// of their own, what they write gathered as it comes, and their ends.

import (
	"errors"
	"io"
	"os"
	"os/exec"
	"strings"
	"sync"
	"time"
)

// outputQueue gathers what a process writes, so that output nobody reads
// never blocks it.
// shortcut: unbounded, where Node pauses a stream nobody reads (the cases: "Child processes" in the stdlib reference); bound it if a program spawns one that writes much unread
type outputQueue struct {
	mu     sync.Mutex
	chunks [][]byte
	closed bool
	signal chan struct{} // closed when a chunk comes or the output ends, then replaced
}

func newOutputQueue() *outputQueue { return &outputQueue{signal: make(chan struct{})} }

func (q *outputQueue) wake() {
	close(q.signal)
	q.signal = make(chan struct{})
}

func (q *outputQueue) Write(b []byte) (int, error) {
	q.mu.Lock()
	q.chunks = append(q.chunks, append([]byte(nil), b...))
	q.wake()
	q.mu.Unlock()
	return len(b), nil
}

func (q *outputQueue) close() {
	q.mu.Lock()
	q.closed = true
	q.wake()
	q.mu.Unlock()
}

// next is the next chunk, waiting for one; false once the output ended.
func (q *outputQueue) next(fb *Fiber) ([]byte, bool) {
	for {
		q.mu.Lock()
		if len(q.chunks) > 0 {
			chunk := q.chunks[0]
			q.chunks = q.chunks[1:]
			q.mu.Unlock()
			return chunk, true
		}
		if q.closed {
			q.mu.Unlock()
			return nil, false
		}
		signal := q.signal
		q.mu.Unlock()
		wait(fb, signal)
	}
}

// all is everything the output gives until it ends.
func (q *outputQueue) all(fb *Fiber) []byte {
	var out []byte
	for {
		chunk, ok := q.next(fb)
		if !ok {
			return out
		}
		out = append(out, chunk...)
	}
}

func (q *outputQueue) stream() Value {
	return &Stream{open: func(*Fiber) pull {
		return func(fb *Fiber) ([]Value, bool) {
			chunk, ok := q.next(fb)
			if !ok {
				return nil, false
			}
			return []Value{byteList(chunk)}, true
		}
	}}
}

type process struct {
	command  *procCommand
	cmds     []*exec.Cmd
	out, err *outputQueue
	exited   chan struct{} // closed when the last command ended
	state    *os.ProcessState
	waitErr  error
}

func (p *process) running() bool {
	select {
	case <-p.exited:
		return false
	default:
		return true
	}
}

// wait is the exit code of the process; one a signal ended fails.
func (p *process) wait(fb *Fiber) Value {
	wait(fb, p.exited)
	if signal, ok := processSignal(p.state); ok {
		return Fail(processError("UnknownError", "exitCode",
			Some("Process interrupted due to receipt of signal: '"+signal+"'"), p.command))
	}
	if p.state == nil {
		return Fail(processError("UnknownError", "exitCode", Some(p.waitErr.Error()), p.command))
	}
	return int32(p.state.ExitCode())
}

// terminate signals the process groups, last first, and waits for the
// process to end: `forceAfter`, then SIGKILL and a second more, or only a
// second without it.
func (p *process) terminate(fb *Fiber, signal string, forceAfter time.Duration) Value {
	for i := len(p.cmds) - 1; i >= 0; i-- {
		if err := processKill(p.cmds[i], signal); err != nil && p.running() {
			return Fail(processError(systemTagOf(err), "kill", None, p.command))
		}
	}
	grace := time.Second
	if forceAfter > 0 {
		grace = forceAfter
	}
	if !p.awaitExit(fb, grace) && forceAfter > 0 {
		for _, c := range p.cmds {
			processKill(c, "SIGKILL")
		}
		p.awaitExit(fb, time.Second)
	}
	return Unit
}

// awaitExit waits for the end of the process, at most `limit`.
func (p *process) awaitExit(fb *Fiber, limit time.Duration) bool {
	timeout := make(chan struct{})
	timer := time.AfterFunc(limit, func() { close(timeout) })
	defer timer.Stop()
	either := make(chan struct{})
	go func() {
		select {
		case <-p.exited:
		case <-timeout:
		}
		close(either)
	}()
	wait(fb, either)
	return !p.running()
}

// start starts the commands of the pipeline, each one's stdout the next
// one's stdin.
func (p *process) start(fb *Fiber) Value {
	commands := p.command.commands()
	var stdin io.Reader
	var closeAfter []io.Closer
	defer func() {
		for _, c := range closeAfter {
			c.Close()
		}
	}()
	for i, command := range commands {
		cmd := exec.Command(command.program, command.args...)
		if command.cwd != nil {
			cmd.Dir = *command.cwd
		}
		if command.env != nil {
			cmd.Env = os.Environ()
			for _, kv := range command.env {
				cmd.Env = append(cmd.Env, kv[0]+"="+kv[1])
			}
		}
		processGroup(cmd)
		switch {
		case command.stdin != nil:
			cmd.Stdin = strings.NewReader(*command.stdin)
		case stdin != nil:
			cmd.Stdin = stdin
		}
		lastOne := i == len(commands)-1
		switch {
		case lastOne && command.inherit:
			idle()
			cmd.Stdout, cmd.Stderr = os.Stdout, os.Stderr
		case lastOne:
			cmd.Stdout, cmd.Stderr = p.out, p.err
		default:
			r, w, err := os.Pipe()
			if err != nil {
				return Fail(processError(systemTagOf(err), "spawn", None, p.command))
			}
			closeAfter = append(closeAfter, r, w)
			cmd.Stdout, cmd.Stderr, stdin = w, io.Discard, r
		}
		if err := cmd.Start(); err != nil {
			for _, started := range p.cmds {
				processKill(started, "SIGKILL")
				started.Wait()
			}
			tag := systemTagOf(err)
			if errors.Is(err, exec.ErrNotFound) {
				tag = "NotFound"
			}
			return Fail(processError(tag, "spawn", None, p.command))
		}
		p.cmds = append(p.cmds, cmd)
	}
	go func() {
		for i, cmd := range p.cmds {
			err := cmd.Wait()
			if i == len(p.cmds)-1 {
				p.state, p.waitErr = cmd.ProcessState, err
			}
		}
		p.out.close()
		p.err.close()
		close(p.exited)
	}()
	return Unit
}

// ChildProcessSpawn starts the command; the scope stops what still runs,
// and signals the process group of one that failed or that left children,
// as Effect's spawner on Node does.
func ChildProcessSpawn(fb *Fiber, command Value) Value {
	p := &process{command: procOf(command), out: newOutputQueue(), err: newOutputQueue(), exited: make(chan struct{})}
	p.start(fb)
	fb.owner.add(fb, func(fb *Fiber, _ *Cause) Value {
		if p.running() || p.state == nil || p.state.ExitCode() != 0 || processGroupsLinger {
			catch(func() Value { return p.terminate(fb, "SIGTERM", 0) })
		}
		return Unit
	})
	return p
}

func processOf(v Value) *process { return v.(*process) }

func ChildProcessPid(p Value) Value {
	cmds := processOf(p).cmds
	return int32(cmds[len(cmds)-1].Process.Pid)
}

func ChildProcessWait(fb *Fiber, p Value) Value { return processOf(p).wait(fb) }

func ChildProcessIsRunning(_ *Fiber, p Value) Value { return processOf(p).running() }

func ChildProcessKill(fb *Fiber, p Value) Value { return processOf(p).terminate(fb, "SIGTERM", 0) }

func ChildProcessKillWith(fb *Fiber, options, p Value) Value {
	o := record(options)
	var after time.Duration
	if d, ok := some(o.Get("force_after")); ok {
		after = time.Duration(bigToFloat(dur(d).toNanos()))
	}
	return processOf(p).terminate(fb, o.Get("signal").(string), after)
}

func ChildProcessStdout(p Value) Value { return processOf(p).out.stream() }

func ChildProcessStderr(p Value) Value { return processOf(p).err.stream() }
