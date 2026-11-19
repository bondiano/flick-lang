import Formal.LetMut.Lazy
import Formal.SubstComp
import Formal.Equiv.Simple
import Formal.Equiv.Pot

/-!
# O2b: the state of a `let lazy` at its first read (lazy.md §6)

`let lazy x = e` is the task `run` and the variable `x`, `None` until an `await x` sets it
(`lzDecl`, `lzAwait`). A read of `x` that its declaration reaches without a `set` of `x` reads
`None`. This holds when the read stays out of the continuation of a `br` or a `scope` before it,
whose translation binds every variable again from a record (`unpack`). So the `br` of such a
read can match `None` itself (`OnceAt`): its arm without the kept outcome, the inlined `await` of
the task, is the one either block takes.

The two translations are one term once the two `let`s of the declaration have stepped
(`OnceAt.tr_subst`). So the two blocks have the same runs to a final term, with the same store
and regions (`lzOnce`), as `T7_pure` speaks of a run of a block.

The release optimizer (`flick-backend-support/src/opt/lazy.rs`) rewrites a read of the state
this way. Then, when it was the only read, it drops the variable, which nothing reads any more:
that is ordinary dead-variable elimination, not proved here.
-/

namespace Flick

/-- `b₁` and `b₂` are one block but for one `br` on the variable `x`: on `x` in `b₁`, on `None`
in `b₂`. The path to it sets no `x` and goes into a `scope` or the arms of a `br`, never into
their continuations. -/
inductive OnceAt : Nat → Blk → Blk → Prop
  | here {x : Nat} {p : Pat} {a a' k : Blk} : OnceAt x (.br (.var x) p a a' k) (.br lzNone p a a' k)
  | let_ {x : Nat} {e : Tm} {b₁ b₂ : Blk} : OnceAt (x + 1) b₁ b₂ → OnceAt x (.let_ e b₁) (.let_ e b₂)
  | var {x : Nat} {e : Tm} {b₁ b₂ : Blk} : OnceAt (x + 1) b₁ b₂ → OnceAt x (.var e b₁) (.var e b₂)
  | set {x y : Nat} {e : Tm} {b₁ b₂ : Blk} : y ≠ x → OnceAt x b₁ b₂ →
      OnceAt x (.set y e b₁) (.set y e b₂)
  | scope {x : Nat} {a₁ a₂ b : Blk} : OnceAt (x + 1) a₁ a₂ → OnceAt x (.scope a₁ b) (.scope a₂ b)
  | arm {x : Nat} {e : Tm} {p : Pat} {a₁ a₂ a' b : Blk} : OnceAt (x + p.binds) a₁ a₂ →
      OnceAt x (.br e p a₁ a' b) (.br e p a₂ a' b)
  | other {x : Nat} {e : Tm} {p : Pat} {a a₁ a₂ b : Blk} : OnceAt x a₁ a₂ →
      OnceAt x (.br e p a a₁ b) (.br e p a a₂ b)

@[simp] theorem lzNone_subst (σ : Nat → Tm) : lzNone.subst σ = lzNone := rfl

theorem lzNone_value : Value lzNone := .ctor

@[simp] theorem lzNone_rename (ξ : Nat → Nat) : lzNone.rename ξ = lzNone := rfl

theorem liftS_none {σ : Nat → Tm} {x : Nat} (n : Nat) (h : σ x = lzNone) :
    liftS n σ (x + n) = lzNone := by
  simp [liftS, h, show ¬ x + n < n by omega]

/-- **The two translations agree** once `x` is `None`. -/
theorem OnceAt.tr_subst {x : Nat} {b₁ b₂ : Blk} (h : OnceAt x b₁ b₂) :
    ∀ (fin : Option (List Nat)) (ms : List Nat) (σ : Nat → Tm), σ x = lzNone →
      (tr fin ms b₁).subst σ = (tr fin ms b₂).subst σ := by
  induction h with
  | here => intro fin ms σ hσ; simp [tr, Tm.subst, hσ]
  | let_ _ ih =>
    intro fin ms σ hσ; simp only [tr, Tm.subst]; rw [ih _ _ _ (liftS_none 1 hσ)]
  | var _ ih =>
    intro fin ms σ hσ; simp only [tr, Tm.subst]; rw [ih _ _ _ (liftS_none 1 hσ)]
  | @set x y _ _ _ hy _ ih =>
    intro fin ms σ hσ
    simp only [tr, Tm.subst, Tm.subst_rename]
    rw [ih _ _ _ (by simp [redirect, Ne.symm hy, liftS_none 1 hσ])]
  | scope _ ih =>
    intro fin ms σ hσ; simp only [tr, Tm.subst]; rw [ih _ _ _ (liftS_none 1 hσ)]
  | arm _ ih =>
    intro fin ms σ hσ; simp only [tr, Tm.subst]; rw [ih _ _ _ (liftS_none _ hσ)]
  | other _ ih =>
    intro fin ms σ hσ; simp only [tr, Tm.subst]; rw [ih _ _ _ (liftS_none _ hσ)]

/-- A run from `let v in b` to a final term is its `let` step, then a run of `b[v]`. -/
theorem Steps.let_val {root : Roots} {Q : Lbl → Prop} {Λ Λ' : List Rgn} {μ μ' : List Tm}
    {v b e' : Tm} (hv : Value v) (hQ : Q .pure) (hf : Final e') :
    Steps root Q Λ μ (.let_ v b) Λ' μ' e' ↔ Steps root Q Λ μ (b.subst (inst [v])) Λ' μ' e' := by
  constructor
  · intro h
    cases h with
    | refl => rcases hf with h | ⟨_, _, h⟩ | ⟨_, _, _, h⟩ <;> cases h
    | step s _ rest =>
      rcases Equiv.Step.simple (E := []) (r := .let_ v b) (fun _ h => nomatch h)
          (Equiv.let_redex hv) trivial s with ⟨r', hh, rfl, rfl, rfl⟩ | ⟨r', hh, -⟩
      · rw [PHead.det hh (.let_ hv)] at rest; exact rest
      · cases hh
  · exact fun h => .step (.pure (E := []) (fun _ h => nomatch h) (.let_ hv)) hQ h

/-- **O2b**: a block whose read of the state of its `let lazy` matches `None` itself runs as the
block that reads it: to the same final term, with the same store and regions. -/
theorem lzOnce {root : Roots} {Q : Lbl → Prop} {Λ Λ' : List Rgn} {μ μ' : List Tm} {run e' : Tm}
    {B₁ B₂ : Blk} (hrun : Value run) (hQ : Q .pure) (h : OnceAt 0 B₁ B₂) (hf : Final e') :
    Steps root Q Λ μ (tr none [] (lzDecl run B₁)) Λ' μ' e' ↔
      Steps root Q Λ μ (tr none [] (lzDecl run B₂)) Λ' μ' e' := by
  simp only [lzDecl, tr, Option.map_none, List.map_nil]
  rw [Steps.let_val hrun hQ hf, Steps.let_val hrun hQ hf]
  simp only [Tm.subst, lzNone_subst]
  rw [Steps.let_val lzNone_value hQ hf, Steps.let_val lzNone_value hQ hf, Tm.subst_subst, Tm.subst_subst,
    h.tr_subst none [0] _ (by simp [liftS, Tm.subst, inst])]

end Flick
