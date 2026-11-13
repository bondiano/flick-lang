import Formal.TypeEquiv

/-!
# Outer forms of equivalent types

Rows can have different syntax. Every value type retains its constructor and
metadata, and its arguments are related by the type congruence.
-/

namespace Flick

inductive Ty.SameForm : Ty → Ty → Prop
  | rows {a b} : a.RowForm → b.RowForm → Ty.SameForm a b
  | tvar (n) : Ty.SameForm (.tvar n) (.tvar n)
  | rg (p) : Ty.SameForm (.rg p) (.rg p)
  | base (b) : Ty.SameForm (.base b) (.base b)
  | arrow {a a' b b' φ C} : Ty.Equiv a a' → Ty.Equiv b b' →
      Ty.SameForm (.arrow a φ b C) (.arrow a' φ b' C)
  | ref {a b} : Ty.Equiv a b → Ty.SameForm (.ref a) (.ref b)
  | adt (N) : Ty.SameForm (.adt N) (.adt N)
  | tapp {f f' a a'} : Ty.Equiv f f' → Ty.Equiv a a' → Ty.SameForm (.tapp f a) (.tapp f' a')
  | rnil : Ty.SameForm .rnil .rnil
  | rcons {l a a' r r'} : Ty.Equiv a a' → Ty.Equiv r r' →
      Ty.SameForm (.rcons l a r) (.rcons l a' r')
  | eff {r e a b C} : Ty.Equiv a b → Ty.SameForm (.eff r e a C) (.eff r e b C)
  | errors (e) : Ty.SameForm (.errors e) (.errors e)
  | hnd {a b} : Ty.Equiv a b → Ty.SameForm (.hnd a) (.hnd b)
  | owner {a b} : Ty.Equiv a b → Ty.SameForm (.owner a) (.owner b)
  | dict {c a b} : Ty.Equiv a b → Ty.SameForm (.dict c a) (.dict c b)
  | all {k a b} : Ty.Equiv a b → Ty.SameForm (.all k a) (.all k b)
  | bare : Ty.SameForm .bare .bare
  | pure : Ty.SameForm .pure .pure
  | rt {r r' e e'} : Ty.Equiv r r' → Ty.Equiv e e' → Ty.SameForm (.rt r e) (.rt r' e')

theorem Ty.SameForm.refl (t : Ty) : t.SameForm t := by
  cases t with
  | caps | row | cnil | rnm | run | rless => exact .rows (by constructor) (by constructor)
  | tvar n => exact .tvar n
  | rg p => exact .rg p
  | base b => exact .base b
  | arrow => exact .arrow .refl .refl
  | ref => exact .ref .refl
  | adt N => exact .adt N
  | tapp => exact .tapp .refl .refl
  | rnil => exact .rnil
  | rcons => exact .rcons .refl .refl
  | eff => exact .eff .refl
  | errors e => exact .errors e
  | hnd => exact .hnd .refl
  | owner => exact .owner .refl
  | dict => exact .dict .refl
  | all => exact .all .refl
  | bare => exact .bare
  | pure => exact .pure
  | rt => exact .rt .refl .refl

theorem Ty.SameForm.symm {a b : Ty} (h : a.SameForm b) : b.SameForm a := by
  cases h with
  | rows ha hb => exact .rows hb ha
  | arrow ha hb => exact .arrow ha.symm hb.symm
  | ref h => exact .ref h.symm
  | tapp hf ha => exact .tapp hf.symm ha.symm
  | rcons ha hr => exact .rcons ha.symm hr.symm
  | eff h => exact .eff h.symm
  | hnd h => exact .hnd h.symm
  | owner h => exact .owner h.symm
  | dict h => exact .dict h.symm
  | all h => exact .all h.symm
  | rt hr he => exact .rt hr.symm he.symm
  | _ => exact .refl _

