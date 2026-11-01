package flickrt

import (
	"errors"
	"fmt"
	"os"
	"syscall"
	"unicode/utf16"
	"unsafe"
)

var (
	promptKernel32          = syscall.NewLazyDLL("kernel32.dll")
	promptSetConsoleMode    = promptKernel32.NewProc("SetConsoleMode")
	promptReadConsoleInput  = promptKernel32.NewProc("ReadConsoleInputW")
	promptConsoleScreenInfo = promptKernel32.NewProc("GetConsoleScreenBufferInfo")
)

type promptConsoleRecord struct {
	kind      uint16
	padding   uint16
	down      int32
	repeat    uint16
	key       uint16
	scan      uint16
	char      uint16
	modifiers uint32
}

type promptTerminalState struct {
	input, output       syscall.Handle
	inputMode, outMode  uint32
	inputRaw, outputRaw bool
	pump                *stdinPump
	keys                promptKeyReader
	high                uint16
	pending             *promptConsoleRecord
	repeats             int
	repeated            Value
}

func promptConsoleMode(handle syscall.Handle, mode uint32) error {
	ok, _, err := promptSetConsoleMode.Call(uintptr(handle), uintptr(mode))
	if ok == 0 {
		return err
	}
	return nil
}

func promptOpenTerminal(input, output *os.File) (*promptTerminalState, error) {
	tty := &promptTerminalState{input: syscall.Handle(input.Fd()), output: syscall.Handle(output.Fd()), pump: pumpOf(input)}
	if err := syscall.GetConsoleMode(tty.input, &tty.inputMode); err == nil {
		// Read key events directly: no line buffering, echo, signal interception or Quick Edit pause.
		mode := tty.inputMode &^ uint32(0x1|0x2|0x4|0x8|0x10|0x40|0x200)
		if err := promptConsoleMode(tty.input, mode|0x80); err != nil {
			return nil, err
		}
		tty.inputRaw = true
	} else if err != syscall.Errno(6) {
		return nil, err
	}
	if err := syscall.GetConsoleMode(tty.output, &tty.outMode); err == nil {
		if err := promptConsoleMode(tty.output, tty.outMode|0x1|0x4); err != nil {
			return nil, errors.Join(err, promptRestoreTerminal(tty))
		}
		tty.outputRaw = true
	} else if err != syscall.Errno(6) {
		return nil, errors.Join(err, promptRestoreTerminal(tty))
	}
	tty.keys.read = tty.readByte
	return tty, nil
}

func promptRestoreTerminal(tty *promptTerminalState) error {
	var input, output error
	if tty.outputRaw {
		output = promptConsoleMode(tty.output, tty.outMode)
	}
	if tty.inputRaw {
		input = promptConsoleMode(tty.input, tty.inputMode)
	}
	return errors.Join(output, input)
}

// readByte reads a pipe or file through the pump, so stdin has one reader.
func (tty *promptTerminalState) readByte(fb *Fiber, timeout int) (byte, bool, error) {
	return tty.pump.byte(fb, timeout)
}

func (tty *promptTerminalState) consoleRecord(fb *Fiber) (promptConsoleRecord, error) {
	if tty.pending != nil {
		record := *tty.pending
		tty.pending = nil
		return record, nil
	}
	for {
		Check(fb)
		event, err := syscall.WaitForSingleObject(tty.input, 30)
		if err != nil {
			return promptConsoleRecord{}, err
		}
		if event == syscall.WAIT_TIMEOUT {
			continue
		}
		if event != syscall.WAIT_OBJECT_0 {
			return promptConsoleRecord{}, fmt.Errorf("waiting for console input: %d", event)
		}
		Check(fb)
		var record promptConsoleRecord
		var read uint32
		ok, _, err := promptReadConsoleInput.Call(uintptr(tty.input), uintptr(unsafe.Pointer(&record)), 1, uintptr(unsafe.Pointer(&read)))
		if ok == 0 {
			return record, err
		}
		if read != 0 && record.kind == 1 && record.down != 0 {
			return record, nil
		}
	}
}

func promptTerminalRead(fb *Fiber, tty *promptTerminalState) (Value, error) {
	if !tty.inputRaw {
		return tty.keys.key(fb)
	}
	if tty.repeats > 0 {
		tty.repeats--
		return tty.repeated, nil
	}
	for {
		record, err := tty.consoleRecord(fb)
		if err != nil {
			return nil, err
		}
		ctrl, meta, shift := record.modifiers&0xc != 0, record.modifiers&0x3 != 0, record.modifiers&0x10 != 0
		var character rune
		if tty.high != 0 {
			if record.char >= 0xdc00 && record.char <= 0xdfff {
				character = utf16.DecodeRune(rune(tty.high), rune(record.char))
				tty.high = 0
			} else {
				tty.high, tty.pending = 0, &record
				return promptInput("�", Some("�"), false, false, false), nil
			}
		} else if record.char >= 0xd800 && record.char <= 0xdbff {
			tty.high = record.char
			continue
		} else if record.char >= 0xdc00 && record.char <= 0xdfff {
			character = '�'
		} else {
			character = rune(record.char)
		}
		// AltGr produces printable text with Right Alt and Control held.
		if character >= ' ' && record.modifiers&0x1 != 0 && ctrl {
			ctrl, meta = false, false
		}
		name := ""
		switch record.key {
		case 8:
			name = "backspace"
		case 9:
			name = "tab"
		case 13:
			name = "return"
		case 27:
			name = "escape"
		case 35:
			name = "end"
		case 36:
			name = "home"
		case 37:
			name = "left"
		case 38:
			name = "up"
		case 39:
			name = "right"
		case 40:
			name = "down"
		case 46:
			name = "delete"
		}
		var input Value = None
		if ctrl && record.key >= 'A' && record.key <= 'Z' {
			name = string(rune(record.key - 'A' + 'a'))
		} else if name == "" && character >= ' ' {
			name = string(character)
			input = Some(name)
		}
		if name == "" {
			continue
		}
		value := promptInput(name, input, ctrl, meta, shift)
		tty.repeats, tty.repeated = max(0, int(record.repeat)-1), value
		return value, nil
	}
}

func promptTerminalSize(tty *promptTerminalState) (int, int) {
	var info struct {
		sizeX, sizeY                   int16
		cursorX, cursorY               int16
		attributes                     uint16
		left, top, right, bottom       int16
		maximumWindowX, maximumWindowY int16
	}
	ok, _, _ := promptConsoleScreenInfo.Call(uintptr(tty.output), uintptr(unsafe.Pointer(&info)))
	if ok == 0 {
		return 0, 0
	}
	return int(info.right - info.left + 1), int(info.bottom - info.top + 1)
}

func fileIsTerminal(f *os.File) bool {
	var mode uint32
	return syscall.GetConsoleMode(syscall.Handle(f.Fd()), &mode) == nil
}

func fileTerminalSize(f *os.File) (int, int) {
	if !fileIsTerminal(f) {
		return 0, 0
	}
	return promptTerminalSize(&promptTerminalState{output: syscall.Handle(f.Fd())})
}
