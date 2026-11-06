import Formal.NestScope

/-!
# Closing a region keeps `⊢ Λ`; L5
-/

namespace Flick

variable {S : Sig} {Ψ : List Ty} {B anc : Nat → Nat → Prop}

attribute [local instance] Classical.propDecidable

theorem inside_not_value {p : Nat} {w : Tm} : ¬ Value (.inside p w) := fun h => by cases h

theorem Cfg.at_setRgn {Λ : List Rgn} {μ : List Tm} {m : Tm} {p : Nat} {g' : Rgn}
    {i : Option (Nat × Nat)} (hi : ∀ j, i ≠ some (p, j)) :
    (⟨setRgn Λ p g', μ, m⟩ : Cfg).at i = (⟨Λ, μ, m⟩ : Cfg).at i := by
  rcases i with _ | ⟨r, j⟩
  · rfl
  · have hr : r ≠ p := fun h => hi j (by rw [h])
    apply Option.ext; intro k
    rw [Cfg.at_some, Cfg.at_some]
    simp only [setRgn, List.getElem?_set_ne (Ne.symm hr)]

theorem Cfg.at_setRgn_dead {Λ : List Rgn} {μ : List Tm} {m : Tm} {p j : Nat} {g g' : Rgn}
    (hg : Λ[p]? = some g) (hl : g'.live = false) : (⟨setRgn Λ p g', μ, m⟩ : Cfg).at (some (p, j)) = none := by
  apply Option.eq_none_iff_forall_ne_some.mpr
  intro k h
  obtain ⟨g'', h₁, h₂, -⟩ := Cfg.at_some.mp h
  simp only [setRgn, List.getElem?_set_self (getElem?_lt hg), Option.some.injEq] at h₁
  subst h₁; rw [hl] at h₂; cases h₂

/-- **L5 (nesting).** When the body of `π` and its children are finished, no live
region but `π` has `π` among its ancestors: every region created in `π` is closed. -/
theorem Nest.nested {c : Cfg} {home i E π w g} (hn : Nest Ψ c anc B home)
    (hi : c.at i = some (plug E (.inside π w))) (hE : OkE E) (hw : Final w)
    (hg : c.rgs[π]? = some g) (hk : ∀ k ∈ g.kids, Final k) :
    ∀ q, Live c.rgs q → anc q π → q = π := by
  have hW := (hn.thread i _ hi).1
  obtain ⟨hWE, hπH, -, hwW⟩ := within_of_plug inside_not_value hE hW
  have hwn := Final.noFrame hw hwW
  have homeπ : home π = i := ((hn.thread i _ hi).2 π (plug_has E (by simp [Tm.Has]))).1
  intro q
  induction q using Nat.strongRecOn with
  | _ q ih =>
  intro hq hqπ
  apply Classical.byContradiction
  intro hne
  obtain ⟨e_q, he_q, hfq⟩ := hn.home q hq
  have hWq := (hn.thread _ _ he_q).1
  obtain ⟨hnotA, -, hsub, hanc⟩ := hWq.frame hfq
  rcases hanc π hqπ with hA | hfπ
  · rcases hh : home q with _ | ⟨r, j⟩
    · rw [hh] at hA; exact hA
    · rw [hh] at hA he_q hsub hnotA hWq
      have hr := Cfg.at_live he_q r j rfl
      by_cases hrπ : r = π
      · subst hrπ
        obtain ⟨g', hg', -, hk'⟩ := Cfg.at_some.mp he_q
        rw [hg] at hg'; cases hg'
        exact Final.noFrame (hk _ (List.mem_of_getElem? hk')) hWq q hfq
      · have hrq : anc q r := hsub r (hn.self r)
        have hrne : r ≠ q := fun h => hnotA (h ▸ hn.self r)
        exact hrπ (ih r (Nat.lt_of_le_of_ne (hn.le q r hrq) hrne) hr hA)
  · have h₁ := ((hn.thread _ _ he_q).2 π hfπ).1
    rw [homeπ] at h₁
    rw [← h₁, hi] at he_q; cases he_q
    rcases plug_has_inv (r := .var 0) E hfq with h | h
    · rcases (hWE.frameAnc h).2.2.2 π hqπ with h | h
      · exact hπH h
      · exact within_disj inside_not_value hE hW (by simp [Tm.Has]) h
    · rcases h with h | h | h | h
      · cases h
      · cases h
      · cases h; exact hne rfl
      · exact hwn q h

/-- Closing `π`: its body leaves the region with what its type shows. -/
theorem Nest.close {c : Cfg} {home i E π w g} (hn : Nest Ψ c anc B home)
    (hi : c.at i = some (plug E (.inside π w))) (hE : OkE E) (hw : Final w)
    (hg : c.rgs[π]? = some g) (hk : ∀ k ∈ g.kids, Final k)
    (hmove : w.Within Ψ B anc (fun x => holeE (baseOf anc i) E x ∨ x = π) →
      ¬ holeE (baseOf anc i) E π → w.Within Ψ B anc (holeE (baseOf anc i) E)) :
    Nest Ψ ((⟨setRgn c.rgs π ⟨false, g.kids, []⟩, c.mem, c.main⟩ : Cfg).put i (plug E w)) anc B home := by
  have hW := (hn.thread i _ hi).1
  obtain ⟨hWE, hπH, -, hwW⟩ := within_of_plug inside_not_value hE hW
  have hw' := hmove hwW hπH
  have hwn := Final.noFrame hw hw'
  have homeπ : home π = i := ((hn.thread i _ hi).2 π (plug_has E (by simp [Tm.Has]))).1
  have nested := hn.nested hi hE hw hg hk
  have hiπ : ∀ j, i ≠ some (π, j) := by
    rintro j rfl
    exact (hW.frame (plug_has E (by simp [Tm.Has] : (Tm.inside π w).Has (.frame π)))).1 (hn.self π)
  let c₁ : Cfg := ⟨setRgn c.rgs π ⟨false, g.kids, []⟩, c.mem, c.main⟩
  have hc₁ : c₁.at i = some (plug E (.inside π w)) := by rw [Cfg.at_setRgn hiπ]; exact hi
  have hat : ∀ i', (c₁.put i (plug E w)).at i' =
      if i' = i then some (plug E w) else if ∃ j, i' = some (π, j) then none else c.at i' := by
    intro i'; rw [Cfg.at_put hc₁]
    split
    · rfl
    · split
      · next h => obtain ⟨j, rfl⟩ := h; exact Cfg.at_setRgn_dead hg rfl
      · next h => exact Cfg.at_setRgn fun j h' => h ⟨j, h'⟩
  have get : ∀ {r g''}, c₁.rgs[r]? = some g'' →
      (r = π ∧ g'' = ⟨false, g.kids, []⟩) ∨ (r ≠ π ∧ c.rgs[r]? = some g'') := fun h => setRgn_get hg h
  have hlive : ∀ q, Live (c₁.put i (plug E w)).rgs q ↔ Live c.rgs q ∧ q ≠ π := by
    intro q; rw [Cfg.put_live]
    constructor
    · rintro ⟨g'', h₁, h₂⟩
      rcases get h₁ with ⟨rfl, rfl⟩ | ⟨hq, h₁⟩
      · cases h₂
      · exact ⟨⟨g'', h₁, h₂⟩, hq⟩
    · rintro ⟨⟨g'', h₁, h₂⟩, hq⟩
      refine ⟨g'', ?_, h₂⟩
      show (setRgn c.rgs π _)[q]? = some g''
      rw [setRgn, List.getElem?_set_ne (Ne.symm hq)]; exact h₁
  have frE : ∀ q, (plug E (.inside π w)).Has (.frame q) → q ≠ π → (plug E (.var 0)).Has (.frame q) := by
    intro q hq hne
    rcases plug_has_inv (r := .var 0) E hq with h | h
    · exact h
    · rcases h with h | h | h | h
      · cases h
      · cases h
      · cases h; exact (hne rfl).elim
      · exact (hwn q h).elim
  refine ⟨hn.self, hn.le, fun q hq => hn.chain q ((hlive q).mp hq).1, fun q x hq hx => ?_,
    fun i' e' he' => ?_, fun q hq => ?_, fun r g'' hr hgl f hf => ?_, fun l v hv => ?_⟩
  · obtain ⟨hq, hqπ⟩ := (hlive q).mp hq
    exact (hlive x).mpr ⟨hn.live q x hq hx, fun h => hqπ (nested q hq (h ▸ hx))⟩
  · rw [hat] at he'
    split at he'
    · subst i'; cases he'
      refine ⟨hW.replug inside_not_value hE hw' (fun x hx => (hwn x hx).elim), fun q hq => ?_⟩
      have hq' := (plug_frames_iff hwn).mp hq
      have hqπ : q ≠ π := fun h => within_disj inside_not_value hE hW (x := π) (by simp [Tm.Has]) (h ▸ hq')
      obtain ⟨h₁, h₂⟩ := (hn.thread i _ hi).2 q (plug_of_hole hq')
      exact ⟨h₁, (hlive q).mpr ⟨h₂, hqπ⟩⟩
    · split at he'
      · cases he'
      · next hne _ =>
        obtain ⟨h₁, h₂⟩ := hn.thread i' e' he'
        refine ⟨h₁, fun q hq => ?_⟩
        obtain ⟨h₃, h₄⟩ := h₂ q hq
        exact ⟨h₃, (hlive q).mpr ⟨h₄, fun h => hne (by rw [← h₃, h, homeπ])⟩⟩
  · obtain ⟨hq, hqπ⟩ := (hlive q).mp hq
    obtain ⟨e_q, he_q, hfq⟩ := hn.home q hq
    rw [hat]
    split
    · next h => rw [h, hi] at he_q; cases he_q
                exact ⟨_, rfl, plug_of_hole (frE q hfq hqπ)⟩
    · split
      · next h =>
        obtain ⟨j, hj⟩ := h
        rw [hj] at he_q
        obtain ⟨g', hg', -, hk'⟩ := Cfg.at_some.mp he_q
        rw [hg] at hg'; cases hg'
        have hWq := (hn.thread _ _ he_q).1
        exact (Final.noFrame (hk _ (List.mem_of_getElem? hk')) hWq q hfq).elim
      · exact ⟨e_q, he_q, hfq⟩
  · obtain ⟨g₀, h₀, h₁, h₂⟩ := Cfg.put_get hr
    rcases get h₀ with ⟨rfl, rfl⟩ | ⟨_, h₀⟩
    · rw [h₁] at hgl; cases hgl
    · rw [h₂] at hf; exact hn.fins r g₀ h₀ (h₁ ▸ hgl) f hf
  · rw [Cfg.put_mem] at hv; exact hn.cells l v hv

end Flick
