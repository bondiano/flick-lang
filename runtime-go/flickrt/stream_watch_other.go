//go:build !linux && !darwin && !windows

package flickrt

import "errors"

// watchNative has no notifications of this system to watch with: the
// path is polled.
func watchNative(*watchQueue, string, bool, bool) (func(), error) {
	return nil, errors.ErrUnsupported
}
