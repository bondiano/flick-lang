---
name: Flick — Signal
description: Cool-paper engineering editorial with graphite geometry, violet signals, and real code.
colors:
  violet: "#584dd0"
  violet-wash: "#eeecfa"
  cool-paper: "#fafafc"
  graphite: "#101016"
  body-ink: "#373740"
  secondary-ink: "#484852"
  muted-ink: "#686873"
  control-border: "#d6d6e0"
  hairline: "#e0e0e8"
  soft-surface: "#eeeef5"
  editor-top: "#f5f4fb"
  inverse-white: "#ffffff"
  success: "#347b57"
  error: "#b43b4f"
  night-paper: "#17171d"
  night-ink: "#f1f0f8"
  night-violet: "#b1a6ff"
typography:
  display:
    fontFamily: "Geist, sans-serif"
    fontSize: "clamp(3.25rem, 5.05vw, 4.8rem)"
    fontWeight: 720
    lineHeight: 1.08
    letterSpacing: "-0.035em"
  headline:
    fontFamily: "Geist, sans-serif"
    fontSize: "clamp(1.85rem, 2.6vw, 2.5rem)"
    fontWeight: 650
    lineHeight: 1.2
    letterSpacing: "-0.028em"
  title:
    fontFamily: "Geist, sans-serif"
    fontSize: "1.2rem"
    fontWeight: 600
    lineHeight: 1.35
    letterSpacing: "-0.015em"
  body:
    fontFamily: "Geist, sans-serif"
    fontSize: "1.0625rem"
    lineHeight: 1.8
  label:
    fontFamily: "Geist, sans-serif"
    fontSize: "0.8125rem"
  code:
    fontFamily: "var(--__sl-font-mono, ui-monospace, SFMono-Regular, Menlo, monospace)"
    fontSize: "0.8125rem"
    lineHeight: 1.75
rounded:
  navigation: "5px"
  control: "6px"
  container: "12px"
spacing:
  compact: "0.5rem"
  control: "0.75rem"
  standard: "1rem"
  inset: "1.25rem"
  panel: "1.5rem"
  section-gap: "2rem"
components:
  button-primary:
    backgroundColor: "{colors.violet}"
    textColor: "{colors.inverse-white}"
    rounded: "{rounded.control}"
    padding: "0.7rem 1.25rem"
  button-secondary:
    backgroundColor: "{colors.cool-paper}"
    textColor: "{colors.graphite}"
    rounded: "{rounded.control}"
    padding: "0.7rem 1.25rem"
  search-field:
    textColor: "{colors.graphite}"
    rounded: "{rounded.control}"
    padding: "0.75rem 1rem"
  navigation-current:
    backgroundColor: "{colors.violet-wash}"
    textColor: "{colors.violet}"
    rounded: "{rounded.navigation}"
    padding: "0.45rem 0.65rem"
  choice-selected:
    backgroundColor: "{colors.violet-wash}"
    textColor: "{colors.violet}"
    rounded: "{rounded.control}"
    padding: "0.375rem 0.75rem"
  operate-primary:
    backgroundColor: "{colors.violet}"
    textColor: "{colors.cool-paper}"
    rounded: "{rounded.control}"
    padding: "0.375rem 0.75rem"
  source-field:
    backgroundColor: "{colors.cool-paper}"
    textColor: "{colors.graphite}"
    typography: "{typography.code}"
    padding: "1.25rem"
  editor-container:
    backgroundColor: "{colors.cool-paper}"
    textColor: "{colors.graphite}"
    rounded: "{rounded.container}"
---

# Design System: Flick — Signal

## Overview

**Creative North Star: "Signal"**

Signal is a cool-paper engineering editorial: precise graphite geometry, violet emphasis, generous breathing room, and code with a real job to do. The owner-approved world is confident without becoming a dashboard or a decorative card collection. The geometric F and violet spark carry the identity; the drifting dot field is the signature atmospheric material.

The visual language stays shared while density follows the task. Persuade surfaces give the headline and working code room to lead. Read surfaces quiet the decoration and prioritize navigation, line length, and document hierarchy. Operate surfaces spend their width on source, diagnostics, output, and generated JavaScript. These are different applications of the same system, not a requirement to reuse the homepage composition everywhere.

**Key Characteristics:**
- Cool paper, graphite text, and restrained violet signals.
- Geist editorial hierarchy paired with practical monospace code.
- Unboxed content sections, fine rules, and small controlled radii.
- Real interactive examples with explicit loading, failure, and recovery states.
- A static dot geometry with one slow, interruptible group drift.

