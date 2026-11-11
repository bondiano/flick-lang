import Formal.Elab.CoreTy
import Formal.Elab.CoreLaws
import Formal.Elab.Exh

/-!
# Prelude schemes with `CRow` qualifiers: the lazy `Monad Eff` is typed in the core

`Eff` of the model is a nominal type of the core, `Eff q b`, whose constructor `MkEff` has
the field `eff ∅ ∅ b {q}`: the hidden qualifier of the computation is the parameter `q` of
kind `CRow` (simplification 16). `⌈Eff q b⌉ = ⌈q⌉ ∪ ⌈b⌉` through `tapp`. A lazy body is
`MkEff (task …)`, typed by (Task) of the core: the task runs where the result's qualifier
is live, so it awaits only tasks whose qualifiers that one covers. `pure`, `map2` and `flat_map` of `Monad Eff` are typed at the translations of their
schemes in `Sugar` (`bindTy`, `map2Ty`), the class variable instantiated by `Eff`.
-/

namespace Flick.Elab

variable {S : Sig} {mk E : Name}

/-- `Eff q b` of the core. -/
def effT (E : Name) (q b : Flick.Ty) : Flick.Ty := .tapp (.tapp (.adt E) q) b

/-- The field of `MkEff : eff ∅ ∅ b {q} → Eff q b`, over `q = tvar 0`, `b = tvar 1`. -/
def effField : Flick.Ty := .eff Row.none Row.none (.tvar 1) fun a => a = .v 0

/-- `match e with MkEff t → b`, exhaustive as `MkEff` is the only constructor (`exh_one`). -/
def unEff (mk : Name) (e b : Flick.Tm) : Flick.Tm :=
  .match_ e (.acons (pvars mk 1) b .anil)

/-- `MkEff (task e)`. -/
def mkEff (mk : Name) (e : Flick.Tm) : Flick.Tm := .app (.ctor mk) (.task e)

/-- `pure x = MkEff (task x)`. -/
def effPure (mk : Name) : Flick.Tm := .lam (mkEff mk (.var 0))

/-- `flat_map k x = MkEff (task (k (await x)).await)`, unwrapping both. -/
def effBind (mk : Name) : Flick.Tm :=
  .lam (.lam (mkEff mk (unEff mk (.var 0)
    (unEff mk (.app (.var 2) (.await (.var 0))) (.await (.var 0))))))

/-- `map2 g x y = MkEff (task (g (await x) (await y)))`, unwrapping both. -/
def effMap2 (mk : Name) : Flick.Tm :=
  .lam (.lam (.lam (mkEff mk (unEff mk (.var 1) (unEff mk (.var 1)
    (.app (.app (.var 4) (.await (.var 1))) (.await (.var 0))))))))

theorem effField_inst (q b : Flick.Ty) :
    effField.substT (instT [q, b]) = .eff Row.none Row.none b q.reach := by
  simp only [effField, Flick.Ty.substT, Row.none_substT]
  congr 1
  funext a; apply propext
  simp [Caps.substT, instT]

theorem ctorTy_eff (q b : Flick.Ty) :
    Sig.ctorTy [effField] E [q, b] = .arrow (.eff Row.none Row.none b q.reach) .pure (effT E q b) Caps.none := by
  simp only [Sig.ctorTy, List.map, effField_inst]; rfl

theorem typ_var_mono {Ψ Θ Γ i t φ} (h : Γ[i]? = some (Flick.Scheme.mono t)) :
    Typ S Ψ Θ Γ (.var i) (.ex t φ) :=
  .var (ts := []) h rfl (fun _ _ h => h.elim)
    (by rw [Flick.Scheme.inst]; simp only [Flick.Scheme.mono]; rw [substT_instT_nil]; exact .refl)

/-- The stage of a lazy body whose `Eff` has the qualifier `Q`: it may use `Q`. -/
abbrev useStage (Q : Caps) : Flick.Stage := Stage.use (.rt Row.none Row.none) Q

