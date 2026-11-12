import Formal.Rules
import Formal.Live.Order

/-!
# Signature elision of the live discipline

The model (`flick-model/src/check/regions/live.rs`, `elide`) reads a signature of the live
discipline with `Live` of what the parameters of each stage, and of the stages before it,
reach, in the row of every runtime arrow and `Eff` after them. `Ty.elide H t` is that
reading in the core, `Live C` being `Row.ofCaps C` (`Stage.use`).

Elision changes no reach of a type without `bare` (`Ty.reach_elide`) and never shrinks one
(`Ty.reach_le_elide`), so it drops nothing a value holds. It also reads the hidden row of data
with hidden storage that a signature writes bare (regions.md §1.4): `bare` as an argument of the
outer spine of a result is `caps` of what the parameters before it reach, in a parameter a fresh
variable `Scheme.elide` quantifies, and anywhere else `∅`. Its
reading of a constant is a supertype of the constant's live type (`Const.ty_le_elide`), so
the core types a constant at its elided type as well (`Typ.const_elided`): a call the model
types through an elided signature of a constant is a call the core types by (Sub). The
instance types are asked to be fixed by elision, also under the binder of `scope`
(`CArg.Fixed`), which a type without runtime arrows and `Eff` is. `K` is what a reach counts:
the model counts regions and capture rows, and for each scheme of the Prelude its `elide` is
`Ty.elide` at the binders of those kinds (`Live/ElideSchemes.lean`, generated).
-/

namespace Flick

/-- `H` under `k` binders. -/
def Caps.under (k : Nat) (H : Caps) : Caps := fun a => ∃ b, H b ∧ a = b.up k

/-- `t` with `Live` of `H` and of the atoms of `K` that the parameters before each stage
reach. The model counts regions and capture rows (`shown`); under a `∀` `K` counts no binder of
it, and the Prelude schemes keep none of those kinds there; `K := ⊤` counts every atom. -/
def Ty.elide (K : Caps) : Ty → Caps → Ty
  | .arrow a φ b C, H =>
    .arrow (a.elide K Caps.none) (Stage.use φ fun x => H x ∨ K x ∧ (a.elide K Caps.none).reach x)
      (b.elide K fun x => H x ∨ K x ∧ (a.elide K Caps.none).reach x) C
  | .eff r e t C, H => .eff (Row.union r (Row.ofCaps H)) e (t.elide K Caps.none) C
  | .all k t, H => .all k (t.elide (Caps.under k K) (Caps.under k H))
  | .ref t, _ => .ref (t.elide K Caps.none)
  | .tapp f .bare, H => .tapp (f.elide K H) (.caps H)
  | .tapp f a, H => .tapp (f.elide K H) (a.elide K Caps.none)
  | .rcons l t r, _ => .rcons l (t.elide K Caps.none) (r.elide K Caps.none)
  | .bare, _ => .caps Caps.none
  | t, _ => t

/-- A type that writes no `bare`. -/
def Ty.noBare : Ty → Bool
  | .arrow a _ b _ => a.noBare && b.noBare
  | .ref t | .eff _ _ t _ | .hnd t | .owner t | .dict _ t | .all _ t => t.noBare
  | .tapp f a | .rcons _ f a => f.noBare && a.noBare
  | .bare => false
  | .tvar _ | .rg _ | .caps _ | .row _ | .base _ | .adt _ | .rnil | .errors _ => true

/-- Elision of a type application whose argument is not `bare`. -/
theorem Ty.elide_tapp {K H : Caps} {f a : Ty} (h : a ≠ .bare) :
    (Ty.tapp f a).elide K H = .tapp (f.elide K H) (a.elide K Caps.none) := by
  cases a <;> first | exact (h rfl).elim | rfl

