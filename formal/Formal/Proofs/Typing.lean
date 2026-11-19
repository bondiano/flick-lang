import Formal.Proofs.Syntax

/-!
# Typing and evaluation of λPT

Terms are typed without annotations: a binder takes the type its uses
need. A global has a scheme over its type variables, and each use takes
an instance of it. A program is an acyclic environment: the body of the
global `g` is typed with the globals before it only.

Evaluation is by value, big-step and by substitution of values; it is
deterministic, and every result is a value.
-/

namespace Flick.PT

/-- The instance of a scheme at the types `ts`, by position. -/
def tyInst (ts : List Ty) (n : Nat) : Ty := ts.getD n (.var n)

/-- `Γ ⊢ e : t`, with `GT` the schemes of the globals. The rules read no
annotation: a binder takes any type its uses need, and a global any
instance of its scheme, so a global's body typed at its scheme is typed
at each instance. `infer` checks a term by its annotations, and what it
accepts is typed (`infer_sound`). -/
inductive Typed (GT : List Ty) : List Ty → Tm → Ty → Prop where
  | var : Γ[n]? = some t → Typed GT Γ (.var n) t
  | glob : GT[g]? = some σ → Typed GT Γ (.glob g l) (σ.subst θ)
  | lam : Typed GT (a :: Γ) b t → Typed GT Γ (.lam d b) (.arrow a t)
  | app : Typed GT Γ f (.arrow a t) → Typed GT Γ x a → Typed GT Γ (.app f x) t
  | unit : Typed GT Γ .unit .unit
  | pair : Typed GT Γ x a → Typed GT Γ y b → Typed GT Γ (.pair x y) (.prod a b)
  | split : Typed GT Γ e (.prod a b) → Typed GT (b :: a :: Γ) c t →
      Typed GT Γ (.split e c) t
  | inl : Typed GT Γ x a → Typed GT Γ (.inl x d) (.sum a b)
  | inr : Typed GT Γ x b → Typed GT Γ (.inr d x) (.sum a b)
  | case : Typed GT Γ e (.sum a b) → Typed GT (a :: Γ) l t → Typed GT (b :: Γ) r t →
      Typed GT Γ (.case e l r) t
  | let_ : Typed GT Γ e a → Typed GT (a :: Γ) c t → Typed GT Γ (.let_ e c) t

/-- A substitution of a typed term reads only the variables of its context. -/
theorem Typed.subst_agree {GT : List Ty} {Γ : List Ty} {e : Tm} {t : Ty}
    (h : Typed GT Γ e t) : ∀ {σ τ : Nat → Tm}, (∀ n, n < Γ.length → σ n = τ n) →
    e.subst σ = e.subst τ := by
  have up : ∀ {Γ : List Ty} {a : Ty} {σ τ : Nat → Tm},
      (∀ n, n < Γ.length → σ n = τ n) → ∀ n, n < (a :: Γ).length → upS σ n = upS τ n := by
    intro Γ a σ τ h n hn
    cases n with
    | zero => rfl
    | succ n => simp only [upS]; rw [h n (by simp at hn; omega)]
  induction h with
  | var hn =>
    intro σ τ hστ
    exact hστ _ (List.getElem?_eq_some_iff.mp hn).1
  | glob | unit => intro _ _ _; rfl
  | lam _ ih => intro σ τ h; simp only [Tm.subst]; rw [ih (up h)]
  | app _ _ ihf ihx => intro σ τ h; simp only [Tm.subst]; rw [ihf h, ihx h]
  | pair _ _ ihx ihy => intro σ τ h; simp only [Tm.subst]; rw [ihx h, ihy h]
  | split _ _ ihe ihb => intro σ τ h; simp only [Tm.subst]; rw [ihe h, ihb (up (up h))]
  | inl _ ih | inr _ ih => intro σ τ h; simp only [Tm.subst]; rw [ih h]
  | case _ _ _ ihe ihl ihr => intro σ τ h; simp only [Tm.subst]; rw [ihe h, ihl (up h), ihr (up h)]
  | let_ _ _ ihe ihb => intro σ τ h; simp only [Tm.subst]; rw [ihe h, ihb (up h)]

