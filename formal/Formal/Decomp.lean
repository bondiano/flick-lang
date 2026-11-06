import Formal.Frames

/-!
# Unique decomposition of a term into a context and a redex

A redex is a term that is not a value and is not a frame around a non-value. A term has at
most one decomposition `plug E r` with an ordered context `E` and a redex `r`
(`decomp_unique`): two ordered frames around non-values are equal (`frame_inj`), and the
outermost frame of a context is found from the term. A step inside a frame is a step of the
term in the frame (`decomp_frame`).

`PStep` is a pure step of one thread that does not read the store or the regions: a pure
head step or the unfolding of a global. It is deterministic (`PStep.det`).
-/

namespace Flick

/-- `r` is not a value, and every ordered frame around it holds a value. -/
def Redex (r : Tm) : Prop := ¬ Value r ∧ ∀ (F : Frame) (a : Tm), F.ok → F.plug a = r → Value a

theorem snoc_or (E : List Frame) : E = [] ∨ ∃ E₀ F, E = E₀ ++ [F] := by
  rcases List.eq_nil_or_concat E with h | ⟨E₀, F, h⟩
  · exact .inl h
  · exact .inr ⟨E₀, F, by simpa using h⟩

theorem plug_snoc (E : List Frame) (F : Frame) (e : Tm) : plug (E ++ [F]) e = F.plug (plug E e) :=
  plug_append E [F] e

/-- Two ordered frames around non-values are equal. -/
theorem frame_inj {F G : Frame} {a b : Tm} (hF : F.ok) (hG : G.ok) (ha : ¬ Value a)
    (hb : ¬ Value b) (h : F.plug a = G.plug b) : F = G ∧ a = b := by
  cases F <;> cases G <;> simp only [Frame.plug, Tm.app.injEq, Tm.let_.injEq, Tm.match_.injEq,
    Tm.rcons.injEq, Tm.proj.injEq, Tm.update.injEq, Tm.await.injEq, Tm.try_.injEq,
    Tm.provide.injEq, Tm.bind.injEq, Tm.join.injEq, Tm.with_.injEq, Tm.raise.injEq,
    Tm.inside.injEq, reduceCtorEq] at h <;>
    simp only [Frame.ok] at hF hG <;> simp_all

