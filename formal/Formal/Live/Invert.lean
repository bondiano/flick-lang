import Formal.Live.Order

/-!
# Inversion of mode-indexed subtyping

Both disciplines preserve type formers. The stage comparison is on effective
use stages, not on a qualified value's raw latent stage.
-/

namespace Flick

theorem TypingMode.subtype.arrow_left {mode : TypingMode} {a φ t C T}
    (h : mode.subtype (.arrow a φ t C) T) :
    ∃ a' φ' t' C', T = .arrow a' φ' t' C' ∧ mode.subtype a' a ∧
      (mode.use φ C).le (mode.use φ' C') ∧ mode.subtype t t' ∧
      Caps.Sub (Ty.arrow a φ t C).reach T.reach := by
  cases mode with
  | captures => exact Le.arrow_left h
  | live => exact LeLive.arrow_left h

theorem TypingMode.subtype.arrow_right {mode : TypingMode} {a φ t C T}
    (h : mode.subtype T (.arrow a φ t C)) :
    ∃ a' φ' t' C', T = .arrow a' φ' t' C' ∧ mode.subtype a a' ∧
      (mode.use φ' C').le (mode.use φ C) ∧ mode.subtype t' t ∧
      Caps.Sub T.reach (Ty.arrow a φ t C).reach := by
  cases mode with
  | captures => exact Le.arrow_right h
  | live => exact LeLive.arrow_right h

theorem TypingMode.subtype.eff_left_mode {mode : TypingMode} {r ε t C T}
    (h : mode.subtype (.eff r ε t C) T) :
    ∃ r' ε' t' C', T = .eff r' ε' t' C' ∧
      (mode.use (.rt r ε) C).le (mode.use (.rt r' ε') C') ∧
      mode.subtype t t' ∧ Caps.Sub (Ty.eff r ε t C).reach T.reach := by
  cases mode with
  | captures =>
    obtain ⟨_, _, _, _, he, hr, hε, ht, hc⟩ := Le.eff_left h
    exact ⟨_, _, _, _, he, ⟨hr, hε⟩, ht, hc⟩
  | live => exact LeLive.eff_left h

theorem TypingMode.subtype.eff_right_mode {mode : TypingMode} {r ε t C T}
    (h : mode.subtype T (.eff r ε t C)) :
    ∃ r' ε' t' C', T = .eff r' ε' t' C' ∧
      (mode.use (.rt r' ε') C').le (mode.use (.rt r ε) C) ∧
      mode.subtype t' t ∧ Caps.Sub T.reach (Ty.eff r ε t C).reach := by
  cases mode with
  | captures =>
    obtain ⟨_, _, _, _, he, hr, hε, ht, hc⟩ := Le.eff_right h
    exact ⟨_, _, _, _, he, ⟨hr, hε⟩, ht, hc⟩
  | live => exact LeLive.eff_right h

end Flick
