/-!
# λPT: the total calculus of proofs

The terms a Flick `proof` is about (`docs/formal/proofs.md` §3). A data type
that does not contain itself is a sum of products of its fields, so the
calculus has `unit`, products and binary sums in place of declared data
types; the generator writes `Option a` as `unit + a` and `Bool` as
`unit + unit`. Type variables are rigid: a statement holds at every closed
type. Terms use de Bruijn indices, and a global names a closed definition
of an acyclic environment.
-/

namespace Flick.PT

inductive Ty where
  | var : Nat → Ty
  | unit : Ty
  | prod : Ty → Ty → Ty
  | sum : Ty → Ty → Ty
  | arrow : Ty → Ty → Ty
  deriving DecidableEq, Repr

/-- Binders and injections carry the types a checker needs, as the
compiler's terms do; evaluation ignores them. -/
inductive Tm where
  | var : Nat → Tm
  /-- A global at an instance of its scheme. -/
  | glob : Nat → List Ty → Tm
  /-- `fun (x : a) -> b`. -/
  | lam : Ty → Tm → Tm
  | app : Tm → Tm → Tm
  | unit : Tm
  | pair : Tm → Tm → Tm
  /-- `split e b`: `b` with the two components of `e` bound, the second innermost. -/
  | split : Tm → Tm → Tm
  /-- `inl x` of `sum a b`, with `b`. -/
  | inl : Tm → Ty → Tm
  /-- `inr x` of `sum a b`, with `a`. -/
  | inr : Ty → Tm → Tm
  /-- `case e l r`: `l` or `r` with the payload of `e` bound. -/
  | case : Tm → Tm → Tm → Tm
  | let_ : Tm → Tm → Tm
  deriving DecidableEq, Repr

/-! ## Types -/

/-- Each type variable replaced by its type. -/
def Ty.subst (θ : Nat → Ty) : Ty → Ty
  | .var n => θ n
  | .unit => .unit
  | .prod a b => .prod (a.subst θ) (b.subst θ)
  | .sum a b => .sum (a.subst θ) (b.subst θ)
  | .arrow a b => .arrow (a.subst θ) (b.subst θ)

/-- A type without variables. -/
def Ty.Closed : Ty → Prop
  | .var _ => False
  | .unit => True
  | .prod a b | .sum a b | .arrow a b => a.Closed ∧ b.Closed

theorem Ty.subst_closed (θ : Nat → Ty) (hθ : ∀ n, (θ n).Closed) : ∀ t : Ty, (t.subst θ).Closed
  | .var n => hθ n
  | .unit => trivial
  | .prod a b | .sum a b | .arrow a b => ⟨subst_closed θ hθ a, subst_closed θ hθ b⟩

theorem Ty.subst_subst (θ θ' : Nat → Ty) : ∀ t : Ty,
    (t.subst θ).subst θ' = t.subst (fun n => (θ n).subst θ')
  | .var _ => rfl
  | .unit => rfl
  | .prod a b | .sum a b | .arrow a b => by
    simp [Ty.subst, subst_subst θ θ' a, subst_subst θ θ' b]

/-! ## Renaming and substitution of terms -/

/-- A renaming under one more binder. -/
def upR (ξ : Nat → Nat) : Nat → Nat
  | 0 => 0
  | n + 1 => ξ n + 1

def Tm.ren (ξ : Nat → Nat) : Tm → Tm
  | .var n => .var (ξ n)
  | .glob g ts => .glob g ts
  | .lam a b => .lam a (b.ren (upR ξ))
  | .app f a => .app (f.ren ξ) (a.ren ξ)
  | .unit => .unit
  | .pair a b => .pair (a.ren ξ) (b.ren ξ)
  | .split e b => .split (e.ren ξ) (b.ren (upR (upR ξ)))
  | .inl x b => .inl (x.ren ξ) b
  | .inr a x => .inr a (x.ren ξ)
  | .case e l r => .case (e.ren ξ) (l.ren (upR ξ)) (r.ren (upR ξ))
  | .let_ e b => .let_ (e.ren ξ) (b.ren (upR ξ))

/-- A substitution under one more binder. -/
def upS (σ : Nat → Tm) : Nat → Tm
  | 0 => .var 0
  | n + 1 => (σ n).ren Nat.succ

def Tm.subst (σ : Nat → Tm) : Tm → Tm
  | .var n => σ n
  | .glob g ts => .glob g ts
  | .lam a b => .lam a (b.subst (upS σ))
  | .app f a => .app (f.subst σ) (a.subst σ)
  | .unit => .unit
  | .pair a b => .pair (a.subst σ) (b.subst σ)
  | .split e b => .split (e.subst σ) (b.subst (upS (upS σ)))
  | .inl x b => .inl (x.subst σ) b
  | .inr a x => .inr a (x.subst σ)
  | .case e l r => .case (e.subst σ) (l.subst (upS σ)) (r.subst (upS σ))
  | .let_ e b => .let_ (e.subst σ) (b.subst (upS σ))

