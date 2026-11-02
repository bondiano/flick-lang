import Formal.Elab.Witness.Real

/-!
# Witness: the bodies of the methods and constants (W2)

Each body is typed by the rules of the core at the translation of its scheme in `Sugar`, in
the empty context. `OptQ` is eager: `flat_map k x` matches `x` and copies the result of `k`
into the row of the result, as the `CRow` argument is invariant. The lists `Seq` and `ListQ`
are in `Seq.lean`, `unc n`, `from_result_n` and `eq` in `Unc.lean`.
-/

namespace Flick.Elab.Witness

open Flick.Elab

/-- `match e with K x → b | _ → n`. -/
def un1 (K : Name) (e b n : Flick.Tm) : Flick.Tm :=
  .match_ e (.acons (.app (.ctor K) .var) b (.acons .wild n .anil))

def tc (T : Name) (us : List Flick.Ty) : Flick.Ty := Flick.Ty.tcon T us

variable {Ψ : List Flick.Ty} {Θ : Facts} {Γ : List Flick.Scheme}

theorem foldl_hd : ∀ (us : List Flick.Ty) (t : Flick.Ty), (us.foldl .tapp t).hd = t.hd
  | [], _ => rfl
  | u :: us, t => (foldl_hd us (.tapp t u)).trans rfl

theorem tcon_hd (T : Name) (us : List Flick.Ty) : (tc T us).hd = .adt T := foldl_hd us _

theorem typ_con0 {K T us φ} (hK : ctorF K = some (us.length, [], T)) :
    Typ Sg Ψ Θ Γ (.ctor K) (.ex (tc T us) φ) := .ctor hK rfl .refl

theorem typ_con1 {K f T us e φ} (hK : ctorF K = some (us.length, [f], T))
    (he : Typ Sg Ψ Θ Γ e (.ex (f.substT (instT us)) φ)) :
    Typ Sg Ψ Θ Γ (.app (.ctor K) e) (.ex (tc T us) φ) :=
  .app (ψ := .pure) (.ctor hK rfl .refl) he trivial .refl

