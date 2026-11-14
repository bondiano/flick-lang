import Formal.TyBase

/-!
# Definitional equality of types and row terms

The relation is available to the syntax of facts and constraints. Its algebra,
substitution and observation lemmas are proved in `TypeEquiv`.
-/

namespace Flick

/-- A row view remains a row under arbitrary substitution. A bare variable
is viewed as a row with `rless (fun _ => False)`, so an unkinded substitution
cannot use the row laws to turn an ordinary value type into a row. -/
inductive Ty.RowForm : Ty → Prop
  | cnil : Ty.RowForm .cnil
  | rnm (N a) : Ty.RowForm (.rnm N a)
  | run (r s) : Ty.RowForm (.run r s)
  | rless (G r) : Ty.RowForm (.rless G r)
  | caps (C) : Ty.RowForm (.caps C)
  | row (r) : Ty.RowForm (.row r)

theorem Ty.RowForm.substT {r : Ty} (h : r.RowForm) (σ : Atom → Ty) :
    (r.substT σ).RowForm := by
  cases h <;> simp only [Ty.substT] <;> constructor

inductive Ty.Equiv : Ty → Ty → Prop
  | refl {t} : Ty.Equiv t t
  | symm {a b} : Ty.Equiv a b → Ty.Equiv b a
  | trans {a b c} : Ty.Equiv a b → Ty.Equiv b c → Ty.Equiv a c
  | arrow {a a' b b' φ C} : Ty.Equiv a a' → Ty.Equiv b b' →
      Ty.Equiv (.arrow a φ b C) (.arrow a' φ b' C)
  | ref {a b} : Ty.Equiv a b → Ty.Equiv (.ref a) (.ref b)
  | tapp {f f' a a'} : Ty.Equiv f f' → Ty.Equiv a a' → Ty.Equiv (.tapp f a) (.tapp f' a')
  | rcons {l a a' r r'} : Ty.Equiv a a' → Ty.Equiv r r' →
      Ty.Equiv (.rcons l a r) (.rcons l a' r')
  | eff {r e t t' C} : Ty.Equiv t t' → Ty.Equiv (.eff r e t C) (.eff r e t' C)
  | hnd {a b} : Ty.Equiv a b → Ty.Equiv (.hnd a) (.hnd b)
  | owner {a b} : Ty.Equiv a b → Ty.Equiv (.owner a) (.owner b)
  | dict {c a b} : Ty.Equiv a b → Ty.Equiv (.dict c a) (.dict c b)
  | all {k a b} : Ty.Equiv a b → Ty.Equiv (.all k a) (.all k b)
  | rnm {N a b} : Ty.Equiv a b → Ty.Equiv (.rnm N a) (.rnm N b)
  | run {r r' s s'} : Ty.Equiv r r' → Ty.Equiv s s' → Ty.Equiv (.run r s) (.run r' s')
  | rless (G : Name → Prop) {r s} : Ty.Equiv r s → Ty.Equiv (.rless G r) (.rless G s)
  | rt {r r' e e'} : Ty.Equiv r r' → Ty.Equiv e e' → Ty.Equiv (.rt r e) (.rt r' e')
  | run_comm (r s) : Ty.Equiv (.run r s) (.run s r)
  | run_assoc (r s t) : Ty.Equiv (.run (.run r s) t) (.run r (.run s t))
  | run_idem (r) : r.RowForm → Ty.Equiv (.run r r) r
  | run_nil (r) : r.RowForm → Ty.Equiv (.run r .cnil) r
  | rless_nil (G) : Ty.Equiv (.rless G .cnil) .cnil
  | rless_none (r) : r.RowForm → Ty.Equiv (.rless (fun _ => False) r) r
  | rless_run (G r s) : Ty.Equiv (.rless G (.run r s)) (.run (.rless G r) (.rless G s))
  | rless_merge (G H r) :
      Ty.Equiv (.rless G (.rless H r)) (.rless (fun n => G n ∨ H n) r)
  | rless_keep (G N a) : ¬ G N → Ty.Equiv (.rless G (.rnm N a)) (.rnm N a)
  | rless_drop (G N a) : G N → Ty.Equiv (.rless G (.rnm N a)) (.caps a.reach)
  | caps_nil : Ty.Equiv (.caps Caps.none) .cnil
  | caps_union (C D) : Ty.Equiv (.run (.caps C) (.caps D)) (.caps (fun a => C a ∨ D a))
  | rless_caps (G C) : Ty.Equiv (.rless G (.caps C)) (.caps C)


end Flick