Evidence: `docs/src/styles/custom.css`, `docs/src/styles/playground.css`, and the shipped components and Flick widgets named below. The frontmatter records the recurring light-theme primitives and the principal dark-theme anchors; contextual values and theme overrides remain owned by those stylesheets. The sidecar extends this record with motion, breakpoints, depth, and self-contained visual specimens. Its synthesized tonal ramps are preview aids, not additional shipped palette tokens.

## Colors

The palette is cool and near-neutral until violet marks an action, a selected state, a language token, or the brand spark.

### Primary
- **Signal Violet** (`violet`): primary learning actions, links, active navigation, keyword/type highlighting, focus outlines, and the atmospheric field. **Violet Wash** (`violet-wash`) is the low-intensity selected background, not a second accent.
- **Night Violet** (`night-violet`): the dark theme's high-emphasis text and control accent. Theme-responsive UI follows the CSS semantic properties rather than baking in the light palette.

### Neutral
- **Cool Paper** (`cool-paper`): page, navigation, and source surfaces. **Graphite** (`graphite`) is the strongest heading, brand, and control ink.
- **Body Ink**, **Secondary Ink**, and **Muted Ink**: prose, secondary controls/navigation, and explanatory labels respectively. Muted content is still readable content, not decoration.
- **Hairline** and **Control Border**: section separation and container/field boundaries. **Soft Surface** and **Editor Top**: hover or utility regions and editor chrome.
- **Inverse White**: the fixed light-theme marketing action foreground.
- **Night Paper** and **Night Ink**: dark-theme anchors. The stylesheet also replaces gray steps, washes, rules, editor surfaces, selection, success, and error values; dark mode is a full semantic reassignment, not an inverted screenshot.

Success and error colors are functional feedback: string/success output and failure states in the hero use the corresponding semantic properties. The playground also uses Starlight's green/red/orange semantic values with explicit fallbacks; do not silently substitute hero colors for every diagnostic severity.

**The Signal Rule.** Violet denotes identity, action, selection, focus, or language structure; neutral ink carries ordinary reading.

## Typography

**Display and Body Font:** Geist, with a sans-serif fallback. Local variable fonts cover Latin and Cyrillic with `font-display: swap`.

**Code Font:** the general site uses `'SFMono-Regular', Consolas, 'Liberation Mono', monospace`; the playground uses Starlight's resolved monospace property with `ui-monospace, SFMono-Regular, Menlo, monospace` as its fallback. Preserve this observed distinction rather than imposing another font dependency.

**Character:** compact, slightly tightened headlines sit above calm, open prose. Technical labels and source remain utilitarian; code ligatures are disabled in the hero and documentation code blocks.

### Hierarchy
- **Display:** the frontmatter's fluid, medium-bold hero role, tightened tracking, balanced lines, and a graphite/violet split. At the compact hero breakpoint it becomes `clamp(2.65rem, 10.7vw, 4rem)` with a line height of (1.1).
- **Headline:** the fluid section-heading role; balanced text and intentional line breaks carry hierarchy without a decorative label above it.
- **Title:** the smaller feature-heading role. Navigation destinations in the homepage's ruled link rows are larger (1.75rem) and are not a universal card title style.
- **Body:** document text uses the body role with an open reading line height; homepage paragraphs use contextual line heights around (1.6–1.65). Feature explanations are generally constrained to (38ch), comparison copy to (44ch), and tool introductions to (75ch).
- **Label:** the compact tool-label role. Sidebar links use (0.84rem), while table-of-contents links use (0.78rem); neither competes with prose headings.
- **Code:** source and generated code use the frontmatter code role, preserving whitespace and a tab size of (2). Hero code is deliberately larger/fluid on desktop and becomes (0.75rem) with a line height of (2.1) on narrow screens.

**The Code Is Content Rule.** Preserve code whitespace, line breaks, and horizontal access to long lines; never shrink the whole page to fit a source line.

## Layout

The shared fixed header uses a navigation height of (5.25rem), reduced to (4.5rem) below (49.99rem). Page content is offset by that height. The desktop reading sidebar is (17rem); ordinary document content has a maximum width of (48rem). When the playground shell is present, the content-width property expands to (88rem), while introductory paragraphs remain bounded to (75ch).

Homepage sections have a maximum width of (1536px) and a fluid side gutter of `clamp(1.5rem, 4.6vw, 4.5rem)`. They use open sections and thin separators rather than turning every feature into a card. Repeated internal spacing is recorded in the frontmatter; it is an observed vocabulary, not a mandatory mathematical scale.

