import Formal.Elab.Witness.Seq
import Formal.Weaken
import Formal.Closed

/-!
# Witness: `unc n`, `from_result` and the structural `eq`

`unc n` is built by a Lean function of `n` (`uncB`): `unc 1 f t = f t`, and
`unc (n+1) f t = match t with (t', x) → unc n f t' x`, `unc n` taken at the result
`aₙ₊₁ →^c r`. Its typing is an induction on `n` (`typ_uncB`). `from_result` matches
`Ok x → x | Err e → raise e`; `⟦R⟧` lets it raise `Errors e`: its name is in `allNames`, and what
it reaches, the atom `e`, in `errRow` (`docs/formal/elaboration.md#from_result-could-not-raise-in-the-core`).
The structural `eq` of `Rec = MkRec Int` compares the one field by the `eq` of the slot
dictionary `D_{Eq Int}` (`docs/formal/elaboration.md#equivalence-did-not-relate-structural-slots-to-superclass-dictionaries`), so its `&&` over the fields is that one comparison.
-/

namespace Flick.Elab.Witness

open Flick.Elab

variable {Ψ : List Flick.Ty} {Θ : Facts} {Γ : List Flick.Scheme}

/-- A term typed in the empty context is typed in any context. -/
theorem Typ.anyCtx {e : Flick.Tm} {J} (h : Typ Sg Ψ Θ [] e J) : Typ Sg Ψ Θ Γ e J := by
  have hc : e.Closed := fun i hi => by have := h.frees i hi; simp at this
  have := h.rename (Γ' := Γ) (ξ := id) (fun i t hi => by simp at hi)
  rwa [Flick.Tm.Closed.rename hc] at this

/-- `a₁ →^C … →^C aₙ →^C r` of the core. -/
def curryC (C : Caps) (as : List Flick.Ty) (r : Flick.Ty) : Flick.Ty :=
  as.foldr (fun a r => .arrow a .pure r C) r

/-- `((t, a₁), …, aₙ)` of the core. -/
def nestC (t : Flick.Ty) (as : List Flick.Ty) : Flick.Ty := as.foldl (fun t a => tc "Tup" [t, a]) t

theorem curryC_snoc (C : Caps) (as : List Flick.Ty) (a r : Flick.Ty) :
    curryC C (as ++ [a]) r = curryC C as (.arrow a .pure r C) := by
  simp [curryC, List.foldr_append]

theorem nestC_snoc (t : Flick.Ty) (as : List Flick.Ty) (a : Flick.Ty) :
    nestC t (as ++ [a]) = tc "Tup" [nestC t as, a] := by
  simp [nestC, List.foldl_append]

theorem curryC_reach {C : Caps} {r : Flick.Ty} {x : Atom} :
    ∀ {as : List Flick.Ty}, (curryC C as r).reach x → C x ∨ (∃ a ∈ as, a.reach x) ∨ r.reach x
  | [], h => .inr (.inr h)
  | a :: as, h => by
    rcases h with h | h | h
    · exact .inr (.inl ⟨a, by simp, h⟩)
    · rcases curryC_reach h with h | ⟨b, hb, h⟩ | h
      · exact .inl h
      · exact .inr (.inl ⟨b, by simp [hb], h⟩)
      · exact .inr (.inr h)
    · exact .inl h

theorem nestC_reach {x : Atom} : ∀ {as : List Flick.Ty} {t a : Flick.Ty}, a ∈ t :: as → a.reach x →
    (nestC t as).reach x
  | [], t, a, ha, h => by simp at ha; subst ha; exact h
  | b :: as, t, a, ha, h => by
    simp only [List.mem_cons] at ha
    show (nestC (tc "Tup" [t, b]) as).reach x
    rcases ha with rfl | rfl | ha
    · exact nestC_reach (List.mem_cons_self ..) (.inl (.inr h))
    · exact nestC_reach (List.mem_cons_self ..) (.inr h)
    · exact nestC_reach (List.mem_cons_of_mem _ ha) h

theorem hone_tup : ∀ K k ts, Sg.ctor K = some (k, ts, "Tup") → K = "Tup" := by
  intro K k ts h; simp only [Sg] at h; unfold ctorF at h; split at h <;> simp_all

theorem hone_rec : ∀ K k ts, Sg.ctor K = some (k, ts, "Rec") → K = "MkRec" := by
  intro K k ts h; simp only [Sg] at h; unfold ctorF at h; split at h <;> simp_all

/-- The body of `unc (m+1)`. -/
def uncB : Nat → Flick.Tm
  | 0 => .lam (.lam (.app (.var 1) (.var 0)))
  | m + 1 => .lam (.lam (.match_ (.var 0) (.acons (pvars "Tup" 2)
      (.app (.app (.app (uncB m) (.var 3)) (.var 0)) (.var 1)) .anil)))

@[simp] theorem uncB_has (m : Nat) (s : Site) : ¬ (uncB m).Has s := by
  induction m <;> simp_all [uncB, Flick.Tm.Has]

/-- **`unc (m+1)`** is typed at `(a₀ →^C … → aₘ →^C r) →P (((a₀, a₁), …, aₘ) →^C r)`. -/
theorem typ_uncB : ∀ m (As : List Flick.Ty), As.length = m → ∀ {A₀ R : Flick.Ty} {C C0 : Caps} {φ},
    Typ Sg Ψ Θ [] (uncB m) (.ex (.arrow (curryC C (A₀ :: As) R) .pure
      (.arrow (nestC A₀ As) .pure R C) C0) φ)
  | 0, [], _, A₀, R, C, C0, φ =>
    typ_lam (typ_lam (.app (ψ := .pure) (typ_var_mono rfl) (typ_var_mono rfl) trivial .refl)
      (fun s h => by simp [Flick.Tm.Has] at h) (mem1 fun x h => (Flick.Scheme.reach_mono _ _).1 h))
      (fun s h => by simp [Flick.Tm.Has] at h) (by simp)
  | m + 1, As, hl, A₀, R, C, C0, φ => by
    obtain ⟨As', A, rfl⟩ : ∃ As' A, As = As' ++ [A] :=
      ⟨As.dropLast, As.getLast (by rintro rfl; simp at hl), (List.dropLast_concat_getLast _).symm⟩
    have hl' : As'.length = m := by simp at hl; omega
    have hc := curryC_snoc C (A₀ :: As') A R
    simp only [List.cons_append] at hc
    rw [hc, nestC_snoc]
    refine typ_lam (typ_lam (.match_ (typ_var_mono rfl)
      (.acons (.adt (.app (.app (.head (K := "Tup") (ts := [.tvar 0, .tvar 1]) (us := [nestC A₀ As', A]) rfl Sig.inj_plain rfl) .var) .var) (tcon_hd _ _))
        (.app (ψ := .pure) (.app (ψ := .pure) (.app (ψ := .pure)
          (Typ.anyCtx (typ_uncB m As' hl' (R := .arrow A .pure R C) (C := C) (C0 := Caps.none)))
          (typ_var_mono rfl) trivial .refl) (typ_var_mono rfl) trivial .refl)
          (typ_var_mono rfl) trivial .refl) .anil)
      (.inl (exh_one (rfl : Sg.ctor "Tup" = _) hone_tup _)))
      (fun s h => by simp [Flick.Tm.Has] at h) ?_) (fun s h => by simp [Flick.Tm.Has] at h) (by simp)
    refine mem1 fun x h => ?_
    rcases curryC_reach ((Flick.Scheme.reach_mono _ _).1 h) with h | ⟨a, ha, h⟩ | (h | h | h)
    · exact .inr (.inr h)
    · exact .inl (.inl (.inr (nestC_reach ha h)))
    · exact .inl (.inr h)
    · exact .inr (.inl h)
    · exact .inr (.inr h)

theorem trT_curry (q : Ty) (as : List Ty) (r : Ty) :
    trT N (curry q as r) = curryC (trT N q).reach (as.map (trT N)) (trT N r) := by
  induction as <;> simp_all [curry, curryC, trT]

theorem trT_nest (t : Ty) (as : List Ty) :
    trT N (nest 10 t as) = nestC (trT N t) (as.map (trT N)) := by
  induction as generalizing t with
  | nil => rfl
  | cons a as ih => simp only [nest, ih]; rfl

/-- `unc (m+1)` is typed at the translation of its scheme. -/
theorem typ_unc_tr (m : Nat) {φ} : Typ Sg Ψ Θ [] (uncB m) (.ex (trT N (uncSch 10 (m + 1)).ty) φ) := by
  have hv : vars (m + 1) = .var 0 :: (List.range m).map fun i => .var (i + 1) := by
    simp [vars, List.range_succ_eq_map]
  simp only [uncSch, arrP, trT, trT_curry, trT_nest, hv, List.map_cons, List.tail_cons]
  exact typ_uncB m _ (by simp)

/-- `from_result r = match r with Ok x → x | Err e → raise e`. -/
def frBody : Flick.Tm := .lam (.match_ (.var 0)
  (.acons (pvars "Ok" 1) (.var 0) (.acons (pvars "Err" 1) (.raise (.var 0)) .anil)))

theorem hall_res : ∀ K k ts, Sg.ctor K = some (k, ts, "Result") →
    pvars K ts.length ∈ [pvars "Ok" 1, pvars "Err" 1] := by
  intro K k ts h; simp only [Sg] at h; unfold ctorF at h
  split at h <;> simp at h <;> (obtain ⟨rfl, rfl⟩ := h; simp)

theorem typ_frBody {E A : Flick.Ty} {C0 : Caps} {e : Ty} {φ}
    (hE : ∀ x, E.reach x → errRow (trT N e) (.atm x)) :
    Typ Sg Ψ Θ [] frBody (.ex (.arrow (tc "Result" [tc "Errors" [E], A]) (trStage N (.R .cnil e)) A
      C0) φ) :=
  .lam (.match_ (typ_var_mono rfl)
      (.acons (.adt (.app (.head (K := "Ok") (ts := [.tvar 1]) (us := [tc "Errors" [E], A]) rfl Sig.inj_plain rfl)
          .var) (tcon_hd _ _))
        (typ_var_mono rfl)
        (.acons (.adt (.app (.head (K := "Err") (ts := [.tvar 0]) (us := [tc "Errors" [E], A]) rfl Sig.inj_plain rfl)
          .var) (tcon_hd _ _))
          (.raiseAdt (typ_var_mono rfl) (tcon_hd _ _) (by
            refine ⟨fun _ h => h.elim, fun x hx => ?_⟩
            cases x with
            | atm a =>
              simp [Row.err, Row.union, Row.single, Row.ofCaps, Flick.Ty.substT, instT] at hx
              rcases hx with h | h
              · exact h.elim
              · exact .inl (hE _ h)
            | _ => simp [Row.err, Row.union, Row.single, Row.ofCaps, allNames] at hx ⊢)) .anil))
      (.inl (exh_ctors hall_res _)))
    (fun h => by simp [Flick.Tm.Has] at h)
    (held_sub (fun s h => by simp [Flick.Tm.Has] at h) (by simp)) .refl

/-- The method `eq` of the structural `D_{Eq Rec}`, under its argument `d : D_{Eq Int}`:
`λa b. match a with MkRec x → match b with MkRec y → d.eq x y`. -/
def eqRec : Flick.Tm := .lam (.lam (.match_ (.var 1) (.acons (pvars "MkRec" 1)
  (.match_ (.var 1) (.acons (pvars "MkRec" 1)
    (.app (.app (.proj (.var 4) "eq") (.var 1)) (.var 0)) .anil)) .anil)))

end Flick.Elab.Witness
