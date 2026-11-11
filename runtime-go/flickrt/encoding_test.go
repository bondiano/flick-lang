package flickrt

import (
	"os"
	"strings"
	"testing"
)

// testdata/{ini,toml,yaml}.txt are the parses of each input by the Effect
// library (tools/go-encoding/gen.mjs): the JSON of the value or the
// error's text.
func TestEncodingsParseAsEffect(t *testing.T) {
	for name, parse := range map[string]func(string) (Value, string){"ini": parseIni, "toml": parseToml, "yaml": parseYaml} {
		data, err := os.ReadFile("testdata/" + name + ".txt")
		if err != nil {
			t.Fatal(err)
		}
		for _, line := range strings.Split(strings.TrimSuffix(string(data), "\n"), "\n") {
			quoted, want, _ := strings.Cut(line, "\t")
			input := value(JsonParse(quoted)).(string)
			v, got := parse(input)
			if got == "" {
				got = JsonStringify(v).(string)
			}
			if got != want {
				t.Errorf("%s %s:\n got %s\nwant %s", name, quoted, got, want)
			}
		}
	}
}
