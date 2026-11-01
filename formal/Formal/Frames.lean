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

theorem OkE.cons {E : List Frame} {F : Frame} (h : OkE (F :: E)) : F.ok ∧ OkE E :=
  ⟨h F (by simp), fun G hG => h G (by simp [hG])⟩

theorem lookupE_append (E : List Frame) (F : Frame) (N : Name) :
    lookupE (E ++ [F]) N = match lookupE E N with
      | some v => some v
      | none => F.lookup N := by
  induction E with
  | nil => simp [lookupE]; cases F.lookup N <;> rfl
  | cons G E ih => simp only [List.cons_append, lookupE]; cases G.lookup N <;> simp [ih]

/-- Services a frame provides: those of `with θ`, and `Scope π` of `inside π`; the
obligations it may discharge and the uses of a `provide` region it ends. -/
def Frame.dom (F : Frame) : Row
  | .nm N => (F.lookup N).isSome
  | .atm a => ∃ p, F = .inside p ∧ a = .r p
  | .ob N a => (F.lookup N).isSome ∨ ∃ p, F = .inside p ∧ a = .r p
  | .need _ a => ∃ p, F = .inside p ∧ a = .r p

/-- Services the frames of `E` provide. -/
def domE : List Frame → Row
  | [] => Row.none
  | F :: E => Row.union F.dom (domE E)

theorem domE_cons (F : Frame) (E : List Frame) (n : Elem) :
    domE (F :: E) n ↔ F.dom n ∨ domE E n := Iff.rfl

theorem domE_nm (E : List Frame) (N : Name) : domE E (.nm N) ↔ (lookupE E N).isSome := by
  induction E with
  | nil => simp [domE, Row.none, lookupE]
  | cons F E ih =>
    simp only [domE, Row.union, Frame.dom, lookupE, ih]
    cases F.lookup N <;> simp

theorem not_value_plug {r : Tm} (hr : ¬ Value r) (F : Frame) : ¬ Value (F.plug r) := by
  intro hv
  cases F <;> simp only [Frame.plug] at hv
  case appL => cases hv with
    | ctorApp hf _ _ => exact hr hf
    | constApp _ _ => exact hr .const
    | constApp2 h₁ _ hv₁ _ => exact hr (.constApp (.inl h₁) hv₁)
  case appR => cases hv with
    | ctorApp _ _ he => exact hr he
    | constApp _ he => exact hr he
    | constApp2 _ _ _ he => exact hr he
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
    | (rename_i hc _; simp [Const.arity, Const.layer] at hc)
    | (rename_i hf; simp [Tm.head] at hf)

theorem RHead.not_value {Λ μ e Λ' μ' e'} (h : RHead Λ μ e Λ' μ' e') : ¬ Value e := by
  intro hv
  cases h <;> cases hv
  all_goals first
    | (rename_i hf _; simp [Tm.head] at hf)
    | (rename_i hc _; simp [Const.arity, Const.layer] at hc)
    | (rename_i hf; simp [Tm.head] at hf)
    | simp_all [Const.layer]

theorem ask_not_value {N w} : ¬ Value (.app (.const (.ask N)) w) := by
  intro hv
  cases hv with
  | ctorApp _ hf _ => simp [Tm.head] at hf
  | constApp hc _ => simp [Const.arity, Const.layer] at hc

theorem raise_not_value {v} : ¬ Value (.raise v) := by
  intro hv; cases hv

end Flick
