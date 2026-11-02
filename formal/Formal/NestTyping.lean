import Formal.NestClose

/-!
# What the types of a redex give `⊢ Λ`

The hole of a run-time step is typed with the facts of the order of creation; a
finalizer or a child holds only regions that outlive its owner (`docs/formal/regions.md#spawn-bounds-everything-the-task-holds`); the body of
a closing region shows no `π`; a service holds no region.
-/

namespace Flick

variable {S : Sig} {Ψ : List Ty} {B anc : Nat → Nat → Prop}

/-- The hole of a run-time step: typed with facts of the order of creation. -/
theorem hole_rt {Λ μ Λ' μ' E r r' t φ} (hh : RHead Λ μ r Λ' μ' r') (hE : OkE E)
    (h : Typ S Ψ Facts.age [] (plug E r) (.ex t φ)) :
    ∃ Θ' t' φ', Typ S Ψ Θ' [] r (.ex t' φ') ∧ ∀ C s, Θ' C s → Facts.age C s := by
  obtain ⟨K, Θ', t', φ', hr, _, hK, _⟩ := ctx_typing _ hE hh.not_value h
  obtain ⟨rfl, hΘ, -⟩ := hK.resolve_left fun hp => by subst hp; exact hh.not_pure hr
  rw [List.map_up_zero] at hr
  exact ⟨Θ', t', φ', hr, fun C s h => (hΘ.2 C s h).elim id id⟩

/-- The work a finalizer or a child is given holds regions that outlive the owner. -/
theorem owner_arg {Θ : Facts} {c : Const} {p : Nat} {T : Ty} {φ : Stage} {w : Tm}
    (h : Typ S Ψ Θ [] (.app (.app (.const c) (.own p)) w) (.ex T φ)) (hc : c = .addFin ∨ c = .spawn)
    (hΘ : ∀ C s, Θ C s → Facts.age C s) :
    ∃ U, Typ S Ψ Θ [] w (.ex U φ) ∧ ∀ x, U.reach (.r x) → x ≤ p := by
  cases h with
  | app hf ht _ _ =>
    cases hf with
    | app hc' ho _ hT₁ =>
      cases hc' with
      | const hok hT' =>
        have out : ∀ (C : Caps) (s : Ty), Outl Θ C s → s = .rg p → ∀ x, C (.r x) → x ≤ p := by
          intro C s hO hs x hx
          subst hs
          rcases hO _ hx with h' | ⟨C', h₁, h₂⟩
          · cases h'; exact Nat.le_refl _
          · exact Nat.le_of_lt (hΘ C' _ h₁ x p h₂ rfl)
        rcases hc with rfl | rfl
        · obtain ⟨_, _, _, _, h, la, _, lt, -⟩ := hT'.arrow_left
          cases h
          cases la
          obtain ⟨q, hq, hs⟩ := canonical_owner .own ho
          cases hq
          obtain ⟨_, _, _, _, h, la', _, _, -⟩ := (lt.trans hT₁).arrow_left
          cases h
          refine ⟨_, ht, fun x hx => out _ _ hok.1 hs x ?_⟩
          rcases la'.reach _ hx with h' | h'
          · cases h'
          · exact h'
        · obtain ⟨_, _, _, _, h, la, _, lt, -⟩ := hT'.arrow_left
          cases h
          cases la
          obtain ⟨q, hq, hs⟩ := canonical_owner .own ho
          cases hq
          obtain ⟨_, _, _, _, h, la', _, _, -⟩ := (lt.trans hT₁).arrow_left
          cases h
          refine ⟨_, ht, fun x hx => out _ _ hok.1 hs x ?_⟩
          rcases la'.reach _ hx with h' | h'
          · exact .inr h'
          · exact .inl h'
  | errVal hv' =>
    exfalso
    rcases hc with rfl | rfl <;>
    · cases hv' with
      | ctorApp _ hK _ => simp [Tm.head] at hK
      | constApp2 _ hl => simp [Const.layer] at hl

/-- The body of a closing region leaves it for the place around it. -/
theorem close_move {Θ : Facts} {p : Nat} {w : Tm} {T : Ty} {φ : Stage} {A : Nat → Prop}
    (h : Typ S Ψ Θ [] (.inside p w) (.ex T φ)) (hw : Final w)
    (hW : w.Within Ψ B anc (fun x => A x ∨ x = p)) (hA : ¬ A p) : w.Within Ψ B anc A := by
  cases h with
  | inside _ hb hφ' ht hε _ _ =>
    have iff : ∀ x, x ≠ p → ((A x ∨ x = p) ↔ A x) := fun x hx =>
      ⟨fun h => h.resolve_right hx, .inl⟩
    rcases hw with hv | ⟨v, hv, rfl⟩
    · exact hW.moveT hv hb fun x hx => iff x fun h => ht (h ▸ hx)
    · have hv' := raise_errors hv hb
      have : v.Within Ψ B anc A := Tm.Within.moveT hv hv' hW fun x hx => iff x fun h => hε (by
        subst h; exact Stage.le_err hφ' _ hx)
      exact this
  | errVal hv => exact absurd hv inside_not_value

theorem envLookup_value {θ v : Tm} {N : Name} (hθ : Value θ) (h : θ.envLookup N = some v) : Value v := by
  induction θ with
  | econs N' w θ' _ ih =>
    cases hθ with
    | econs hw hθ' =>
      simp only [Tm.envLookup] at h
      split at h
      · cases h; exact hw
      · exact ih hθ' h
  | _ => simp [Tm.envLookup] at h

theorem lookupE_value {v : Tm} {N : Name} {q : Nat} :
    ∀ {E : List Frame}, OkE E → lookupE E N = some (q, v) → Value v
  | [], _, h => by simp [lookupE] at h
  | F :: E, hE, h => by
    obtain ⟨hF, hE'⟩ := hE.cons
    simp only [lookupE] at h
    cases hF' : F.lookup N with
    | some w =>
      rw [hF'] at h; cases h
      cases F with
      | with_ q' θ =>
        simp only [Frame.lookup, Option.map_eq_some_iff] at hF'
        obtain ⟨w, hw, hwq⟩ := hF'
        cases hwq
        exact envLookup_value hF hw
      | _ => simp [Frame.lookup] at hF'
    | none => rw [hF'] at h; exact lookupE_value hE' h

end Flick
