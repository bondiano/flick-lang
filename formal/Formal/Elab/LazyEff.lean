import Formal.Elab.CoreTy
import Formal.Elab.CoreLaws

/-!
# Р188: the lazy `Monad Eff` is typed in the core

`Eff` of the model is a nominal type of the core, `Eff q b`, whose constructor `MkEff` has
the field `eff ∅ ∅ b {q}`: the hidden qualifier of the computation is the parameter `q` of
kind `CRow` (simplification 15). `⌈Eff q b⌉ = ⌈q⌉ ∪ ⌈b⌉` through `tapp`. A lazy body is
`MkEff (task …)`, typed by (Task) of the core: the task holds what the result's qualifier
shows. `pure`, `map2` and `flat_map` of `Monad Eff` are typed at the translations of their
schemes in `Sugar` (`bindTy`, `map2Ty`), the class variable instantiated by `Eff`.
-/

namespace Flick.Elab

variable {S : Sig} {mk E : Name}

/-- `Eff q b` of the core. -/
def effT (E : Name) (q b : Flick.Ty) : Flick.Ty := .tapp (.tapp (.adt E) q) b

/-- The field of `MkEff : eff ∅ ∅ b {q} → Eff q b`, over `q = tvar 0`, `b = tvar 1`. -/
def effField : Flick.Ty := .eff Row.none Row.none (.tvar 1) fun a => a = .v 0

/-- `match e with MkEff t → b | _ → todo`. -/
def unEff (mk : Name) (e b : Flick.Tm) : Flick.Tm :=
  .match_ e (.acons (.app (.ctor mk) .var) b (.acons .wild .todo .anil))

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

/-- What a term without run-time forms holds is within what its context reaches. -/
theorem held_sub {Ψ Γ e} {C : Caps} (hn : ∀ s, ¬ e.Has s)
    (h : ∀ s ∈ Γ, ∀ a, Flick.Scheme.reach s a → C a) : Caps.Sub (held Ψ Γ e) C := by
  rintro a (⟨i, s, -, hs, hr⟩ | ⟨⟨p, hp, -⟩ | ⟨l, u, hl, -⟩⟩)
  · exact h s (List.mem_of_getElem? hs) a hr
  · exact (hn _ hp).elim
  · exact (hn _ hl).elim

theorem typ_await_var {Ψ Θ Γ i a q} (h : Γ[i]? = some (.mono (.eff Row.none Row.none a q))) :
    Typ S Ψ Θ Γ (.await (.var i)) (.ex a (.rt Row.none Row.none)) :=
  .await (typ_var_mono h) ⟨fun _ h => h, fun _ h => h⟩ .refl

theorem noHas_mkEff {e : Flick.Tm} (h : ∀ s, ¬ e.Has s) : ∀ s, ¬ (mkEff mk e).Has s := by
  intro s hs; simp only [mkEff, Flick.Tm.Has] at hs; rcases hs with hs | hs
  · exact hs
  · exact h s hs

variable (hmk : S.ctor mk = some (2, [effField], E))
include hmk

theorem typ_mkEff {Ψ Θ Γ e q b ψ φ} (he : Typ S Ψ Θ Γ e (.ex b ψ))
    (hψ : ψ.le (.rt Row.none Row.none)) (hn : ∀ s, ¬ e.Has s)
    (hh : ∀ s ∈ Γ, ∀ a, Flick.Scheme.reach s a → b.reach a ∨ q.reach a) :
    Typ S Ψ Θ Γ (mkEff mk e) (.ex (effT E q b) φ) :=
  .app (ψ := .pure) (.ctor hmk (us := [q, b]) rfl (by rw [ctorTy_eff]; exact .refl))
    (.task he hψ (hn .ins) (held_sub (by simpa [Flick.Tm.Has] using hn) hh) .refl) trivial .refl

theorem typ_unEff {Ψ Θ Γ e b q a t φ} (he : Typ S Ψ Θ Γ e (.ex (effT E q a) φ))
    (hb : Typ S Ψ Θ (.mono (.eff Row.none Row.none a q.reach) :: Γ) b (.ex t φ)) :
    Typ S Ψ Θ Γ (unEff mk e b) (.ex t φ) := by
  have hh := PatTy.head (S := S) hmk (us := [q, a]) rfl
  rw [ctorTy_eff] at hh
  exact .match_ he (.acons (.adt (.app hh .var) rfl) hb (.acons .wild .todo .anil))
    (.inl fun _ _ => ⟨.wild, by simp [Flick.Tm.unguarded], rfl⟩)

