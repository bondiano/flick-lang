package flickrt

import (
	"os"
	"os/exec"
	"strconv"
	"syscall"
)

// On Windows a process starts in a group of its own, and stopping it ends
// its tree with `taskkill`, as Effect's spawner does; what outlived the
// leader is left running.
const processGroupsLinger = false

func processGroup(cmd *exec.Cmd) {
	cmd.SysProcAttr = &syscall.SysProcAttr{CreationFlags: syscall.CREATE_NEW_PROCESS_GROUP}
}

// processKill ends the process tree; Windows has no signals to send.
func processKill(cmd *exec.Cmd, _ string) error {
	if err := exec.Command("taskkill", "/pid", strconv.Itoa(cmd.Process.Pid), "/T", "/F").Run(); err != nil {
		return cmd.Process.Kill()
	}
	return nil
}

// processGroupAlive: on Windows only the leader is waited for.
func processGroupAlive(*exec.Cmd) bool { return false }

// processSignal: a Windows process ends with an exit code only.
func processSignal(*os.ProcessState) (string, bool) { return "", false }
