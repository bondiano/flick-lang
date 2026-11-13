import Formal.TypeEquivSubst
import Formal.TypeEquivView

/-!
# Congruence of nominal arguments

Conversion keeps the nominal head and relates every argument by equality,
including parameters declared invariant by the signature.
-/

namespace Flick

/-- The name and the arguments of a spine of type applications. -/
def Ty.unspine : Ty → Option (Name × List Ty)
  | .adt T => some (T, [])
  | .tapp f a => (Ty.unspine f).map fun p => (p.1, p.2 ++ [a])
  | _ => none

theorem Ty.unspine_foldl (us : List Ty) : ∀ (acc : Ty) (T : Name) (vs : List Ty),
    acc.unspine = some (T, vs) → (us.foldl .tapp acc).unspine = some (T, vs ++ us) := by
  induction us with
  | nil => intro acc T vs h; simpa using h
  | cons u us ih =>
    intro acc T vs h
    have := ih (.tapp acc u) T (vs ++ [u]) (by simp [Ty.unspine, h])
    simpa using this

theorem Ty.unspine_tcon (T : Name) (us : List Ty) : (Ty.tcon T us).unspine = some (T, us) := by
  simpa using Ty.unspine_foldl us (.adt T) T [] rfl

theorem Ty.tcon_eq {T T' : Name} {us vs : List Ty} (h : Ty.tcon T us = Ty.tcon T' vs) :
    T = T' ∧ us = vs := by
  have := congrArg Ty.unspine h
  rw [Ty.unspine_tcon, Ty.unspine_tcon] at this
  simpa using this


inductive Ty.ArgsEquiv : List Ty → List Ty → Prop
  | nil : Ty.ArgsEquiv [] []
  | cons {a b : Ty} {as bs : List Ty} :
      Ty.Equiv a b → Ty.ArgsEquiv as bs → Ty.ArgsEquiv (a :: as) (b :: bs)

theorem Ty.ArgsEquiv.symm {as bs} (h : Ty.ArgsEquiv as bs) : Ty.ArgsEquiv bs as := by
  induction h with
  | nil => exact .nil
  | cons he _ ih => exact .cons he.symm ih

theorem Ty.ArgsEquiv.length {as bs} (h : Ty.ArgsEquiv as bs) : as.length = bs.length := by
  induction h with
  | nil => rfl
  | cons _ _ ih => simp [ih]

theorem Ty.ArgsEquiv.get {as bs} (h : Ty.ArgsEquiv as bs) :
    ∀ (j : Nat) (a b : Ty), as[j]? = some a → bs[j]? = some b → Ty.Equiv a b := by
  induction h with
  | nil => intros; simp_all
  | cons he _ ih =>
    intro j a b ha hb
    cases j with
    | zero =>
      simp only [List.getElem?_cons_zero, Option.some.injEq] at ha hb
      subst a; subst b; exact he
    | succ j => exact ih j a b ha hb

theorem Ty.ArgsEquiv.of_get {as bs : List Ty} (hl : as.length = bs.length)
    (he : ∀ (j : Nat) (a b : Ty), as[j]? = some a → bs[j]? = some b → Ty.Equiv a b) :
    Ty.ArgsEquiv as bs := by
  induction as generalizing bs with
  | nil => cases bs with
    | nil => exact .nil
    | cons => simp at hl
  | cons a as ih => cases bs with
    | nil => simp at hl
    | cons b bs =>
      exact .cons (he 0 a b rfl rfl) (ih (by simpa using hl)
        (fun j x y hx hy => he (j + 1) x y (by simpa using hx) (by simpa using hy)))

theorem Ty.ArgsEquiv.foldl {as bs : List Ty} (h : Ty.ArgsEquiv as bs) {a b : Ty}
    (he : Ty.Equiv a b) : Ty.Equiv (as.foldl .tapp a) (bs.foldl .tapp b) := by
  induction h generalizing a b with
  | nil => exact he
  | cons hx _ ih => exact ih (.tapp he hx)

theorem Ty.ArgsEquiv.tcon {as bs : List Ty} (h : Ty.ArgsEquiv as bs) (T : Name) :
    Ty.Equiv (Ty.tcon T as) (Ty.tcon T bs) := h.foldl .refl

theorem Ty.ArgsEquiv.append_one {as bs a b} (h : Ty.ArgsEquiv as bs) (he : Ty.Equiv a b) :
    Ty.ArgsEquiv (as ++ [a]) (bs ++ [b]) := by
  induction h with
  | nil => exact .cons he .nil
  | cons h _ ih => exact .cons h ih

