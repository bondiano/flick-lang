import Formal.Elab.CoreLaws
import Formal.Elab.Basic

/-!
# Witness: closed types and the instance of a method field

A substitution acts on a translated type only through the variables the type mentions, in the
source and in the core (`Ty.subst_congr`, `trTT_substT_congr`); so a type whose variables lie
below `k` gives a closed scheme. `field_inst`: the field of a method, its own variables rotated
first (`rot`), instantiated at a ground class argument `U`, is the translation of the method's
type instantiated at `U`.
-/

namespace Flick.Elab

theorem Ty.subst_congr {σ σ' : Nat → Ty} {t : Ty} (h : ∀ n, t.Occurs n → σ n = σ' n) :
    t.subst σ = t.subst σ' :=
  Ty.subst_agree h

theorem TTy.closed_of {k : Nat} {t : Ty} (h : ∀ n, t.Occurs n → n < k) : (TTy.mono t).Closed k :=
  fun _ _ hσ => by simp only [TTy.subst]; rw [Ty.subst_congr fun n hn => hσ n (h n hn)]

variable (N : Names)

theorem caps_congr {C : Caps} {σ σ' : Atom → Flick.Ty} (h : ∀ b, C b → σ b = σ' b) :
    C.substT σ = C.substT σ' := by
  funext a; apply propext; constructor
  · rintro ⟨b, hb, h'⟩; exact ⟨b, hb, h b hb ▸ h'⟩
  · rintro ⟨b, hb, h'⟩; exact ⟨b, hb, (h b hb).symm ▸ h'⟩

