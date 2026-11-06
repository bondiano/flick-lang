import Formal.Elab.Unify

/-!
# Elaboration: the source and `Γ; Δ ⊢ e : τ ! φ ⇝ e'` (elaboration.md §2.2, §2.4)

Source types are the `Ty` of constraints, with the arrow `a →φ^q b` (`docs/formal/elaboration.md#the-source-arrow-was-a-reserved-constructor`); `Bool` is
`con 2`, `Int` is `con 4` and `Option a` is `app (con 3) a`. Schemes bind `var 0 … var (k-1)`, capture
variables among them; under a binder of `k` variables the context is shifted by `k`, as
`Typ.letGen` does in the core. Terms are Curry-style with de Bruijn variables; a premise `δ`
is an index into `Δ`. (Lam) and (Let-Sig-Dict) show what a closure holds, as (Lam) of
`regions.md` (`docs/formal/elaboration.md#t6a-without-captures-does-not-carry-over-to-the-core-with-regions`); qualifiers match exactly, there is no (≤-Cap).
-/

namespace Flick.Elab

def boolTy : Ty := .con 2

def intTy : Ty := .con 4

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
  ⟨s.k, s.ctx.map (·.subst (lift s.k σ)), s.ty.subst (lift s.k σ), s.q.subst (lift s.k σ)⟩

/-- `⌈s⌉` below its binders: the reach of `⟦s⟧ = ∀ᾱ. D_Q̄ →P^q τ`, as `⌈D_Q⌉ = ∅`. -/
def Scheme.reach (s : Scheme) (n : Nat) : Prop :=
  s.ty.reach (n + s.k) ∨ (s.ctx ≠ [] ∧ s.q.reach (n + s.k))

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
  /-- a constructor of a data type -/
  | con (K : Nat)
  /-- `match e with K y₁ … yₙ → b | _ → r`: an arm on the constructor `K`, binding its `n`
  fields; a match with more arms nests them -/
  | caseK (e : Exp) (K n : Nat) (b r : Exp)

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
  | con (K : Nat)
  /-- the arm binds the `n` fields of `K` as variables and its `m` dictionaries as premises -/
  | caseK (e : Tm) (K n m : Nat) (b r : Tm)

/-- `x` is free in `e`. -/
def Exp.Free : Exp → Nat → Prop
  | .var y, x => y = x
  | .meth _, _ => False
  | .const _, _ => False
  | .app f a, x => f.Free x ∨ a.Free x
  | .lam b, x => b.Free (x + 1)
  | .let_ e b, x => e.Free x ∨ b.Free (x + 1)
  | .letSig _ e b, x => e.Free x ∨ b.Free (x + 1)
  | .ite c a b, x => c.Free x ∨ a.Free x ∨ b.Free x
  | .caseOpt e n s, x => e.Free x ∨ n.Free x ∨ s.Free (x + 1)
  | .con _, _ => False
  | .caseK e _ n b r, x => e.Free x ∨ b.Free (x + n) ∨ r.Free x

/-- `x` is free in `e`; `lamD` binds premises, not term variables. -/
def Tm.Free : Tm → Nat → Prop
  | .var y, x => y = x
  | .hole _, _ => False
  | .dict _, _ => False
  | .meth e _, x => e.Free x
  | .const _, _ => False
  | .app f a, x => f.Free x ∨ a.Free x
  | .lam b, x => b.Free (x + 1)
  | .let_ e b, x => e.Free x ∨ b.Free (x + 1)
  | .lamD _ b, x => b.Free x
  | .ite c a b, x => c.Free x ∨ a.Free x ∨ b.Free x
  | .caseOpt e n s, x => e.Free x ∨ n.Free x ∨ s.Free (x + 1)
  | .con _, _ => False
  | .caseK e _ n _ b r, x => e.Free x ∨ b.Free (x + n) ∨ r.Free x