/-- The redex of an ordered context is unique. -/
theorem decomp_unique {r r' : Tm} (hr : Redex r) (hr' : Redex r') :
    ∀ (n : Nat) (E E' : List Frame), E.length + E'.length ≤ n → OkE E → OkE E' →
      plug E r = plug E' r' → E = E' ∧ r = r' := by
  intro n
  induction n with
  | zero =>
    intro E E' hn _ _ h
    cases E <;> cases E' <;> simp at hn
    exact ⟨rfl, h⟩
  | succ n ih =>
    intro E E' hn hE hE' h
    rcases snoc_or E with rfl | ⟨E₀, F, rfl⟩ <;>
      rcases snoc_or E' with rfl | ⟨E₀', G, rfl⟩
    · exact ⟨rfl, h⟩
    · rw [plug_snoc] at h
      exact absurd (hr.2 G _ (hE' G (by simp)) h.symm) (not_value_plugs hr'.1 _)
    · rw [plug_snoc] at h
      exact absurd (hr'.2 F _ (hE F (by simp)) h) (not_value_plugs hr.1 _)
    · rw [plug_snoc, plug_snoc] at h
      obtain ⟨rfl, h'⟩ := frame_inj (hE F (by simp)) (hE' G (by simp))
        (not_value_plugs hr.1 _) (not_value_plugs hr'.1 _) h
      obtain ⟨rfl, rfl⟩ := ih E₀ E₀' (by simp at hn; omega)
        (fun F h => hE F (by simp [h])) (fun F h => hE' F (by simp [h])) h'
      exact ⟨rfl, rfl⟩

theorem decomp_eq {r r' : Tm} {E E' : List Frame} (hr : Redex r) (hr' : Redex r') (hE : OkE E)
    (hE' : OkE E') (h : plug E r = plug E' r') : E = E' ∧ r = r' :=
  decomp_unique hr hr' _ E E' (Nat.le_refl _) hE hE' h

/-- A redex under an ordered frame around a non-value lies in it. -/
theorem decomp_frame {r a : Tm} {E : List Frame} {F : Frame} (hr : Redex r) (hE : OkE E)
    (hF : F.ok) (ha : ¬ Value a) (h : plug E r = F.plug a) :
    ∃ E₀, E = E₀ ++ [F] ∧ OkE E₀ ∧ a = plug E₀ r := by
  rcases snoc_or E with rfl | ⟨E₀, G, rfl⟩
  · exact absurd (hr.2 F a hF h.symm) ha
  · rw [plug_snoc] at h
    obtain ⟨rfl, h'⟩ := frame_inj (hE G (by simp)) hF (not_value_plugs hr.1 _) ha h
    exact ⟨E₀, rfl, fun F h => hE F (by simp [h]), h'.symm⟩

theorem PHead.redex {e e' : Tm} (h : PHead e e') : Redex e := by
  refine ⟨h.not_value, fun F a hF ha => ?_⟩
  cases h <;> cases F <;> simp only [Frame.plug, Frame.ok, reduceCtorEq, Tm.app.injEq,
    Tm.let_.injEq, Tm.match_.injEq, Tm.proj.injEq, Tm.update.injEq, Tm.try_.injEq,
    Tm.with_.injEq, Tm.bind.injEq, Tm.join.injEq] at ha hF <;>
    first
    | (obtain ⟨rfl, -⟩ := ha; first | assumption | exact .lam | exact .fix | exact IsEnv.value ‹_›)
    | (obtain ⟨-, rfl⟩ := ha; first | assumption | exact IsEnv.value ‹_›)
    | (obtain ⟨-, -, rfl⟩ := ha; assumption)
    | (obtain ⟨-, rfl, -⟩ := ha; assumption)
    | (subst ha; assumption)
    | (obtain ⟨rfl, -⟩ := ha; exact .constApp (.inl rfl) .lit)
    | (obtain ⟨-, rfl⟩ := ha; exact .lit)

theorem glob_redex (g : Nat) : Redex (.glob g) := by
  unfold Redex
  refine ⟨(fun h => nomatch h), fun F a _ h => ?_⟩
  cases F <;> simp [Frame.plug] at h

theorem hole_redex (p : Nat) : Redex (.hole p) :=
  ⟨fun h => (by cases h), fun F a _ ha => (by cases F <;> simp [Frame.plug] at ha)⟩

/-- A handler parks one body: its hole has one context. -/
theorem plug_hole_inj {E E' : List Frame} {p p' : Nat} (hE : OkE E) (hE' : OkE E')
    (h : plug E (.hole p) = plug E' (.hole p')) : E = E' ∧ p = p' := by
  have ⟨h₁, h₂⟩ := decomp_eq (hole_redex p) (hole_redex p') hE hE' h
  exact ⟨h₁, by cases h₂; rfl⟩

theorem PHead.det {e e₁ e₂ : Tm} (h₁ : PHead e e₁) (h₂ : PHead e e₂) : e₁ = e₂ := by
  cases h₁ with
  | hctlTail hE hk _ => cases h₂ with
    | hctlTail hE' hk' _ => obtain ⟨rfl, -⟩ := plug_hole_inj hE hE' (hk.symm.trans hk'); rfl
  | hctlDisc hE hk _ => cases h₂ with
    | hctlDisc hE' hk' _ => obtain ⟨rfl, -⟩ := plug_hole_inj hE hE' (hk.symm.trans hk'); rfl
  | hresVal hE hk _ => cases h₂ with
    | hresVal hE' hk' _ => obtain ⟨rfl, -⟩ := plug_hole_inj hE hE' (hk.symm.trans hk'); rfl
  | _ => cases h₂ <;> simp_all

/-- A pure step of one thread that does not read the store or the regions. -/
inductive PStep (root : Roots) : Tm → Tm → Prop
  | head {E e e'} : OkE E → PHead e e' → PStep root (plug E e) (plug E e')
  | glob {E g v} : OkE E → root.glob g = some v → PStep root (plug E (.glob g)) (plug E v)

theorem PStep.inv {root : Roots} {a b : Tm} (h : PStep root a b) :
    ∃ E e e', OkE E ∧ Redex e ∧ a = plug E e ∧ b = plug E e' ∧
      (PHead e e' ∨ ∃ g, e = .glob g ∧ root.glob g = some e') := by
  cases h with
  | head hE hp => exact ⟨_, _, _, hE, hp.redex, rfl, rfl, .inl hp⟩
  | glob hE hg => exact ⟨_, _, _, hE, glob_redex _, rfl, rfl, .inr ⟨_, rfl, hg⟩⟩

theorem PStep.not_value {root : Roots} {a b : Tm} (h : PStep root a b) : ¬ Value a := by
  obtain ⟨E, e, _, _, he, rfl, -, -⟩ := h.inv
  exact not_value_plugs he.1 _

theorem PStep.det {root : Roots} {a b c : Tm} (h₁ : PStep root a b) (h₂ : PStep root a c) :
    b = c := by
  obtain ⟨E, e, e₁, hE, he, rfl, rfl, hs₁⟩ := h₁.inv
  obtain ⟨E', e', e₂, hE', he', hq, rfl, hs₂⟩ := h₂.inv
  obtain ⟨rfl, rfl⟩ := decomp_eq he he' hE hE' hq
  clear hq h₁ h₂
  rcases hs₁ with h₁ | ⟨g, hg₀, hg⟩ <;> rcases hs₂ with h₂ | ⟨g', hg', hg''⟩
  · rw [h₁.det h₂]
  · subst hg'; cases h₁
  · subst hg₀; cases h₂
  · rw [hg₀] at hg'; cases hg'; rw [hg] at hg''; cases hg''; rfl

theorem PStep.frame {root : Roots} {a b : Tm} (h : PStep root a b) {F : Frame} (hF : F.ok) :
    PStep root (F.plug a) (F.plug b) := by
  cases h with
  | head hE hp => rw [← plug_snoc, ← plug_snoc]; exact .head (hE.append hF) hp
  | glob hE hg => rw [← plug_snoc, ← plug_snoc]; exact .glob (hE.append hF) hg

theorem PStep.mk {root : Roots} {E : List Frame} {e e' : Tm} (hE : OkE E)
    (h : PHead e e' ∨ ∃ g, e = .glob g ∧ root.glob g = some e') : PStep root (plug E e) (plug E e') := by
  rcases h with h | ⟨g, rfl, hg⟩
  · exact .head hE h
  · exact .glob hE hg

/-- A pure step of a frame around a non-value is a step inside it. -/
theorem PStep.inv_frame {root : Roots} {a b : Tm} {F : Frame} (hF : F.ok) (ha : ¬ Value a)
    (h : PStep root (F.plug a) b) : ∃ a', PStep root a a' ∧ b = F.plug a' := by
  obtain ⟨E, e, e', hE, he, hq, rfl, hs⟩ := h.inv
  obtain ⟨E₀, rfl, hE₀, rfl⟩ := decomp_frame he hE hF ha hq.symm
  exact ⟨_, .mk hE₀ hs, plug_snoc _ _ _⟩

/-- A pure step is a step of the thread in any store and regions. -/
theorem PStep.step {root : Roots} {a b : Tm} (h : PStep root a b) (Λ : List Rgn) (μ : List Tm) :
    Step root .pure Λ μ a (.cfg Λ μ b) := by
  cases h with
  | head hE hp => exact .pure hE hp
  | glob hE hg => exact .glob hE hg

end Flick
