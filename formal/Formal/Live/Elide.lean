import Formal.Rules
import Formal.Live.Order

/-!
# Signature elision of the live discipline

The model (`flick-model/src/check/regions/live.rs`, `elide`) reads a signature of the live
discipline with `Live` of what the parameters of each stage, and of the stages before it,
reach, in the row of every runtime arrow and `Eff` after them. `Ty.elide H t` is that
reading in the core, `Live C` being `Row.ofCaps C` (`TypingMode.use`).

Elision changes no reach, so it changes nothing a value holds (`Ty.reach_elide`). Its
reading of a constant is a supertype of the constant's live type (`Const.ty_le_elide`), so
the core types a constant at its elided type as well (`TypM.const_elided`): a call the model
types through an elided signature of a constant is a call the core types by (Sub). The
instance types are asked to be fixed by elision, also under the binder of `scope`
(`CArg.Fixed`), which a type without runtime arrows and `Eff` is. Unlike the model, every atom of a reach counts, not only regions and capture rows.
-/

namespace Flick

/-- `H` under `k` binders. -/
def Caps.under (k : Nat) (H : Caps) : Caps := fun a => ∃ b, H b ∧ a = b.up k

/-- `t` with `Live` of `H` and of what the parameters before each stage reach. -/
def Ty.elide : Ty → Caps → Ty
  | .arrow a φ b C, H =>
    .arrow (a.elide Caps.none) (TypingMode.live.use φ fun x => H x ∨ (a.elide Caps.none).reach x)
      (b.elide fun x => H x ∨ (a.elide Caps.none).reach x) C
  | .eff r e t C, H => .eff (Row.union r (Row.ofCaps H)) e (t.elide Caps.none) C
  | .all k t, H => .all k (t.elide (Caps.under k H))
  | .ref t, _ => .ref (t.elide Caps.none)
  | .tapp f a, _ => .tapp (f.elide Caps.none) (a.elide Caps.none)
  | .rcons l t r, _ => .rcons l (t.elide Caps.none) (r.elide Caps.none)
  | t, _ => t

/-- Elision changes nothing a value of the type holds. -/
theorem Ty.reach_elide : ∀ (t : Ty) (H : Caps), (t.elide H).reach = t.reach
  | .arrow a φ b C, H => by
    funext x; simp only [Ty.elide, Ty.reach, Ty.reach_elide a, Ty.reach_elide b]
  | .eff r e t C, H => by funext x; simp only [Ty.elide, Ty.reach, Ty.reach_elide t]
  | .all k t, H => by funext x; simp only [Ty.elide, Ty.reach, Ty.reach_elide t]
  | .ref t, _ => by simp only [Ty.elide, Ty.reach, Ty.reach_elide t]
  | .tapp f a, _ => by funext x; simp only [Ty.elide, Ty.reach, Ty.reach_elide f, Ty.reach_elide a]
  | .rcons l t r, _ => by funext x; simp only [Ty.elide, Ty.reach, Ty.reach_elide t, Ty.reach_elide r]
  | .tvar _, _ | .rg _, _ | .caps _, _ | .row _, _ | .base _, _ | .adt _, _ | .rnil, _
  | .errors _, _ | .hnd _, _ | .owner _, _ | .dict _ _, _ => rfl

theorem Row.union_ofCaps_none (r : Row) : Row.union r (Row.ofCaps Caps.none) = r := by
  funext n; cases n <;> simp [Row.union, Row.ofCaps, Caps.none]

theorem Caps.under_none (k : Nat) : Caps.under k Caps.none = Caps.none := by
  funext a; simp [Caps.under, Caps.none]

/-- The types whose elided reading is a live supertype: a chain of arrows and `Eff` whose
parameters elision leaves alone, ending in a type elision leaves alone. -/
inductive Ty.Kept : Ty → Prop
  | fix {t : Ty} : (∀ H, t.elide H = t) → Ty.Kept t
  | arrow {a φ b C} : a.elide Caps.none = a → Ty.Kept b → Ty.Kept (.arrow a φ b C)
  | eff {r e t C} : t.elide Caps.none = t → Ty.Kept (.eff r e t C)

theorem TypingMode.use_live_mono {φ : Stage} {H C : Caps} :
    (TypingMode.live.use φ C).le (TypingMode.live.use (TypingMode.live.use φ H) C) := by
  cases φ with
  | pure => trivial
  | rt r e =>
    exact ⟨fun n h => h.elim (fun h => .inl (.inl h)) (fun h => .inr h), fun _ h => h⟩

