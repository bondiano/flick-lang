import Formal.LetMut
import Formal.Elab.Exh

/-!
# `let lazy` as a variable of its function (lazy.md §4)

`let lazy x = e` is a variable `x` whose state is `None` until an `await x` runs `e`. The
outcome of `e`, `Ok v` or `Err err` (`lzAttempt`), is kept as `Some o`, and `await x` gives it
back: the value, or the error raised again (`lzOut`, the body of `from_result`). A write
`x = v` of a `let lazy mut` is `Some (Ok v)`. These are blocks of `LetMut`:

- `lzDecl`: `var None b`;
- `lzAwait x e k`, for `let y = await x in k`: a two-armed `br` on the state, whose
  first arm gives the kept outcome and whose other arm runs `e`, `set`s the state and gives
  the new outcome;
- `lzWrite x v k`: `set x (Some (Ok v)) k`.

The lowering writes the initializer once. With one `await`, it runs `lzAttempt e` in place
(`lzAwait`). With more, the declaration is `lzDeclFn`: a `let` of the function
`λ_. lzAttempt e` before the state, and each `await` takes the outcome from `lzCall f`, the
call of that function (`lzAwaitOf`, `MutOk.lzAwaitFn`).

Each is `MutOk` when its parts are typed (`MutOk.lzDecl`, `MutOk.lzDeclFn`, `MutOk.lzAwaitOf`,
`MutOk.lzAwait`, `MutOk.lzAwaitFn`, `MutOk.lzWrite`): they are derived rules of `MutOk`, so **T7** holds of
every block written with them. Its translation is a core term of the block's type at the
block's stage, and `T7_pure` and `T7_regions` hold of it too. A signal or a defect leaves `e`
before the `set`, so the state stays `None`, as the reset of lazy.md §3 leaves a cell pending.

The initializer reads the values of the declaration. The function is a value made there, and
`f` is no variable of the block. An initializer run in place reads no variable of the block
(`FLICK-G005`), so a `set` between the declaration and the `await` does not change what it
reads: `tr` renames each term after `set x` by `redirect x`, which is the plain shift on a
term without `x` (`Tm.rename_redirect`).

The constructors are the Prelude's `Option` and `Result`, with the fields of their
declarations (`LzSig`).
-/

namespace Flick

/-- The Prelude's `Option` and `Result` in the signature of the core: `None | Some a` and
`Err e | Ok a`, ordinary constructors, and `Ok` and `Err` are all of `Result`. -/
structure LzSig (S : Sig) : Prop where
  ctorNone : S.ctor "None" = some (1, [], "Option")
  ctorSome : S.ctor "Some" = some (1, [.tvar 0], "Option")
  ctorErr : S.ctor "Err" = some (2, [.tvar 0], "Result")
  ctorOk : S.ctor "Ok" = some (2, [.tvar 1], "Result")
  resNone : S.res "None" = idTs 1
  resSome : S.res "Some" = idTs 1
  resErr : S.res "Err" = idTs 2
  resOk : S.res "Ok" = idTs 2
  result : ∀ K k ts, S.ctor K = some (k, ts, "Result") → K = "Ok" ∨ K = "Err"

def optT (t : Ty) : Ty := .tcon "Option" [t]

def resT (e a : Ty) : Ty := .tcon "Result" [e, a]

/-- The state of a `let lazy` of `a` with the errors `ε`: `Option (Result (Errors ε) a)`. -/
def lzT (a : Ty) (ε : Row) : Ty := optT (resT (.errors ε) a)

def lzNone : Tm := .ctor "None"

def lzSome (o : Tm) : Tm := .app (.ctor "Some") o

def lzOk (v : Tm) : Tm := .app (.ctor "Ok") v

def lzErr (v : Tm) : Tm := .app (.ctor "Err") v

/-- `attempt e`: the outcome of `e`, its value as `Ok v` or its error as `Err err`. -/
def lzAttempt (e : Tm) : Tm := .try_ (lzOk e) (.acons .var (lzErr (.var 0)) .anil)

/-- `out o`: the value of the outcome `o`, or its error raised again. -/
def lzOut (o : Tm) : Tm :=
  .match_ o (.acons (pvars "Ok" 1) (.var 0) (.acons (pvars "Err" 1) (.raise (.var 0)) .anil))

/-- `let lazy x = e in b`: the variable `x`, `None`. -/
def lzDecl (b : Blk) : Blk := .var lzNone b