theorem typ_effPure {Ψ Θ A Q C0 φ} :
    Typ S Ψ Θ [] (effPure mk) (.ex (.arrow A .pure (effT E Q A) C0) φ) :=
  .lam (typ_mkEff hmk (ψ := .pure) (typ_var_mono rfl) trivial (fun _ h => h)
      (by intro s hs a h; simp at hs; subst hs; exact .inl ((Flick.Scheme.reach_mono _ _).1 h)))
    (fun h => by simp [mkEff, Flick.Tm.Has] at h)
    (held_sub (fun _ h => by simp [mkEff, Flick.Tm.Has] at h) (by simp)) .refl

theorem typ_effBind {Ψ Θ A B C C0 C2 D Q1 Q3 φ}
    (hx : ∀ a, (effT E D A).reach a → B.reach a ∨ Q3.reach a)
    (hk : ∀ a, (Flick.Ty.arrow A .pure (effT E Q1 B) C).reach a → B.reach a ∨ Q3.reach a)
    (hk' : ∀ a, (Flick.Ty.arrow A .pure (effT E Q1 B) C).reach a →
      (Flick.Ty.arrow (effT E D A) .pure (effT E Q3 B) C2).reach a) :
    Typ S Ψ Θ [] (effBind mk) (.ex (.arrow (.arrow A .pure (effT E Q1 B) C) .pure
      (.arrow (effT E D A) .pure (effT E Q3 B) C2) C0) φ) := by
  have hn : ∀ s, ¬ (unEff mk (.var 0) (unEff mk (.app (.var 2) (.await (.var 0)))
      (.await (.var 0)))).Has s := fun s h => by simp [unEff, Flick.Tm.Has] at h
  refine .lam (.lam (typ_mkEff hmk (ψ := .rt Row.none Row.none)
    (typ_unEff hmk (typ_var_mono rfl) (typ_unEff hmk
      (.app (ψ := .pure) (typ_var_mono rfl) (typ_await_var rfl) trivial .refl) (typ_await_var rfl)))
    ⟨fun _ h => h, fun _ h => h⟩ hn ?_) (fun h => noHas_mkEff hn _ h)
    (held_sub (fun s h => noHas_mkEff hn s h) ?_) .refl)
    (fun h => noHas_mkEff hn _ h) (held_sub (fun s h => noHas_mkEff hn s h) (by simp)) .refl
  · intro s hs a h
    simp only [List.mem_cons, List.not_mem_nil, or_false] at hs
    rcases hs with rfl | rfl
    · exact hx a ((Flick.Scheme.reach_mono _ _).1 h)
    · exact hk a ((Flick.Scheme.reach_mono _ _).1 h)
  · intro s hs a h
    simp only [List.mem_cons, List.not_mem_nil, or_false] at hs
    subst hs; exact hk' a ((Flick.Scheme.reach_mono _ _).1 h)