/-- `v` for the innermost variable, then `σ`. -/
def scons (v : Tm) (σ : Nat → Tm) : Nat → Tm
  | 0 => v
  | n + 1 => σ n

/-- The innermost variable replaced by `v`. -/
def Tm.inst (b v : Tm) : Tm := b.subst (scons v Tm.var)

/-- The two innermost variables replaced by `v` and then `w`, `w` innermost. -/
def Tm.inst2 (b v w : Tm) : Tm := b.subst (scons w (scons v Tm.var))

/-! ## The algebra of renamings and substitutions -/

theorem upR_ext {ξ ζ : Nat → Nat} (h : ∀ n, ξ n = ζ n) : ∀ n, upR ξ n = upR ζ n
  | 0 => rfl
  | n + 1 => by simp [upR, h]

theorem upR_comp (ξ ζ : Nat → Nat) : ∀ n, upR ζ (upR ξ n) = upR (fun n => ζ (ξ n)) n
  | 0 => rfl
  | _ + 1 => rfl

theorem upS_ext {σ τ : Nat → Tm} (h : ∀ n, σ n = τ n) : ∀ n, upS σ n = upS τ n
  | 0 => rfl
  | n + 1 => by simp [upS, h]

theorem Tm.ren_ext {ξ ζ : Nat → Nat} (h : ∀ n, ξ n = ζ n) : ∀ t : Tm, t.ren ξ = t.ren ζ := by
  intro t
  induction t generalizing ξ ζ with
  | var n => simp [Tm.ren, h]
  | glob | unit => rfl
  | lam _ b ih => simp [Tm.ren, ih (upR_ext h)]
  | app f a ihf iha => simp [Tm.ren, ihf h, iha h]
  | pair a b iha ihb => simp [Tm.ren, iha h, ihb h]
  | split e b ihe ihb => simp [Tm.ren, ihe h, ihb (upR_ext (upR_ext h))]
  | inl x _ ih | inr _ x ih => simp [Tm.ren, ih h]
  | case e l r ihe ihl ihr => simp [Tm.ren, ihe h, ihl (upR_ext h), ihr (upR_ext h)]
  | let_ e b ihe ihb => simp [Tm.ren, ihe h, ihb (upR_ext h)]

theorem Tm.subst_ext {σ τ : Nat → Tm} (h : ∀ n, σ n = τ n) : ∀ t : Tm, t.subst σ = t.subst τ := by
  intro t
  induction t generalizing σ τ with
  | var n => simp [Tm.subst, h]
  | glob | unit => rfl
  | lam _ b ih => simp [Tm.subst, ih (upS_ext h)]
  | app f a ihf iha => simp [Tm.subst, ihf h, iha h]
  | pair a b iha ihb => simp [Tm.subst, iha h, ihb h]
  | split e b ihe ihb => simp [Tm.subst, ihe h, ihb (upS_ext (upS_ext h))]
  | inl x _ ih | inr _ x ih => simp [Tm.subst, ih h]
  | case e l r ihe ihl ihr => simp [Tm.subst, ihe h, ihl (upS_ext h), ihr (upS_ext h)]
  | let_ e b ihe ihb => simp [Tm.subst, ihe h, ihb (upS_ext h)]

theorem Tm.ren_ren (t : Tm) : ∀ ξ ζ : Nat → Nat, (t.ren ξ).ren ζ = t.ren (fun n => ζ (ξ n)) := by
  induction t with
  | var n => intro _ _; rfl
  | glob | unit => intro _ _; rfl
  | lam _ b ih =>
    intro ξ ζ; simp only [Tm.ren, ih]; exact congrArg _ (Tm.ren_ext (upR_comp ξ ζ) b)
  | app f a ihf iha => intro ξ ζ; simp [Tm.ren, ihf, iha]
  | pair a b iha ihb => intro ξ ζ; simp [Tm.ren, iha, ihb]
  | split e b ihe ihb =>
    intro ξ ζ; simp only [Tm.ren, ihe, ihb]; congr 1
    exact Tm.ren_ext (fun n => (upR_comp _ _ n).trans (upR_ext (upR_comp ξ ζ) n)) b
  | inl x _ ih | inr _ x ih => intro ξ ζ; simp [Tm.ren, ih]
  | case e l r ihe ihl ihr =>
    intro ξ ζ; simp only [Tm.ren, ihe, ihl, ihr]; congr 1
    · exact Tm.ren_ext (upR_comp ξ ζ) l
    · exact Tm.ren_ext (upR_comp ξ ζ) r
  | let_ e b ihe ihb =>
    intro ξ ζ; simp only [Tm.ren, ihe, ihb]; exact congrArg _ (Tm.ren_ext (upR_comp ξ ζ) b)

