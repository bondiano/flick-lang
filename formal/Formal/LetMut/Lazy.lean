import Formal.LetMut
import Formal.Elab.Exh

/-!
# `let lazy` as a variable of its function (lazy.md §4)

`let lazy x = e` is a variable `x` whose state is a task that gives the outcome of the
initializer: first `task (attempt e)` (`lzAttempt`, `Ok v` or `Err err`), made at the
declaration. `await x` runs the state, makes it the task `task o` that gives the outcome `o`
back, and gives the value of `o` or raises its error again (`lzOut`, the body of
`from_result`). A write `x = v` of a `let lazy mut` makes the state `task (Ok v)`, `v` evaluated
at the write. These are blocks of `LetMut`:

- `lzDecl run b`: `var run b`;
- `lzAwait x k`, for `let y = await x in k`: a `let` of `await x`, a `set` of the state to
  `task o` and a `let` of `lzOut o`, which `k` reads;
- `lzWrite x v k`: a `let` of `v` and a `set` of the state to `task (Ok v)`.

Each is `MutOk` when its parts are typed (`MutOk.lzDecl`, `MutOk.lzAwait`, `MutOk.lzWrite`, and
`LzSig.typ_run` for the first state): they are derived rules of `MutOk`, so **T7** holds of
every block written with them. Its translation is a core term of the block's type at the
block's stage, and `T7_pure` and `T7_regions` hold of it too.

The initializer is written once. A signal or a defect leaves the `await` before the `set`, so
the state still holds the initializer, as the reset of lazy.md §3 leaves a cell pending. It
reads the values of the declaration: the first state is a value made there, and a `set` of a
variable of the block does not change it.

The constructors are the Prelude's `Result`, with the fields of its declaration (`LzSig`).
-/

namespace Flick

/-- The Prelude's `Result` in the signature of the core: `Err e | Ok a`, ordinary constructors,
and the only ones of `Result`. -/
structure LzSig (S : Sig) : Prop where
  ctorErr : S.ctor "Err" = some (2, [.tvar 0], "Result")
  ctorOk : S.ctor "Ok" = some (2, [.tvar 1], "Result")
  resErr : S.res "Err" = idTs 2
  resOk : S.res "Ok" = idTs 2
  result : ∀ K k ts, S.ctor K = some (k, ts, "Result") → K = "Ok" ∨ K = "Err"

def resT (e a : Ty) : Ty := .tcon "Result" [e, a]

/-- The state of a `let lazy` of `a` with the errors `ε`: a task of the rows `r`, `ε₀` and the
captures `C` that gives a `Result (Errors ε) a`. -/
def lzT (r ε₀ : Ty) (C : Caps) (a ε : Ty) : Ty := .eff r ε₀ (resT (.errors ε) a) C

def lzOk (v : Tm) : Tm := .app (.ctor "Ok") v

def lzErr (v : Tm) : Tm := .app (.ctor "Err") v

/-- `attempt e`: the outcome of `e`, its value as `Ok v` or its error as `Err err`. -/
def lzAttempt (e : Tm) : Tm := .try_ (lzOk e) (.acons .var (lzErr (.var 0)) .anil)

/-- `out o`: the value of the outcome `o`, or its error raised again. -/
def lzOut (o : Tm) : Tm :=
  .match_ o (.acons (pvars "Ok" 1) (.var 0) (.acons (pvars "Err" 1) (.raise (.var 0)) .anil))

/-- `let lazy x = e in b`: the variable `x`, whose first state `run` is
`task (lzAttempt e)`. -/
def lzDecl (run : Tm) (b : Blk) : Blk := .var run b

/-- `let y = await x in k` of the `let lazy` `x`: the outcome `o` of a run of the state, the
state made `task o`, and `y = lzOut o`, under which `k` reads `y` and `o`. -/
def lzAwait (x : Nat) (k : Blk) : Blk :=
  .let_ (.await (.var x)) (.set (x + 1) (.task (.var 0)) (.let_ (lzOut (.var 0)) k))

