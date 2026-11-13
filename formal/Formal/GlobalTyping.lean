import Formal.RaiseTyping

/-!
# A global at a converted result type
-/

namespace Flick

variable {S : Sig}

theorem Typ.glob_type {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ g T φ, e = .glob g → J = .ex T φ →
      ∃ s ts, S.glob g = some s ∧ ts.length = s.k ∧ s.Holds Θ ts ∧ LeLive S (s.inst ts) T := by
  induction h with
  | conv _ he ih =>
    intro g T φ hv hJ; cases hJ
    obtain ⟨s, ts, hg, hl, hH, ht⟩ := ih g _ _ hv rfl
    exact ⟨s, ts, hg, hl, hH, ht.trans (.equiv he)⟩
  | glob hg hl hH ht => intro g T φ hv hJ; cases hv; cases hJ; exact ⟨_, _, hg, hl, hH, ht⟩
  | allI hv | errVal hv => intro g T φ he _; cases he; cases hv
  | _ => intro g T φ hv _; cases hv

end Flick
