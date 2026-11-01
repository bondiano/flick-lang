package flickrt

import "regexp"

// CliOutput.displayWidth: the columns text takes in a terminal, without
// its ANSI escapes, by grapheme.

var cliAnsi = regexp.MustCompile("\x1b\\[[0-9;]*m")

var zeroWidthTable, emojiTable onFirstUse[[]valueRange]

// cliZeroWidth is `[\p{Mark}\p{Control}\p{Default_Ignorable_Code_Point}]`.
func cliZeroWidth(r rune) bool {
	return valueAt(zeroWidthTable.get(func() []valueRange { return parseValueRanges(zeroWidthData) }), r) != 0
}

// cliEmojiPresentation is `\p{Emoji_Presentation}`.
func cliEmojiPresentation(r rune) bool {
	return valueAt(emojiTable.get(func() []valueRange { return parseValueRanges(emojiData) }), r) != 0
}

func cliFullWidth(r rune) bool {
	return r >= 0x1100 && (r <= 0x115f || r == 0x2329 || r == 0x232a ||
		r >= 0x2e80 && r <= 0x303e || r >= 0x3040 && r <= 0xa4cf || r >= 0xac00 && r <= 0xd7a3 ||
		r >= 0xf900 && r <= 0xfaff || r >= 0xfe10 && r <= 0xfe19 || r >= 0xfe30 && r <= 0xfe6f ||
		r >= 0xff00 && r <= 0xff60 || r >= 0xffe0 && r <= 0xffe6 || r >= 0x1b000 && r <= 0x1b2ff ||
		r >= 0x1f200 && r <= 0x1f251 || r >= 0x20000 && r <= 0x3fffd)
}

func cliDisplayWidth(text string) int {
	total := 0
	for _, g := range graphemes(cliAnsi.ReplaceAllString(text, "")) {
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
