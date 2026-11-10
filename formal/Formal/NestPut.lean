import Formal.Nest

/-!
# Replacing a thread keeps `⊢ Λ`
-/

namespace Flick

variable {S : Sig}

theorem setKid_get' {Λ : List Rgn} {p j : Nat} {k : Tm} {q : Nat} {g₀ : Rgn} (h : Λ[q]? = some g₀) :
    (setKid Λ p j k)[q]? = some (if q = p then ⟨g₀.live, g₀.kids.set j k, g₀.fins⟩ else g₀) := by
  unfold setKid
  split
  · next g₁ hg₁ =>
    by_cases hq : q = p
    · subst hq; rw [h] at hg₁; cases hg₁
      simp [setRgn, List.getElem?_set_self (getElem?_lt h)]
    · simp [setRgn, List.getElem?_set_ne (Ne.symm hq), h, hq]
  · next hn =>
    by_cases hq : q = p
    · subst hq; rw [h] at hn; cases hn
    · simp [h, hq]

theorem Cfg.put_get {c : Cfg} {i : Option (Nat × Nat)} {e : Tm} {r : Nat} {g : Rgn} (h : (c.put i e).rgs[r]? = some g) :
    ∃ g₀, c.rgs[r]? = some g₀ ∧ g.live = g₀.live ∧ g.fins = g₀.fins := by
  cases i with
  | none => exact ⟨g, h, rfl, rfl⟩
  | some rj =>
    obtain ⟨g₀, h₀, h₁, h₂⟩ := setKid_get h
    by_cases hr : r = rj.1
    · rw [h₁ hr]; exact ⟨g₀, h₀, rfl, rfl⟩
    · rw [h₂ hr]; exact ⟨g₀, h₀, rfl, rfl⟩

theorem Cfg.put_get' {c : Cfg} {i : Option (Nat × Nat)} {e : Tm} {r : Nat} {g₀ : Rgn} (h : c.rgs[r]? = some g₀) :
    ∃ g, (c.put i e).rgs[r]? = some g ∧ g.live = g₀.live ∧ g.fins = g₀.fins := by
  cases i with
  | none => exact ⟨g₀, h, rfl, rfl⟩
  | some rj =>
    refine ⟨_, setKid_get' h, ?_, ?_⟩ <;> split <;> rfl

theorem Cfg.put_live {c : Cfg} {i : Option (Nat × Nat)} {e : Tm} {q : Nat} : Live (c.put i e).rgs q ↔ Live c.rgs q := by
  constructor
  · rintro ⟨g, hg, hl⟩
    obtain ⟨g₀, h₀, h₁, -⟩ := Cfg.put_get hg
    exact ⟨g₀, h₀, h₁ ▸ hl⟩
  · rintro ⟨g₀, hg, hl⟩
    obtain ⟨g, h₀, h₁, -⟩ := Cfg.put_get' (i := i) (e := e) hg
    exact ⟨g, h₀, h₁ ▸ hl⟩

theorem Cfg.at_put {c : Cfg} {i : Option (Nat × Nat)} {e₀ e : Tm} (h : c.at i = some e₀) (i' : Option (Nat × Nat)) :
    (c.put i e).at i' = if i' = i then some e else c.at i' := by
  rcases i with _ | ⟨r, j⟩ <;> rcases i' with _ | ⟨r', j'⟩
  · rfl
  · simp only [Cfg.put, Cfg.at]; simp
  · simp only [Cfg.put, Cfg.at]; simp
  · obtain ⟨g, hg, hl, hk⟩ := Cfg.at_some.mp h
    apply Option.ext; intro k
    rw [Cfg.at_some]
    by_cases hr : r' = r
    · subst hr
      simp only [Cfg.put, setKid_get' hg, if_true, Option.some.injEq]
      by_cases hj : j' = j
      · subst hj
        simp [hl, List.getElem?_set_self (getElem?_lt hk)]
      · simp [hl, List.getElem?_set_ne (Ne.symm hj), hj, Cfg.at_some, hg]
    · have hne : some (r', j') ≠ some (r, j) := by simp [hr]
      simp only [hne, if_false, Cfg.at_some]
      constructor
      · rintro ⟨g', hg', h₁, h₂⟩
        obtain ⟨g₀, h₀, -, h₃⟩ := setKid_get hg'
        rw [h₃ hr] at h₁ h₂; exact ⟨g₀, h₀, h₁, h₂⟩
      · rintro ⟨g', hg', h₁, h₂⟩
        refine ⟨_, setKid_get' hg', ?_⟩
        simp [hr, h₁, h₂]

theorem Cfg.put_mem {c : Cfg} {i : Option (Nat × Nat)} {e : Tm} : (c.put i e).mem = c.mem := by
  cases i <;> rfl

theorem Cfg.at_put_self {c : Cfg} {i : Option (Nat × Nat)} {e₀ e : Tm} (h : c.at i = some e₀) : (c.put i e).at i = some e := by
  rw [Cfg.at_put h]; simp

theorem Cfg.at_put_ne {c : Cfg} {i i' : Option (Nat × Nat)} {e₀ e : Tm} (h : c.at i = some e₀) (hi : i' ≠ i) :
    (c.put i e).at i' = c.at i' := by
  rw [Cfg.at_put h]; simp [hi]

end Flick