/-- Elision never shrinks what a value of the type holds. -/
theorem Ty.reach_le_elide : ∀ (K : Caps) (t : Ty) (H : Caps), Caps.Sub t.reach (t.elide K H).reach
  | K, .arrow a φ b C, H => fun x hx => by
    simp only [Ty.elide, Ty.reach] at hx ⊢
    exact hx.imp (Ty.reach_le_elide K a _ x) (Or.imp (Ty.reach_le_elide K b _ x) id)
  | K, .eff r e t C, H => fun x hx => by
    simp only [Ty.elide, Ty.reach] at hx ⊢; exact hx.imp_left (Ty.reach_le_elide K t _ x)
  | K, .all k t, H => fun x hx => by
    simp only [Ty.elide, Ty.reach] at hx ⊢; exact Ty.reach_le_elide _ t _ _ hx
  | K, .ref t, _ => fun x hx => by
    simp only [Ty.elide, Ty.reach] at hx ⊢; exact Ty.reach_le_elide K t _ x hx
  | K, .tapp f a, H => fun x hx => by
    by_cases ha : a = .bare
    · subst ha
      simp only [Ty.elide, Ty.reach, Caps.none] at hx ⊢
      exact hx.elim (fun h => .inl (Ty.reach_le_elide K f _ x h)) False.elim
    · rw [Ty.elide_tapp ha]
      simp only [Ty.reach] at hx ⊢
      exact hx.imp (Ty.reach_le_elide K f _ x) (Ty.reach_le_elide K a _ x)
  | K, .rcons l t r, _ => fun x hx => by
    simp only [Ty.elide, Ty.reach] at hx ⊢
    exact hx.imp (Ty.reach_le_elide K t _ x) (Ty.reach_le_elide K r _ x)
  | _, .bare, _ => fun _ h => (h : False).elim
  | _, .tvar _, _ | _, .rg _, _ | _, .caps _, _ | _, .row _, _ | _, .base _, _ | _, .adt _, _
  | _, .rnil, _ | _, .errors _, _ | _, .hnd _, _ | _, .owner _, _ | _, .dict _ _, _ =>
    fun _ h => h

/-- Elision of a type without `bare` changes nothing a value of it holds. -/
theorem Ty.reach_elide : ∀ (K : Caps) (t : Ty) (H : Caps), t.noBare = true →
    (t.elide K H).reach = t.reach
  | K, .arrow a φ b C, H, h => by
    simp only [Ty.noBare, Bool.and_eq_true] at h
    funext x; simp only [Ty.elide, Ty.reach, Ty.reach_elide K a _ h.1, Ty.reach_elide K b _ h.2]
  | K, .eff r e t C, H, h => by
    funext x; simp only [Ty.elide, Ty.reach, Ty.reach_elide K t _ h]
  | K, .all k t, H, h => by funext x; simp only [Ty.elide, Ty.reach, Ty.reach_elide _ t _ h]
  | K, .ref t, _, h => by simp only [Ty.elide, Ty.reach, Ty.reach_elide K t _ h]
  | K, .tapp f a, H, h => by
    simp only [Ty.noBare, Bool.and_eq_true] at h
    have ha : a ≠ .bare := by rintro rfl; simp [Ty.noBare] at h
    funext x
    rw [Ty.elide_tapp ha]
    simp only [Ty.reach, Ty.reach_elide K f _ h.1, Ty.reach_elide K a _ h.2]
  | K, .rcons l t r, _, h => by
    simp only [Ty.noBare, Bool.and_eq_true] at h
    funext x; simp only [Ty.elide, Ty.reach, Ty.reach_elide K t _ h.1, Ty.reach_elide K r _ h.2]
  | _, .bare, _, h => by simp [Ty.noBare] at h
  | _, .tvar _, _, _ | _, .rg _, _, _ | _, .caps _, _, _ | _, .row _, _, _ | _, .base _, _, _
  | _, .adt _, _, _ | _, .rnil, _, _ | _, .errors _, _, _ | _, .hnd _, _, _ | _, .owner _, _, _
  | _, .dict _ _, _, _ => rfl

theorem Row.union_ofCaps_none (r : Row) : Row.union r (Row.ofCaps Caps.none) = r := by
  funext n; cases n <;> simp [Row.union, Row.ofCaps, Caps.none]

theorem Caps.under_none (k : Nat) : Caps.under k Caps.none = Caps.none := by
  funext a; simp [Caps.under, Caps.none]

/-- The types whose elided reading is a live supertype: a chain of arrows and `Eff` whose
parameters elision leaves alone, ending in a type elision leaves alone. -/
inductive Ty.Kept (K : Caps) : Ty → Prop
  | fix {t : Ty} : (∀ H, t.elide K H = t) → Ty.Kept K t
  | arrow {a φ b C} : a.elide K Caps.none = a → Ty.Kept K b → Ty.Kept K (.arrow a φ b C)
  | eff {r e t C} : t.elide K Caps.none = t → Ty.Kept K (.eff r e t C)

