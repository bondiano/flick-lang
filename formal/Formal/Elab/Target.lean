import Formal.Elab.Source
import Formal.Elab.Hyp

/-!
# Elaboration: the target λFlick⁺ as an interface (elaboration.md §0.1, §2.1, §2.2)

The target is abstract: `Target` gives its typing `⊢⁺` of elaborated terms as a field, and
the laws T6 needs as fields too. P4a instantiates it with λFlick⁺ (Р54).

Types of elaborated terms: monotypes `⟦τ⟧ = τ`, arrows `a →φ b` and dictionary types `D_Q`.
A context entry is `∀ᾱ. t`; a record of `D_Q` with `∀`-fields shows only through its laws:
projecting a method instantiates the field's `∀`, projecting a slot gives the superclass.
-/

namespace Flick.Elab

/-- Types of elaborated terms. -/
inductive TTy where
  | mono (t : Ty)
  | arr (φ : Stage) (a b : TTy)
  | dict (q : Cn)

def TTy.subst (σ : Nat → Ty) : TTy → TTy
  | .mono t => .mono (t.subst σ)
  | .arr φ a b => .arr φ (a.subst σ) (b.subst σ)
  | .dict q => .dict (q.subst σ)

/-- `D_{Q₁} →P … →P D_{Qₗ} →P t`. -/
def dicts (qs : List Cn) (t : TTy) : TTy := qs.foldr (fun q t => .arr .P (.dict q) t) t

/-- `∀ᾱ. t` with `k` binders. -/
structure TSch where
  k : Nat
  ty : TTy

def TSch.subst (σ : Nat → Ty) (s : TSch) : TSch := ⟨s.k, s.ty.subst (lift s.k σ)⟩

/-- `⟦∀ᾱ. Q̄ ⇒ τ⟧ = ∀ᾱ. D_Q̄ →P τ`. -/
def Scheme.tr (s : Scheme) : TSch := ⟨s.k, dicts s.ctx (.mono s.ty)⟩

/-- The target. `Has Γ Δ e t φ` is `Γ, ⟦Δ⟧ ⊢⁺ e : t ! φ`, where a premise `δ : Q` of `Δ` is
the variable `δ : D_Q`. -/
structure Target (P : Prog) where
  Has : List TSch → List Cn → Tm → TTy → Stage → Prop
  /-- (Var) with the instantiation of `∀` -/
  var : ∀ {Γ Δ x s ts φ}, Γ[x]? = some s → ts.length = s.k →
    Has Γ Δ (.var x) (s.ty.subst (inst ts)) φ
  app : ∀ {Γ Δ f x a b φ}, Has Γ Δ f (.mono (arrTy φ a b)) φ → Has Γ Δ x (.mono a) φ →
    Has Γ Δ (.app f x) (.mono b) φ
  lam : ∀ {Γ Δ b a t φ ψ}, Has (⟨0, .mono a⟩ :: Γ) Δ b (.mono t) φ →
    Has Γ Δ (.lam b) (.mono (arrTy φ a t)) ψ
  /-- (Let-Sig) of λFlick⁺; `k = 0` is (Let-Mono) -/
  let_ : ∀ {Γ Δ e b k t u φ₁ φ}, Has (Γ.map (·.subst (up k))) (Δ.map (·.subst (up k))) e t φ₁ →
    (k ≠ 0 → φ₁ = .P) → (φ₁ = .P ∨ φ₁ = φ) → Has (⟨k, t⟩ :: Γ) Δ b u φ →
    Has Γ Δ (.let_ e b) u φ
  /-- the abstraction over dictionaries: a pure function of a pure body -/
  lamD : ∀ {Γ Δ qs e t φ}, Has Γ (qs ++ Δ) e t φ → (qs ≠ [] → φ = .P) →
    Has Γ Δ (.lamD qs.length e) (dicts qs t) φ
  /-- applying a dictionary: a pure arrow, at any stage -/
  dapp : ∀ {Γ Δ f x a t φ}, Has Γ Δ f (.arr .P a t) φ → Has Γ Δ x a φ →
    Has Γ Δ (.app f x) t φ
  /-- a method of `D_{C ᾱθ}`: projection and instantiation of `∀β̄` -/
  meth : ∀ {Γ Δ e m C s c₀ rest θ φ}, P.methods m = some (C, s) → s.ctx = c₀ :: rest →
    Has Γ Δ e (.dict (c₀.subst θ)) φ →
    Has Γ Δ (.meth e m) (dicts (rest.map (·.subst θ)) (.mono (s.ty.subst θ))) φ
  dvar : ∀ {Γ Δ k q φ}, Δ[k]? = some q → Has Γ Δ (.dict (.var k)) (.dict q) φ
  /-- the superclass slots of `D_Q` -/
  slot : ∀ {Γ Δ d q i S φ}, Has Γ Δ (.dict d) (.dict q) φ → P.super q i = some S →
    Has Γ Δ (.dict (.slot d i)) (.dict S) φ
  dap : ∀ {Γ Δ f x a t φ}, Has Γ Δ (.dict f) (.arr .P a t) φ → Has Γ Δ (.dict x) a φ →
    Has Γ Δ (.dict (.ap f x)) t φ
  /-- the program's instances: `I : ∀β̄. D_ctx →P D_head`, by (Instance) of §2.5 with the
  slots of К7 -/
  inst : ∀ {Γ Δ I ins σ φ}, P.insts I = some ins →
    Has Γ Δ (.dict (.inst I))
      (dicts (ins.ctx.map (·.subst σ)) (.dict ⟨ins.cls, ins.head.map (·.subst σ)⟩)) φ
  /-- (Struct): `struct_C : D_{C τ₁} →P … →P D_{C τ}` -/
  strct : ∀ {Γ Δ C τ ts φ}, P.structC C = true → P.fields τ = some ts →
    Has Γ Δ (.dict (.strct C)) (dicts (ts.map fun t => ⟨C, [t]⟩) (.dict ⟨C, [τ]⟩)) φ
  /-- (Prim) -/
  prim : ∀ {Γ Δ q p φ}, P.primC q.cls = true → P.prims q = some p →
    Has Γ Δ (.dict (.prim p)) (.dict q) φ

end Flick.Elab