/-- What `e` holds in `Γ`: the reach of its free variables. `Δ` adds nothing: `⌈D_Q⌉ = ∅`. -/
def heldS (Γ : List Scheme) (e : Exp) (n : Nat) : Prop :=
  ∃ x s, e.Free x ∧ Γ[x]? = some s ∧ s.reach n

/-- `e d₁ … dₗ`. -/
def Tm.dapps (f : Tm) (ds : List Dict) : Tm := ds.foldl (fun f d => .app f (.dict d)) f

/-- The premises from `L` on go to `ρ`; those below `L` are bound inside the term. -/
def dsub (L : Nat) (ρ : Nat → Dict) : Nat → Dict := fun k => if k < L then .var k else ρ (k - L)

/-- `e[ρ]` for the premises from `L` on. -/
def Tm.dsubst (ρ : Nat → Dict) : Nat → Tm → Tm
  | _, .var x => .var x
  | _, .hole i => .hole i
  | L, .dict d => .dict (d.subst (dsub L ρ))
  | L, .meth e m => .meth (e.dsubst ρ L) m
  | _, .const c => .const c
  | L, .app f a => .app (f.dsubst ρ L) (a.dsubst ρ L)
  | L, .lam b => .lam (b.dsubst ρ L)
  | L, .let_ e b => .let_ (e.dsubst ρ L) (b.dsubst ρ L)
  | L, .lamD l b => .lamD l (b.dsubst ρ (l + L))
  | L, .ite c a b => .ite (c.dsubst ρ L) (a.dsubst ρ L) (b.dsubst ρ L)
  | L, .caseOpt e n s => .caseOpt (e.dsubst ρ L) (n.dsubst ρ L) (s.dsubst ρ L)
  | _, .con K => .con K
  | L, .caseK e K n m b r => .caseK (e.dsubst ρ L) K n m (b.dsubst ρ (m + L)) (r.dsubst ρ L)

theorem Tm.dsubst_dapps (ρ : Nat → Dict) (L : Nat) (f : Tm) (ds : List Dict) :
    (f.dapps ds).dsubst ρ L = (f.dsubst ρ L).dapps (ds.map (·.subst (dsub L ρ))) := by
  induction ds generalizing f with
  | nil => rfl
  | cons d ds ih => exact ih (.app f (.dict d))

theorem Tm.free_dsubst {ρ : Nat → Dict} {e : Tm} {L x : Nat} (h : (e.dsubst ρ L).Free x) :
    e.Free x := by
  induction e generalizing L x with
  | var | hole | dict | const | con => exact h
  | meth _ _ ih | lam _ ih | lamD _ _ ih => exact ih h
  | app _ _ ih₁ ih₂ | let_ _ _ ih₁ ih₂ => exact h.imp ih₁ ih₂
  | ite _ _ _ ih₁ ih₂ ih₃ | caseOpt _ _ _ ih₁ ih₂ ih₃ | caseK _ _ _ _ _ _ ih₁ ih₂ ih₃ =>
    exact h.imp ih₁ (Or.imp ih₂ ih₃)

/-- A dictionary whose premises lie below `L`. -/
def Dict.Below (L : Nat) : Dict → Prop
  | .var k => k < L
  | .slot d _ => d.Below L
  | .ap d e => d.Below L ∧ e.Below L
  | _ => True

theorem Dict.subst_below {L : Nat} {ρ : Nat → Dict} : ∀ {d : Dict}, d.Below L →
    d.subst (dsub L ρ) = d
  | .var k, h => by simp [Dict.subst, dsub, show k < L from h]
  | .slot d i, h => by simp [Dict.subst, Dict.subst_below (d := d) h]
  | .ap d e, h => by simp [Dict.subst, Dict.subst_below h.1, Dict.subst_below h.2]
  | .inst _, _ | .strct _ _, _ | .prim _ _, _ => rfl

