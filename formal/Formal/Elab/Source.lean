import Formal.Elab.Syntax

/-!
# Elaboration: the source and `Γ; Δ ⊢ e : τ ! φ ⇝ e'` (elaboration.md §2.2, §2.4)

Source types are the first-order `Ty` of constraints; the arrow `a →φ b` is the type
`app (app (con c_φ) a) b` with reserved constructors `c_P = 0`, `c_R = 1`; `Bool` is `con 2`
and `Option a` is `app (con 3) a`. Schemes bind
`var 0 … var (k-1)`; under a binder of `k` variables the context is shifted by `k`, as
`Typ.letGen` does in the core. Terms are Curry-style with de Bruijn variables; a premise
`δ` is an index into `Δ`.
-/

namespace Flick.Elab

/-- The two stages; `P ⊑ R`. -/
inductive Stage where
  | P
  | R
  deriving DecidableEq

/-- The reserved constructor of the arrow of a stage. -/
def Stage.con : Stage → Nat
  | .P => 0
  | .R => 1

/-- `a →φ b`. -/
def arrTy (φ : Stage) (a b : Ty) : Ty := .app (.app (.con φ.con) a) b

def boolTy : Ty := .con 2

def optTy (a : Ty) : Ty := .app (.con 3) a

/-- Shift the free variables by `k`. -/
def up (k : Nat) : Nat → Ty := fun n => .var (n + k)

/-- A substitution under `k` binders. -/
def lift (k : Nat) (σ : Nat → Ty) : Nat → Ty :=
  fun n => if n < k then .var n else (σ (n - k)).subst (up k)

/-- Instantiate the binders of a scheme with `ts`. -/
def inst (ts : List Ty) : Nat → Ty :=
  fun n => if h : n < ts.length then ts[n] else .var (n - ts.length)

def Scheme.subst (σ : Nat → Ty) (s : Scheme) : Scheme :=
  ⟨s.k, s.ctx.map (·.subst (lift s.k σ)), s.ty.subst (lift s.k σ)⟩

/-- Source terms; `letSig σ e b` is `let x : σ = e in b`. -/
inductive Exp where
  | var (x : Nat)
  | meth (m : Nat)
  | const (c : Nat)
  | app (f a : Exp)
  | lam (b : Exp)
  | let_ (e b : Exp)
  | letSig (σ : Scheme) (e b : Exp)
  /-- `if c then a else b` -/
  | ite (c a b : Exp)
  /-- `match e with None → n | Some y → s` -/
  | caseOpt (e n s : Exp)

/-- Elaborated terms: a subset of λFlick⁺. `dict d` is a dictionary term, `meth e m` the
projection of a method, `lamD l e` the abstraction `λδ₁ … δₗ. e` whose `i`-th argument is the
premise `i`. `hole i` is a dictionary position of a skeleton (T6(b)); elaboration does not
produce it. -/
inductive Tm where
  | var (x : Nat)
  | hole (i : Nat)
  | dict (d : Dict)
  | meth (e : Tm) (m : Nat)
  | const (c : Nat)
  | app (f a : Tm)
  | lam (b : Tm)
  | let_ (e b : Tm)
  | lamD (l : Nat) (b : Tm)
  | ite (c a b : Tm)
  | caseOpt (e n s : Tm)

/-- `e d₁ … dₗ`. -/
def Tm.dapps (f : Tm) (ds : List Dict) : Tm := ds.foldl (fun f d => .app f (.dict d)) f

variable (P : Prog)

/-- `Γ; Δ ⊢ e : τ ! φ ⇝ e'` of §2.4. Values take any stage, and an application, a `let` or a
`match` runs at one stage `φ`: the arrow applied and the right side of a `let` run at
`ψ ⊑ φ` (the stage part of (Sub), Р182). -/
inductive Elab : List Scheme → List Cn → Exp → Ty → Stage → Tm → Prop
  /-- (Var-Dict) -/
  | var {Γ Δ x σ ts ds φ} : Γ[x]? = some σ → ts.length = σ.k →
      Ent P Δ (σ.ctx.map (·.subst (inst ts))) ds →
      Elab Γ Δ (.var x) (σ.ty.subst (inst ts)) φ ((Tm.var x).dapps ds)
  /-- (Method) -/
  | meth {Γ Δ m C σ c₀ rest ts d₀ ds φ} : P.methods m = some (C, σ) → σ.ctx = c₀ :: rest →
      c₀.cls = C → ts.length = σ.k → Ent P Δ [c₀.subst (inst ts)] [d₀] →
      Ent P Δ (rest.map (·.subst (inst ts))) ds →
      Elab Γ Δ (.meth m) (σ.ty.subst (inst ts)) φ ((Tm.meth (.dict d₀) m).dapps ds)
  /-- (Const): a constant of Γ₀, with the dictionaries of its scheme, as (Var-Dict) -/
  | const {Γ Δ c σ ts ds φ} : P.consts c = some σ → ts.length = σ.k →
      Ent P Δ (σ.ctx.map (·.subst (inst ts))) ds →
      Elab Γ Δ (.const c) (σ.ty.subst (inst ts)) φ ((Tm.const c).dapps ds)
  | app {Γ Δ f x a b ψ φ f' x'} : Elab Γ Δ f (arrTy ψ a b) φ f' → Elab Γ Δ x a φ x' →
      (ψ = .P ∨ ψ = φ) → Elab Γ Δ (.app f x) b φ (.app f' x')
  | lam {Γ Δ b a t φ ψ b'} : Elab (⟨0, [], a⟩ :: Γ) Δ b t φ b' →
      Elab Γ Δ (.lam b) (arrTy φ a t) ψ (.lam b')
  | let_ {Γ Δ e b a t φ e' b'} : Elab Γ Δ e a φ e' → Elab (⟨0, [], a⟩ :: Γ) Δ b t φ b' →
      Elab Γ Δ (.let_ e b) t φ (.let_ e' b')
  /-- (Let-Sig-Dict), with the premise of Р55 -/
  | letSig {Γ Δ σ e b t φ₁ φ e' b'} :
      Elab (Γ.map (·.subst (up σ.k))) (σ.ctx ++ Δ.map (·.subst (up σ.k))) e σ.ty φ₁ e' →
      (σ.ctx ≠ [] ∨ σ.k ≠ 0 → φ₁ = .P) → (φ₁ = .P ∨ φ₁ = φ) →
      Elab (σ :: Γ) Δ b t φ b' →
      Elab Γ Δ (.letSig σ e b) t φ (.let_ (.lamD σ.ctx.length e') b')
  | ite {Γ Δ c a b t φ c' a' b'} : Elab Γ Δ c boolTy φ c' → Elab Γ Δ a t φ a' →
      Elab Γ Δ b t φ b' → Elab Γ Δ (.ite c a b) t φ (.ite c' a' b')
  | caseOpt {Γ Δ e n s a t φ e' n' s'} : Elab Γ Δ e (optTy a) φ e' → Elab Γ Δ n t φ n' →
      Elab (⟨0, [], a⟩ :: Γ) Δ s t φ s' → Elab Γ Δ (.caseOpt e n s) t φ (.caseOpt e' n' s')

end Flick.Elab