/-- Awaiting a task of the qualifier `q` needs `q` live in the stage. -/
theorem typ_await_var {Ψ Θ Γ i a q} {Q : Caps} (h : Γ[i]? = some (.mono (.eff Row.none Row.none a q)))
    (hq : ∀ x, q x → Q x) :
    Typ S Ψ Θ Γ (.await (.var i)) (.ex a (useStage Q)) :=
  .await (typ_var_mono h) ⟨fun x hx => hx.elim (fun h => h.elim) fun h => by
    cases x with
    | atm b => exact .inr (hq b h)
    | _ => exact h.elim, fun _ h => h⟩ .refl

theorem noHas_mkEff {e : Flick.Tm} (h : ∀ s, ¬ e.Has s) : ∀ s, ¬ (mkEff mk e).Has s := by
  intro s hs; simp only [mkEff, Flick.Tm.Has] at hs; rcases hs with hs | hs
  · exact hs
  · exact h s hs

variable (hmk : S.ctor mk = some (2, [effField], E)) (hmkr : S.res mk = idTs 2)
  (hone : ∀ K k ts, S.ctor K = some (k, ts, E) → K = mk)
include hmk hmkr hone

omit hone in
/-- A lazy body of the qualifier `q` runs where `q` is live; what it holds is not asked. -/
theorem typ_mkEff {Ψ Θ Γ e q b ψ φ} (he : Typ S Ψ Θ Γ e (.ex b ψ))
    (hψ : ψ.le (useStage q.reach)) (hn : ∀ s, ¬ e.Has s) :
    Typ S Ψ Θ Γ (mkEff mk e) (.ex (effT E q b) φ) :=
  .app (ψ := .pure) (Typ.ctor_plain (S := S) hmk hmkr (us := [q, b]) rfl (by rw [ctorTy_eff]; exact .refl))
    (.task he hψ (hn .ins) .refl) trivial .refl

theorem typ_unEff {Ψ Θ Γ e b q a t φ} (he : Typ S Ψ Θ Γ e (.ex (effT E q a) φ))
    (hb : Typ S Ψ Θ (.mono (.eff Row.none Row.none a q.reach) :: Γ) b (.ex t φ)) :
    Typ S Ψ Θ Γ (unEff mk e b) (.ex t φ) := by
  have hh := PatTy.head_plain (S := S) hmk hmkr (us := [q, a]) rfl
  rw [ctorTy_eff] at hh
  exact .match_ he (.acons (.adt (.app hh .var) rfl) hb .anil) (.inl (exh_one hmk hone [q, a]))

omit hone in
theorem typ_effPure {Ψ Θ A Q C0 φ} :
    Typ S Ψ Θ [] (effPure mk) (.ex (.arrow A .pure (effT E Q A) C0) φ) :=
  .lam (typ_mkEff hmk hmkr (ψ := .pure) (typ_var_mono rfl) trivial (fun _ h => h))
    (fun h => by simp [mkEff, Flick.Tm.Has] at h) .refl

/-- `flat_map` of the lazy `Eff`: its body awaits `x` and the result of `k`, so the qualifier
`Q3` of its result covers theirs, `D` and `Q1`. -/
theorem typ_effBind {Ψ Θ A B C C0 C2 D Q1 Q3 φ}
    (hD : ∀ a, D.reach a → Q3.reach a) (hQ : ∀ a, Q1.reach a → Q3.reach a) :
    Typ S Ψ Θ [] (effBind mk) (.ex (.arrow (.arrow A .pure (effT E Q1 B) C) .pure
      (.arrow (effT E D A) .pure (effT E Q3 B) C2) C0) φ) := by
  have hn : ∀ s, ¬ (unEff mk (.var 0) (unEff mk (.app (.var 2) (.await (.var 0)))
      (.await (.var 0)))).Has s := fun s h => by simp [unEff, Flick.Tm.Has] at h
  exact .lam (.lam (typ_mkEff hmk hmkr (ψ := useStage Q3.reach)
    (typ_unEff hmk hmkr hone (typ_var_mono rfl) (typ_unEff hmk hmkr hone
      (.app (ψ := .pure) (typ_var_mono rfl) (typ_await_var rfl hD) trivial .refl)
      (typ_await_var rfl hQ)))
    (Stage.le_refl _) hn) (fun h => noHas_mkEff hn _ h) .refl)
    (fun h => noHas_mkEff hn _ h) .refl

