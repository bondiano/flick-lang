import Formal.Rules
import Formal.Order

/-!
# Structural laws for typing by use

`LeLive` compares effective use stages, including qualifiers.
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

theorem LeLive.reach {a b : Ty} (h : LeLive a b) : Caps.Sub a.reach b.reach := by
  cases h with
  | refl => exact Caps.Sub.refl _
  | arrow _ _ _ hc | eff _ _ hc => exact hc

theorem LeLive.arrow_left {a φ t C T} (h : LeLive (.arrow a φ t C) T) :
    ∃ a' φ' t' C', T = .arrow a' φ' t' C' ∧ LeLive a' a ∧
      (Stage.use φ C).le (Stage.use φ' C') ∧ LeLive t t' ∧
      Caps.Sub (Ty.arrow a φ t C).reach T.reach := by
  cases h with
  | refl => exact ⟨_, _, _, _, rfl, .refl, Stage.le_refl _, .refl, Caps.Sub.refl _⟩
  | arrow h₁ h₂ h₃ h₄ => exact ⟨_, _, _, _, rfl, h₁, h₂, h₃, h₄⟩

theorem LeLive.arrow_right {a φ t C T} (h : LeLive T (.arrow a φ t C)) :
    ∃ a' φ' t' C', T = .arrow a' φ' t' C' ∧ LeLive a a' ∧
      (Stage.use φ' C').le (Stage.use φ C) ∧ LeLive t' t ∧
      Caps.Sub T.reach (Ty.arrow a φ t C).reach := by
  cases h with
  | refl => exact ⟨_, _, _, _, rfl, .refl, Stage.le_refl _, .refl, Caps.Sub.refl _⟩
  | arrow h₁ h₂ h₃ h₄ => exact ⟨_, _, _, _, rfl, h₁, h₂, h₃, h₄⟩