theorem Ty.SameForm.trans {a b c : Ty} (h : a.SameForm b) (h' : b.SameForm c) :
    a.SameForm c := by
  cases h with
  | rows ha hb => cases hb <;> cases h' with | rows _ hc => exact .rows ha hc
  | arrow ha hb => cases h' with
    | rows h _ => cases h
    | arrow ha' hb' => exact .arrow (ha.trans ha') (hb.trans hb')
  | ref h => cases h' with
    | rows h _ => cases h
    | ref h' => exact .ref (h.trans h')
  | tapp hf ha => cases h' with
    | rows h _ => cases h
    | tapp hf' ha' => exact .tapp (hf.trans hf') (ha.trans ha')
  | rcons ha hr => cases h' with
    | rows h _ => cases h
    | rcons ha' hr' => exact .rcons (ha.trans ha') (hr.trans hr')
  | eff h => cases h' with
    | rows h _ => cases h
    | eff h' => exact .eff (h.trans h')
  | hnd h => cases h' with
    | rows h _ => cases h
    | hnd h' => exact .hnd (h.trans h')
  | owner h => cases h' with
    | rows h _ => cases h
    | owner h' => exact .owner (h.trans h')
  | dict h => cases h' with
    | rows h _ => cases h
    | dict h' => exact .dict (h.trans h')
  | all h => cases h' with
    | rows h _ => cases h
    | all h' => exact .all (h.trans h')
  | rt hr he => cases h' with
    | rows h _ => cases h
    | rt hr' he' => exact .rt (hr.trans hr') (he.trans he')
  | tvar | rg | base | adt | rnil | errors | bare | pure =>
    cases h' with
    | rows h _ => cases h
    | _ => exact .refl _

theorem Ty.Equiv.form {a b : Ty} (h : Ty.Equiv a b) : a.SameForm b := by
  induction h with
  | refl => exact .refl _
  | symm _ ih => exact ih.symm
  | trans _ _ ih₁ ih₂ => exact ih₁.trans ih₂
  | arrow ha hb => exact .arrow ha hb
  | ref h => exact .ref h
  | tapp hf ha => exact .tapp hf ha
  | rcons ha hr => exact .rcons ha hr
  | eff h => exact .eff h
  | hnd h => exact .hnd h
  | owner h => exact .owner h
  | dict h => exact .dict h
  | all h => exact .all h
  | rt hr he => exact .rt hr he
  | run_idem r hr | run_nil r hr | rless_none r hr => exact .rows (by constructor) hr
  | _ => exact .rows (by constructor) (by constructor)

theorem Ty.Equiv.base_right {a : Ty} {b} (h : Ty.Equiv a (.base b)) : a = .base b := by
  cases h.form with
  | rows _ hr => cases hr
  | base => rfl

theorem Ty.Equiv.rg_right {a : Ty} {p} (h : Ty.Equiv a (.rg p)) : a = .rg p := by
  cases h.form with
  | rows _ hr => cases hr
  | rg => rfl

theorem Ty.Equiv.tvar_right {a : Ty} {n} (h : Ty.Equiv a (.tvar n)) : a = .tvar n := by
  cases h.form with
  | rows _ hr => cases hr
  | tvar => rfl

theorem Ty.Equiv.rnil_right {a : Ty} (h : Ty.Equiv a .rnil) : a = .rnil := by
  cases h.form with
  | rows _ hr => cases hr
  | rnil => rfl

theorem Ty.Equiv.errors_right {a : Ty} {e} (h : Ty.Equiv a (.errors e)) : a = .errors e := by
  cases h.form with
  | rows _ hr => cases hr
  | errors => rfl

theorem Ty.Equiv.hnd_rg_right {a : Ty} {p} (h : Ty.Equiv a (.hnd (.rg p))) :
    a = .hnd (.rg p) := by
  cases h.form with
  | rows _ hr => cases hr
  | hnd hr => cases hr.rg_right; rfl

theorem Ty.Equiv.owner_rg_right {a : Ty} {p} (h : Ty.Equiv a (.owner (.rg p))) :
    a = .owner (.rg p) := by
  cases h.form with
  | rows _ hr => cases hr
  | owner hr => cases hr.rg_right; rfl

theorem Ty.Equiv.owner_v_right {a : Ty} {n} (h : Ty.Equiv a (.owner (.tvar n))) :
    a = .owner (.tvar n) := by
  cases h.form with
  | rows _ hr => cases hr
  | owner hr => cases hr.tvar_right; rfl

end Flick