/-- The premises `e` names lie below `L`, past the premises bound inside it. -/
def Tm.DB : Nat → Tm → Prop
  | L, .dict d => d.Below L
  | L, .meth e _ => e.DB L
  | L, .app f a => f.DB L ∧ a.DB L
  | L, .lam b => b.DB L
  | L, .let_ e b => e.DB L ∧ b.DB L
  | L, .lamD l b => b.DB (l + L)
  | L, .ite c a b => c.DB L ∧ a.DB L ∧ b.DB L
  | L, .caseOpt e n s => e.DB L ∧ n.DB L ∧ s.DB L
  | L, .caseK e _ _ m b r => e.DB L ∧ b.DB (m + L) ∧ r.DB L
  | _, _ => True

theorem Tm.dsubst_of_DB {ρ : Nat → Dict} : ∀ {e : Tm} {L : Nat}, e.DB L → e.dsubst ρ L = e
  | .var _, _, _ | .hole _, _, _ | .const _, _, _ | .con _, _, _ => rfl
  | .dict _, _, h => by simp [Tm.dsubst, Dict.subst_below h]
  | .meth e _, _, h => by simp [Tm.dsubst, Tm.dsubst_of_DB (e := e) h]
  | .app f a, _, h => by simp [Tm.dsubst, Tm.dsubst_of_DB (e := f) h.1, Tm.dsubst_of_DB (e := a) h.2]
  | .lam b, _, h => by simp [Tm.dsubst, Tm.dsubst_of_DB (e := b) h]
  | .let_ e b, _, h => by
    simp [Tm.dsubst, Tm.dsubst_of_DB (e := e) h.1, Tm.dsubst_of_DB (e := b) h.2]
  | .lamD _ b, _, h => by simp [Tm.dsubst, Tm.dsubst_of_DB (e := b) h]
  | .ite c a b, _, h => by
    simp [Tm.dsubst, Tm.dsubst_of_DB (e := c) h.1, Tm.dsubst_of_DB (e := a) h.2.1,
      Tm.dsubst_of_DB (e := b) h.2.2]
  | .caseOpt e n s, _, h => by
    simp [Tm.dsubst, Tm.dsubst_of_DB (e := e) h.1, Tm.dsubst_of_DB (e := n) h.2.1,
      Tm.dsubst_of_DB (e := s) h.2.2]
  | .caseK e _ _ _ b r, _, h => by
    simp [Tm.dsubst, Tm.dsubst_of_DB (e := e) h.1, Tm.dsubst_of_DB (e := b) h.2.1,
      Tm.dsubst_of_DB (e := r) h.2.2]

theorem Dict.subst_congr_below {ρ ρ' : Nat → Dict} {M : Nat} (h : ∀ k, k < M → ρ k = ρ' k) :
    ∀ {d : Dict}, d.Below M → d.subst ρ = d.subst ρ'
  | .var k, hd => h k hd
  | .slot d _, hd => by simp [Dict.subst, Dict.subst_congr_below h (d := d) hd]
  | .ap d e, hd => by
    simp [Dict.subst, Dict.subst_congr_below h (d := d) hd.1, Dict.subst_congr_below h (d := e) hd.2]
  | .inst _, _ | .strct _ _, _ | .prim _ _, _ => rfl

