import Formal.Reach
import Formal.DictOps

/-!
# Values, patterns, shapes and subtyping (core.md §1.3, §2.2, §2.4)
-/

namespace Flick

/-- Values `v` of core.md §1.3 and §3.1. A partially applied constructor is a
spine with a `ctor` head (`docs/formal/core.md#a-partial-constructor-application-is-a-value`). A constant of arity 2 applied once is a value,
and so is a saturated `Layer.*` application. -/
inductive Value : Tm → Prop
  | lam : Value (.lam b)
  | fix : Value (.fix f)
  | lit : Value (.lit l)
  | loc : Value (.loc l)
  | const : Value (.const c)
  | ctor : Value (.ctor K)
  | rnil : Value .rnil
  | rcons : Value e → Value r → Value (.rcons l e r)
  | ctorApp : Value f → f.head = .ctor K → Value e → Value (.app f e)
  | constApp : (c.arity = 2 ∨ c.layer) → Value e → Value (.app (.const c) e)
  | constApp2 : c.arity = 2 → c.layer → Value e₁ → Value e₂ →
      Value (.app (.app (.const c) e₁) e₂)
  | task : Value (.task e)
  | enil : Value .enil
  | econs : Value v → Value θ → Value (.econs S v θ)
  | cap : Value (.cap p)
  | own : Value (.own p)
  | drec : Value (.drec c r)
  | kont : Value (.kont p)

/-- Environment values `env θ` of core.md §3.1. -/
inductive IsEnv : Tm → Prop
  | enil : IsEnv .enil
  | econs : Value v → IsEnv θ → IsEnv (.econs S v θ)

theorem IsEnv.value {θ : Tm} (h : IsEnv θ) : Value θ := by
  induction h with
  | enil => exact .enil
  | econs hv _ ih => exact .econs hv ih

/-- `match(p, v)` of core.md §3.3: the values bound by `p`, left to right. -/
def matchP : Pat → Tm → Option (List Tm)
  | .wild, _ => some []
  | .var, v => some [v]
  | .lit l, .lit l' => if l = l' then some [] else none
  | .ctor K, .ctor K' => if K = K' then some [] else none
  | .app p q, .app f e => do
    let a ← matchP p f
    let b ← matchP q e
    pure (a ++ b)
  | .rany, .rnil => some []
  | .rany, .rcons .. => some []
  | .rfield l p r, v => do
    let x ← v.field l
    let a ← matchP p x
    let b ← matchP r v
    pure (a ++ b)
  | .or p q, v => (matchP p v).or (matchP q v)
  | _, _ => none

/-- Subtyping `τ ≤ τ'` of core.md §2.2 with (≤-Cap) of regions.md §2.2: only
arrows and `Eff` have it, and the whole value is compared by `⌈·⌉`. -/
inductive Le : Ty → Ty → Prop
  | refl : Le t t
  | arrow : Le a' a → Stage.le φ φ' → Le t t' →
      Caps.Sub (Ty.arrow a φ t C).reach (Ty.arrow a' φ' t' C').reach →
      Le (.arrow a φ t C) (.arrow a' φ' t' C')
  | eff : Row.Sub r r' → Row.Sub e e' → Le t t' →
      Caps.Sub (Ty.eff r e t C).reach (Ty.eff r' e' t' C').reach →
      Le (.eff r e t C) (.eff r' e' t' C')

/-- Types whose values patterns cannot inspect: all but base types, records and
nominal data, read at the head of an application. -/
def Ty.Opaque (t : Ty) : Prop :=
  match t.hd with
  | .base _ | .adt _ | .rnil | .rcons .. => False
  | _ => True

theorem Ty.reach_tcon_arg {T : Name} {us : List Ty} {u : Ty} (hu : u ∈ us) :
    Caps.Sub u.reach (Ty.tcon T us).reach := by
  have key : ∀ (us : List Ty) (f : Ty), Caps.Sub f.reach (us.foldl .tapp f).reach ∧
      ∀ u ∈ us, Caps.Sub u.reach (us.foldl .tapp f).reach := by
    intro us
    induction us with
    | nil => intro f; exact ⟨Caps.Sub.refl _, by simp⟩
    | cons x xs ih =>
      intro f
      obtain ⟨h₁, h₂⟩ := ih (.tapp f x)
      refine ⟨fun a h => h₁ a (.inl h), fun u hu => ?_⟩
      rcases List.mem_cons.mp hu with rfl | hu
      · exact fun a h => h₁ a (.inr h)
      · exact h₂ u hu
  exact (key us _).2 u hu

