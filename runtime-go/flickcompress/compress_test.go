package flickcompress

import (
	"bytes"
	"io"
	"strings"
	"testing"

	"flick.app/flickrt"
	"github.com/andybalholm/brotli"
	"github.com/klauspost/compress/zstd"
)

// What each algorithm writes decodes back, and what it flushes decodes as
// far as it was written, so a streamed response streams compressed.
func TestCompressorsRoundTrip(t *testing.T) {
	decoders := map[string]func(io.Reader) io.Reader{
		"br": func(r io.Reader) io.Reader { return brotli.NewReader(r) },
		"zstd": func(r io.Reader) io.Reader {
			d, err := zstd.NewReader(r)
			if err != nil {
				t.Fatal(err)
			}
			return d
		},
	}
	opens := map[string]func(int, io.Writer) (flickrt.Compressor, error){"br": openBrotli, "zstd": openZstd}
	chunks := []string{strings.Repeat("flick ", 300), "{\"a\":1}", strings.Repeat("z", 5000)}
	for name, level := range map[string]int{"br": 4, "zstd": 3} {
		var out bytes.Buffer
		w, err := opens[name](level, &out)
		if err != nil {
			t.Fatal(err)
		}
		written := ""
		for _, chunk := range chunks {
			w.Write([]byte(chunk))
			if err := w.Flush(); err != nil {
				t.Fatalf("%s: flush: %v", name, err)
			}
			written += chunk
			got := make([]byte, len(written))
			if _, err := io.ReadFull(decoders[name](bytes.NewReader(out.Bytes())), got); err != nil || string(got) != written {
				t.Fatalf("%s: flushed %d bytes decode to %d: %v", name, len(written), len(got), err)
			}
		}
		if err := w.Close(); err != nil {
			t.Fatalf("%s: close: %v", name, err)
		}
		got, err := io.ReadAll(decoders[name](&out))
		if err != nil || string(got) != written {
			t.Fatalf("%s: decodes to %d bytes of %d: %v", name, len(got), len(written), err)
		}
	}
}
