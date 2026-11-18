import Formal.LetMut
import Formal.Elab.Exh

/-!
# `let lazy` as a variable of its function (lazy.md §4)

`let lazy x = e` is the task `run = task (attempt e)` (`lzAttempt`: `Ok v` or `Err err`), made once
at the declaration, and a variable `x` whose state is `None` until an `await x` runs `run` and
keeps the outcome `o` as `Some o`; `await x` gives the value of the outcome or raises its error
again (`lzOut`, the body of `from_result`). A force after the first only reads the state. A
write `x = v` of a `let lazy mut` makes the state `Some (Ok v)`. These are blocks of `LetMut`:

- `lzDecl run b`: a `let` of `run` and a `var` of the state, `None`;
- `lzAwait x f k`, for `let y = await x in k` with the task at `f`: a two-armed `br` on the
  state, whose first arm gives the kept outcome and whose other arm runs `await f`, `set`s the
  state and gives the new outcome;
- `lzWrite x v k`: `set x (Some (Ok v)) k`.

Each is `MutOk` when its parts are typed (`MutOk.lzDecl`, `MutOk.lzAwait`, `MutOk.lzWrite`, and
`LzSig.typ_run` for the task): they are derived rules of `MutOk`, so **T7** holds of every block
written with them. Its translation is a core term of the block's type at the block's stage, and
`T7_pure` and `T7_regions` hold of it too.

The initializer is written once, in the task. A signal or a defect leaves the `await` before the
`set`, so the state stays `None`, as the reset of lazy.md §3 leaves a cell pending. It reads the
values of the declaration: the task is a value made there, bound by a `let`, which no `set`
changes.

The constructors are the Prelude's `Option` and `Result`, with the fields of their declarations
(`LzSig`).
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
def lzT (a ε : Ty) : Ty := optT (resT (.errors ε) a)

/-- The task of the initializer: of the rows `r`, `ε₀` and the captures `C`, it gives a
`Result (Errors ε) a`. -/
def lzRunT (r ε₀ : Ty) (C : Caps) (a ε : Ty) : Ty := .eff r ε₀ (resT (.errors ε) a) C

def lzNone : Tm := .ctor "None"

def lzSome (o : Tm) : Tm := .app (.ctor "Some") o

def lzOk (v : Tm) : Tm := .app (.ctor "Ok") v

def lzErr (v : Tm) : Tm := .app (.ctor "Err") v

/-- `attempt e`: the outcome of `e`, its value as `Ok v` or its error as `Err err`. -/
def lzAttempt (e : Tm) : Tm := .try_ (lzOk e) (.acons .var (lzErr (.var 0)) .anil)

/-- `out o`: the value of the outcome `o`, or its error raised again. -/
def lzOut (o : Tm) : Tm :=
  .match_ o (.acons (pvars "Ok" 1) (.var 0) (.acons (pvars "Err" 1) (.raise (.var 0)) .anil))

/-- `let lazy x = e in b`: the task `run = task (lzAttempt e)` and the variable `x`, `None`. -/
def lzDecl (run : Tm) (b : Blk) : Blk := .let_ run (.var lzNone b)

/-- `let y = await x in k` of the `let lazy` `x` whose task is at `f`: the kept outcome, or the
outcome of `await f`, kept. -/
def lzAwait (x f : Nat) (k : Blk) : Blk :=
  .br (.var x) (pvars "Some" 1) (.ret (lzOut (.var 0)))
    (.let_ (.await (.var f)) (.set (x + 1) (lzSome (.var 0)) (.ret (lzOut (.var 0))))) k

/-- `x = v` of a `let lazy mut`. -/
def lzWrite (x : Nat) (v : Tm) (k : Blk) : Blk := .set x (lzSome (lzOk v)) k

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
    (hε : (Stage.rt Row.none ε.asRow).le φ) : Typ S Ψ Θ Γ (lzOut o) (.ex a φ) := by
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