/-- `x = v` of a `let lazy mut`: `v`, and the state made `task (Ok v)`. -/
def lzWrite (x : Nat) (v : Tm) (k : Blk) : Blk :=
  .let_ v (.set (x + 1) (.task (lzOk (.var 0))) k)

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

/-- The first state `task (lzAttempt e)`: `e` runs at the stage of the task. -/
theorem LzSig.typ_run (h : LzSig S) {Ψ Θ Γ e a r' r ε₀ ε C φ}
    (he : Typ S Ψ Θ Γ e (.ex a (.rt r' ε.asRow)))
    (hr : (Stage.rt r' Row.none).le (Stage.use (.rt r.asRow ε₀.asRow) C)) (hn : e.NoIns) :
    Typ S Ψ Θ Γ (.task (lzAttempt e)) (.ex (lzT r ε₀ C a ε) φ) :=
  .task (h.typ_attempt he hr) (Stage.le_refl _)
    (fun hi => by simp only [lzAttempt, lzOk, lzErr, Tm.Has, false_or, or_false] at hi; exact hn hi)
    LeLive.refl

/-- `let lazy x = e in b`: the state is the variable `x` from here on. -/
theorem MutOk.lzDecl {Ψ Θ Γ ms run b r ε₀ C a ε t φ}
    (hrun : Typ S Ψ Θ Γ run (.ex (lzT r ε₀ C a ε) φ))
    (hb : MutOk S Ψ Θ (.mono (lzT r ε₀ C a ε) :: Γ) (0 :: ms.map (· + 1)) b t φ) :
    MutOk S Ψ Θ Γ ms (Flick.lzDecl run b) t φ :=
  .var hrun hb

/-- `let y = await x in k`: the state runs at its stage, and the outcome given back raises its
errors. -/
theorem MutOk.lzAwait (h : LzSig S) {Ψ Θ Γ ms x k r ε₀ C a ε t φ} (hx : x ∈ ms)
    (hΓ : Γ[x]? = some (.mono (lzT r ε₀ C a ε)))
    (hs : (Stage.use (.rt r.asRow ε₀.asRow) C).le φ) (hε : (Stage.rt Row.none ε.asRow).le φ)
    (hk : MutOk S Ψ Θ (.mono a :: .mono (resT (.errors ε) a) :: Γ)
      ((ms.map (· + 1)).map (· + 1)) k t φ) :
    MutOk S Ψ Θ Γ ms (Flick.lzAwait x k) t φ :=
  .let_ (.await (Typ.var_mono hΓ) hs LeLive.refl)
    (.set (List.mem_map_of_mem hx) (by simpa using hΓ)
      (.task (Typ.var_mono rfl) (Stage.pure_le _) (by simp [Tm.NoIns, Tm.Has]) LeLive.refl)
      (.let_ (h.typ_out (Typ.var_mono rfl) hε) hk))

/-- `x = v` of a `let lazy mut` makes the state `task (Ok v)`. -/
theorem MutOk.lzWrite (h : LzSig S) {Ψ Θ Γ ms x v k r ε₀ C a ε t φ} (hx : x ∈ ms)
    (hΓ : Γ[x]? = some (.mono (lzT r ε₀ C a ε))) (hv : Typ S Ψ Θ Γ v (.ex a φ))
    (hk : MutOk S Ψ Θ (.mono a :: Γ) (ms.map (· + 1)) k t φ) :
    MutOk S Ψ Θ Γ ms (Flick.lzWrite x v k) t φ :=
  .let_ hv (.set (List.mem_map_of_mem hx) (by simpa using hΓ)
    (.task (h.typ_ok (Typ.var_mono rfl)) (Stage.pure_le _) (by simp [Tm.NoIns, Tm.Has, lzOk])
      LeLive.refl) hk)

end Flick