/-- A substitution acts on a row only through the atoms its elements name. -/
theorem Row.substT_congr {r : Row} {σ σ' : Atom → Flick.Ty}
    (h : ∀ b, (r (.atm b) ∨ r (.rv b) ∨ (∃ N, r (.ob N b)) ∨
      ∃ c, r (.obv b c) ∨ r (.obv c b)) → σ b = σ' b) :
    r.substT σ = r.substT σ' := by
  funext x; apply propext
  cases x with
  | nm N =>
    simp only [Row.substT]
    constructor
    · exact fun h' => h'.imp id fun ⟨b, hb, h''⟩ => ⟨b, hb, h b (.inr (.inl hb)) ▸ h''⟩
    · exact fun h' => h'.imp id fun ⟨b, hb, h''⟩ => ⟨b, hb, (h b (.inr (.inl hb))).symm ▸ h''⟩
  | atm a =>
    simp only [Row.substT]
    constructor
    · exact fun h' => h'.imp (fun ⟨b, hb, h''⟩ => ⟨b, hb, h b (.inl hb) ▸ h''⟩)
        fun ⟨b, hb, h''⟩ => ⟨b, hb, h b (.inr (.inl hb)) ▸ h''⟩
    · exact fun h' => h'.imp (fun ⟨b, hb, h''⟩ => ⟨b, hb, (h b (.inl hb)).symm ▸ h''⟩)
        fun ⟨b, hb, h''⟩ => ⟨b, hb, (h b (.inr (.inl hb))).symm ▸ h''⟩
  | ob N a =>
    simp only [Row.substT]
    have hv : ∀ {v b}, r (.obv v b) → σ v = σ' v ∧ σ b = σ' b := fun hb =>
      ⟨h _ (.inr (.inr (.inr ⟨_, .inl hb⟩))), h _ (.inr (.inr (.inr ⟨_, .inr hb⟩)))⟩
    constructor
    · exact fun h' => h'.imp (fun ⟨b, hb, h''⟩ => ⟨b, hb, h b (.inr (.inr (.inl ⟨N, hb⟩))) ▸ h''⟩)
        fun ⟨v, b, hb, h₁, h₂⟩ => ⟨v, b, hb, (hv hb).1 ▸ h₁, (hv hb).2 ▸ h₂⟩
    · exact fun h' => h'.imp
        (fun ⟨b, hb, h''⟩ => ⟨b, hb, (h b (.inr (.inr (.inl ⟨N, hb⟩)))).symm ▸ h''⟩)
        fun ⟨v, b, hb, h₁, h₂⟩ => ⟨v, b, hb, (hv hb).1.symm ▸ h₁, (hv hb).2.symm ▸ h₂⟩
  | obv w a =>
    simp only [Row.substT]
    have hv : ∀ {v b}, r (.obv v b) → σ v = σ' v ∧ σ b = σ' b := fun hb =>
      ⟨h _ (.inr (.inr (.inr ⟨_, .inl hb⟩))), h _ (.inr (.inr (.inr ⟨_, .inr hb⟩)))⟩
    constructor
    · exact fun ⟨v, b, hb, h₁, h₂⟩ => ⟨v, b, hb, (hv hb).1 ▸ h₁, (hv hb).2 ▸ h₂⟩
    · exact fun ⟨v, b, hb, h₁, h₂⟩ => ⟨v, b, hb, (hv hb).1.symm ▸ h₁, (hv hb).2.symm ▸ h₂⟩
  | rv a =>
    simp only [Row.substT]
    constructor
    · exact fun ⟨b, hb, h''⟩ => ⟨b, hb, h b (.inr (.inl hb)) ▸ h''⟩
    · exact fun ⟨b, hb, h''⟩ => ⟨b, hb, (h b (.inr (.inl hb))).symm ▸ h''⟩

/-- What a translated row names is among the variables of the source row. -/
theorem asRow_trT_atoms {t : Ty} {b : Atom}
    (h : (trT N t).asRow (.atm b) ∨ (trT N t).asRow (.rv b) ∨
      (∃ M, (trT N t).asRow (.ob M b)) ∨
      ∃ c, (trT N t).asRow (.obv b c) ∨ (trT N t).asRow (.obv c b)) :
    ∃ n, b = .v n ∧ t.Occurs n := by
  refine trT_reach_sub N ((Flick.Ty.reach_asRow _ _).2 ?_)
  rcases h with h | h | ⟨M, h⟩ | ⟨c, h | h⟩
  · exact .inl h
  · exact .inr h
  all_goals simp at h

/-- What a translated row of errors names is among the variables of the source row. -/
theorem errRow_trT_atoms {t : Ty} {b : Atom}
    (h : errRow (trT N t) (.atm b) ∨ errRow (trT N t) (.rv b) ∨
      (∃ M, errRow (trT N t) (.ob M b)) ∨
      ∃ c, errRow (trT N t) (.obv b c) ∨ errRow (trT N t) (.obv c b)) :
    ∃ n, b = .v n ∧ t.Occurs n := by
  simp only [errRow, Row.union, Row.ofCaps, or_false] at h
  rcases h with (h | h) | h | ⟨M, h⟩ | ⟨c, h | h⟩
  · exact asRow_trT_atoms N (.inl h)
  · exact trT_reach_sub N h
  · exact asRow_trT_atoms N (.inr (.inl h))
  all_goals simp at h

theorem trT_substT_congr {σ σ' : Atom → Flick.Ty} {t : Ty}
    (h : ∀ n, t.Occurs n → σ (.v n) = σ' (.v n)) : (trT N t).substT σ = (trT N t).substT σ' := by
  have hc : ∀ b, (trT N t).reach b → σ b = σ' b := by
    intro b hb; obtain ⟨n, rfl, hn⟩ := trT_reach_sub N hb; exact h n hn
  induction t using Ty.ind generalizing σ σ' with
  | all k t iht =>
    simp only [trT, Flick.Ty.substT]
    have hl : ∀ n, t.Occurs n → liftT k σ (.v n) = liftT k σ' (.v n) := by
      intro n hn; simp only [liftT]; split
      · rfl
      · rw [h (n - k) (by rwa [Ty.Occurs, Nat.sub_add_cancel (by omega)])]
    rw [iht hl fun b hb => by obtain ⟨n, rfl, hn⟩ := trT_reach_sub N hb; exact hl n hn]
  | var n => exact h n rfl
  | con c => unfold trT; split <;> (try split) <;> rfl
  | cnil => simp [trT, Flick.Ty.substT]
  | app t u iht ihu =>
    simp only [trT, Flick.Ty.substT]
    rw [iht (fun n hn => h n (.inl hn)) (fun b hb => hc b (.inl hb)),
      ihu (fun n hn => h n (.inr hn)) (fun b hb => hc b (.inr hb))]
  | cun q q' _ _ => exact congrArg Flick.Ty.caps (caps_congr hc)
  | rnm _ _ _ | run _ _ _ _ =>
    simp only [trT, Flick.Ty.substT]
    congr 1
    refine Row.substT_congr fun b hb => ?_
    rcases hb with hb | hb | ⟨M, hb⟩ | ⟨c, hb | hb⟩
    · exact hc b (Or.inl hb)
    · exact hc b (Or.inr hb)
    all_goals simp [Row.err, Row.union, Row.single, Row.ofCaps] at hb
  | arr φ a b q iha ihb _ _ =>
    have hst : (trStage N φ).substT σ = (trStage N φ).substT σ' := by
      cases φ with
      | P => rfl
      | R r e =>
        have stage : ∀ x, (∃ n, x = .v n ∧ (Stage.R r e).Occurs n) → σ x = σ' x := by
          rintro x ⟨n, rfl, hn⟩; exact h n (.inr (.inr (.inr hn)))
        simp only [trStage, Flick.Stage.substT]
        rw [Row.substT_congr fun x hx => stage x (let ⟨n, e, h⟩ := asRow_trT_atoms N hx; ⟨n, e, .inl h⟩),
          Row.substT_congr (r := Row.union _ allNames) fun x hx => stage x ?_]
        simp only [Row.union, allNames, or_false] at hx
        obtain ⟨n, e, h⟩ := errRow_trT_atoms N hx
        exact ⟨n, e, .inr h⟩
    rw [trT_arr] at hc ⊢
    simp only [Flick.Ty.substT]
    rw [iha (fun n hn => h n (.inl hn)) (fun x hx => hc x (.inl hx)),
      ihb (fun n hn => h n (.inr (.inl hn))) (fun x hx => hc x (.inr (.inl hx))), hst,
      caps_congr fun x hx => hc x (.inr (.inr hx))]

/-- A translated type whose variables lie below `k` is closed at `k`. -/
theorem trT_closed {k k' : Nat} {t : Ty} (h : ∀ n, t.Occurs n → n < k) :
    Flick.Scheme.Closed k ⟨k', Facts.none, trT N t⟩ :=
  fun _ _ hσ => ⟨trT_substT_congr N fun n hn => hσ n (h n hn), by simp⟩

/-- A ground `D_Q` is closed. -/
theorem trD_closed {k k' : Nat} {c : Nat} {args : List Ty} (h : ∀ t ∈ args, ∀ n, t.Occurs n → n < k) :
    Flick.Scheme.Closed k ⟨k', Facts.none, trD N ⟨c, args⟩⟩ := by
  intro σ σ' hσ
  refine ⟨?_, by simp⟩
  simp only [trD, Flick.Ty.substT, Flick.Ty.spine_substT, List.map_map]
  congr 2
  exact List.map_congr_left fun t ht => trT_substT_congr N fun n hn => hσ n (h t ht n hn)

theorem rot_v {p kk n : Nat} (h : n < p + kk) : ∃ m, rot p kk (.v n) = .tvar m ∧ m < p + kk := by
  simp only [rot]
  by_cases h1 : n < p
  · rw [if_pos h1]; exact ⟨_, rfl, by omega⟩
  · rw [if_neg h1, if_pos h]; exact ⟨_, rfl, by omega⟩

/-- The field of a method with the variables of its type below `p + kk` is closed. -/
theorem mfield_closed {p kk k' : Nat} {t : Ty} (h : ∀ n, t.Occurs n → n < p + kk) :
    Flick.Scheme.Closed (p + kk) ⟨k', Facts.none, (trT N t).substT (rot p kk)⟩ := by
  intro σ σ' hσ
  refine ⟨?_, by simp⟩
  simp only [Flick.Ty.substT_comp]
  apply trT_substT_congr
  intro n hn
  obtain ⟨m, e, hm⟩ := rot_v (h n hn)
  rw [e]; exact hσ m hm

theorem trT_con_up (c kk : Nat) : (trT N (.con c)).up kk = trT N (.con c) := by
  unfold trT; split <;> (try split) <;> rfl

/-- **The field of a method at a ground class argument** `con c`. -/
theorem field_inst (t : Ty) (c kk : Nat) :
    ((trT N t).substT (rot 1 kk)).substT (liftT kk (instT [trT N (.con c)])) =
      trT N (t.subst (inst [.con c])) := by
  rw [trT_subst, Flick.Ty.substT_comp]
  apply Flick.Ty.substT_congr
  intro a
  cases a with
  | r p => rfl
  | v j =>
    simp only [rot, trσ, inst, List.length_singleton]
    by_cases h0 : j < 1
    · have : j = 0 := by omega
      subst this
      simp [Flick.Ty.substT, liftT, instT, trT_con_up]
    · rw [if_neg h0, dif_neg h0]
      by_cases h1 : j < 1 + kk
      · rw [if_pos h1]; simp only [Flick.Ty.substT, liftT, trT]; rw [if_pos (by omega)]
      · rw [if_neg h1]
        simp only [Flick.Ty.substT, liftT, if_neg (show ¬ j < kk by omega), instT,
          List.length_singleton, trT]
        rw [dif_neg (by omega)]
        simp only [Flick.Ty.up_def, Flick.Ty.substT, shiftV]; congr 1; omega

end Flick.Elab