theorem Stage.use_live_mono {φ : Stage} {H C : Caps} :
    (Stage.use φ C).le (Stage.use (Stage.use φ H) C) := by
  cases φ with
  | pure => trivial
  | rt r e =>
    exact ⟨fun n h => h.elim (fun h => .inl (.inl h)) (fun h => .inr h), fun _ h => h⟩

/-- The elided reading of a kept type is a supertype of it. -/
theorem Ty.Kept.le_elide {K : Caps} {t : Ty} (h : t.Kept K) : ∀ H, LeLive S t (t.elide K H) := by
  induction h with
  | fix hf => intro H; rw [hf H]; exact .refl
  | arrow ha _ ih =>
    intro H
    simp only [Ty.elide]
    refine .arrow (by rw [ha]; exact .refl) Stage.use_live_mono (ih _) fun x hx => ?_
    simp only [Ty.reach] at hx ⊢
    exact hx.imp (Ty.reach_le_elide K _ _ x) (Or.imp (Ty.reach_le_elide K _ _ x) id)
  | eff ht =>
    intro H
    simp only [Ty.elide]
    refine .eff ⟨fun n h => h.elim (fun h => .inl (.inl h)) (fun h => .inr h), fun _ h => h⟩
      (by rw [ht]; exact .refl) fun x hx => ?_
    simp only [Ty.reach] at hx ⊢
    exact hx.imp_left (Ty.reach_le_elide K _ _ x)

/-- Instance types that elision leaves alone, also under the binder of `scope`. -/
def CArg.Fixed (ι : CArg) : Prop :=
  (∀ K H, ι.t.elide K H = ι.t) ∧ ∀ K H, (ι.t.up 1).elide K H = ι.t.up 1

/-- The `Live` that `scope` adds to the body's stage is already its `Scope`. -/
theorem Row.union_scope_self (K D : Caps) (r : Row) :
    Row.union (Row.union (Row.ofCaps D) r) (Row.ofCaps fun x => Caps.none x ∨ K x ∧ D x) =
      Row.union (Row.ofCaps D) r := by
  funext n; apply propext; cases n with
  | atm a =>
    simp only [Row.union, Row.ofCaps, Caps.none, false_or]
    exact ⟨fun h => h.elim id fun h => .inl h.2, .inl⟩
  | _ => simp only [Row.union, Row.ofCaps, or_false]

/-- The live type of a constant that the core types is kept by elision. -/
theorem Const.ty_kept {S : Sig} {Θ : Facts} {c : Const} {ι : CArg} {K : Caps}
    (hc : c.ok Θ ι) (hι : ι.Fixed) : (c.ty S ι).Kept K := by
  obtain ⟨ht, hup⟩ := hι
  have hk := ht K
  cases c with
  | refMake => exact .arrow (hk _) (.fix fun _ => by simp only [Ty.elide, hk])
  | refGet => exact .arrow (by simp only [Ty.elide, hk]) (.fix hk)
  | refSet => exact .arrow (by simp only [Ty.elide, hk]) (.arrow (hk _) (.fix fun _ => rfl))
  | open_ => exact .arrow rfl (.fix fun _ => rfl)
  | op => exact .arrow rfl (.fix fun _ => rfl)
  | addFin =>
    exact .arrow rfl (.arrow (by simp only [Ty.elide, Row.union_ofCaps_none]) (.fix fun _ => rfl))
  | spawn =>
    exact .arrow rfl (.arrow (by simp only [Ty.elide, Row.union_ofCaps_none, hk]) (.fix fun _ => rfl))
  | intEq => exact .arrow rfl (.arrow rfl (.fix fun _ => rfl))
  | scopeRun =>
    refine .arrow ?_ (.fix hk)
    simp only [Ty.elide, Caps.under_none, hup, Stage.use, Ty.reach, Row.scope]
    rw [Row.union_scope_self]
  | perform => exact (hc : False).elim

/-- The elided reading of a constant is a live supertype of its live type. -/
theorem Const.ty_le_elide {S : Sig} {Θ : Facts} {c : Const} {ι : CArg} {K : Caps}
    (hc : c.ok Θ ι) (hι : ι.Fixed) (H : Caps) :
    LeLive S (c.ty S ι) ((c.ty S ι).elide K H) :=
  (Const.ty_kept hc hι).le_elide H