/-- Closing the premises from `L` on reads only the dictionaries of the premises named. -/
theorem Tm.dsubst_congr {ρ ρ' : Nat → Dict} {M : Nat} (h : ∀ k, k < M → ρ k = ρ' k) :
    ∀ {e : Tm} {L : Nat}, e.DB (L + M) → e.dsubst ρ L = e.dsubst ρ' L
  | .var _, _, _ | .hole _, _, _ | .const _, _, _ | .con _, _, _ => rfl
  | .dict _, L, hd => by
    simp only [Tm.dsubst]
    rw [Dict.subst_congr_below (ρ := dsub L ρ) (ρ' := dsub L ρ') (M := L + M) (fun k hk => by
      simp only [dsub]
      by_cases hkL : k < L
      · simp [hkL]
      · simp only [hkL, if_false]; exact h _ (by omega)) hd]
  | .meth e _, _, hd => by simp [Tm.dsubst, Tm.dsubst_congr h (e := e) hd]
  | .app f a, _, hd => by
    simp [Tm.dsubst, Tm.dsubst_congr h (e := f) hd.1, Tm.dsubst_congr h (e := a) hd.2]
  | .lam b, _, hd => by simp [Tm.dsubst, Tm.dsubst_congr h (e := b) hd]
  | .let_ e b, _, hd => by
    simp [Tm.dsubst, Tm.dsubst_congr h (e := e) hd.1, Tm.dsubst_congr h (e := b) hd.2]
  | .lamD l b, L, hd => by
    simp only [Tm.dsubst]
    rw [Tm.dsubst_congr h (e := b) (L := l + L) (by rwa [Nat.add_assoc])]
  | .ite c a b, _, hd => by
    simp [Tm.dsubst, Tm.dsubst_congr h (e := c) hd.1, Tm.dsubst_congr h (e := a) hd.2.1,
      Tm.dsubst_congr h (e := b) hd.2.2]
  | .caseOpt e n s, _, hd => by
    simp [Tm.dsubst, Tm.dsubst_congr h (e := e) hd.1, Tm.dsubst_congr h (e := n) hd.2.1,
      Tm.dsubst_congr h (e := s) hd.2.2]
  | .caseK e _ _ m b r, L, hd => by
    simp only [Tm.dsubst]
    rw [Tm.dsubst_congr h (e := e) hd.1, Tm.dsubst_congr h (e := r) hd.2.2,
      Tm.dsubst_congr h (e := b) (L := m + L) (by rw [Nat.add_assoc]; exact hd.2.1)]

/-- `K : ∀ᾱ. Q̄ ⇒ τ̄ → T ρ̄` as a type: `τ₁ →P^∅ … →P^∅ T ρ̄`. -/
def Ctor.ty (c : Ctor) : Ty := c.fields.foldr (fun a r => .arr .P a r .cnil) (Ty.tcon c.T c.res)

/-- The equations of an arm on `c` at `T us`: what `c` builds against `us`, both past the
variables of `c`. -/
def Ctor.eqs (c : Ctor) (us : List Ty) : Eqs := c.res.zip (us.map (·.subst (up c.k)))

/-- The context of an arm on `c` under `θ`: its fields, then `Γ` past the variables of `c`. -/
def armS (c : Ctor) (θ : Nat → Ty) (Γ : List Scheme) : List Scheme :=
  c.fields.map (fun t => (⟨0, [], t.subst θ, .cnil⟩ : Scheme)) ++ (Γ.map (·.subst (up c.k))).map (·.subst θ)

/-- The premises of an arm on `c` under `θ`: the dictionaries of the value, then `Δ`. -/
def armΔ (c : Ctor) (θ : Nat → Ty) (Δ : List Cn) : List Cn :=
  c.ctx.map (·.subst θ) ++ (Δ.map (·.subst (up c.k))).map (·.subst θ)

/-- The body of an arm that no value reaches. -/
def Tm.dead : Tm := .lam (.var 0)

variable (P : Prog)

