// Package flickcompress is the brotli and zstd of the go target's
// compression middleware: a program imports it only when it reaches
// `Http.Middleware.compression`, so other programs carry neither module.
package flickcompress

import (
	"io"

	"flick.app/flickrt"
	"github.com/andybalholm/brotli"
	"github.com/klauspost/compress/zstd"
)

func init() {
	flickrt.RegisterCompression("br", openBrotli)
	flickrt.RegisterCompression("zstd", openZstd)
}

func openBrotli(level int, w io.Writer) (flickrt.Compressor, error) {
	return brotli.NewWriterLevel(w, level), nil
}

// openZstd compresses on the calling goroutine, as Node's zstd does.
func openZstd(level int, w io.Writer) (flickrt.Compressor, error) {
	return zstd.NewWriter(w, zstd.WithEncoderLevel(zstd.EncoderLevelFromZstd(level)), zstd.WithEncoderConcurrency(1))
}