theorem upS_upR (σ : Nat → Tm) (ξ : Nat → Nat) : ∀ n, upS σ (upR ξ n) = upS (fun n => σ (ξ n)) n
  | 0 => rfl
  | _ + 1 => rfl

theorem Tm.ren_subst (t : Tm) : ∀ (ξ : Nat → Nat) (σ : Nat → Tm),
    (t.ren ξ).subst σ = t.subst (fun n => σ (ξ n)) := by
  induction t with
  | var n => intro _ _; rfl
  | glob | unit => intro _ _; rfl
  | lam _ b ih =>
    intro ξ σ; simp only [Tm.ren, Tm.subst, ih]; exact congrArg _ (Tm.subst_ext (upS_upR σ ξ) b)
  | app f a ihf iha => intro ξ σ; simp [Tm.ren, Tm.subst, ihf, iha]
  | pair a b iha ihb => intro ξ σ; simp [Tm.ren, Tm.subst, iha, ihb]
  | split e b ihe ihb =>
    intro ξ σ; simp only [Tm.ren, Tm.subst, ihe, ihb]; congr 1
    exact Tm.subst_ext (fun n => (upS_upR _ _ n).trans (upS_ext (upS_upR σ ξ) n)) b
  | inl x _ ih | inr _ x ih => intro ξ σ; simp [Tm.ren, Tm.subst, ih]
  | case e l r ihe ihl ihr =>
    intro ξ σ; simp only [Tm.ren, Tm.subst, ihe, ihl, ihr]; congr 1
    · exact Tm.subst_ext (upS_upR σ ξ) l
    · exact Tm.subst_ext (upS_upR σ ξ) r
  | let_ e b ihe ihb =>
    intro ξ σ; simp only [Tm.ren, Tm.subst, ihe, ihb]; exact congrArg _ (Tm.subst_ext (upS_upR σ ξ) b)

theorem upR_upS (σ : Nat → Tm) (ξ : Nat → Nat) :
    ∀ n, (upS σ n).ren (upR ξ) = upS (fun n => (σ n).ren ξ) n
  | 0 => rfl
  | n + 1 => by
    simp only [upS, Tm.ren_ren]; exact Tm.ren_ext (fun _ => rfl) (σ n)

theorem Tm.subst_ren (t : Tm) : ∀ (σ : Nat → Tm) (ξ : Nat → Nat),
    (t.subst σ).ren ξ = t.subst (fun n => (σ n).ren ξ) := by
  induction t with
  | var n => intro _ _; rfl
  | glob | unit => intro _ _; rfl
  | lam _ b ih =>
    intro σ ξ; simp only [Tm.ren, Tm.subst, ih]; exact congrArg _ (Tm.subst_ext (upR_upS σ ξ) b)
  | app f a ihf iha => intro σ ξ; simp [Tm.ren, Tm.subst, ihf, iha]
  | pair a b iha ihb => intro σ ξ; simp [Tm.ren, Tm.subst, iha, ihb]
  | split e b ihe ihb =>
    intro σ ξ; simp only [Tm.ren, Tm.subst, ihe, ihb]; congr 1
    exact Tm.subst_ext (fun n => (upR_upS _ _ n).trans (upS_ext (upR_upS σ ξ) n)) b
  | inl x _ ih | inr _ x ih => intro σ ξ; simp [Tm.ren, Tm.subst, ih]
  | case e l r ihe ihl ihr =>
    intro σ ξ; simp only [Tm.ren, Tm.subst, ihe, ihl, ihr]; congr 1
    · exact Tm.subst_ext (upR_upS σ ξ) l
    · exact Tm.subst_ext (upR_upS σ ξ) r
  | let_ e b ihe ihb =>
    intro σ ξ; simp only [Tm.ren, Tm.subst, ihe, ihb]; exact congrArg _ (Tm.subst_ext (upR_upS σ ξ) b)

theorem upS_upS (σ τ : Nat → Tm) :
    ∀ n, (upS σ n).subst (upS τ) = upS (fun n => (σ n).subst τ) n
  | 0 => rfl
  | n + 1 => by
    simp only [upS, Tm.ren_subst, Tm.subst_ren]