/-- A term typed in the empty context has no free variable. -/
theorem Typed.closed {GT : List Ty} {e : Tm} {t : Ty} (h : Typed GT [] e t) (σ : Nat → Tm) :
    e.subst σ = e :=
  (h.subst_agree (σ := σ) (τ := Tm.var) (fun _ hn => absurd hn (Nat.not_lt_zero _))).trans
    (Tm.subst_var e)

/-- Typing with the globals before `g` is typing with all of them. -/
theorem Typed.weaken_globals {GT GT' : List Ty} {Γ : List Ty} {e : Tm} {t : Ty}
    (h : Typed GT Γ e t) (hGT : ∀ (g : Nat) (σ : Ty), GT[g]? = some σ → GT'[g]? = some σ) :
    Typed GT' Γ e t := by
  induction h with
  | var hn => exact .var hn
  | glob hg => exact .glob (hGT _ _ hg)
  | lam _ ih => exact .lam ih
  | app _ _ ihf ihx => exact .app ihf ihx
  | unit => exact .unit
  | pair _ _ ihx ihy => exact .pair ihx ihy
  | split _ _ ihe ihb => exact .split ihe ihb
  | inl _ ih => exact .inl ih
  | inr _ ih => exact .inr ih
  | case _ _ _ ihe ihl ihr => exact .case ihe ihl ihr
  | let_ _ _ ihe ihb => exact .let_ ihe ihb

/-- A renaming that keeps the types of the variables keeps a term typed. -/
theorem Typed.ren {GT : List Ty} {Γ : List Ty} {e : Tm} {t : Ty} (h : Typed GT Γ e t) :
    ∀ {Δ : List Ty} {ξ : Nat → Nat}, (∀ n s, Γ[n]? = some s → Δ[ξ n]? = some s) →
    Typed GT Δ (e.ren ξ) t := by
  have up : ∀ {Γ Δ : List Ty} {ξ : Nat → Nat} {a : Ty},
      (∀ n s, Γ[n]? = some s → Δ[ξ n]? = some s) →
      ∀ n s, (a :: Γ)[n]? = some s → (a :: Δ)[upR ξ n]? = some s := by
    intro Γ Δ ξ a h n s hn
    cases n with
    | zero => simpa [upR] using hn
    | succ n => simpa [upR] using h n s (by simpa using hn)
  induction h with
  | var hn => intro Δ ξ h; exact .var (h _ _ hn)
  | glob hg => intro Δ ξ _; exact .glob hg
  | lam _ ih => intro Δ ξ h; exact .lam (ih (up h))
  | app _ _ ihf ihx => intro Δ ξ h; exact .app (ihf h) (ihx h)
  | unit => intro Δ ξ _; exact .unit
  | pair _ _ ihx ihy => intro Δ ξ h; exact .pair (ihx h) (ihy h)
  | split _ _ ihe ihc => intro Δ ξ h; exact .split (ihe h) (ihc (up (up h)))
  | inl _ ih => intro Δ ξ h; exact .inl (ih h)
  | inr _ ih => intro Δ ξ h; exact .inr (ih h)
  | case _ _ _ ihe ihl ihr => intro Δ ξ h; exact .case (ihe h) (ihl (up h)) (ihr (up h))
  | let_ _ _ ihe ihc => intro Δ ξ h; exact .let_ (ihe h) (ihc (up h))

/-- A substitution of typed terms for the variables keeps a term typed. -/
theorem Typed.subst {GT : List Ty} {Γ : List Ty} {e : Tm} {t : Ty} (h : Typed GT Γ e t) :
    ∀ {Δ : List Ty} {σ : Nat → Tm}, (∀ n s, Γ[n]? = some s → Typed GT Δ (σ n) s) →
    Typed GT Δ (e.subst σ) t := by
  have up : ∀ {Γ Δ : List Ty} {σ : Nat → Tm} {a : Ty},
      (∀ n s, Γ[n]? = some s → Typed GT Δ (σ n) s) →
      ∀ n s, (a :: Γ)[n]? = some s → Typed GT (a :: Δ) (upS σ n) s := by
    intro Γ Δ σ a h n s hn
    cases n with
    | zero => simp at hn; subst hn; exact .var rfl
    | succ n =>
      exact (h n s (by simpa using hn)).ren (fun m s hm => by simpa using hm)
  induction h with
  | var hn => intro Δ σ h; exact h _ _ hn
  | glob hg => intro Δ σ _; exact .glob hg
  | lam _ ih => intro Δ σ h; exact .lam (ih (up h))
  | app _ _ ihf ihx => intro Δ σ h; exact .app (ihf h) (ihx h)
  | unit => intro Δ σ _; exact .unit
  | pair _ _ ihx ihy => intro Δ σ h; exact .pair (ihx h) (ihy h)
  | split _ _ ihe ihc => intro Δ σ h; exact .split (ihe h) (ihc (up (up h)))
  | inl _ ih => intro Δ σ h; exact .inl (ih h)
  | inr _ ih => intro Δ σ h; exact .inr (ih h)
  | case _ _ _ ihe ihl ihr => intro Δ σ h; exact .case (ihe h) (ihl (up h)) (ihr (up h))
  | let_ _ _ ihe ihc => intro Δ σ h; exact .let_ (ihe h) (ihc (up h))

theorem Typed.inst {GT : List Ty} {Γ : List Ty} {a t : Ty} {b v : Tm}
    (hb : Typed GT (a :: Γ) b t) (hv : Typed GT Γ v a) : Typed GT Γ (b.inst v) t :=
  hb.subst fun n s hn => by
    cases n with
    | zero => simp at hn; subst hn; exact hv
    | succ n => exact .var (by simpa using hn)

theorem Typed.inst2 {GT : List Ty} {Γ : List Ty} {a b t : Ty} {c v w : Tm}
    (hc : Typed GT (b :: a :: Γ) c t) (hv : Typed GT Γ v a) (hw : Typed GT Γ w b) :
    Typed GT Γ (c.inst2 v w) t :=
  hc.subst fun n s hn => by
    match n with
    | 0 => simp at hn; subst hn; exact hw
    | 1 => simp at hn; subst hn; exact hv
    | n + 2 => exact .var (by simpa using hn)

/-- A term typed in the empty context is typed in any. -/
theorem Typed.weaken_closed {GT : List Ty} {e : Tm} {t : Ty} (h : Typed GT [] e t)
    (Γ : List Ty) : Typed GT Γ e t := by
  have := h.ren (Δ := Γ) (ξ := id) (fun n s hn => by simp at hn)
  rwa [Tm.ren_as_subst, h.closed] at this

/-- The types of a typed term instantiated are the types of the term. -/
theorem Typed.tsubst {GT : List Ty} {Γ : List Ty} {e : Tm} {t : Ty} (h : Typed GT Γ e t)
    (θ : Nat → Ty) : Typed GT (Γ.map (·.subst θ)) e (t.subst θ) := by
  induction h with
  | var hn => exact .var (by simp [hn])
  | glob hg => rw [Ty.subst_subst]; exact .glob hg
  | lam _ ih => exact .lam ih
  | app _ _ ihf ihx => exact .app ihf ihx
  | unit => exact .unit
  | pair _ _ ihx ihy => exact .pair ihx ihy
  | split _ _ ihe ihc => exact .split ihe ihc
  | inl _ ih => exact .inl ih
  | inr _ ih => exact .inr ih
  | case _ _ _ ihe ihl ihr => exact .case ihe ihl ihr
  | let_ _ _ ihe ihc => exact .let_ ihe ihc

/-- The type `infer` gives a term, where it has one. -/
def infer (GT : List Ty) : List Ty → Tm → Option Ty
  | Γ, .var n => Γ[n]?
  | _, .glob g l => (GT[g]?).map (·.subst (tyInst l))
  | Γ, .lam a b => (infer GT (a :: Γ) b).map (.arrow a)
  | Γ, .app f x =>
    match infer GT Γ f, infer GT Γ x with
    | some (.arrow a t), some a' => if a = a' then some t else none
    | _, _ => none
  | _, .unit => some .unit
  | Γ, .pair x y =>
    match infer GT Γ x, infer GT Γ y with
    | some a, some b => some (.prod a b)
    | _, _ => none
  | Γ, .split e c =>
    match infer GT Γ e with
    | some (.prod a b) => infer GT (b :: a :: Γ) c
    | _ => none
  | Γ, .inl x b => (infer GT Γ x).map (fun a => .sum a b)
  | Γ, .inr a x => (infer GT Γ x).map (fun b => .sum a b)
  | Γ, .case e l r =>
    match infer GT Γ e with
    | some (.sum a b) =>
      match infer GT (a :: Γ) l, infer GT (b :: Γ) r with
      | some t, some t' => if t = t' then some t else none
      | _, _ => none
    | _ => none
  | Γ, .let_ e c =>
    match infer GT Γ e with
    | some a => infer GT (a :: Γ) c
    | none => none

theorem infer_sound {GT : List Ty} : ∀ {Γ : List Ty} {e : Tm} {t : Ty},
    infer GT Γ e = some t → Typed GT Γ e t := by
  intro Γ e
  induction e generalizing Γ with
  | var n => intro t h; exact .var h
  | glob g l =>
    intro t h; simp only [infer, Option.map_eq_some_iff] at h
    obtain ⟨σ, hσ, rfl⟩ := h; exact .glob hσ
  | lam a b ih =>
    intro t h; simp only [infer, Option.map_eq_some_iff] at h
    obtain ⟨t', ht', rfl⟩ := h; exact .lam (ih ht')
  | app f x ihf ihx =>
    intro t h; simp only [infer] at h
    split at h
    · rename_i a t' a' hf hx
      split at h
      · cases h; rename_i hEq; subst hEq; exact .app (ihf hf) (ihx hx)
      · cases h
    · cases h
  | unit => intro t h; simp only [infer] at h; cases h; exact .unit
  | pair x y ihx ihy =>
    intro t h; simp only [infer] at h
    split at h
    · rename_i a b hx hy; cases h; exact .pair (ihx hx) (ihy hy)
    · cases h
  | split e c ihe ihc =>
    intro t h; simp only [infer] at h
    split at h
    · rename_i a b he; exact .split (ihe he) (ihc h)
    · cases h
  | inl x b ih =>
    intro t h; simp only [infer, Option.map_eq_some_iff] at h
    obtain ⟨a, ha, rfl⟩ := h; exact .inl (ih ha)
  | inr a x ih =>
    intro t h; simp only [infer, Option.map_eq_some_iff] at h
    obtain ⟨b, hb, rfl⟩ := h; exact .inr (ih hb)
  | case e l r ihe ihl ihr =>
    intro t h; simp only [infer] at h
    split at h
    · rename_i a b he
      split at h
      · rename_i t₁ t₂ hl hr
        split at h
        · cases h; rename_i hEq; subst hEq; exact .case (ihe he) (ihl hl) (ihr hr)
        · cases h
      · cases h
    · cases h
  | let_ e c ihe ihc =>
    intro t h; simp only [infer] at h
    split at h
    · rename_i a he; exact .let_ (ihe he) (ihc h)
    · cases h

/-- A program: the body and the scheme of each global, in order. -/
abbrev Prog := List (Tm × Ty)

def Prog.bodies (P : Prog) : List Tm := P.map Prod.fst
def Prog.schemes (P : Prog) : List Ty := P.map Prod.snd

/-- Each body is typed at its scheme with the globals before it. -/
def Prog.WF (P : Prog) : Prop :=
  ∀ g (h : g < P.length), Typed (P.schemes.take g) [] P[g].1 P[g].2

/-! ## Values and evaluation -/

inductive Value : Tm → Prop where
  | lam : Value (.lam a b)
  | unit : Value .unit
  | pair : Value v → Value w → Value (.pair v w)
  | inl : Value v → Value (.inl v b)
  | inr : Value v → Value (.inr a v)

/-- `e ⇓ v` with the bodies `G` of the globals. -/
inductive Eval (G : List Tm) : Tm → Tm → Prop where
  | lam : Eval G (.lam a b) (.lam a b)
  | glob : G[g]? = some t → Eval G t v → Eval G (.glob g l) v
  | app : Eval G f (.lam a b) → Eval G x v → Eval G (b.inst v) w → Eval G (.app f x) w
  | unit : Eval G .unit .unit
  | pair : Eval G x v → Eval G y w → Eval G (.pair x y) (.pair v w)
  | split : Eval G e (.pair v w) → Eval G (c.inst2 v w) u → Eval G (.split e c) u
  | inl : Eval G x v → Eval G (.inl x b) (.inl v b)
  | inr : Eval G x v → Eval G (.inr a x) (.inr a v)
  | caseL : Eval G e (.inl v b) → Eval G (l.inst v) u → Eval G (.case e l r) u
  | caseR : Eval G e (.inr a v) → Eval G (r.inst v) u → Eval G (.case e l r) u
  | let_ : Eval G e v → Eval G (c.inst v) u → Eval G (.let_ e c) u

theorem Eval.value {G : List Tm} {e v : Tm} (h : Eval G e v) : Value v := by
  induction h with
  | lam => exact .lam
  | unit => exact .unit
  | pair _ _ ihx ihy => exact .pair ihx ihy
  | inl _ ih => exact .inl ih
  | inr _ ih => exact .inr ih
  | glob _ _ ih | app _ _ _ _ _ ih | split _ _ _ ih | caseL _ _ _ ih | caseR _ _ _ ih
  | let_ _ _ _ ih => exact ih

theorem Value.eval {G : List Tm} {v : Tm} (h : Value v) : Eval G v v := by
  induction h with
  | lam => exact .lam
  | unit => exact .unit
  | pair _ _ ihv ihw => exact .pair ihv ihw
  | inl _ ih => exact .inl ih
  | inr _ ih => exact .inr ih

theorem Eval.det {G : List Tm} {e v : Tm} (h : Eval G e v) : ∀ {w}, Eval G e w → v = w := by
  induction h with
  | lam => intro w h; cases h; rfl
  | unit => intro w h; cases h; rfl
  | glob hg _ ih =>
    intro w h; cases h with
    | glob hg' h' => rw [hg] at hg'; cases hg'; exact ih h'
  | app _ _ _ ihf ihx ihb =>
    intro w h; cases h with
    | app hf hx hb => cases ihf hf; cases ihx hx; exact ihb hb
  | pair _ _ ihx ihy =>
    intro w h; cases h with
    | pair hx hy => rw [ihx hx, ihy hy]
  | split _ _ ihe ihb =>
    intro w h; cases h with
    | split he hb => cases ihe he; exact ihb hb
  | inl _ ih => intro w h; cases h with | inl h => rw [ih h]
  | inr _ ih => intro w h; cases h with | inr h => rw [ih h]
  | caseL _ _ ihe ihl =>
    intro w h; cases h with
    | caseL he hl => cases ihe he; exact ihl hl
    | caseR he _ => cases ihe he
  | caseR _ _ ihe ihr =>
    intro w h; cases h with
    | caseL he _ => cases ihe he
    | caseR he hr => cases ihe he; exact ihr hr
  | let_ _ _ ihe ihb =>
    intro w h; cases h with
    | let_ he hb => cases ihe he; exact ihb hb

end Flick.PT
