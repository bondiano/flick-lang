import Formal.Live.LRgs
import Formal.Nest

/-!
# Typed configurations with live stages for stored work

The main stage is the program's unchanged stage, with no runtime region atoms. Stored
work has a stage of its owner's ancestors. Structural places are supplied by `LNest`.
-/

namespace Flick

variable {S : Sig}

theorem Stage.no_atm_zero {φ : Stage} (h : φ.substT (kill 0) = φ) :
    ∀ x, ¬ φ.svc (.atm (.r x)) := by
  intro x hx
  rw [← h] at hx
  exact Nat.not_lt_zero x (Stage.kill_atm hx).2

def LCfgTy (S : Sig) (Ψ : List Ty) (c : Cfg) (t : Ty) (φ : Stage)
    (anc : Nat → Nat → Prop) : Prop :=
  CfgTyM S .live Ψ c t φ ∧ LRgs S Ψ anc c.rgs

/-- A step of the main thread keeps typing and the stages of stored work, with the same store
extension; its new term keeps the program's stage. -/
theorem LCfgTy.main {root Ψ Λ μ e Λ' μ' e' t φ l anc} (hroot : RootTyM S .live root)
    (hrb : RootBelow root) (ht : t.YF 0) (hφ : φ.substT (kill 0) = φ)
    (hc : LCfgTy S Ψ ⟨Λ, μ, e⟩ t φ anc) (hself : ∀ p, anc p p)
    (hplace : ∀ E r, OkE E → e = plug E r → ¬ Value r → ∃ H, Place Ψ anc φ E H)
    (hs : Step root l Λ μ e (.cfg Λ' μ' e')) :
    ∃ X, LCfgTy S (Ψ ++ X) ⟨Λ', μ', e'⟩ t φ anc ∧ ∀ u ∈ X, u.YF Λ.length := by
  obtain ⟨⟨hμ, h, -, hb, hΨ, ha, hcs, hcc, hcg⟩, hΛ⟩ := hc
  obtain ⟨X, hμ', h', hX⟩ := preservation hroot h hμ hb.main hb.mem hΨ hs
  obtain ⟨hb', hl⟩ := hs.below hrb hb
  obtain ⟨ha', hka⟩ := hs.asc hroot ha hb.main
  obtain ⟨hs', cell', -, hcc'⟩ := hs.caught hroot hcg ha hb (Nat.zero_le _)
  have hlr := hΛ.step hself h ha.main hplace hs
  rw [ht.mono (Nat.zero_le _), Stage.YF_mono hφ (Nat.zero_le _)] at h'
  refine ⟨X, ⟨⟨hμ', h', (hlr.weaken X).toRgs, hb', fun u hu => ?_,
    hka _ ha', hs', cell', hcc'⟩, hlr.weaken X⟩, hX⟩
  rcases List.mem_append.mp hu with hu | hu
  · exact (hΨ u hu).mono hl
  · exact (hX u hu).mono hl

/-- A step of a child of a live region keeps typing and the stages of stored work, with the
same store extension; the child stays at a stage of its owner's ancestors. -/
theorem LCfgTy.kid {root Ψ Λ μ e p g j k Λ' μ' k' t φ l anc} (hroot : RootTyM S .live root)
    (hrb : RootBelow root) (hc : LCfgTy S Ψ ⟨Λ, μ, e⟩ t φ anc) (hself : ∀ p, anc p p)
    (hg : Λ[p]? = some g) (hj : g.kids[j]? = some k)
    (hplace : ∀ t' ψ, TypM S .live Ψ Facts.age [] k (.ex t' ψ) → (∀ x, ψ.svc (.atm (.r x)) → anc p x) →
      ∀ E r, OkE E → k = plug E r → ¬ Value r → ∃ H, Place Ψ anc ψ E H)
    (hs : Step root l Λ μ k (.cfg Λ' μ' k')) :
    ∃ X, LCfgTy S (Ψ ++ X) ⟨setKid Λ' p j k', μ', e⟩ t φ anc ∧ (∀ u ∈ X, u.YF Λ.length) ∧
      LTy S (Ψ ++ X) (anc p) k' := by
  obtain ⟨⟨hμ, h, -, hb, hΨ, ha, hcs, hcc, hcg⟩, hΛ⟩ := hc
  have hkm := List.mem_of_getElem? hj
  obtain ⟨t', φ', hk, hat⟩ := (hΛ _ _ hg).1 _ hkm
  have hkb := (hb.rgs _ _ hg).1 _ hkm
  have hkA := ha.rgs _ _ hg _ (List.mem_append_left _ hkm)
  obtain ⟨X, hμ', hk', hX⟩ := preservation hroot hk hμ hkb hb.mem hΨ hs
  obtain ⟨hb', hlen⟩ := hs.below hrb ⟨hkb, hb.mem, hb.rgs⟩
  obtain ⟨ha', hka⟩ := hs.asc hroot ⟨hkA, ha.rgs, ha.mem⟩ hkb
  obtain ⟨m, hm, hkm'⟩ := hcg.rgs _ _ hg _ (List.mem_append_left _ hkm)
  obtain ⟨hs', cell', hg', hkc⟩ := hs.caught hroot (n := m)
    ⟨hkm', hcg.rgs⟩ ⟨hkA, ha.rgs, ha.mem⟩ ⟨hkb, hb.mem, hb.rgs⟩ hm
  have hlr₀ := (hΛ.step hself hk hkA (hplace _ _ hk hat) hs).weaken X
  refine ⟨X, ⟨⟨hμ', h.weakenStore X, (hlr₀.setKid (LTy.kill hk' hat)).toRgs,
    CfgBelow.kid ⟨hb.main.mono hlen, hb'.mem, hb'.rgs⟩ hb'.main, fun u hu => ?_,
    (hka _ ha.main).setKid ha', hs', cell',
    hkc.setKid (Nat.le_trans hm hlen) hkc.main (hcg.main.ghostB hb.main hg')⟩,
    hlr₀.setKid (LTy.kill hk' hat)⟩, hX, LTy.kill hk' hat⟩
  rw [setKid_length]
  rcases List.mem_append.mp hu with hu | hu
  · exact (hΨ u hu).mono hlen
  · exact (hX u hu).mono hlen

theorem LCfgTy.init {e t φ} (h : TypM S .live [] Facts.none [] e (.ex t φ)) (hb : e.Below 0)
    (anc : Nat → Nat → Prop) : LCfgTy S [] ⟨[], [], e⟩ t φ anc :=
  ⟨CfgTyM.init h hb, fun _ _ hg => by simp at hg⟩

end Flick