/-- `map2` of the lazy `Eff`: its body awaits `x` and `y`, so the qualifier `R` of its result
covers theirs, `D` and `D'`. -/
theorem typ_effMap2 {Ψ Θ A Bt X Cg C C0 C1 C2 D D' R φ}
    (hD : ∀ a, D.reach a → R.reach a) (hD' : ∀ a, D'.reach a → R.reach a) :
    Typ S Ψ Θ [] (effMap2 mk) (.ex (.arrow (.arrow A .pure (.arrow Bt .pure X Cg) C) .pure
      (.arrow (effT E D A) .pure (.arrow (effT E D' Bt) .pure (effT E R X) C2) C1) C0) φ) := by
  have hn : ∀ s, ¬ (unEff mk (.var 1) (unEff mk (.var 1)
      (.app (.app (.var 4) (.await (.var 1))) (.await (.var 0))))).Has s :=
    fun s h => by simp [unEff, Flick.Tm.Has] at h
  have hm : ∀ s, ¬ (Flick.Tm.lam (mkEff mk (unEff mk (.var 1) (unEff mk (.var 1)
      (.app (.app (.var 4) (.await (.var 1))) (.await (.var 0))))))).Has s :=
    fun s h => noHas_mkEff hn s h
  exact .lam (.lam (.lam (typ_mkEff hmk hmkr (ψ := useStage R.reach)
    (typ_unEff hmk hmkr hone (typ_var_mono rfl) (typ_unEff hmk hmkr hone (typ_var_mono rfl)
      (.app (ψ := .pure) (.app (ψ := .pure) (typ_var_mono rfl) (typ_await_var rfl hD) trivial .refl)
        (typ_await_var rfl hD') trivial .refl)))
    (Stage.le_refl _) hn) (fun h => noHas_mkEff hn _ h) .refl)
    (fun h => hm _ h) .refl)
    (fun h => hm _ h) .refl

variable (N : Names) {eC : Nat} (hE : N.con eC = E) (h2 : eC ≠ 2) (h4 : eC ≠ 4)
include hE h2 h4

omit hone in
/-- **`pure` of `Monad Eff`** is typed at its scheme in `Sugar`. -/
theorem typ_effPure_tr (c a : Ty) {Ψ Θ φ} :
    Typ S Ψ Θ [] (effPure mk) (.ex (trT N (arrP a (appQ (.con eC) c a))) φ) := by
  simp only [trT, if_neg h2, if_neg h4, hE]; exact typ_effPure hmk hmkr

/-- **`flat_map` of `Monad Eff`** is typed at its scheme in `Sugar` (`bindTy`). -/
theorem typ_effBind_tr (c d a b e : Ty) {Ψ Θ φ} :
    Typ S Ψ Θ [] (effBind mk) (.ex (trT N (bindTy (.con eC) c d a b e)) φ) := by
  simp only [bindTy, arrP, appQ, trT, if_neg h2, if_neg h4, hE]
  refine typ_effBind hmk hmkr hone ?_ ?_ <;> intro x h <;> rcases x with n | p <;>
    simp [Flick.Ty.reach, trT_reach] at h ⊢ <;> grind

/-- **`map2` of `Monad Eff`** is typed at its scheme in `Sugar` (`map2Ty`). -/
theorem typ_effMap2_tr (c d a b x d' : Ty) {Ψ Θ φ} :
    Typ S Ψ Θ [] (effMap2 mk) (.ex (trT N (map2Ty (.con eC) c d a b x d')) φ) := by
  simp only [map2Ty, arrP, appQ, trT, if_neg h2, if_neg h4, hE]
  refine typ_effMap2 hmk hmkr hone ?_ ?_ <;> intro y h <;> rcases y with n | p <;>
    simp [Flick.Ty.reach, trT_reach] at h ⊢ <;> grind

end Flick.Elab
