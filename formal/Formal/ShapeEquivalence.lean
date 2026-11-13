import Formal.VarySpine

/-!
# Observable outer forms of equivalent types

Type conversion preserves the kind of a type and whether patterns can inspect
it. Row syntax can vary without changing either observation.
-/

namespace Flick

/-- The outermost type former. -/
def Ty.kind : Ty → Nat
  | .tvar _ => 9
  | .base _ => 0
  | .arrow .. => 1
  | .ref _ => 2
  | .adt _ | .tapp .. => 3
  | .rnil | .rcons .. => 4
  | .eff .. => 5
  | .errors _ => 7
  | .hnd _ => 10
  | .owner _ => 11
  | .rg _ | .caps _ | .row _ | .bare | .cnil | .rnm .. | .run .. | .rless .. | .pure | .rt .. => 12
  | .dict .. => 13
  | .all .. => 14

theorem Ty.tcon_kind' (T : Name) (us : List Ty) : (Ty.tcon T us).kind = 3 := by
  have : ∀ (us : List Ty) (f : Ty), f.kind = 3 → (us.foldl .tapp f).kind = 3 := by
    intro us; induction us with
    | nil => intro f h; exact h
    | cons x xs ih => intro f _; exact ih _ rfl
  exact this us _ rfl


theorem Ty.RowForm.kind {r : Ty} (h : r.RowForm) : r.kind = 12 := by
  cases h <;> rfl

theorem Ty.Equiv.kind {a b : Ty} (h : Ty.Equiv a b) : a.kind = b.kind := by
  cases h.form with
  | rows ha hb => rw [ha.kind, hb.kind]
  | _ => rfl

theorem Ty.RowForm.opaque {r : Ty} (h : r.RowForm) : r.Opaque := by
  cases h <;> simp [Ty.Opaque, Ty.hd]

theorem Ty.Equiv.opaque {a b : Ty} (h : Ty.Equiv a b) : a.Opaque ↔ b.Opaque := by
  induction h with
  | refl => rfl
  | symm _ ih => exact ih.symm
  | trans _ _ ih₁ ih₂ => exact ih₁.trans ih₂
  | tapp _ _ ih _ => simpa only [Ty.Opaque, Ty.hd] using ih
  | run_idem r hr | run_nil r hr | rless_none r hr =>
    constructor
    · intro _; exact hr.opaque
    · intro _; simp [Ty.Opaque, Ty.hd]
  | _ => simp [Ty.Opaque, Ty.hd]

theorem LeLive.kind {a b : Ty} (h : LeLive S a b) : a.kind = b.kind := by
  cases h with
  | refl | arrow | eff | rcons => rfl
  | equiv he => exact he.kind
  | hide _ _ _ _ ha hb => subst ha; subst hb; rw [Ty.tcon_kind', Ty.tcon_kind']
  | data _ _ _ _ _ ha hb => subst ha; subst hb; rw [Ty.tcon_kind', Ty.tcon_kind']

theorem Ty.opaque_of_kind {t : Ty} (h : t.kind = 1) : t.Opaque := by
  cases t <;> simp [Ty.kind] at h <;> simp [Ty.Opaque, Ty.hd]

theorem Ty.tcon_kind (T : Name) (us : List Ty) : (Ty.tcon T us).kind = 3 := by
  have : ∀ (us : List Ty) (f : Ty), f.kind = 3 → (us.foldl .tapp f).kind = 3 := by
    intro us; induction us with
    | nil => intro f h; exact h
    | cons x xs ih => intro f _; exact ih _ rfl
  exact this us _ rfl

theorem Ty.kind_of_hd {U : Ty} {E : Name} (h : U.hd = .adt E) : U.kind = 3 := by
  cases U <;> simp [Ty.hd, Ty.kind] at h ⊢

theorem LeLive.opaque {a b : Ty} (h : LeLive S a b) (ha : a.Opaque) : b.Opaque := by
  cases h with
  | refl => exact ha
  | equiv he => exact he.opaque.mp ha
  | arrow => simp [Ty.Opaque, Ty.hd]
  | eff => simp [Ty.Opaque, Ty.hd]
  | rcons => simp [Ty.Opaque, Ty.hd] at ha
  | hide _ _ _ _ ha' hb => subst ha'; subst hb; rw [Ty.Opaque, Ty.tcon_hd] at ha ⊢; exact ha
  | data _ _ _ _ _ ha' hb => subst ha'; subst hb; rw [Ty.Opaque, Ty.tcon_hd] at ha ⊢; exact ha


theorem Ty.RowForm.not_nominal {r : Ty} (h : r.RowForm) (T : Name) : r.hd ≠ .adt T := by
  cases h <;> simp [Ty.hd]

theorem Ty.Equiv.nominalHead {a b : Ty} (h : Ty.Equiv a b) (T : Name) :
    a.hd = .adt T ↔ b.hd = .adt T := by
  induction h with
  | refl => rfl
  | symm _ ih => exact ih.symm
  | trans _ _ ih₁ ih₂ => exact ih₁.trans ih₂
  | tapp _ _ ih => exact ih
  | run_idem r hr | run_nil r hr | rless_none r hr =>
    simp [Ty.hd, hr.not_nominal T]
  | _ => simp [Ty.hd]


theorem Ty.Equiv.hd_adt {a b : Ty} (h : Ty.Equiv a b) {T : Name}
    (ha : a.hd = .adt T) : b.hd = .adt T := (h.nominalHead T).mp ha

private theorem Shp.equivCore {S : Sig} {fl a e} (hs : Shp S fl a e) :
    fl = true → ∀ {b}, Ty.Equiv a b → Shp S true b e := by
  induction hs with
  | hidden ho => intro _ b he; exact .hidden (he.opaque.mp ho)
  | lit =>
    intro _ b he
    obtain rfl := he.symm.base_right
    exact .lit
  | rnil =>
    intro _ b he
    obtain rfl := he.symm.rnil_right
    exact .rnil
  | rcons _ _ ih₁ ih₂ =>
    intro _ b he
    cases he.form with
    | rows hr _ => cases hr
    | rcons ht hr => exact .rcons (ih₁ rfl ht) (ih₂ rfl hr)
  | head | app => intro hf _ _; cases hf
  | adt hU hUh hTh hB _ =>
    intro _ b he
    exact .adt hU hUh (he.hd_adt hTh) fun K hK => (hB K hK).equiv he

/-- Type congruence changes no shape observable by a value pattern. -/
theorem Shp.equiv {S : Sig} {a b : Ty} {e : Tm} (hs : Shp S true a e)
    (he : Ty.Equiv a b) : Shp S true b e := hs.equivCore rfl he

end Flick