theorem typ_attempt {Ψ Θ Γ e a r ε φ} (he : Typ S Ψ Θ Γ e (.ex a (.rt r ε.asRow)))
    (hr : (Stage.rt r Row.none).le φ) : Typ S Ψ Θ Γ (lzAttempt e) (.ex (resT (.errors ε) a) φ) :=
  .tryR (H := ε.asRow) (ε' := Row.none) (h.typ_ok he)
    (.hcons (H' := Row.none) HPat.var (h.typ_err (Typ.var_mono rfl)) (.hnil fun _ h => h)
      fun n hn => .inl (by cases n <;> simp [Catches, hn]))
    hr fun x hn => .inl (by cases x <;> first | exact ⟨_, hn⟩ | exact hn)

end LzSig

/-- The task `task (lzAttempt e)`: `e` runs at the stage of the task. -/
theorem LzSig.typ_run (h : LzSig S) {Ψ Θ Γ e a r' r ε₀ ε C φ}
    (he : Typ S Ψ Θ Γ e (.ex a (.rt r' ε.asRow)))
    (hr : (Stage.rt r' Row.none).le (Stage.use (.rt r.asRow ε₀.asRow) C)) (hn : e.NoIns) :
    Typ S Ψ Θ Γ (.task (lzAttempt e)) (.ex (lzRunT r ε₀ C a ε) φ) :=
  .task (h.typ_attempt he hr) (Stage.le_refl _)
    (fun hi => by simp only [lzAttempt, lzOk, lzErr, Tm.Has, false_or, or_false] at hi; exact hn hi)
    LeLive.refl

/-- `let lazy x = e in b`: the task, then the state `None`, a variable from here on. -/
theorem MutOk.lzDecl (h : LzSig S) {Ψ Θ Γ ms run b r ε₀ C a ε t φ}
    (hrun : Typ S Ψ Θ Γ run (.ex (lzRunT r ε₀ C a ε) φ))
    (hb : MutOk S Ψ Θ (.mono (lzT a ε) :: .mono (lzRunT r ε₀ C a ε) :: Γ)
      (0 :: (ms.map (· + 1)).map (· + 1)) b t φ) :
    MutOk S Ψ Θ Γ ms (Flick.lzDecl run b) t φ :=
  .let_ hrun (.var h.typ_none hb)

/-- `let y = await x in k`: the kept outcome, or the one of the task, kept. The task runs at its
stage, and the outcome given back raises its errors. -/
theorem MutOk.lzAwait (h : LzSig S) {Ψ Θ Γ ms x f k r ε₀ C a ε t φ} (hx : x ∈ ms)
    (hΓ : Γ[x]? = some (.mono (lzT a ε))) (hf : Γ[f]? = some (.mono (lzRunT r ε₀ C a ε)))
    (hs : (Stage.use (.rt r.asRow ε₀.asRow) C).le φ) (hε : (Stage.rt Row.none ε.asRow).le φ)
    (hk : MutOk S Ψ Θ (.mono a :: Γ) (ms.map (· + 1)) k t φ) :
    MutOk S Ψ Θ Γ ms (Flick.lzAwait x f k) t φ := by
  have hp : PatTy S true (pvars "Some" 1) (lzT a ε) [resT (.errors ε) a] :=
    pat_one h.ctorSome h.resSome rfl (by simp [Ty.substT, instT])
  refine .br (Typ.var_mono hΓ) hp (.ret (h.typ_out (Typ.var_mono rfl) hε))
    (.let_ (.await (Typ.var_mono hf) hs LeLive.refl) (.set (List.mem_map_of_mem hx)
      (by simpa using hΓ) (h.typ_some (Typ.var_mono rfl)) (.ret (h.typ_out (Typ.var_mono rfl) hε))))
    hk

/-- `x = v` of a `let lazy mut` sets the state to `Some (Ok v)`. -/
theorem MutOk.lzWrite (h : LzSig S) {Ψ Θ Γ ms x v k a ε t φ} (hx : x ∈ ms)
    (hΓ : Γ[x]? = some (.mono (lzT a ε))) (hv : Typ S Ψ Θ Γ v (.ex a φ))
    (hk : MutOk S Ψ Θ Γ ms k t φ) : MutOk S Ψ Θ Γ ms (Flick.lzWrite x v k) t φ :=
  .set hx hΓ (h.typ_some (h.typ_ok hv)) hk

end Flick