theorem Ty.unspine_eq {a : Ty} {T us} (h : a.unspine = some (T, us)) : a = Ty.tcon T us := by
  induction a generalizing T us with
  | adt N => simp only [Ty.unspine, Option.some.injEq, Prod.mk.injEq] at h
             obtain ⟨rfl, rfl⟩ := h; rfl
  | tapp f a ih _ =>
    simp only [Ty.unspine, Option.map_eq_some_iff] at h
    obtain ⟨⟨N, vs⟩, hf, h⟩ := h
    cases h
    rw [ih hf]
    simp [Ty.tcon, List.foldl_append]
  | _ => simp [Ty.unspine] at h

theorem Ty.Equiv.unspine {a b : Ty} (h : Ty.Equiv a b) :
    ∀ {T us}, a.unspine = some (T, us) →
      ∃ vs, b.unspine = some (T, vs) ∧ Ty.ArgsEquiv us vs := by
  induction a generalizing b with
  | adt N =>
    intro T us hu
    simp only [Ty.unspine, Option.some.injEq, Prod.mk.injEq] at hu
    obtain ⟨rfl, rfl⟩ := hu
    cases h.form with
    | rows hr _ => cases hr
    | adt => exact ⟨[], rfl, .nil⟩
  | tapp f a ih _ =>
    intro T us hu
    simp only [Ty.unspine, Option.map_eq_some_iff] at hu
    obtain ⟨⟨N, xs⟩, hf, hu⟩ := hu
    cases hu
    cases h.form with
    | rows hr _ => cases hr
    | tapp hf' ha =>
      obtain ⟨ys, hy, hxy⟩ := ih hf' hf
      exact ⟨ys ++ [_], by simp [Ty.unspine, hy], hxy.append_one ha⟩
  | _ => intros; simp_all [Ty.unspine]

theorem Ty.Equiv.tcon_left {T us b} (h : Ty.Equiv (Ty.tcon T us) b) :
    ∃ vs, b = Ty.tcon T vs ∧ Ty.ArgsEquiv us vs := by
  obtain ⟨vs, hb, he⟩ := h.unspine (Ty.unspine_tcon T us)
  exact ⟨vs, Ty.unspine_eq hb, he⟩

theorem Ty.ArgsEquiv.refl (as : List Ty) : Ty.ArgsEquiv as as := by
  induction as with
  | nil => exact .nil
  | cons _ _ ih => exact .cons .refl ih

theorem Ty.ArgsEquiv.trans {as bs cs} (h : Ty.ArgsEquiv as bs) (h' : Ty.ArgsEquiv bs cs) :
    Ty.ArgsEquiv as cs := by
  induction h generalizing cs with
  | nil => cases h'; exact .nil
  | cons he _ ih => cases h' with
    | cons he' ht => exact .cons (he.trans he') (ih ht)

theorem Ty.ArgsEquiv.substT {as bs} (h : Ty.ArgsEquiv as bs) (σ : Atom → Ty) :
    Ty.ArgsEquiv (as.map (Ty.substT σ)) (bs.map (Ty.substT σ)) := by
  induction h with
  | nil => exact .nil
  | cons he _ ih => exact .cons (he.substT σ) ih

theorem Ty.Equiv.tcon_inv {T T' : Name} {us vs : List Ty}
    (h : Ty.Equiv (Ty.tcon T us) (Ty.tcon T' vs)) : T = T' ∧ Ty.ArgsEquiv us vs := by
  obtain ⟨ws, hw, he⟩ := h.tcon_left
  obtain ⟨rfl, rfl⟩ := Ty.tcon_eq hw
  exact ⟨rfl, he⟩

theorem Ty.ArgsEquiv.instT {as bs} (h : Ty.ArgsEquiv as bs) (a : Atom) :
    Ty.Equiv (instT as a) (instT bs a) := by
  cases a with
  | r p => exact .refl
  | v i =>
    by_cases hi : i < as.length
    · have hi' : i < bs.length := h.length ▸ hi
      simpa only [Flick.instT, hi, hi', ↓reduceDIte] using
        h.get i _ _ (List.getElem?_eq_getElem hi) (List.getElem?_eq_getElem hi')
    · have hi' : ¬ i < bs.length := h.length ▸ hi
      simp only [Flick.instT, hi', ↓reduceDIte, h.length]; exact .refl

theorem Ty.Equiv.spine_args {as bs : List Ty} (h : Ty.Equiv (Ty.spine as) (Ty.spine bs)) :
    Ty.ArgsEquiv as bs := by
  induction as generalizing bs with
  | nil => cases bs with
    | nil => exact .nil
    | cons => cases h.form with | rows hr _ => cases hr
  | cons a as ih => cases bs with
    | nil => cases h.form with | rows hr _ => cases hr
    | cons b bs => cases h.form with
      | rows hr _ => cases hr
      | tapp ha hr => exact .cons ha (ih hr)

end Flick
