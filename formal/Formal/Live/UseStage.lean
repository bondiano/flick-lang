import Formal.Rules
import Formal.Order

/-!
# Use stages

`Stage.use φ C`, the stage at which a value qualified `C` is used, under substitution and
ordering.
-/

namespace Flick

@[simp] theorem Stage.use_none (φ : Stage) :
    Stage.use φ Caps.none = φ := by
  cases φ with
  | pure => rfl
  | rt r ε =>
    change Stage.rt (Row.union r (Row.ofCaps Caps.none)) ε = Stage.rt r ε
    congr 1
    funext x
    cases x <;> simp [Row.union, Row.ofCaps, Caps.none]

@[simp] theorem Stage.use_pure (C : Caps) :
    Stage.use .pure C = .pure := by rfl

@[simp] theorem Stage.use_err (φ : Stage) (C : Caps) :
    (Stage.use φ C).err = φ.err := by cases φ <;> rfl

/-- A use stage adds only the atoms of what it uses, and only in the live discipline. -/
theorem Stage.use_svc (φ : Stage) (C : Caps) {n : Elem}
    (h : (Stage.use φ C).svc n) : φ.svc n ∨ ∃ a, C a ∧ n = .atm a := by
  cases φ with
  | pure => exact .inl h
  | rt r ε =>
    rcases h with h | h
    · exact .inl h
    · cases n <;> simp [Row.ofCaps] at h
      exact .inr ⟨_, h, rfl⟩

theorem Stage.le_use (φ : Stage) (C : Caps) :
    φ.le (Stage.use φ C) := by
  cases φ with
  | pure => trivial
  | rt _ _ => exact ⟨fun _ h => .inl h, fun _ h => h⟩

theorem Stage.use_mono {φ ψ : Stage} {C D : Caps}
    (h : φ.le ψ) (hCD : Caps.Sub C D) : (Stage.use φ C).le (Stage.use ψ D) := by
  cases φ with
  | pure => trivial
  | rt r ε =>
    cases ψ with
    | pure => exact h.elim
    | rt r' ε' =>
      refine ⟨?_, h.2⟩
      intro x hx
      rcases hx with hx | hx
      · exact .inl (h.1 x hx)
      · exact .inr (by cases x <;> first | exact hCD _ hx | exact hx)

theorem Stage.use_substT (φ : Stage) (C : Caps) (σ : Atom → Ty) :
    (Stage.use φ C).substT σ = Stage.use (φ.substT σ) (C.substT σ) := by
  cases φ <;>
    simp [Stage.use, Stage.substT, Row.union_substT, Row.ofCaps_substT]

theorem Caps.substT_rg {p : Nat} {σ : Atom → Ty} (hp : σ (.r p) = .rg p) :
    ((Ty.rg p).reach).substT σ = (Ty.rg p).reach := by
  funext a
  apply propext
  constructor
  · rintro ⟨b, rfl, h⟩; rw [hp] at h; exact h
  · intro h; exact ⟨.r p, rfl, by rw [hp]; exact h⟩

theorem Caps.substT_if_rg {tl : Bool} {p : Nat} {σ : Atom → Ty} (hp : σ (.r p) = .rg p) :
    ((if tl = true then Caps.none else (Ty.rg p).reach) : Caps).substT σ
      = if tl = true then Caps.none else (Ty.rg p).reach := by
  cases tl <;> simp [Caps.substT_rg hp, Caps.none_substT]

theorem Stage.use_substT_eq (φ : Stage) {C : Caps} {σ : Atom → Ty}
    (hC : C.substT σ = C) : (Stage.use φ C).substT σ = Stage.use (φ.substT σ) C := by
  rw [Stage.use_substT, hC]
theorem Stage.use_substT_rg (φ : Stage) {p : Nat} {σ : Atom → Ty}
    (hp : σ (.r p) = .rg p) :
    (Stage.use φ (Ty.rg p).reach).substT σ = Stage.use (φ.substT σ) (Ty.rg p).reach :=
  Stage.use_substT_eq φ (Caps.substT_rg hp)

/-- A named effect of a use stage is one of the stage used: typing by use adds only
region atoms to a row. -/
theorem Stage.use_svc_nm {φ : Stage} {C : Caps} {N : Name}
    (h : (Stage.use φ C).svc (.nm N)) : φ.svc (.nm N) := by
  cases φ with
  | pure => exact h
  | rt r ε =>
    rcases h with h | h
    · exact h
    · exact h.elim

/-- A stage above a run-time stage that holds the atom of a region is above the use of
that region: the frame of a handler supplies the atom its own region needs. -/
theorem Stage.use_rg_le {r ε : Row} {φ : Stage} {p : Nat}
    (h : (Stage.rt r ε).le φ) (hp : φ.svc (.atm (.r p))) :
    (Stage.use (.rt r ε) (Ty.rg p).reach).le φ := by
  obtain ⟨r', ε', rfl, hr, he⟩ := Stage.rt_le h
  simp only [Stage.use]
  refine ⟨fun n hn => ?_, he⟩
  cases n with
  | nm N =>
    rcases hn with hn | hn
    · exact hr _ hn
    · simp [Row.ofCaps] at hn
  | atm a =>
    rcases hn with hn | hn
    · exact hr _ hn
    · rw [show a = .r p by simpa [Row.ofCaps, Ty.reach] using hn]; exact hp
  | ob N a =>
    rcases hn with hn | hn
    · exact hr _ hn
    · simp [Row.ofCaps] at hn
  | rv a =>
    rcases hn with hn | hn
    · exact hr _ hn
    · simp [Row.ofCaps] at hn
  | obv v a =>
    rcases hn with hn | hn
    · exact hr _ hn
    · simp [Row.ofCaps] at hn

/-- A control arm may call `resume`, which uses the region of the handler; a `tail` arm
does not. -/
theorem Stage.use_ctl_le (tl : Bool) (r ε : Row) (p : Nat) :
    (Stage.use (.rt r ε) (if tl then Caps.none else (Ty.rg p).reach)).le
      (Stage.use (.rt r ε) (Ty.rg p).reach) := by
  cases tl with
  | true =>
    simp only [if_true, Stage.use_none]
    exact Stage.le_use (Stage.rt r ε) (Ty.rg p).reach
  | false => exact Stage.le_refl _

end Flick
