---
name: conduit
description: The RealWorld Conduit client as a two-ink risograph zine — Flick's flagship showcase.
colors:
  paper: "#f8f4ea"
  stock: "#fffdf7"
  ink: "#1b1b1f"
  fluoro: "#ff4a1c"
  fluoro-press: "#ff5e32"
  rubric: "#c93405"
typography:
  display:
    fontFamily: "Anton, 'Arial Narrow', Impact, sans-serif"
    fontWeight: 400
    lineHeight: 0.92
    letterSpacing: "0.015em"
  body:
    fontFamily: "Figtree, system-ui, sans-serif"
    fontSize: "1rem"
    fontWeight: 400
    lineHeight: 1.6
  article:
    fontFamily: "'Source Serif 4', Georgia, serif"
    fontSize: "1.18rem"
    lineHeight: 1.75
  mono:
    fontFamily: "'IBM Plex Mono', ui-monospace, Menlo, monospace"
    fontSize: "0.74rem"
    lineHeight: 1.65
rounded:
  card: "4px"
  control: "8px"
  pill: "999px"
components:
  button-primary:
    backgroundColor: "{colors.fluoro}"
    textColor: "{colors.ink}"
    rounded: "{rounded.control}"
    padding: "0.5rem 1.1rem"
  button-secondary:
    backgroundColor: "{colors.ink}"
    textColor: "{colors.paper}"
    rounded: "{rounded.control}"
  button-outline:
    backgroundColor: "transparent"
    textColor: "{colors.ink}"
    rounded: "{rounded.control}"
  card-preview:
    backgroundColor: "{colors.stock}"
    rounded: "{rounded.card}"
    padding: "1.5rem 1.6rem"
  chip-tag:
    backgroundColor: "rgba(27,27,31,0.08)"
    textColor: "{colors.ink}"
    rounded: "{rounded.pill}"
    padding: "0.4rem 0.78rem"
---

# Design System: conduit

## Overview

**Creative North Star: "The Press Proof"**

Conduit is printed, not rendered: a warm-paper Medium clone pulled through a
two-ink risograph press. One fluoro-orange ink does all the shouting —
wordmark, hovers, active states, stamps; one indigo-black ink does all the
structure — rules, borders, bands, text. Nothing else is allowed on press.

The frame is ink: the sticky masthead rule, the dark article and profile
bands, and the footer slab pin the page at its edges; warm paper carries
the reading surfaces between them. The signature is misregistration —
orange display type carrying a faint indigo ghost, settling into register
on first paint — and every hover is the second ink rolling over the first.

**Key Characteristics:**

- Two inks, one paper: any third color is a bug.
- Ink bands frame paper content on every route.
- Misregistration (offset shadows, halftone raster, registration mark) is
  the only ornament, because it is the press's own material.
- Sharp-cut cards on print stock; soft dies only on small controls.

## Colors

Two inks over warm uncoated stock; the loud ink has a text-weight grade
for small type.