The implemented responsive rules are contextual:
- At (79rem) and below, header spacing tightens, search becomes an icon control, and the hero's inter-column gap narrows.
- At (63rem) and below, primary desktop links yield to the native popover menu; hero and comparison regions stack. The code region stays substantial rather than becoming a thumbnail.
- Below (50rem), the desktop sidebar is hidden; the fixed mobile contents strip contributes its own (2.75rem) height when the page has a contents list. Feature columns become one column and section vertical padding becomes (3rem). At (72rem) and above, the mobile contents strip is hidden.
- At (40rem) and below, inline locale, Transpile, and motion controls leave the compact header. Play remains visible; both locales and Transpile remain reachable in the popover. The locale links precede the long reference navigation. Comparison and exploration links become single-column.
- Play and Transpile use paired panes: Play favors source at (1.1fr / 0.9fr), Transpile is equal-width. At (56rem) and below, the result stacks after source and uses a top separator instead of a left separator. Source minimum height changes from (30rem) to (22rem); result regions generally become (20rem) minimum.
- At (36rem) and below, example choices form a two-column grid, tool controls grow to a minimum height of (2.75rem), status gets its own toolbar row, and editor padding tightens.

**The Task-Width Rule.** Keep reading columns bounded, but give Play and Transpile their wider workspace; the document measure is not the editor measure.

## Elevation & Depth

The system is predominantly flat: tonal editor chrome, thin rules, whitespace, and ordering supply depth. Editors and feature sections do not acquire ambient card shadows. The native search dialog is the deliberate exception, with a diffuse shadow (`0 18px 65px #10101626`) and a darkened backdrop (`#10101655`). The mobile popover uses a lighter backdrop (`#10101622`) and a fine container border.

The dot field sits behind the hero editor within an isolated composition; it is atmosphere, not an interactive layer. Its opacity is (0.4), with a radial mask solid through (48%) and faded by (83%). Keep the field visible without sacrificing code contrast.

## Shapes

Fine one-pixel rules and restrained rounded rectangles establish the form language. Small navigation/copy affordances use the navigation radius; buttons and fields use the control radius; hero/workspace/dialog/popover shells use the container radius. Internal code panes stay square so adjacent panels meet cleanly. Status dots are circles; they are not a general invitation to pill-shaped controls.

The brand is an actual SVG silhouette: two rising graphite strokes form the F, with a separate four-point violet spark. Reuse `Brand.astro` and its paths rather than replacing the mark with a text glyph or an unrelated icon. Small supporting icons are explicit SVG paths.

## Components

### Buttons

Direct, compact, and clearly labeled. The primary marketing link uses violet with white text; the secondary uses page tone, graphite ink, and a fine border. Both use the control radius, a minimum height of (50px), and the frontmatter padding. Hover darkens the primary to (`#493dbd`) or brings the secondary border into violet; these links do not inherit the tool's animated transition. At the narrow breakpoint the minimum height becomes (48px).

Tool buttons have their own denser measure and state system: minimum height (2.125rem), selected wash, violet border emphasis, and transitions of (160ms ease-out) for background, border, and text color. Hover adds neutral surface/border; active uses the soft surface; disabled is visibly dimmed with a not-allowed cursor. The main Run control uses semantic accent foreground/background inversion and becomes Stop while executing. Do not force the marketing button dimensions onto the toolbar.

Global keyboard focus is a visible two-pixel accent outline offset by (4px); tool controls use (3px), and source focus is inset by (-2px). Preserve focus rather than substituting hover for it.

### Navigation

The shared header connects Learn, Guides, API, Ecosystem, Transpile, and Play with the same brand and controls across the site. Text links gain violet on hover/current state. The reading sidebar uses a quiet rounded row, neutral hover surface, and violet wash plus heavier ink for the current page. `aria-current` carries the state, not color alone.

The mobile menu is a native `popover="auto"` with its own scrollable bounds. It preserves English and Russian navigation before the long document tree. Search is a native dialog; focus, Escape, and modal containment remain platform responsibilities. The desktop keyboard shortcut invokes the same search action as pointer input.

Header controls are server-rendered from the same Flick view and hydrated in place, not inserted into an empty header after boot. Theme glyph visibility follows pre-paint CSS so browser preferences do not produce a mismatched server view.

### Fields and Search