/-- The core types a constant at each supertype of its elided reading. -/
theorem Typ.const_elided {S : Sig} {Ψ : List Ty} {Θ : Facts} {Γ : List Scheme} {c : Const}
    {ι : CArg} {K : Caps} {T : Ty} {φ : Stage} (hc : c.ok Θ ι) (hι : ι.Fixed) (H : Caps)
    (hT : LeLive S ((c.ty S ι).elide K H) T) : Typ S Ψ Θ Γ (.const c) (.ex T φ) :=
  .const hc ((Const.ty_le_elide hc hι H).trans hT)

/-- Whether a type is `bare`. -/
def Ty.isBare : Ty → Bool
  | .bare => true
  | _ => false

/-- `t` with the first `bare` among the arguments of its outer spine replaced by `x`, if it
writes one there: the hidden row of data with hidden storage, one per data. -/
def Ty.fillBare : Ty → Ty → Option Ty
  | .tapp f a, x => if a.isBare then some (.tapp f x) else (f.fillBare x).map (.tapp · a)
  | _, _ => none

/-- The stages of `t` with the hidden row each parameter writes bare on its outer spine read as
the variables `j`, `j + 1`, … in order, the parameters of a parameter that is a function
included, as elision reads `Live` (regions.md §1.4), and the next index left. -/
def Ty.fresh (j : Nat) : Ty → Ty × Nat
  | .arrow a φ b C =>
    let p : Ty × Nat := match a with
      | .arrow .. => a.fresh j
      | _ => match a.fillBare (.tvar j) with
        | some a' => (a', j + 1)
        | none => (a, j)
    let q := b.fresh p.2
    (.arrow p.1 φ q.1 C, q.2)
  | t => (t, j)

/-- An atom of a scheme with `m` binders added after its `k`: the free variables move up. -/
def Atom.shiftFrom (k m : Nat) : Atom → Atom
  | .v i => if i < k then .v i else .v (i + m)
  | .r p => .r p

/-- What elision counts in a scheme with `m` fresh binders after its `k`: the atoms of `K`
moved up, and the fresh binders, of kind `CRow`. -/
def Caps.fresh (K : Caps) (k m : Nat) : Caps := fun a =>
  (∃ b, K b ∧ a = b.shiftFrom k m) ∨ ∃ i, k ≤ i ∧ i < k + m ∧ a = .v i

/-- A scheme read with `Ty.elide`; its facts, the value bounds, stay, as the model's
`live_scheme` keeps them. Each hidden row a parameter writes bare is a fresh binder of kind
`CRow` after those of the scheme (the free variables move up), the order the checker quantifies
them (`regions/hidden_rows.rs`); a scheme without one keeps its binders. -/
def Scheme.elide (K : Caps) (s : Scheme) : Scheme :=
  match (s.t.fresh 0).2 with
  | 0 => ⟨s.k, s.cs, s.t.elide K Caps.none⟩
  | m => ⟨s.k + m, (s.up m).cs, (((s.up m).t.fresh s.k).1).elide (K.fresh s.k m) Caps.none⟩

/-- `Ty.elide K t H = t'` for closed `t`, `t'` written with predicates of finitely many atoms and
elements (`Live/ElideSchemes.lean`), and so `Scheme.elide`: unfold elision, split the
constructors, and compare each set point by point; an atom is `.v n` or `.r p`, so what is
left is arithmetic. -/
macro "elide_eq" : tactic => `(tactic| (
  simp only [Scheme.elide, Ty.fresh, Ty.fillBare, Ty.isBare, Bool.false_eq_true, ↓reduceIte,
    Option.map, Scheme.up, Scheme.substT, Facts.none_substT, Ty.substT, liftT, shiftV,
    Nat.reduceAdd, Nat.reduceLT, Stage.substT, Scheme.mk.injEq, Ty.elide, Stage.use, Ty.caps.injEq,
    Ty.arrow.injEq, Ty.tapp.injEq,
    Ty.eff.injEq, Ty.all.injEq, Ty.ref.injEq, Ty.rcons.injEq, Stage.rt.injEq, true_and, and_true]
  repeat' (first
    | rfl
    | constructor
    | (funext x
       rcases x with n | a | ⟨n, a⟩ | a | ⟨v, a⟩ <;> (try cases a) <;> (try cases v) <;>
         simp [Row.union, Row.ofCaps, Row.none, Ty.reach, Caps.none, Caps.under, Atom.up,
           Caps.fresh, Atom.shiftFrom, Caps.substT, Row.substT] <;> omega)
    | (funext x
       cases x <;> simp [Ty.reach, Caps.none, Caps.under, Atom.up, Caps.fresh, Atom.shiftFrom,
         Caps.substT] <;> omega))))

end Flick
