package flickrt

import (
	"regexp"
	"unicode"
)

// CliOutput.displayWidth: the columns text takes in a terminal, without
// its ANSI escapes, by grapheme.

var cliAnsi = regexp.MustCompile("\x1b\\[[0-9;]*m")

// cliZeroWidth is `[\p{Mark}\p{Control}\p{Default_Ignorable_Code_Point}]`.
func cliZeroWidth(r rune) bool {
	return unicode.Is(unicode.M, r) || unicode.Is(unicode.Cc, r) || unicode.Is(unicode.Other_Default_Ignorable_Code_Point, r) ||
		r == 0xad || r == 0x34f || r == 0x61c || r == 0x115f || r == 0x1160 || r == 0x17b4 || r == 0x17b5 ||
		r >= 0x180b && r <= 0x180f || r >= 0x200b && r <= 0x200f || r >= 0x202a && r <= 0x202e ||
		r >= 0x2060 && r <= 0x206f || r == 0x3164 || r >= 0xfe00 && r <= 0xfe0f || r == 0xfeff || r == 0xffa0 ||
		r >= 0xfff0 && r <= 0xfff8 || r >= 0x1bca0 && r <= 0x1bca3 || r >= 0x1d173 && r <= 0x1d17a ||
		r >= 0xe0000 && r <= 0xe0fff
}

// cliEmojiPresentation approximates `\p{Emoji_Presentation}`.
// shortcut: the main emoji blocks, not Unicode's exact property list; generate the table if a program measures rarer emoji
func cliEmojiPresentation(r rune) bool {
	return r >= 0x1f300 && r <= 0x1f64f || r >= 0x1f680 && r <= 0x1f6ff || r >= 0x1f900 && r <= 0x1f9ff ||
		r >= 0x1fa70 && r <= 0x1faff || r >= 0x1f1e6 && r <= 0x1f1ff || r == 0x231a || r == 0x231b ||
		r >= 0x23e9 && r <= 0x23ec || r == 0x23f0 || r == 0x23f3 || r == 0x25fd || r == 0x25fe ||
		r == 0x2614 || r == 0x2615 || r >= 0x2648 && r <= 0x2653 || r == 0x267f || r == 0x2693 || r == 0x26a1 ||
		r == 0x26aa || r == 0x26ab || r == 0x26bd || r == 0x26be || r == 0x26c4 || r == 0x26c5 || r == 0x26ce ||
		r == 0x26d4 || r == 0x26ea || r == 0x26f2 || r == 0x26f3 || r == 0x26f5 || r == 0x26fa || r == 0x26fd ||
		r == 0x2705 || r == 0x270a || r == 0x270b || r == 0x2728 || r == 0x274c || r == 0x274e ||
		r >= 0x2753 && r <= 0x2755 || r == 0x2757 || r >= 0x2795 && r <= 0x2797 || r == 0x27b0 || r == 0x27bf ||
		r == 0x2b1b || r == 0x2b1c || r == 0x2b50 || r == 0x2b55 || r >= 0x1f004 && r <= 0x1f0cf || r == 0x1f18e ||
		r >= 0x1f191 && r <= 0x1f19a || r >= 0x1f200 && r <= 0x1f2ff
}

func cliFullWidth(r rune) bool {
	return r >= 0x1100 && (r <= 0x115f || r == 0x2329 || r == 0x232a ||
		r >= 0x2e80 && r <= 0x303e || r >= 0x3040 && r <= 0xa4cf || r >= 0xac00 && r <= 0xd7a3 ||
		r >= 0xf900 && r <= 0xfaff || r >= 0xfe10 && r <= 0xfe19 || r >= 0xfe30 && r <= 0xfe6f ||
		r >= 0xff00 && r <= 0xff60 || r >= 0xffe0 && r <= 0xffe6 || r >= 0x1b000 && r <= 0x1b2ff ||
		r >= 0x1f200 && r <= 0x1f251 || r >= 0x20000 && r <= 0x3fffd)
}

// cliGraphemes splits text into extended grapheme clusters, approximately.
// shortcut: marks, joiners, variation selectors, regional-indicator pairs
// and Hangul jamo extend a cluster; port UAX #29 if wider text misaligns
func cliGraphemes(text string) [][]rune {
	out := [][]rune{}
	for _, r := range text {
		if n := len(out); n > 0 {
			last := out[n-1]
			prev := last[len(last)-1]
			ri := func(x rune) bool { return x >= 0x1f1e6 && x <= 0x1f1ff }
			if unicode.Is(unicode.M, r) || r == 0x200d || prev == 0x200d || r >= 0xfe00 && r <= 0xfe0f ||
				r >= 0x1f3fb && r <= 0x1f3ff || r >= 0xe0020 && r <= 0xe007f ||
				ri(r) && ri(prev) && len(last) == 1 || r >= 0x1160 && r <= 0x11ff && prev >= 0x1100 && prev <= 0x11ff {
				out[n-1] = append(last, r)
				continue
			}
		}
		out = append(out, []rune{r})
	}
	return out
}

func cliDisplayWidth(text string) int {
	total := 0
	for _, g := range cliGraphemes(cliAnsi.ReplaceAllString(text, "")) {
		var visible rune = -1
		emoji := false
		for _, r := range g {
			if r == 0xfe0f || cliEmojiPresentation(r) {
				emoji = true
			}
			if visible < 0 && !cliZeroWidth(r) {
				visible = r
			}
		}
		switch {
		case visible < 0:
		case emoji, cliFullWidth(visible):
			total += 2
		default:
			total++
		}
	}
	return total
}

// CliDisplayWidth is `Cli.display_width`.
func CliDisplayWidth(text Value) Value { return int32(cliDisplayWidth(text.(string))) }
