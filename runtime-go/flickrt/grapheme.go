package flickrt

import (
	"sort"
	"strconv"
	"strings"
)

// Extended grapheme clusters by UAX #29, as `Intl.Segmenter` splits them,
// on the tables tools/go-unicode/graphemes.mjs writes from Node
// (grapheme_tables.go).

// The Grapheme_Cluster_Break classes, in the generator's order.
const (
	gbOther uint8 = iota
	gbCR
	gbLF
	gbControl
	gbExtend
	gbZWJ
	gbRI
	gbPrepend
	gbSpacingMark
	gbL
	gbV
	gbT
	gbLV
	gbLVT
)

// The flags beside the class: Extended_Pictographic and Indic_Conjunct_Break.
const (
	gbClass      = 15
	gbPict       = 16
	gbIncb       = 96
	gbConsonant  = 32
	gbLinker     = 64
	gbIncbExtend = 96
)

// valueRange is a run of code points with one value of a property.
type valueRange struct {
	lo, hi rune
	value  uint8
}

// parseValueRanges reads `lo-hi=value` and `cp=value` entries in hex.
func parseValueRanges(data string) []valueRange {
	var out []valueRange
	for _, entry := range strings.Split(data, ";") {
		run, value, _ := strings.Cut(entry, "=")
		lo, hi, isRange := strings.Cut(run, "-")
		if !isRange {
			hi = lo
		}
		v, _ := strconv.ParseUint(value, 16, 8)
		out = append(out, valueRange{parseRunes(lo)[0], parseRunes(hi)[0], uint8(v)})
	}
	return out
}

// valueAt is the value of `r` in the ranges, 0 outside them.
func valueAt(ranges []valueRange, r rune) uint8 {
	i := sort.Search(len(ranges), func(i int) bool { return ranges[i].hi >= r })
	if i < len(ranges) && ranges[i].lo <= r {
		return ranges[i].value
	}
	return 0
}

var graphemeTable onFirstUse[[]valueRange]

func graphemeProperties(r rune) uint8 {
	return valueAt(graphemeTable.get(func() []valueRange { return parseValueRanges(graphemeData) }), r)
}

// graphemeBreak is whether UAX #29 breaks between code points of
// properties a and b, given the regional indicators in a row up to a, and
// whether a ends `ExtPict Extend* ZWJ` and `Consonant [Extend Linker]*
// Linker [Extend Linker]*`.
func graphemeBreak(a, b uint8, ri int, emojiZWJ, conjunct bool) bool {
	ca, cb := a&gbClass, b&gbClass
	switch {
	case ca == gbCR && cb == gbLF: // GB3
		return false
	case ca == gbControl || ca == gbCR || ca == gbLF || cb == gbControl || cb == gbCR || cb == gbLF: // GB4, GB5
		return true
	case ca == gbL && (cb == gbL || cb == gbV || cb == gbLV || cb == gbLVT), // GB6
		(ca == gbLV || ca == gbV) && (cb == gbV || cb == gbT),                   // GB7
		(ca == gbLVT || ca == gbT) && cb == gbT,                                 // GB8
		cb == gbExtend || cb == gbZWJ || cb == gbSpacingMark || ca == gbPrepend, // GB9, GB9a, GB9b
		conjunct && b&gbIncb == gbConsonant,                                     // GB9c
		emojiZWJ && b&gbPict != 0,                                               // GB11
		ca == gbRI && cb == gbRI && ri%2 == 1:                                   // GB12, GB13
		return false
	}
	return true
}

// graphemes splits text into extended grapheme clusters.
func graphemes(text string) []string {
	var out []string
	start, ri := 0, 0
	var prev uint8
	// pict: after ExtPict Extend*, with zwj: and then a ZWJ; consonant:
	// after Consonant [Extend Linker]*, with linker: with a Linker among them
	pict, zwj, consonant, linker := false, false, false, false
	for i, r := range text {
		p := graphemeProperties(r)
		if i > 0 && graphemeBreak(prev, p, ri, zwj, linker) {
			out = append(out, text[start:i])
			start = i
		}
		class, incb := p&gbClass, p&gbIncb
		if class == gbRI {
			ri++
		} else {
			ri = 0
		}
		zwj = pict && class == gbZWJ
		pict = p&gbPict != 0 || pict && class == gbExtend
		switch {
		case incb == gbConsonant:
			consonant, linker = true, false
		case consonant && incb == gbLinker:
			linker = true
		case consonant && incb == gbIncbExtend:
		default:
			consonant, linker = false, false
		}
		prev = p
	}
	if start < len(text) {
		out = append(out, text[start:])
	}
	return out
}
