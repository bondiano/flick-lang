import Formal.Elab.Witness.Prog
import Formal.Elab.Witness.Closed
import Formal.Elab.CloseTy

/-!
# Witness: the constants of `Γ₀` have closed schemes

`consts_closed`: every constant of the witness, the family `unc n` included, mentions only
the variables it binds, so closing the premises by types keeps its instantiations
(`T6a_close`).
-/

namespace Flick.Elab.Witness

open Flick.Elab

theorem curry_lt {q r : Ty} {as : List Ty} {k : Nat} (hq : ∀ n, q.Occurs n → n < k)
    (hr : ∀ n, r.Occurs n → n < k) (ha : ∀ a ∈ as, ∀ n, a.Occurs n → n < k) :
    ∀ n, (curry q as r).Occurs n → n < k := by
  induction as with
  | nil => exact hr
  | cons a as ih =>
    intro n hn
    rcases hn with hn | hn | hn | hn
    · exact ha a (.head _) n hn
    · exact ih (fun b hb => ha b (.tail _ hb)) n hn
    · exact hq n hn
    · cases hn

theorem nest_lt {tup k : Nat} : ∀ {t : Ty} {as : List Ty}, (∀ n, t.Occurs n → n < k) →
    (∀ a ∈ as, ∀ n, a.Occurs n → n < k) → ∀ n, (nest tup t as).Occurs n → n < k
  | _, [], ht, _ => ht
  | t, a :: as, ht, ha => nest_lt (t := .app (.app (.con tup) t) a) (as := as)
      (fun n hn => by
        rcases hn with (hn | hn) | hn
        · cases hn
        · exact ht n hn
        · exact ha a (.head _) n hn)
      (fun b hb => ha b (.tail _ hb))

theorem vars_lt {k : Nat} : ∀ a ∈ vars k, ∀ n, a.Occurs n → n < k := by
  intro a ha n hn
  obtain ⟨j, hj, rfl⟩ := List.mem_map.1 ha
  simp only [Ty.Occurs] at hn
  exact hn ▸ List.mem_range.1 hj

theorem constSch_lt {c : Nat} {s : Scheme} (h : constSch c = some s) :
    s.ctx = [] ∧ ∀ n, s.ty.Occurs n → n < s.k := by
  unfold constSch at h
  split at h
  any_goals
    cases h
    exact ⟨rfl, fun n hn => by
      simp [Ty.Occurs, Stage.Occurs, bindTy, appQ, arrP, optTy] at hn ⊢ <;> omega⟩
  · cases h
  · split at h
    · split at h
      · cases h
        refine ⟨rfl, fun n hn => ?_⟩
        rcases hn with hn | hn | hn | hn
        · exact curry_lt (fun m hm => by simp [Ty.Occurs] at hm ⊢; omega)
            (fun m hm => by simp [Ty.Occurs] at hm ⊢; omega)
            (fun a ha m hm => Nat.lt_succ_of_lt (Nat.lt_succ_of_lt (vars_lt a ha m hm))) n hn
        · rcases hn with hn | hn | hn
          · exact nest_lt (fun m hm => by simp [Ty.Occurs] at hm ⊢; omega)
              (fun a ha m hm => Nat.lt_succ_of_lt (Nat.lt_succ_of_lt
                (vars_lt a (List.mem_of_mem_tail ha) m hm))) n hn
          · simp [Ty.Occurs, uncSch] at hn ⊢; omega
          · simp [Ty.Occurs, Stage.Occurs, uncSch] at hn ⊢; omega
        · cases hn
        · cases hn
      · cases h
    · split at h
      · cases h
        exact ⟨rfl, fun n hn => by
          simp [PreludeSchemes.from_result, Ty.Occurs, Stage.Occurs] at hn ⊢; omega⟩
      · cases h

/-- **The constants of the witness have closed schemes.** -/
theorem consts_closed : P.ConstsClosed := by
  intro c s hc σ σ' hσ
  obtain ⟨hctx, hlt⟩ := constSch_lt hc
  simp only [hctx, dicts, List.foldr_nil, TTy.subst]
  rw [Ty.subst_congr fun n hn => hσ n (hlt n hn)]

end Flick.Elab.Witness
