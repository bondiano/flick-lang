package flickrt

import (
	"io/fs"
	"math/big"
	"syscall"
	"time"
)

func fsAccessTime(info fs.FileInfo) time.Time {
	t := info.Sys().(*syscall.Stat_t).Atimespec
	return time.Unix(t.Sec, t.Nsec)
}

// fsStatSys is what Node's stat gives beside the type, mode, size and
// times: the device, inode, links, owner and blocks.
func fsStatSys(info fs.FileInfo) (dev *big.Int, ino, nlink, uid, gid, rdev, blksize, blocks, birth Value) {
	st, ok := info.Sys().(*syscall.Stat_t)
	if !ok {
		return big.NewInt(0), None, None, None, None, None, None, None, None
	}
	return big.NewInt(int64(st.Dev)), Some(new(big.Int).SetUint64(uint64(st.Ino))), Some(big.NewInt(int64(st.Nlink))),
		Some(float64(st.Uid)), Some(float64(st.Gid)), Some(big.NewInt(int64(st.Rdev))),
		Some(big.NewInt(int64(st.Blksize))), Some(big.NewInt(int64(st.Blocks))), Some(float64(time.Unix(st.Birthtimespec.Sec, st.Birthtimespec.Nsec).UnixMilli()))
}