theorem typ_con2 {K f g T us e e' φ} (hK : ctorF K = some (us.length, [f, g], T))
    (he : Typ Sg Ψ Θ Γ e (.ex (f.substT (instT us)) φ))
    (he' : Typ Sg Ψ Θ Γ e' (.ex (g.substT (instT us)) φ)) :
    Typ Sg Ψ Θ Γ (.app (.app (.ctor K) e) e') (.ex (tc T us) φ) :=
  .app (ψ := .pure) (.app (ψ := .pure) (.ctor hK rfl .refl) he trivial .refl) he' trivial .refl

theorem typ_un1 {K f T us e b n t φ} (hK : ctorF K = some (us.length, [f], T))
    (he : Typ Sg Ψ Θ Γ e (.ex (tc T us) φ))
    (hb : Typ Sg Ψ Θ (.mono (f.substT (instT us)) :: Γ) b (.ex t φ))
    (hn : Typ Sg Ψ Θ Γ n (.ex t φ)) : Typ Sg Ψ Θ Γ (un1 K e b n) (.ex t φ) := by
  have hh := PatTy.head (S := Sg) hK (us := us) rfl
  exact .match_ he (.acons (.adt (.app hh .var) (tcon_hd T us)) hb
    (.acons .wild hn .anil)) (.inl fun _ _ => ⟨.wild, by simp [Flick.Tm.unguarded], rfl⟩)

theorem typ_lam {a t b φ} {C : Caps} (hb : Typ Sg Ψ Θ (.mono a :: Γ) b (.ex t .pure))
    (hn : ∀ s, ¬ b.Has s) (hh : ∀ s ∈ Γ, ∀ x, Flick.Scheme.reach s x → (Flick.Ty.arrow a .pure t C).reach x) :
    Typ Sg Ψ Θ Γ (.lam b) (.ex (.arrow a .pure t C) φ) :=
  .lam hb (hn .ins) (held_sub (fun s h => hn s h) hh) .refl

/-- `OptQ q a` of the core. -/
def optT (q a : Flick.Ty) : Flick.Ty := .tapp (.tapp (.adt "OptQ") q) a

def someQ (e : Flick.Tm) : Flick.Tm := .app (.ctor "SomeQ") e
def noneQ : Flick.Tm := .ctor "NoneQ"

/-- `pure x = SomeQ x`. -/
def optPure : Flick.Tm := .lam (someQ (.var 0))
/-- `flat_map k x`: `k a` copied into the row of the result. -/
def optBind : Flick.Tm := .lam (.lam (un1 "SomeQ" (.var 0)
  (un1 "SomeQ" (.app (.var 2) (.var 0)) (someQ (.var 0)) noneQ) noneQ))
/-- `map2 g x y`. -/
def optMap2 : Flick.Tm := .lam (.lam (.lam (un1 "SomeQ" (.var 1)
  (un1 "SomeQ" (.var 1) (someQ (.app (.app (.var 4) (.var 1)) (.var 0))) noneQ) noneQ)))

theorem typ_someQ {e q a φ} (he : Typ Sg Ψ Θ Γ e (.ex a φ)) :
    Typ Sg Ψ Θ Γ (someQ e) (.ex (optT q a) φ) := typ_con1 (us := [q, a]) rfl he

theorem typ_noneQ {q a φ} : Typ Sg Ψ Θ Γ noneQ (.ex (optT q a) φ) := typ_con0 (us := [q, a]) rfl

theorem typ_unSome {e b n q a t φ} (he : Typ Sg Ψ Θ Γ e (.ex (optT q a) φ))
    (hb : Typ Sg Ψ Θ (.mono a :: Γ) b (.ex t φ)) (hn : Typ Sg Ψ Θ Γ n (.ex t φ)) :
    Typ Sg Ψ Θ Γ (un1 "SomeQ" e b n) (.ex t φ) := typ_un1 (us := [q, a]) rfl he hb hn

theorem mem2 {P : Flick.Scheme → Prop} {a b : Flick.Ty} (ha : P (.mono a)) (hb : P (.mono b)) :
    ∀ s ∈ [Flick.Scheme.mono a, .mono b], P s := by
  intro s hs; simp only [List.mem_cons, List.not_mem_nil, or_false] at hs; rcases hs with rfl | rfl <;> assumption

theorem mem1 {P : Flick.Scheme → Prop} {a : Flick.Ty} (ha : P (.mono a)) :
    ∀ s ∈ [Flick.Scheme.mono a], P s := by
  intro s hs; simp only [List.mem_cons, List.not_mem_nil, or_false] at hs; subst hs; exact ha

theorem typ_optPure {A Q C0 φ} : Typ Sg Ψ Θ [] optPure (.ex (.arrow A .pure (optT Q A) C0) φ) :=
  typ_lam (typ_someQ (typ_var_mono rfl)) (fun s h => by simp [someQ, Flick.Tm.Has] at h) (by simp)

theorem typ_optBind {A B C C0 C2 D E R φ}
    (hk : ∀ x, (Flick.Ty.arrow A .pure (optT E B) C).reach x →
      (Flick.Ty.arrow (optT D A) .pure (optT R B) C2).reach x) :
    Typ Sg Ψ Θ [] optBind (.ex (.arrow (.arrow A .pure (optT E B) C) .pure
      (.arrow (optT D A) .pure (optT R B) C2) C0) φ) :=
  typ_lam (typ_lam (typ_unSome (typ_var_mono rfl)
      (typ_unSome (.app (ψ := .pure) (typ_var_mono rfl) (typ_var_mono rfl) trivial .refl)
        (typ_someQ (typ_var_mono rfl)) typ_noneQ) typ_noneQ)
    (fun s h => by simp [un1, someQ, noneQ, Flick.Tm.Has] at h)
    (mem1 fun x h => hk x ((Flick.Scheme.reach_mono _ _).1 h)))
    (fun s h => by simp [un1, someQ, noneQ, Flick.Tm.Has] at h) (by simp)

theorem typ_optMap2 {A Bt X Cg C C0 C1 C2 D D' R φ}
    (hg₂ : ∀ x, (Flick.Ty.arrow A .pure (.arrow Bt .pure X Cg) C).reach x →
      (Flick.Ty.arrow (optT D' Bt) .pure (optT R X) C2).reach x)
    (hx₂ : ∀ x, (optT D A).reach x → (Flick.Ty.arrow (optT D' Bt) .pure (optT R X) C2).reach x)
    (hg₁ : ∀ x, (Flick.Ty.arrow A .pure (.arrow Bt .pure X Cg) C).reach x →
      (Flick.Ty.arrow (optT D A) .pure (.arrow (optT D' Bt) .pure (optT R X) C2) C1).reach x) :
    Typ Sg Ψ Θ [] optMap2 (.ex (.arrow (.arrow A .pure (.arrow Bt .pure X Cg) C) .pure
      (.arrow (optT D A) .pure (.arrow (optT D' Bt) .pure (optT R X) C2) C1) C0) φ) :=
  typ_lam (typ_lam (typ_lam (typ_unSome (typ_var_mono rfl) (typ_unSome (typ_var_mono rfl)
      (typ_someQ (.app (ψ := .pure) (.app (ψ := .pure) (typ_var_mono rfl) (typ_var_mono rfl)
        trivial .refl) (typ_var_mono rfl) trivial .refl)) typ_noneQ) typ_noneQ)
    (fun s h => by simp [un1, someQ, noneQ, Flick.Tm.Has] at h)
    (mem2 (fun x h => hx₂ x ((Flick.Scheme.reach_mono _ _).1 h))
      (fun x h => hg₂ x ((Flick.Scheme.reach_mono _ _).1 h))))
    (fun s h => by simp [un1, someQ, noneQ, Flick.Tm.Has] at h)
    (mem1 fun x h => hg₁ x ((Flick.Scheme.reach_mono _ _).1 h)))
    (fun s h => by simp [un1, someQ, noneQ, Flick.Tm.Has] at h) (by simp)

def pairB : Flick.Tm := .lam (.lam (.app (.app (.ctor "Tup") (.var 1)) (.var 0)))

theorem typ_pairB {A B C C0 φ} (hC : ∀ x, A.reach x → (Flick.Ty.arrow B .pure (tc "Tup" [A, B]) C).reach x) :
    Typ Sg Ψ Θ [] pairB (.ex (.arrow A .pure (.arrow B .pure (tc "Tup" [A, B]) C) C0) φ) :=
  typ_lam (typ_lam (typ_con2 (us := [A, B]) rfl (typ_var_mono rfl) (typ_var_mono rfl))
      (fun s h => by simp [Flick.Tm.Has] at h)
      (mem1 fun x h => hC x ((Flick.Scheme.reach_mono _ _).1 h)))
    (fun s h => by simp [Flick.Tm.Has] at h) (by simp)

end Flick.Elab.Witness
