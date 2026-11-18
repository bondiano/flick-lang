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
  | rnm _ t iht => simp only [trT, Flick.Ty.substT, iht h hc]
  | run r r' ihr ihr' =>
    simp only [trT, Flick.Ty.substT]
    rw [ihr (fun n hn => h n (.inl hn)) (fun b hb => hc b (.inl hb)),
      ihr' (fun n hn => h n (.inr hn)) (fun b hb => hc b (.inr hb))]
  | arr φ a b q iha ihb _ ihφ =>
    have reach : ∀ {u : Ty}, (∀ n, u.Occurs n → σ (.v n) = σ' (.v n)) →
        ∀ x, (trT N u).reach x → σ x = σ' x := fun hu x hx => by
      obtain ⟨n, rfl, hn⟩ := trT_reach_sub N hx; exact hu n hn
    have hst : (trStageT N φ).substS σ = (trStageT N φ).substS σ' := by
      cases φ with
      | P => rfl
      | R r e =>
        have hr : ∀ n, r.Occurs n → σ (.v n) = σ' (.v n) := fun n hn => h n (.inr (.inr (.inr (.inl hn))))
        have he : ∀ n, e.Occurs n → σ (.v n) = σ' (.v n) := fun n hn => h n (.inr (.inr (.inr (.inr hn))))
        simp only [trStageT, Ty.substS_rt, errT_substT, ihφ.1 hr (reach hr), ihφ.2 he (reach he)]
    rw [trT_arr] at hc ⊢
    simp only [Flick.Ty.substT_arrow]
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

/-- The facts of value bounds under substitutions that agree on the variables of the bounds. -/
theorem trBd_substT_congr {ρ ρ' : Atom → Flick.Ty} :
    ∀ {bd : List (Ty × Ty)}, (∀ b ∈ bd, ∀ n, b.1.Occurs n ∨ b.2.Occurs n → ρ (.v n) = ρ' (.v n)) →
      (trBd N bd).substT ρ = (trBd N bd).substT ρ'
  | [], _ => by simp [trBd]
  | b :: bd, h => by
    simp only [trBd, Facts.union_substT, trB_substT_any]
    rw [trT_substT_congr N fun n hn => h b (.head _) n (.inl hn),
      trT_substT_congr N (t := b.2) fun n hn => h b (.head _) n (.inr hn),
      trBd_substT_congr fun b' hb => h b' (.tail _ hb)]

/-- The field of a method with the variables of its type and of its value bounds below
`p + kk` is closed. -/
theorem mfield_closed {p kk k' : Nat} {t : Ty} {bd : List (Ty × Ty)}
    (h : ∀ n, t.Occurs n → n < p + kk) (hb : ∀ b ∈ bd, ∀ n, b.1.Occurs n ∨ b.2.Occurs n → n < p + kk) :
    Flick.Scheme.Closed (p + kk) ⟨k', (trBd N bd).substT (rot p kk), (trT N t).substT (rot p kk)⟩ := by
  intro σ σ' hσ
  have hr : ∀ {n}, n < p + kk → (rot p kk (.v n)).substT σ = (rot p kk (.v n)).substT σ' := by
    intro n hn; obtain ⟨m, e, hm⟩ := rot_v hn; rw [e]; exact hσ m hm
  refine ⟨?_, ?_⟩
  · simp only [Flick.Ty.substT_comp]
    exact trT_substT_congr N fun n hn => hr (h n hn)
  · simp only [Facts.substT_comp]
    exact trBd_substT_congr N fun b hb' n hn => hr (hb b hb' n hn)

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