theorem LeLive.eff_left {r ε t C T} (h : LeLive (.eff r ε t C) T) :
    ∃ r' ε' t' C', T = .eff r' ε' t' C' ∧
      (Stage.use (.rt r ε) C).le (Stage.use (.rt r' ε') C') ∧
      LeLive t t' ∧ Caps.Sub (Ty.eff r ε t C).reach T.reach := by
  cases h with
  | refl => exact ⟨_, _, _, _, rfl, Stage.le_refl _, .refl, Caps.Sub.refl _⟩
  | eff h₁ h₂ h₃ => exact ⟨_, _, _, _, rfl, h₁, h₂, h₃⟩

theorem LeLive.eff_right {r ε t C T} (h : LeLive T (.eff r ε t C)) :
    ∃ r' ε' t' C', T = .eff r' ε' t' C' ∧
      (Stage.use (.rt r' ε') C').le (Stage.use (.rt r ε) C) ∧
      LeLive t' t ∧ Caps.Sub T.reach (Ty.eff r ε t C).reach := by
  cases h with
  | refl => exact ⟨_, _, _, _, rfl, Stage.le_refl _, .refl, Caps.Sub.refl _⟩
  | eff h₁ h₂ h₃ => exact ⟨_, _, _, _, rfl, h₁, h₂, h₃⟩

theorem LeLive.trans : ∀ {b a c : Ty}, LeLive a b → LeLive b c → LeLive a c
  | .arrow x φ y C, _, _, h₁, h₂ => by
    obtain ⟨_, _, _, _, rfl, ha, hφ, ht, hc⟩ := h₁.arrow_right
    obtain ⟨_, _, _, _, rfl, ga, gφ, gt, gc⟩ := h₂.arrow_left
    exact .arrow (ga.trans ha) (Stage.le_trans hφ gφ) (ht.trans gt) (hc.trans gc)
  | .eff r ε t C, _, _, h₁, h₂ => by
    obtain ⟨_, _, _, _, rfl, hφ, ht, hc⟩ := h₁.eff_right
    obtain ⟨_, _, _, _, rfl, gφ, gt, gc⟩ := h₂.eff_left
    exact .eff (Stage.le_trans hφ gφ) (ht.trans gt) (hc.trans gc)
  | .base _, _, _, h₁, h₂ | .ref _, _, _, h₁, h₂ | .adt _, _, _, h₁, h₂
  | .tvar _, _, _, h₁, h₂ | .rnil, _, _, h₁, h₂ | .rcons _ _ _, _, _, h₁, h₂
  | .rg _, _, _, h₁, h₂ | .caps _, _, _, h₁, h₂ | .row _, _, _, h₁, h₂
  | .tapp _ _, _, _, h₁, h₂ | .hnd _, _, _, h₁, h₂ | .owner _, _, _, h₁, h₂
  | .errors _, _, _, h₁, h₂ | .dict _ _, _, _, h₁, h₂ | .all _ _, _, _, h₁, h₂ => by
    cases h₁; exact h₂

theorem LeLive.substT {a b : Ty} (σ : Atom → Ty) (h : LeLive a b) :
    LeLive (a.substT σ) (b.substT σ) := by
  have sub : ∀ {x y : Ty}, Caps.Sub x.reach y.reach →
      Caps.Sub (x.substT σ).reach (y.substT σ).reach := fun hc a ha => by
    obtain ⟨b, hb, h'⟩ := (Ty.reach_substT σ _ a).mp ha
    exact (Ty.reach_substT σ _ a).mpr ⟨b, hc b hb, h'⟩
  induction h with
  | refl => exact .refl
  | arrow _ hφ _ hc ih₁ ih₂ =>
    refine .arrow ih₁ ?_ ih₂ (sub hc)
    simpa only [Stage.use_substT] using Stage.le_substT σ hφ
  | eff hφ _ hc ih =>
    refine .eff ?_ ih (sub hc)
    simpa only [Stage.use_substT] using Stage.le_substT σ hφ

theorem LeLiveI.trans {a b c : Ty} (h : LeLiveI a b)
    (hc : ∀ {x}, LeLive x b → LeLive x c) : LeLiveI a c := by
  induction h with
  | le h => exact .le (hc h)
  | inst hus _ ih => exact .inst hus (ih hc)

theorem LeLiveI.substT {a b : Ty} (σ : Atom → Ty) (h : LeLiveI a b) :
    LeLiveI (a.substT σ) (b.substT σ) := by
  induction h with
  | le h => exact .le (h.substT σ)
  | @inst k t T us hus _ ih =>
    subst hus
    refine .inst (us := us.map (Ty.substT σ)) (by simp) ?_
    rw [Ty.inst_substT]; exact ih

theorem Row.outlives_substT {Θ r s} (σ : Atom → Ty)
    (h : Row.Outlives Θ r s) : Row.Outlives (Θ.substT σ) (r.substT σ) (s.substT σ) := by
  simpa [Row.Outlives, Ty.reach_caps, Ty.substT] using h.substT σ

theorem Const.ok_substT {Θ c ι} (σ : Atom → Ty)
    (h : Const.ok Θ c ι) : Const.ok (Θ.substT σ) c (ι.substT σ) := by
  cases c <;> simp only [Const.ok, CArg.substT] at h ⊢
  case addFin =>
    exact ⟨h.1.substT σ, Row.outlives_substT σ h.2.1, by rw [← Row.ofCaps_substT]; exact h.2.2.substT σ⟩
  case spawn =>
    refine ⟨?_, Row.outlives_substT σ h.2.1, by rw [← Row.ofCaps_substT]; exact h.2.2.substT σ⟩
    have hs := h.1.substT σ
    intro a ha
    apply hs a
    rcases ha with ⟨b, hb, h'⟩ | ha
    · exact ⟨b, .inl hb, h'⟩
    · obtain ⟨b, hb, h'⟩ := (Ty.reach_substT σ _ a).mp ha
      exact ⟨b, .inr hb, h'⟩
  case scopeRun => rw [← Row.ofCaps_substT]; exact h.substT σ
  case refMake | refGet | refSet => rw [← Ty.reach_caps]; exact h.substT σ
  all_goals exact h

theorem Const.ok_mono {Θ Θ' c ι} (h : Const.ok Θ c ι)
    (hΘ : ∀ C s, Outl Θ C s → Outl Θ' C s) : Const.ok Θ' c ι := by
  cases c <;> simp only [Const.ok] at h ⊢
  case addFin | spawn => exact ⟨hΘ _ _ h.1, hΘ _ _ h.2.1, h.2.2⟩
  case refMake | refGet | refSet => exact hΘ _ _ h
  all_goals exact h

end Flick