/-- The elided reading of a kept type is a supertype of it. -/
theorem Ty.Kept.le_elide {t : Ty} (h : t.Kept) : ∀ H, LeLive t (t.elide H) := by
  induction h with
  | fix hf => intro H; rw [hf H]; exact .refl
  | arrow ha _ ih =>
    intro H
    simp only [Ty.elide]
    refine .arrow (by rw [ha]; exact .refl) TypingMode.use_live_mono (ih _) fun x hx => ?_
    simp only [Ty.reach, Ty.reach_elide] at hx ⊢; exact hx
  | eff ht =>
    intro H
    simp only [Ty.elide]
    refine .eff ⟨fun n h => h.elim (fun h => .inl (.inl h)) (fun h => .inr h), fun _ h => h⟩
      (by rw [ht]; exact .refl) fun x hx => ?_
    simp only [Ty.reach, Ty.reach_elide] at hx ⊢; exact hx

/-- Instance types that elision leaves alone, also under the binder of `scope`. -/
def CArg.Fixed (ι : CArg) : Prop :=
  (∀ H, ι.t.elide H = ι.t) ∧ ∀ H, (ι.t.up 1).elide H = ι.t.up 1

/-- The `Live` that `scope` adds to the body's stage is already its `Scope`. -/
theorem Row.union_scope_self (D : Caps) (r : Row) :
    Row.union (Row.union (Row.ofCaps D) r) (Row.ofCaps fun x => Caps.none x ∨ D x) =
      Row.union (Row.ofCaps D) r := by
  funext n; apply propext; cases n with
  | atm a =>
    simp only [Row.union, Row.ofCaps, Caps.none, false_or]
    exact ⟨fun h => h.elim id .inl, .inl⟩
  | _ => simp only [Row.union, Row.ofCaps, or_false]

/-- The live type of a constant that the core types is kept by elision. -/
theorem Const.ty_kept {S : Sig} {Θ : Facts} {c : Const} {ι : CArg}
    (hc : c.ok Θ ι .live) (hι : ι.Fixed) : (c.ty S ι .live).Kept := by
  obtain ⟨ht, hup⟩ := hι
  cases c with
  | refMake => exact .arrow (ht _) (.fix fun _ => by simp only [Ty.elide, ht])
  | refGet => exact .arrow (by simp only [Ty.elide, ht]) (.fix ht)
  | refSet => exact .arrow (by simp only [Ty.elide, ht]) (.arrow (ht _) (.fix fun _ => rfl))
  | open_ => exact .arrow rfl (.fix fun _ => rfl)
  | op => exact .arrow rfl (.fix fun _ => rfl)
  | addFin =>
    exact .arrow rfl (.arrow (by simp only [Ty.elide, Row.union_ofCaps_none]) (.fix fun _ => rfl))
  | spawn =>
    exact .arrow rfl (.arrow (by simp only [Ty.elide, Row.union_ofCaps_none, ht]) (.fix fun _ => rfl))
  | intEq => exact .arrow rfl (.arrow rfl (.fix fun _ => rfl))
  | scopeRun =>
    refine .arrow ?_ (.fix ht)
    simp only [Ty.elide, Caps.under_none, hup, TypingMode.use, Ty.reach, Row.scope]
    rw [Row.union_scope_self]
  | perform => exact (hc : False).elim

/-- The elided reading of a constant is a live supertype of its live type. -/
theorem Const.ty_le_elide {S : Sig} {Θ : Facts} {c : Const} {ι : CArg}
    (hc : c.ok Θ ι .live) (hι : ι.Fixed) (H : Caps) :
    LeLive (c.ty S ι .live) ((c.ty S ι .live).elide H) :=
  (Const.ty_kept hc hι).le_elide H

/-- The core types a constant at each supertype of its elided reading. -/
theorem TypM.const_elided {S : Sig} {Ψ : List Ty} {Θ : Facts} {Γ : List Scheme} {c : Const}
    {ι : CArg} {T : Ty} {φ : Stage} (hc : c.ok Θ ι .live) (hι : ι.Fixed) (H : Caps)
    (hT : LeLive ((c.ty S ι .live).elide H) T) : TypM S .live Ψ Θ Γ (.const c) (.ex T φ) :=
  .const hc ((Const.ty_le_elide hc hι H).trans hT)

end Flick