### Primary
- **Fluoro Ink** (#ff4a1c): fills, display type, stamps, active rules.
  Never for body-size text on paper (contrast) — use Rubric instead.
- **Rubric** (#c93405): the same ink at text weight (≥4.5:1 on paper) —
  links, errors, small accents.

### Neutral
- **Ink** (#1b1b1f): text, 2px rules, card borders, dark bands, footer.
- **Paper** (#f8f4ea): the page ground.
- **Stock** (#fffdf7): card and control fill, one step brighter than paper.
- **Ink tints** (color-mix of ink 8–76% over paper): panels, pills,
  secondary text — always mixed from the two inks, never new hues.

### Named Rules
**The Two-Ink Rule.** Every color on screen is fluoro (#ff4a1c / #c93405 /
#ff5e32 pressed) or ink (#1b1b1f) mixed with paper (#f8f4ea / #fffdf7). A
third hue never enters the page.

**The Reaction Rule.** Orange is a reaction, ink is structure: resting
text is ink; the loud ink appears on hover, active, brand, and stamp.

## Typography

**Display:** Anton ('Arial Narrow', Impact fallback)
**Body/UI:** Figtree (system-ui fallback)
**Article body:** Source Serif 4 (Georgia fallback)
**Mono:** IBM Plex Mono (code, the sidebar colophon note)

**Character:** A poster-condensed display stamped in one ink over a
workhorse UI grotesque; article bodies drop into a reading serif, so the
feed reads as a zine and the article as the piece it printed.

### Hierarchy
- **Display** (Anton 400, clamp(4.5rem, 13vw, 11rem), 0.92): home wordmark
  only. Article/profile names: clamp(2.1–2.6rem). Auth headings: 2.6rem.
- **Title** (Figtree 800, 1.5rem, -0.012em): article preview headlines.
- **Body** (Figtree 400/500, 1rem, 1.6): UI text; article bodies Source
  Serif 4 1.18rem/1.75 held to max-width 74ch.
- **Label** (Figtree 700, 0.72–0.85rem, +0.05–0.12em tracking,
  uppercase): nav, buttons, tabs, dates, read-more.

### Named Rules
**The Lowercase Rule.** Anton sets the brand and page names in lowercase;
uppercase is reserved for the small UI label voice.

## Layout

- Container 1120px max, 24px gutlets; 12-column CSS grid (`row`, gap
  1.75rem) with `col-md-*` spans and `offset-md-*` starts at ≥768px.
- The app frame (`#app`) is a flex column with `min-height: 100dvh`;
  `#app > footer { margin-top: auto }` pins the footer to the viewport
  bottom on short pages.
- Home: 9/3 split — feed cards left, sticky tags panel right
  (`top: nav + 1.5rem`, static under 768px).
- Article: banner band full-bleed; body column capped at 74ch; comments
  in an offset 8/2 column.
- Mobile: single column, hero padding drops to 3.25rem, small controls
  grow to tap height (44px pagination, taller pills and buttons).

## Elevation & Depth

Flat at rest — depth is ink, not shadow: 2px borders, dark bands, and the
misregistration motif. The one shadow is the press itself: on hover a
preview card lifts and gains a 1px-ish indigo offset print ghost
(`4px 4px 0 -1px ink-18%`) plus a long soft drop. Focus rings are 2px
fluoro with 2px offset; `::selection` is fluoro on ink.

### Shadow Vocabulary
- **Print ghost** (`4px 4px 0 -1px rgba(ink 18%)` + soft drop): hover of
  preview cards and buttons — the misregistration made physical.
- **Text ghost** (`text-shadow: 4px 4px 0 ink-18%`): display type at rest;
  animates from 10px on load (register settle).

## Shapes

Print stock is cut, not rounded: cards and pagination plates at 4px;
controls (buttons, inputs) keep a softer 8px die; pills (tags) are
capsules. Dark bands (article banner, profile, footer) are square-edged
full-bleed slabs. Dashed borders mark destructive/tear actions and the
empty-feed card.

## Components

### Buttons
- **Shape:** 8px radius, 2px ink border, uppercase 700 label.
- **Primary (stamp):** fluoro fill, ink text; pressed state #ff5e32
  (pressed runs lighter, not darker, so ink text keeps 4.5:1).
- **Secondary:** ink fill, paper text; hover rolls text to fluoro.
- **Outline:** transparent, ink border/text; primary-outline hover fills
  fluoro; the heart icon rides in fluoro inside an outline favorite.
- **Danger (tear):** dashed rubric border; hover fills fluoro solid.
- **On ink bands:** outline buttons take paper strokes, fluoro on hover.

### Chips (tags)
- Tinted pill (ink-8% on paper), ink-76% text; hover rolls fluoro fill.
  One language everywhere: sidebar links, preview tags, editor chips.

### Cards
- **Preview:** stock fill, 2px ink border, 4px radius, 1.5×1.6rem padding;
  hover: fluoro border, -2px/-2px lift, print-ghost shadow, headline to
  fluoro. Comment cards share the stock and border with a 1px inner rule
  in the footer.
- **Empty feed:** dashed ink-35% border, transparent fill, centered.

### Inputs
- Stock fill, 2px ink border, 8px radius; focus: fluoro border + 3px
  fluoro-22% ring; placeholders ink-55%. Form groups are unframed
  fieldsets (browser groove reset).

### Navigation
- Sticky masthead: paper-88% + blur, 2px ink bottom rule; Anton brand in
  fluoro with text ghost; uppercase nav links ink-70%, hover bg ink-7%.

### Registration kit (signature)
- Home hero: halftone raster (fluoro 1.2px dots, 15px pitch) masked to
  the right 62%, a 130px registration mark (circle + crosshair, fluoro
  60%) top-right, giant lowercase wordmark animating into register.
  Hidden under 900px.

## Do's and Don'ts

### Do:
- **Do** mix every tint from the two inks and paper (`color-mix`).
- **Do** keep body measure ≤74ch on article pages.
- **Do** theme browser surfaces: fluoro selection, thin ink scrollbar,
  fluoro focus-visible rings, tabular numerals in counters.
- **Do** respect `prefers-reduced-motion` (the register-settle and all
  transitions collapse).

### Don't:
- **Don't** introduce a third color — including greens from the old
  RealWorld theme or semantic blues/reds for errors.
- **Don't** round cards past 4px or give pills hard corners.
- **Don't** stack soft shadows on resting elements; depth at rest is ink.
- **Don't** rename or drop the spec's CSS classes, `name` attributes,
  placeholders, or text labels — the vendored e2e suite finds elements
  by them.
