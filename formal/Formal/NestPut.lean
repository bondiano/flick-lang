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

/-- A thread replaced, the regions and the ancestors kept: the cells and the
locations may change. -/
theorem Nest.put_local {Ψ Ψ' : List Ty} {c : Cfg} {μ' : List Tm} {anc B B' home i e e'}
    (hn : Nest Ψ c anc B home) (hi : c.at i = some e) (hl : ∀ A l, LocOk Ψ B A l → LocOk Ψ' B' A l)
    (h' : e'.Within Ψ' B' anc (baseOf anc i)) (hf : ∀ q, e'.Has (.frame q) ↔ e.Has (.frame q))
    (hc : ∀ (l : Nat) (v : Tm), μ'[l]? = some v → v.Within Ψ' B' anc (B' l) ∧ v.NoIns) :
    Nest Ψ' ((⟨c.rgs, μ', c.main⟩ : Cfg).put i e') anc B' home := by
  have hi₁ : (⟨c.rgs, μ', c.main⟩ : Cfg).at i = some e := by
    rcases i with _ | ⟨r, j⟩
    · exact hi
    · exact Cfg.at_some.mpr (Cfg.at_some.mp hi)
  have hat : ∀ i', (((⟨c.rgs, μ', c.main⟩ : Cfg).put i e').at i' = if i' = i then some e' else c.at i') := by
    intro i'; rw [Cfg.at_put hi₁]
    rcases i' with _ | ⟨r, j⟩
    · rfl
    · split
      · rfl
      · apply Option.ext; intro k; rw [Cfg.at_some, Cfg.at_some]
  have hlive : ∀ q, Live ((⟨c.rgs, μ', c.main⟩ : Cfg).put i e').rgs q ↔ Live c.rgs q :=
    fun q => Cfg.put_live
  refine ⟨hn.self, hn.le, fun q hq => hn.chain q ((hlive q).mp hq),
    fun q x hq hx => (hlive x).mpr (hn.live q x ((hlive q).mp hq) hx), fun i' k hk => ?_,
    fun q hq => ?_, fun r g hg hgl f hf' => ?_, by rw [Cfg.put_mem]; exact hc⟩
  · rw [hat] at hk
    split at hk
    · subst i'; cases hk
      refine ⟨h', fun q hq => ?_⟩
      obtain ⟨h₁, h₂⟩ := (hn.thread i e hi).2 q ((hf q).mp hq)
      exact ⟨h₁, (hlive q).mpr h₂⟩
    · obtain ⟨h₁, h₂⟩ := hn.thread i' k hk
      exact ⟨h₁.mono hl fun _ _ => rfl, fun q hq => let ⟨a, b⟩ := h₂ q hq; ⟨a, (hlive q).mpr b⟩⟩
  · obtain ⟨k, hk, hkq⟩ := hn.home q ((hlive q).mp hq)
    rw [hat]
    by_cases hh : home q = i
    · rw [if_pos hh]; rw [hh] at hk; rw [hi] at hk; cases hk
      exact ⟨e', rfl, (hf q).mpr hkq⟩
    · rw [if_neg hh]; exact ⟨k, hk, hkq⟩
  · obtain ⟨g₀, h₀, h₁, h₂⟩ := Cfg.put_get hg
    rw [h₂] at hf'
    obtain ⟨h₃, h₄⟩ := hn.fins r g₀ h₀ (h₁ ▸ hgl) f hf'
    exact ⟨h₃.mono hl fun _ _ => rfl, h₄⟩

end Flick