theorem typ_effMap2 {Ψ Θ A Bt X Cg C C0 C1 C2 D D' R φ}
    (hg : ∀ a, (Flick.Ty.arrow A .pure (.arrow Bt .pure X Cg) C).reach a → X.reach a ∨ R.reach a)
    (hx : ∀ a, (effT E D A).reach a → X.reach a ∨ R.reach a)
    (hy : ∀ a, (effT E D' Bt).reach a → X.reach a ∨ R.reach a)
    (hg₂ : ∀ a, (Flick.Ty.arrow A .pure (.arrow Bt .pure X Cg) C).reach a →
      (Flick.Ty.arrow (effT E D' Bt) .pure (effT E R X) C2).reach a)
    (hx₂ : ∀ a, (effT E D A).reach a → (Flick.Ty.arrow (effT E D' Bt) .pure (effT E R X) C2).reach a)
    (hg₁ : ∀ a, (Flick.Ty.arrow A .pure (.arrow Bt .pure X Cg) C).reach a →
      (Flick.Ty.arrow (effT E D A) .pure (.arrow (effT E D' Bt) .pure (effT E R X) C2) C1).reach a) :
    Typ S Ψ Θ [] (effMap2 mk) (.ex (.arrow (.arrow A .pure (.arrow Bt .pure X Cg) C) .pure
      (.arrow (effT E D A) .pure (.arrow (effT E D' Bt) .pure (effT E R X) C2) C1) C0) φ) := by
  have hn : ∀ s, ¬ (unEff mk (.var 1) (unEff mk (.var 1)
      (.app (.app (.var 4) (.await (.var 1))) (.await (.var 0))))).Has s :=
    fun s h => by simp [unEff, Flick.Tm.Has] at h
  have hm : ∀ s, ¬ (Flick.Tm.lam (mkEff mk (unEff mk (.var 1) (unEff mk (.var 1)
      (.app (.app (.var 4) (.await (.var 1))) (.await (.var 0))))))).Has s :=
    fun s h => noHas_mkEff hn s h
  refine .lam (.lam (.lam (typ_mkEff hmk (ψ := .rt Row.none Row.none)
    (typ_unEff hmk (typ_var_mono rfl) (typ_unEff hmk (typ_var_mono rfl)
      (.app (ψ := .pure) (.app (ψ := .pure) (typ_var_mono rfl) (typ_await_var rfl) trivial .refl) (typ_await_var rfl)
        trivial .refl)))
    ⟨fun _ h => h, fun _ h => h⟩ hn ?_) (fun h => noHas_mkEff hn _ h)
    (held_sub (fun s h => noHas_mkEff hn s h) ?_) .refl)
    (fun h => hm _ h) (held_sub hm ?_) .refl)
    (fun h => hm _ h) (held_sub (fun s h => hm s h) (by simp)) .refl
  · intro s hs a h
    simp only [List.mem_cons, List.not_mem_nil, or_false] at hs
    rcases hs with rfl | rfl | rfl
    · exact hy a ((Flick.Scheme.reach_mono _ _).1 h)
    · exact hx a ((Flick.Scheme.reach_mono _ _).1 h)
    · exact hg a ((Flick.Scheme.reach_mono _ _).1 h)
  · intro s hs a h
    simp only [List.mem_cons, List.not_mem_nil, or_false] at hs
    rcases hs with rfl | rfl
    · exact hx₂ a ((Flick.Scheme.reach_mono _ _).1 h)
    · exact hg₂ a ((Flick.Scheme.reach_mono _ _).1 h)
  · intro s hs a h
    simp only [List.mem_cons, List.not_mem_nil, or_false] at hs
    subst hs; exact hg₁ a ((Flick.Scheme.reach_mono _ _).1 h)

variable (N : Names) {eC : Nat} (hE : N.con eC = E) (h2 : eC ≠ 2)
include hE h2

/-- **`pure` of `Monad Eff`** is typed at its scheme in `Sugar`. -/
theorem typ_effPure_tr (c a : Ty) {Ψ Θ φ} :
    Typ S Ψ Θ [] (effPure mk) (.ex (trT N (arrP a (appQ (.con eC) c a))) φ) := by
  simp only [trT, if_neg h2, hE, trStage]; exact typ_effPure hmk

/-- **`flat_map` of `Monad Eff`** is typed at its scheme in `Sugar` (`bindTy`). -/
theorem typ_effBind_tr (c d a b e : Ty) {Ψ Θ φ} :
    Typ S Ψ Θ [] (effBind mk) (.ex (trT N (bindTy (.con eC) c d a b e)) φ) := by
  simp only [bindTy, arrP, appQ, trT, if_neg h2, hE, trStage]
  refine typ_effBind hmk ?_ ?_ ?_ <;> intro x h <;> rcases x with n | p <;>
    simp [effT, Flick.Ty.reach, trT_reach, Caps.none] at h ⊢ <;> grind

/-- **`map2` of `Monad Eff`** is typed at its scheme in `Sugar` (`map2Ty`). -/
theorem typ_effMap2_tr (c d a b x g d' : Ty) {Ψ Θ φ} :
    Typ S Ψ Θ [] (effMap2 mk) (.ex (trT N (map2Ty (.con eC) c d a b x g d')) φ) := by
  simp only [map2Ty, arrP, appQ, trT, if_neg h2, hE, trStage]
  refine typ_effMap2 hmk ?_ ?_ ?_ ?_ ?_ ?_ <;> intro y h <;> rcases y with n | p <;>
    simp [effT, Flick.Ty.reach, trT_reach, Caps.none] at h ⊢ <;> grind

end Flick.Elab