/-- `let y = await x in k` of the `let lazy` `x`, where `o` gives the outcome of its
initializer. -/
def lzAwaitOf (x : Nat) (o : Tm) (k : Blk) : Blk :=
  .br (.var x) (pvars "Some" 1) (.ret (lzOut (.var 0)))
    (.let_ o (.set (x + 1) (lzSome (.var 0)) (.ret (lzOut (.var 0))))) k

/-- `let y = await x in k` of the `let lazy` `x` whose initializer `e` runs in place. -/
def lzAwait (x : Nat) (e : Tm) (k : Blk) : Blk := lzAwaitOf x (lzAttempt e) k

/-- `x = v` of a `let lazy mut`. -/
def lzWrite (x : Nat) (v : Tm) (k : Blk) : Blk := .set x (lzSome (lzOk v)) k

/-- `let lazy x = e in b` whose initializer several `await`s run: `fn` is the function
`λ_. lzAttempt e` of the declaration, bound before the state. -/
def lzDeclFn (fn : Tm) (b : Blk) : Blk := .let_ fn (lzDecl b)

/-- The call `f ()` that gives an `await` the outcome of the initializer held by `f`. -/
def lzCall (f : Nat) : Tm := .app (.var f) (.lit .unit)

variable {S : Sig}

/-- The pattern `K x` of a constructor of one field, which binds the field. -/
theorem pat_one {K : Name} {k : Nat} {f f' : Ty} {T : Name} {us : List Ty}
    (hK : S.ctor K = some (k, [f], T)) (hres : S.res K = idTs k) (hk : us.length = k)
    (hf : f.substT (instT us) = f') : PatTy S true (pvars K 1) (.tcon T us) [f'] := by
  have hty : Sig.ctorTyR [f] T (S.res K) us = .arrow f' .pure (.tcon T us) Caps.none := by
    rw [hres, ← hf]; simp [Sig.ctorTyR, Ty.arrows, idTs_inst hk]
  exact .adt (.app (hty ▸ PatTy.head hK (hres ▸ Sig.inj_plain) hk) .var) (Ty.tcon_hd _ _)

namespace LzSig

variable (h : LzSig S)
include h

theorem typ_none {Ψ Θ Γ t φ} : Typ S Ψ Θ Γ lzNone (.ex (optT t) φ) :=
  .ctor (us := [t]) h.ctorNone rfl (Sig.holds_plain S h.ctorNone h.resNone rfl)
    (by rw [h.resNone]; simpa [Sig.ctorTyR, Ty.arrows, idTs_inst, optT] using LeLive.refl)

theorem typ_some {Ψ Θ Γ o t φ} (ho : Typ S Ψ Θ Γ o (.ex t φ)) :
    Typ S Ψ Θ Γ (lzSome o) (.ex (optT t) φ) :=
  .app (.ctor (us := [t]) h.ctorSome rfl (Sig.holds_plain S h.ctorSome h.resSome rfl)
      (by rw [h.resSome]; simpa [Sig.ctorTyR, Ty.arrows, idTs_inst, optT, Ty.substT, instT]
        using LeLive.refl))
    ho (ψ := .pure) (C := Caps.none) (Stage.pure_le _) LeLive.refl

theorem typ_ok {Ψ Θ Γ v e a φ} (hv : Typ S Ψ Θ Γ v (.ex a φ)) :
    Typ S Ψ Θ Γ (lzOk v) (.ex (resT e a) φ) :=
  .app (.ctor (us := [e, a]) h.ctorOk rfl (Sig.holds_plain S h.ctorOk h.resOk rfl)
      (by rw [h.resOk]; simpa [Sig.ctorTyR, Ty.arrows, idTs_inst, resT, Ty.substT, instT]
        using LeLive.refl))
    hv (ψ := .pure) (C := Caps.none) (Stage.pure_le _) LeLive.refl

theorem typ_err {Ψ Θ Γ v e a φ} (hv : Typ S Ψ Θ Γ v (.ex e φ)) :
    Typ S Ψ Θ Γ (lzErr v) (.ex (resT e a) φ) :=
  .app (.ctor (us := [e, a]) h.ctorErr rfl (Sig.holds_plain S h.ctorErr h.resErr rfl)
      (by rw [h.resErr]; simpa [Sig.ctorTyR, Ty.arrows, idTs_inst, resT, Ty.substT, instT]
        using LeLive.refl))
    hv (ψ := .pure) (C := Caps.none) (Stage.pure_le _) LeLive.refl

theorem typ_out {Ψ Θ Γ o a ε φ} (ho : Typ S Ψ Θ Γ o (.ex (resT (.errors ε) a) φ))
    (hε : (Stage.rt Row.none ε).le φ) : Typ S Ψ Θ Γ (lzOut o) (.ex a φ) := by
  have hall : ∀ K k ts, S.ctor K = some (k, ts, "Result") →
      pvars K ts.length ∈ [pvars "Ok" 1, pvars "Err" 1] := by
    intro K k ts hK
    rcases h.result K k ts hK with rfl | rfl
    · rw [h.ctorOk] at hK; cases hK; simp
    · rw [h.ctorErr] at hK; cases hK; simp
  exact .match_ ho
    (.acons (pat_one h.ctorOk h.resOk rfl (by simp [Ty.substT, instT])) (Typ.var_mono rfl)
      (.acons (pat_one h.ctorErr h.resErr rfl (by simp [Ty.substT, instT]))
        (.raise (Typ.var_mono rfl) hε) .anil))
    (.inl (exh_ctors hall _))

theorem typ_attempt {Ψ Θ Γ e a r ε φ} (he : Typ S Ψ Θ Γ e (.ex a (.rt r ε)))
    (hr : (Stage.rt r Row.none).le φ) : Typ S Ψ Θ Γ (lzAttempt e) (.ex (resT (.errors ε) a) φ) :=
  .tryR (H := ε) (ε' := Row.none) (h.typ_ok he)
    (.hcons (H' := Row.none) HPat.var (h.typ_err (Typ.var_mono rfl)) (.hnil fun _ h => h)
      fun n hn => .inl (by cases n <;> simp [Catches, hn]))
    hr fun _ hn => .inl hn

end LzSig

/-- `let lazy x = e in b` is a `let mut` of the state, `None`. -/
theorem MutOk.lzDecl (h : LzSig S) {Ψ Θ Γ ms b a ε t φ}
    (hb : MutOk S Ψ Θ (.mono (lzT a ε) :: Γ) (0 :: ms.map (· + 1)) b t φ) :
    MutOk S Ψ Θ Γ ms (lzDecl b) t φ :=
  .var h.typ_none hb

/-- `let y = await x in k`: the kept outcome, or the outcome `o` gives, kept. The outcome
given back raises its errors. -/
theorem MutOk.lzAwaitOf (h : LzSig S) {Ψ Θ Γ ms x o k a ε t φ} (hx : x ∈ ms)
    (hΓ : Γ[x]? = some (.mono (lzT a ε))) (ho : Typ S Ψ Θ Γ o (.ex (resT (.errors ε) a) φ))
    (hε : (Stage.rt Row.none ε).le φ)
    (hk : MutOk S Ψ Θ (.mono a :: Γ) (ms.map (· + 1)) k t φ) :
    MutOk S Ψ Θ Γ ms (Flick.lzAwaitOf x o k) t φ := by
  have hp : PatTy S true (pvars "Some" 1) (lzT a ε) [resT (.errors ε) a] :=
    pat_one h.ctorSome h.resSome rfl (by simp [Ty.substT, instT])
  refine .br (Typ.var_mono hΓ) hp (.ret (h.typ_out (Typ.var_mono rfl) hε))
    (.let_ ho (.set (List.mem_map_of_mem hx) (by simpa using hΓ)
      (h.typ_some (Typ.var_mono rfl)) (.ret (h.typ_out (Typ.var_mono rfl) hε)))) hk

/-- `let y = await x in k` with the initializer `e` run in place: `e` runs at the stage of
its `Lazy`. -/
theorem MutOk.lzAwait (h : LzSig S) {Ψ Θ Γ ms x e k a r ε t φ} (hx : x ∈ ms)
    (hΓ : Γ[x]? = some (.mono (lzT a ε))) (he : Typ S Ψ Θ Γ e (.ex a (.rt r ε)))
    (hr : (Stage.rt r Row.none).le φ) (hε : (Stage.rt Row.none ε).le φ)
    (hk : MutOk S Ψ Θ (.mono a :: Γ) (ms.map (· + 1)) k t φ) :
    MutOk S Ψ Θ Γ ms (Flick.lzAwait x e k) t φ :=
  .lzAwaitOf h hx hΓ (h.typ_attempt he hr) hε hk

/-- `let lazy x = e in b` with the initializer in the function `fn` of the declaration, which
gives its outcome. -/
theorem MutOk.lzDeclFn (h : LzSig S) {Ψ Θ Γ ms fn b a ε ψ C t φ}
    (hf : Typ S Ψ Θ Γ fn (.ex (.arrow (.base .unit) ψ (resT (.errors ε) a) C) φ))
    (hb : MutOk S Ψ Θ
      (.mono (lzT a ε) :: .mono (.arrow (.base .unit) ψ (resT (.errors ε) a) C) :: Γ)
      (0 :: (ms.map (· + 1)).map (· + 1)) b t φ) :
    MutOk S Ψ Θ Γ ms (Flick.lzDeclFn fn b) t φ :=
  .let_ hf (.lzDecl h hb)

/-- The call of a function of `Unit` runs it at the stage of its arrow. -/
theorem Typ.lzCall {Ψ Θ Γ f ψ u C φ} {S : Sig}
    (hf : Γ[f]? = some (.mono (.arrow (.base .unit) ψ u C)))
    (hψ : (Stage.use ψ C).le φ) : Typ S Ψ Θ Γ (Flick.lzCall f) (.ex u φ) :=
  .app (Typ.var_mono hf) .lit hψ LeLive.refl

/-- `let y = await x in k` of a `let lazy` whose initializer is in the function `f`. -/
theorem MutOk.lzAwaitFn (h : LzSig S) {Ψ Θ Γ ms x f k a ψ C ε t φ} (hx : x ∈ ms)
    (hΓ : Γ[x]? = some (.mono (lzT a ε)))
    (hf : Γ[f]? = some (.mono (.arrow (.base .unit) ψ (resT (.errors ε) a) C)))
    (hψ : (Stage.use ψ C).le φ) (hε : (Stage.rt Row.none ε).le φ)
    (hk : MutOk S Ψ Θ (.mono a :: Γ) (ms.map (· + 1)) k t φ) :
    MutOk S Ψ Θ Γ ms (Flick.lzAwaitOf x (Flick.lzCall f) k) t φ :=
  .lzAwaitOf h hx hΓ (.lzCall hf hψ) hε hk

/-- `x = v` of a `let lazy mut` sets the state to `Some (Ok v)`. -/
theorem MutOk.lzWrite (h : LzSig S) {Ψ Θ Γ ms x v k a ε t φ} (hx : x ∈ ms)
    (hΓ : Γ[x]? = some (.mono (lzT a ε))) (hv : Typ S Ψ Θ Γ v (.ex a φ))
    (hk : MutOk S Ψ Θ Γ ms k t φ) : MutOk S Ψ Θ Γ ms (lzWrite x v k) t φ :=
  .set hx hΓ (h.typ_some (h.typ_ok hv)) hk

/-! ## The initializer reads the values of the declaration -/

/-- `t`, or `t` as one side of the disjunctions of `Tm.Free`. -/
macro "free_at " t:term : tactic =>
  `(tactic| first
    | exact $t | exact .inl $t | exact .inr $t | exact .inr (.inl $t) | exact .inr (.inr $t)
    | exact .inr (.inr (.inl $t)) | exact .inr (.inr (.inr $t))
    | exact .inr (.inr (.inr (.inl $t))) | exact .inr (.inr (.inr (.inr $t))))

theorem liftR_congr {n : Nat} {ξ ζ : Nat → Nat} {P : Nat → Prop}
    (h : ∀ i, P (i + n) → ξ i = ζ i) (i : Nat) (hi : P i) : liftR n ξ i = liftR n ζ i := by
  unfold liftR
  split
  · rfl
  · rw [h (i - n) (by rwa [Nat.sub_add_cancel (by omega)])]

/-- A renaming acts on a term through its free variables. -/
theorem Tm.rename_congr : ∀ (e : Tm) {ξ ζ : Nat → Nat}, (∀ i, e.Free i → ξ i = ζ i) →
    e.rename ξ = e.rename ζ := by
  intro e
  induction e <;> intro ξ ζ h <;> simp only [Tm.rename] <;> (try congr 1) <;>
    first
    | rfl
    | exact h _ rfl
    | (apply_assumption
       intro i hi
       first
       | exact h i (by free_at hi)
       | exact liftR_congr (fun j hj => h j (by free_at hj)) i hi)

/-- After `set x`, `tr` renames what follows by `redirect x`: on a term that does not read
`x`, the shift past the new binding. -/
theorem Tm.rename_redirect {e : Tm} {x : Nat} (hx : ¬ e.Free x) :
    e.rename (redirect x) = e.rename (· + 1) :=
  Tm.rename_congr e fun i hi => by
    unfold redirect
    split
    · subst i; exact (hx hi).elim
    · rfl

end Flick