/-- `Γ; Δ ⊢ e : τ ! φ ⇝ e'` of §2.4. Values take any stage, and an application, a `let` or a
`match` runs at one stage `φ`: the arrow applied and the right side of a `let` run at
`ψ ⊑ φ` (the stage part of (Sub), `docs/formal/elaboration.md#let-sig-dict-lost-the-effect-of-the-right-hand-side`). A closure shows what it holds in its arrow, a
function of dictionaries in the qualifier of its scheme (`docs/formal/elaboration.md#the-dictionary-function-of-let-sig-dict-holds-what-the-right-hand-side-holds`). -/
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
  | app {Γ Δ f x a b q ψ φ f' x'} : Elab Γ Δ f (.arr ψ a b q) φ f' → Elab Γ Δ x a φ x' →
      (ψ = .P ∨ ψ = φ) → Elab Γ Δ (.app f x) b φ (.app f' x')
  | lam {Γ Δ b a t q φ ψ b'} : Elab (⟨0, [], a, .cnil⟩ :: Γ) Δ b t φ b' →
      (∀ n, heldS Γ (.lam b) n → (Ty.arr φ a t q).reach n) →
      Elab Γ Δ (.lam b) (.arr φ a t q) ψ (.lam b')
  | let_ {Γ Δ e b a t φ e' b'} : Elab Γ Δ e a φ e' → Elab (⟨0, [], a, .cnil⟩ :: Γ) Δ b t φ b' →
      Elab Γ Δ (.let_ e b) t φ (.let_ e' b')
  /-- (Let-Sig-Dict), with the premise that the right-hand side is pure -/
  | letSig {Γ Δ σ e b t φ₁ φ e' b'} :
      Elab (Γ.map (·.subst (up σ.k))) (σ.ctx ++ Δ.map (·.subst (up σ.k))) e σ.ty φ₁ e' →
      (σ.ctx ≠ [] ∨ σ.k ≠ 0 → φ₁ = .P) → (φ₁ = .P ∨ φ₁ = φ) →
      (σ.ctx ≠ [] → ∀ n, heldS (Γ.map (·.subst (up σ.k))) e n → σ.ty.reach n ∨ σ.q.reach n) →
      Elab (σ :: Γ) Δ b t φ b' →
      Elab Γ Δ (.letSig σ e b) t φ (.let_ (.lamD σ.ctx.length e') b')
  | ite {Γ Δ c a b t φ c' a' b'} : Elab Γ Δ c boolTy φ c' → Elab Γ Δ a t φ a' →
      Elab Γ Δ b t φ b' → Elab Γ Δ (.ite c a b) t φ (.ite c' a' b')
  | caseOpt {Γ Δ e n s a t φ e' n' s'} : Elab Γ Δ e (optTy a) φ e' → Elab Γ Δ n t φ n' →
      Elab (⟨0, [], a, .cnil⟩ :: Γ) Δ s t φ s' → Elab Γ Δ (.caseOpt e n s) t φ (.caseOpt e' n' s')
  /-- (Con): a constructor with the dictionaries of its constraints, as (Const) -/
  | con {Γ Δ K c ts ds φ} : P.ctors K = some c → ts.length = c.k →
      Ent P Δ (c.ctx.map (·.subst (inst ts))) ds →
      Elab Γ Δ (.con K) (c.ty.subst (inst ts)) φ ((Tm.con K).dapps ds)
  /-- (Case): the arm on `K` under the most general unifier `θ` of what `K` builds with the
  matched type `T us`; the fields of the value are its variables, its dictionaries its
  premises -/
  | caseK {Γ Δ e K n b r T us t φ c θ e' b' r'} : Elab Γ Δ e (Ty.tcon T us) φ e' →
      P.ctors K = some c → c.T = T → us.length = c.res.length → n = c.fields.length →
      Unify (c.eqs us) θ →
      Elab (armS c θ Γ) (armΔ c θ Δ) b ((t.subst (up c.k)).subst θ) ((φ.subst (up c.k)).subst θ) b' →
      Elab Γ Δ r t φ r' → Elab Γ Δ (.caseK e K n b r) t φ (.caseK e' K n c.ctx.length b' r')
  /-- (Case-Dead): `K` builds no value of the matched type; the arm is not elaborated -/
  | caseKDead {Γ Δ e K n b r T us t φ c e' r'} : Elab Γ Δ e (Ty.tcon T us) φ e' →
      P.ctors K = some c → c.T = T → us.length = c.res.length → n = c.fields.length →
      Clash (c.eqs us) → Elab Γ Δ r t φ r' →
      Elab Γ Δ (.caseK e K n b r) t φ (.caseK e' K n c.ctx.length .dead r')

end Flick.Elab
