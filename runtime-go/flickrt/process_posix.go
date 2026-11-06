//go:build darwin || linux

package flickrt

import (
	"os"
	"os/exec"
	"syscall"
)

// A process starts in a group of its own, which stopping it signals, and
// a group that outlived its leader is signalled when its scope closes.
const processGroupsLinger = true

func processGroup(cmd *exec.Cmd) { cmd.SysProcAttr = &syscall.SysProcAttr{Setpgid: true} }

var processSignals = map[string]syscall.Signal{
	"SIGABRT": syscall.SIGABRT, "SIGALRM": syscall.SIGALRM, "SIGHUP": syscall.SIGHUP, "SIGINT": syscall.SIGINT,
	"SIGKILL": syscall.SIGKILL, "SIGPIPE": syscall.SIGPIPE, "SIGQUIT": syscall.SIGQUIT, "SIGTERM": syscall.SIGTERM,
	"SIGUSR1": syscall.SIGUSR1, "SIGUSR2": syscall.SIGUSR2, "SIGCONT": syscall.SIGCONT, "SIGSTOP": syscall.SIGSTOP,
	"SIGTSTP": syscall.SIGTSTP, "SIGWINCH": syscall.SIGWINCH,
}

// processKill signals the command's process group; a group already gone
// is no failure.
func processKill(cmd *exec.Cmd, signal string) error {
	sig, ok := processSignals[signal]
	if !ok {
		return syscall.EINVAL
	}
	if err := syscall.Kill(-cmd.Process.Pid, sig); err != nil && err != syscall.ESRCH {
		return err
	}
	return nil
}

// processGroupAlive is whether the command's process group still has a
// member, one only zombies hold included (EPERM on macOS).
func processGroupAlive(cmd *exec.Cmd) bool {
	err := syscall.Kill(-cmd.Process.Pid, 0)
	return err == nil || err == syscall.EPERM
}

// processSignal is the name of the signal that ended the process, if one did.
func processSignal(state *os.ProcessState) (string, bool) {
	if state == nil {
		return "", false
	}
	ws, ok := state.Sys().(syscall.WaitStatus)
	if !ok || !ws.Signaled() {
		return "", false
	}
	for name, sig := range processSignals {
		if sig == ws.Signal() {
			return name, true
		}
	}
	return ws.Signal().String(), true
}