/-- A field of a constructor at an instance reaches no more than the instance. -/
theorem Sig.field_reach (S : Sig) {K k ts T} (hK : S.ctor K = some (k, ts, T)) {us : List Ty}
    (hk : us.length = k) : ∀ f ∈ ts, Caps.Sub (f.substT (instT us)).reach (Ty.tcon T us).reach := by
  intro f hf a ha
  obtain ⟨b, hb, h⟩ := (Ty.reach_substT _ _ _).mp ha
  obtain ⟨i, hi, rfl⟩ := S.ctor_reach K k ts T hK f hf b hb
  have hi' : i < us.length := by omega
  simp only [instT, hi', dif_pos] at h
  exact Ty.reach_tcon_arg (List.getElem_mem hi') a h

/-- The fields of a constructor that builds `T rs`, at the instance `us`, reach no
more than the value it builds: what a field holds whose type the result does not
mention is in a capture-row argument of the result, the value's hidden capture row. -/
def Sig.Holds (ts : List Ty) (T : Name) (rs us : List Ty) : Prop :=
  ∀ f ∈ ts, Caps.Sub (f.substT (instT us)).reach (Ty.tcon T (rs.map (Ty.substT (instT us)))).reach

/-- Each of the `k` variables of a constructor that builds `T rs` is an argument of
`rs`: the constructor hides no type, and the instance of what it builds fixes its
fields. -/
def Sig.Inj (k : Nat) (rs : List Ty) : Prop := ∀ i, i < k → ∃ j : Nat, rs[j]? = some (Ty.tvar i)

theorem Sig.holds_plain (S : Sig) {K k ts T} (hK : S.ctor K = some (k, ts, T)) {us : List Ty}
    (hres : S.res K = idTs k) (hk : us.length = k) : Sig.Holds ts T (S.res K) us := by
  intro f hf
  rw [hres, idTs_inst hk]
  exact S.field_reach hK hk f hf

theorem Sig.inj_plain {k : Nat} : Sig.Inj k (idTs k) :=
  fun i hi => ⟨i, by simp [idTs, hi]⟩

/-- The instance of a constructor that hides no type is the instance of what it builds. -/
theorem Sig.inst_inj {k : Nat} {rs us us' : List Ty} (hj : Sig.Inj k rs) (hk : us.length = k)
    (hk' : us'.length = k)
    (h : rs.map (Ty.substT (instT us)) = rs.map (Ty.substT (instT us'))) : us = us' := by
  apply List.ext_getElem (by rw [hk, hk'])
  intro i h₁ h₂
  obtain ⟨j, hj⟩ := hj i (by omega)
  have := congrArg (·[j]?) h
  simp only [List.getElem?_map, hj, Option.map_some, Option.some.injEq, Ty.substT, instT] at this
  simpa [h₁, h₂] using this

variable (S : Sig)

/-- `⊢ p : τ ⇒ Δ` of core.md §2.4; the flag is `false` for an unsaturated
constructor spine. -/
inductive PatTy : Bool → Pat → Ty → List Ty → Prop
  | wild : PatTy true .wild t []
  | var : PatTy true .var t [t]
  | lit : PatTy true (.lit l) (.base l.base) []
  | head {ts us : List Ty} : S.ctor K = some (k, ts, T) → Sig.Inj k (S.res K) → us.length = k →
      PatTy false (.ctor K) (Sig.ctorTyR ts T (S.res K) us) []
  | app : PatTy false p (.arrow a .pure t Caps.none) Δ₁ → PatTy true q a Δ₂ →
      PatTy false (.app p q) t (Δ₁ ++ Δ₂)
  | adt : PatTy false p U Δ → U.hd = .adt T → PatTy true p U Δ
  | rany : PatTy true .rany R []
  | rfield : R.field l = some t → PatTy true p t Δ₁ → PatTy true r R Δ₂ →
      PatTy true (.rfield l p r) R (Δ₁ ++ Δ₂)
  | or : PatTy true p t Δ → PatTy true q t Δ → PatTy true (.or p q) t Δ

/-- A substitution that fits a term: it keeps the regions the term writes and the
`provide` regions of its `with` frames, and if the term has a run-time form, it brings
in no run-time region, sends each atom to an atom or to a type without one and brings no
name into a row. -/
def Fits (σ : Atom → Ty) (e : Tm) : Prop :=
  (∀ p, (e.Has (.rgn p) ∨ e.Has (.prov p)) → σ (.r p) = .rg p) ∧
    (e.Has .ins → ∀ b p, (σ b).reach (.r p) → b = .r p) ∧
    (e.Has .ins → ∀ b a, (σ b).reach a → σ b = a.ty) ∧
    (e.Has .ins → NoNames σ)

theorem Fits.rename_iff (σ : Atom → Ty) (ξ : Nat → Nat) (e : Tm) :
    Fits σ (e.rename ξ) ↔ Fits σ e := by
  simp only [Fits, Tm.has_rename]

/-- The patterns of a constructor's fields at the fields' types, and what they bind. -/
inductive PatsTy : List Pat → List Ty → List Ty → Prop
  | nil : PatsTy [] [] []
  | cons {q qs a us Δ₁ Δ₂} : PatTy S true q a Δ₁ → PatsTy qs us Δ₂ →
      PatsTy (q :: qs) (a :: us) (Δ₁ ++ Δ₂)

/-- The head of an ordinary constructor's pattern, at `T us`. -/
theorem PatTy.head_plain {K k ts T us} (hK : S.ctor K = some (k, ts, T)) (hres : S.res K = idTs k)
    (hk : us.length = k) : PatTy S false (.ctor K) (Sig.ctorTy ts T us) [] := by
  have h := PatTy.head (S := S) hK (by rw [hres]; exact Sig.inj_plain) hk
  rwa [Sig.ctorTyR_plain hres hk] at h

/-- `K` can build a value of `t = T us`: what it builds unifies with `t` under some
instance of its variables and of the ones around, regions included. Exhaustiveness
needs no arm on a constructor that cannot. -/
def Sig.Builds (S : Sig) (K : Name) (t : Ty) : Prop :=
  ∀ k ts T us, S.ctor K = some (k, ts, T) → t = Ty.tcon T us → us.length = (S.res K).length →
    ∃ ρ : Atom → Ty, (Ty.tcon T (S.res K)).substT ρ = (t.up k).substT ρ

/-- Shapes of closed values of a type, as far as patterns see them. The fields of
a constructor have the types of its declaration: a field of a parameter type is
opaque. -/
inductive Shp : Bool → Ty → Tm → Prop
  | hidden : t.Opaque → Shp true t v
  | lit : Shp true (.base l.base) (.lit l)
  | rnil : Shp true .rnil .rnil
  | rcons : Shp true t e → Shp true R r → Shp true (.rcons l t R) (.rcons l e r)
  | head {ts : List Ty} : S.ctor K = some (k, ts, T) →
      Shp false (.arrows ts (.tcon T (S.res K))) (.ctor K)
  | app : Shp false (.arrow a .pure t Caps.none) f → Shp true a e → Shp false t (.app f e)
  | adt : Shp false U v → U.hd = .adt T → T'.hd = .adt T → (∀ K, v.head = .ctor K → S.Builds K T') →
      Shp true T' v

/-- `Exh(τ, Π)` of core.md §2.4. -/
def Exh (t : Ty) (ps : List Pat) : Prop :=
  ∀ v, Shp S true t v → ∃ p ∈ ps, (matchP p v).isSome

/-- The coverage premise of (Match), or of (Match-Val) (`docs/formal/core.md#the-residual-match-after-a-false-guard-fails-exh`). -/
def Covers (t : Ty) (ps : List Pat) (e : Tm) : Prop :=
  Exh S t ps ∨ (Value e ∧ ∃ p ∈ ps, (matchP p e).isSome)

/-- Patterns of a `try` arm (core.md §2.3 (Try)): `x` or `_` at `Errors ε_b`,
or a constructor pattern `E p̄` at the error type `E`. -/
inductive HPat (εb : Row) : Pat → Ty → List Ty → Prop
  | wild : HPat εb .wild (.errors εb) []
  | var : HPat εb .var (.errors εb) [.errors εb]
  | ctor : PatTy S false p (.adt T) Δ → HPat εb p (.adt T) Δ

/-- The errors an arm without `when` removes: the set `H` of (Try). An `x` or `_` arm
removes all of `ε_b`, its row variables and atoms too; a constructor arm the name of its
error type. -/
def Catches (εb : Row) (p : Pat) (θ : Ty) : Elem → Prop
  | .nm n => (θ = .errors εb ∧ εb n) ∨ (θ = .adt n ∧ Exh S (.adt n) [p])
  | x => θ = .errors εb ∧ εb x

/-- The error type of the raised value `v` is removed by the arms or let
through: the value form of `ε_b ⊆ H + ε'`. -/
def HCov (v : Tm) (H ε' : Row) : Prop :=
  ∀ K k ts T, v.head = .ctor K → S.ctor K = some (k, ts, T) → H T ∨ ε' T

end Flick