theorem Tm.subst_subst (t : Tm) : ∀ σ τ : Nat → Tm,
    (t.subst σ).subst τ = t.subst (fun n => (σ n).subst τ) := by
  induction t with
  | var n => intro _ _; rfl
  | glob | unit => intro _ _; rfl
  | lam _ b ih =>
    intro σ τ; simp only [Tm.subst, ih]; exact congrArg _ (Tm.subst_ext (upS_upS σ τ) b)
  | app f a ihf iha => intro σ τ; simp [Tm.subst, ihf, iha]
  | pair a b iha ihb => intro σ τ; simp [Tm.subst, iha, ihb]
  | split e b ihe ihb =>
    intro σ τ; simp only [Tm.subst, ihe, ihb]; congr 1
    exact Tm.subst_ext (fun n => (upS_upS _ _ n).trans (upS_ext (upS_upS σ τ) n)) b
  | inl x _ ih | inr _ x ih => intro σ τ; simp [Tm.subst, ih]
  | case e l r ihe ihl ihr =>
    intro σ τ; simp only [Tm.subst, ihe, ihl, ihr]; congr 1
    · exact Tm.subst_ext (upS_upS σ τ) l
    · exact Tm.subst_ext (upS_upS σ τ) r
  | let_ e b ihe ihb =>
    intro σ τ; simp only [Tm.subst, ihe, ihb]; exact congrArg _ (Tm.subst_ext (upS_upS σ τ) b)

/-- A renaming is the substitution of variables. -/
theorem Tm.ren_as_subst (ξ : Nat → Nat) (t : Tm) : t.ren ξ = t.subst (fun n => .var (ξ n)) := by
  have up : ∀ (ξ : Nat → Nat) n, upS (fun n => Tm.var (ξ n)) n = Tm.var (upR ξ n) := by
    intro ξ n; cases n <;> rfl
  induction t generalizing ξ with
  | var | glob | unit => rfl
  | lam _ b ih => simp only [Tm.ren, Tm.subst, ih]; rw [Tm.subst_ext (up ξ) b]
  | app f a ihf iha => simp [Tm.ren, Tm.subst, ihf, iha]
  | pair a b iha ihb => simp [Tm.ren, Tm.subst, iha, ihb]
  | split e b ihe ihb =>
    simp only [Tm.ren, Tm.subst, ihe, ihb]
    rw [Tm.subst_ext (fun n => (upS_ext (up ξ) n).trans (up (upR ξ) n)) b]
  | inl x _ ih | inr _ x ih => simp [Tm.ren, Tm.subst, ih]
  | case e l r ihe ihl ihr =>
    simp only [Tm.ren, Tm.subst, ihe, ihl, ihr]; rw [Tm.subst_ext (up ξ) l, Tm.subst_ext (up ξ) r]
  | let_ e b ihe ihb => simp only [Tm.ren, Tm.subst, ihe, ihb]; rw [Tm.subst_ext (up ξ) b]

theorem upS_var : ∀ n, upS Tm.var n = Tm.var n
  | 0 => rfl
  | _ + 1 => rfl

theorem Tm.subst_var (t : Tm) : t.subst Tm.var = t := by
  induction t with
  | var | glob | unit => rfl
  | lam _ b ih => simp only [Tm.subst]; rw [Tm.subst_ext upS_var b, ih]
  | app f a ihf iha => simp [Tm.subst, ihf, iha]
  | pair a b iha ihb => simp [Tm.subst, iha, ihb]
  | split e b ihe ihb =>
    simp only [Tm.subst, ihe]
    rw [Tm.subst_ext (fun n => (upS_ext upS_var n).trans (upS_var n)) b, ihb]
  | inl x _ ih | inr _ x ih => simp [Tm.subst, ih]
  | case e l r ihe ihl ihr =>
    simp only [Tm.subst, ihe]; rw [Tm.subst_ext upS_var l, Tm.subst_ext upS_var r, ihl, ihr]
  | let_ e b ihe ihb => simp only [Tm.subst, ihe]; rw [Tm.subst_ext upS_var b, ihb]

/-- Under `upS σ`, the body of a binder instantiated with `v` is the body
under `σ` with `v` for the bound variable. -/
theorem Tm.subst_up_inst (b : Tm) (σ : Nat → Tm) (v : Tm) :
    (b.subst (upS σ)).inst v = b.subst (scons v σ) := by
  unfold Tm.inst
  rw [Tm.subst_subst]
  apply Tm.subst_ext
  intro n
  cases n with
  | zero => rfl
  | succ n =>
    simp only [upS, Tm.ren_subst]
    exact Tm.subst_var (σ n)

theorem Tm.subst_up2_inst2 (b : Tm) (σ : Nat → Tm) (v w : Tm) :
    (b.subst (upS (upS σ))).inst2 v w = b.subst (scons w (scons v σ)) := by
  unfold Tm.inst2
  rw [Tm.subst_subst]
  apply Tm.subst_ext
  intro n
  match n with
  | 0 => rfl
  | 1 => rfl
  | n + 2 =>
    simp only [upS, Tm.ren_ren, Tm.ren_subst]
    show (σ n).subst (fun m => scons w (scons v Tm.var) (m + 1 + 1)) = σ n
    exact Tm.subst_var (σ n)

end Flick.PT