The search field is a bordered, control-radius frame with a leading SVG, transparent input, and an accent focus-within outline. Results are ruled rows rather than cards. Empty query, no results, and a recoverable search failure are explicit localized states; result updates use a polite live region. The search dialog is bounded to `min(44rem, calc(100vw - 2rem))` and `min(75dvh, 44rem)` height.

The source editor is a real, vertically resizable textarea, not a painted code image. Lazy syntax coloring overlays Flick-rendered text spans with exactly matching font, line height, padding, tab width and scroll offsets. The textarea retains the caret, selection and native editing; before coloring is ready or in forced-colors mode, native text remains visible. Generated JavaScript uses the same semantic token colors in a scrollable preformatted region; runtime output instead wraps long text. Diagnostics and stale-output explanations stay beside the task they explain.

### Containers and Choices

The hero editor and tools share the fine-border, container-radius shell with tonal chrome, square internal partitions, and no box shadow. Example/profile choices are actual buttons: pressed state adds violet wash, accent text, and a mixed accent border. They are not decorative chips.

Homepage destination rows stay unboxed: a fine top rule, generous vertical padding, heading, explanation, and a trailing SVG arrow. Their hover changes the heading and arrow to violet. Preserve that quiet navigation pattern instead of inventing a raised card grid.

### Signal Wave

`SignalWave.astro` creates a static field of (110 × 62), or **6,820**, SVG circles with radius (0.7) in a (1020 × 600) viewBox. Sine-displaced rows form the interference geometry. CSS animates only their shared group, never individual circles: a (24s) `cubic-bezier(0.22, 1, 0.36, 1)` infinite alternate drift, from `translate(-8px, 5px) rotate(-0.6deg)` to `translate(8px, -7px) rotate(0.6deg)`.

The wave runs continuously on desktop and mobile; there is no header pause control or stored motion state. System reduced-motion preferences remove its animation. The SVG is decorative (`aria-hidden`, non-focusable, pointer-events disabled). Operate transitions also disappear under reduced motion. Do not spread the hero's atmospheric motion into reading or editing surfaces.

### Flick-Owned Interaction

Custom widget state, views, event decisions, and recovery belong to Flick: `Controls.fl`, `Search.fl`, `Copy.fl`, `Hero.fl`, and the modules under `docs/src/playground/`. The playground keeps `main` and `initial_view` in `Playground.fl`; `State.fl` owns its model, `Actions.fl` owns updates and supervised work, `View.fl` composes `Toolbar.fl`, `Editor.fl`, and `Results.fl`, and `Dom.fl` declares browser ports. JavaScript/TypeScript is limited to browser/compiler/platform interoperability and framework mounting. Native dialog/popover behavior stays native; DOM attributes, storage, clipboard, search-index access, and compiler execution cross the FFI boundary rather than becoming a parallel widget implementation.

The hero displays actual source with Flick/Effect selection, copy feedback, execution progress, and honest success/failure output. The tools distinguish loading, compiler failure/retry, valid or invalid source, Debug/Release, diagnostics, Run/Stop, timeout, stale output, and generated code. Copy failure offers manual selection instead of claiming success. These are behavior contracts, not decorative status badges. Sidecar snippets are static visual specimens only; they do not replace the real Flick components.

Play and Transpile expose their real initial Flick view in server HTML. Hydration preserves native drafts and selection, then background compiler/highlighter loading begins without replacing the interface. Compiler work stays in its worker, and one latest-check policy with worker backpressure prevents canceled queued checks from delaying current input. Tokenization supplies allowlisted classes and literal text, never source-derived HTML.

## Do's and Don'ts

### Do:
- **Do** reuse semantic theme properties so controls, code, and reading surfaces respond together to theme changes.
- **Do** keep reading measures bounded and allow Play and Transpile the wider workspace.
- **Do** preserve the graphite geometric F, separate violet spark, fine rules, and restrained corners.
- **Do** keep source whitespace, keyboard focus, localized labels, and explicit recovery feedback intact.
- **Do** author custom widget state and behavior in Flick, leaving platform interoperability and mounting at the JavaScript/TypeScript boundary.
- **Do** honor system reduced-motion preferences; animate the wave as one group only.

### Don't:
- **Don't** turn every feature or destination into an elevated card; the shipped editorial sections and ruled rows are intentionally open.
- **Don't** copy the homepage's headline/editor composition into every reading or operating surface.
- **Don't** remove mobile access to locales, reference navigation, or Transpile when compacting the header.
- **Don't** replace real compiler output, copy failures, diagnostics, or loading states with ornamental success claims.
- **Don't** add per-dot animation or atmospheric motion to reading and editing surfaces.
