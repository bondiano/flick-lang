import Formal.Patterns

/-!
# Evaluation contexts: plugging, values, `lookup`
-/

namespace Flick

theorem plug_append (E E' : List Frame) (e : Tm) : plug (E ++ E') e = plug E' (plug E e) := by
  induction E generalizing e with
  | nil => rfl
  | cons F E ih => exact ih _

theorem OkE.append {E : List Frame} {F : Frame} (hE : OkE E) (hF : F.ok) : OkE (E ++ [F]) := by
  intro G hG
  rcases List.mem_append.mp hG with h | h
  · exact hE G h
  · simp at h; subst h; exact hF

theorem OkE.nil : OkE [] := by simp [OkE]

theorem OkE.cons' {E : List Frame} {F : Frame} (hE : OkE E) (hF : F.ok) : OkE (F :: E) :=
  fun G hG => (List.mem_cons.mp hG).elim (fun h => h ▸ hF) (hE G)

theorem OkE.append' {E E' : List Frame} (h : OkE E) (h' : OkE E') : OkE (E ++ E') :=
  fun G hG => (List.mem_append.mp hG).elim (h G) (h' G)

theorem OkE.cons {E : List Frame} {F : Frame} (h : OkE (F :: E)) : F.ok ∧ OkE E :=
  ⟨h F (by simp), fun G hG => h G (by simp [hG])⟩

/-- What a frame provides its hole: the effect of a handler and `Scope π` of `inside π`; the
obligations it may discharge. -/
def Frame.dom (F : Frame) : Row
  | .nm N => F.installs N
  | .atm a => ∃ p, F = .inside p ∧ a = .r p
  | .ob N a => F.installs N ∨ ∃ p, F = .inside p ∧ a = .r p
  | .obv _ a => ∃ p, F = .inside p ∧ a = .r p
  | .rv _ => False

/-- The region of a runtime form of a handler. Such a frame is created inside the
`inside` form of its region (`Step.resume`, `PHead.hresVal`), so in the live
discipline its hole may need that region live. -/
def Frame.rgn : Frame → Option Nat
  | .hctlA p _ _ _ _ _ _ _ => some p
  | .hdone p => some p
  | .hresB p _ _ => some p
  | _ => none

/-- What a frame provides its hole: `Frame.dom`, and a runtime form of a handler stands for
its own region, which the `inside` form around it keeps live. -/
def Frame.provides (F : Frame) : Row := fun n => F.dom n ∨ ∃ p, F.rgn = some p ∧ n = .atm (.r p)

theorem Frame.provides_of {F : Frame} {n : Elem} (h : F.dom n) : F.provides n := .inl h

/-- At a name, a frame provides what `Frame.dom` does: the rest are region atoms. -/
theorem Frame.provides_nm {F : Frame} {N : Name} (h : F.provides (.nm N)) : F.dom (.nm N) := by
  rcases h with h | ⟨p, -, h'⟩
  · exact h
  · cases h'
/-- What the frames of `E` provide. -/
def domE : List Frame → Row
  | [] => Row.none
  | F :: E => Row.union F.provides (domE E)

theorem domE_cons (F : Frame) (E : List Frame) (n : Elem) :
    domE (F :: E) n ↔ F.provides n ∨ domE E n := Iff.rfl
theorem not_value_plug {r : Tm} (hr : ¬ Value r) (F : Frame) : ¬ Value (F.plug r) := by
  intro hv
  cases F <;> simp only [Frame.plug] at hv
  case appL => cases hv with
    | ctorApp hf _ _ => exact hr hf
    | constApp _ _ => exact hr .const
  case appR => cases hv with
    | ctorApp _ _ he => exact hr he
    | constApp _ he => exact hr he
  case rconsL => cases hv with | rcons he _ => exact hr he
  case rconsR => cases hv with | rcons _ he => exact hr he
  all_goals cases hv

theorem not_value_plugs {r : Tm} (hr : ¬ Value r) : ∀ E : List Frame, ¬ Value (plug E r)
  | [] => hr
  | F :: E => not_value_plugs (r := F.plug r) (not_value_plug hr F) E

theorem PHead.not_value {e e'} (h : PHead e e') : ¬ Value e := by
  intro hv
  cases h <;> cases hv
  all_goals first
    | (rename_i hf _; simp [Tm.head] at hf)
    | (rename_i hc _; simp [Const.arity] at hc)
    | (rename_i hf; simp [Tm.head] at hf)
    | simp_all

theorem RHead.not_value {Λ μ e Λ' μ' e'} (h : RHead Λ μ e Λ' μ' e') : ¬ Value e := by
  intro hv
  cases h <;> cases hv
  all_goals first
    | (rename_i hf _; simp [Tm.head] at hf)
    | (rename_i hc _; simp [Const.arity] at hc)
    | (rename_i hf; simp [Tm.head] at hf)
    | simp_all

theorem raise_not_value {v} : ¬ Value (.raise v) := by
  intro hv; cases hv

theorem perform_not_value {E o c w} : ¬ Value (.app (.const (.perform E o c)) w) := by
  intro hv
  cases hv with
  | ctorApp _ hf _ => simp [Tm.head] at hf
  | constApp hc _ => simp [Const.arity] at hc

theorem hole_not_value {p} : ¬ Value (.hole p) := by
  intro hv; cases hv

theorem kont_not_value {p ℓ w} : ¬ Value (.app (.kont p ℓ) w) := by
  intro hv
  cases hv with
  | ctorApp _ hf _ => simp [Tm.head] at hf

end Flick
