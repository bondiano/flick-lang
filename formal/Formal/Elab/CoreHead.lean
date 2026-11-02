import Formal.Elab.CoreRel
import Formal.Elab.CoreDictSem

/-!
# λFlick⁺ as a `Target`: head steps of related terms (P4a4c)

A pure or runtime head step of a term related by a congruence rule is matched by the same
rule on the other side, with related results, store and regions (`phead_sim`, `rhead_sim`).
The other side needs no step before it: the values at the redex are related to values
(`Cg.value`). A base pair at the redex itself is left to the caller.
-/

namespace Flick.Elab

/-- Lists of alternatives. -/
def IsAlts : Flick.Tm → Prop
  | .anil | .acons .. | .aguard .. => True
  | _ => False

/-- The left side of a base pair is never a list of alternatives. -/
def BNoAlts (B : Flick.Tm → Flick.Tm → Prop) : Prop := ∀ a b, B a b → ¬ IsAlts a

theorem DP.noAlts {S : Sig} {root : Roots} : BNoAlts (DP S root) := by
  rintro a b ⟨hv, -, -, Θ, t, ht⟩ ha
  cases a <;> simp [IsAlts] at ha <;> cases ht <;> exact hv ‹_›

variable {B : Flick.Tm → Flick.Tm → Prop}

theorem All2.mono {α β : Type} {R R' : α → β → Prop} {l : List α} {l' : List β} (h : All2 R l l')
    (f : ∀ a b, R a b → R' a b) : All2 R' l l' := by
  induction h with
  | nil => exact .nil
  | cons r _ ih => exact .cons (f _ _ r) ih

theorem SubR.inst {vs ws : List Flick.Tm} (h : All2 (fun a b => ∀ x, Cg B x a b) vs ws) :
    SubR B (Flick.inst vs) (Flick.inst ws) := by
  intro i x
  unfold Flick.inst
  have hl := h.length
  by_cases hi : i < vs.length
  · rw [dif_pos hi, dif_pos (hl ▸ hi)]; exact h.get i hi (hl ▸ hi) x
  · rw [dif_neg hi, dif_neg (hl ▸ hi), hl]; exact .var

theorem Cg.setField {v v₂ w w₂ : Flick.Tm} (h : Cg B true v v₂) (hw : Cg B true w w₂) (l : Label) :
    ORel (Cg B true) (v.setField l w) (v₂.setField l w₂) := by
  induction v generalizing v₂ with
  | rcons l' e r _ ihr =>
    cases h with
    | rcons h₁ h₂ =>
      simp only [Flick.Tm.setField]
      split
      · exact .rcons hw h₂
      · have := ihr h₂
        revert this
        cases (_ : Flick.Tm).setField l w <;> cases (_ : Flick.Tm).setField l w₂ <;>
          simp [ORel] <;> exact fun h => .rcons h₁ h
  | _ => cases h <;> simp [Flick.Tm.setField, ORel]

/-- Matched values, related at either index. -/
theorem matchP_sub {p : Pat} {v v₂ : Flick.Tm} (h : Cg B true v v₂) {δ : List Flick.Tm}
    (hm : Flick.matchP p v = some δ) :
    ∃ δ₂, Flick.matchP p v₂ = some δ₂ ∧ SubR B (Flick.inst δ) (Flick.inst δ₂) := by
  obtain ⟨δ₂, h₂, hδ⟩ := (Cg.matchP (p := p) h).some_left hm
  refine ⟨δ₂, h₂, SubR.inst (hδ.mono fun a b r x => ?_)⟩
  cases x
  · exact r.lower
  · exact r

/-- **A pure head step of related terms.** -/
theorem phead_sim (hC : BClosed B) (hV : BNoVal B) (hA : BNoAlts B) {r r' r₂ : Flick.Tm}
    (hh : PHead r r') (h : Cg B false r r₂) : B r r₂ ∨ ∃ r₂', PHead r₂ r₂' ∧ Cg B false r' r₂' := by
  have nb : ∀ {a b : Flick.Tm}, B a b → Value a → False := fun hb hv => hV _ _ hb hv
  cases hh with
  | beta hv =>
    cases h with
    | base hb => exact .inl hb
    | app h₁ h₂ =>
      cases h₁ with
      | lam hb =>
        obtain ⟨hv₂, -, hy⟩ := Cg.value hV hv h₂
        exact .inr ⟨_, .beta hv₂, hb.subst hC _ _ (SubR.inst (.cons hy .nil))⟩
      | base hb => exact (nb hb .lam).elim
  | fix hv =>
    cases h with
    | base hb => exact .inl hb
    | app h₁ h₂ =>
      cases h₁ with
      | fix hb =>
        obtain ⟨hv₂, -, hy⟩ := Cg.value hV hv h₂
        exact .inr ⟨_, .fix hv₂, hb.subst hC _ _
          (SubR.inst (.cons hy (.cons (fun _ => .fix hb) .nil)))⟩
      | base hb => exact (nb hb .fix).elim
  | let_ hv =>
    cases h with
    | base hb => exact .inl hb
    | let_ h₁ h₂ =>
      obtain ⟨hv₂, -, hy⟩ := Cg.value hV hv h₁
      exact .inr ⟨_, .let_ hv₂, h₂.subst hC _ _ (SubR.inst (.cons hy .nil))⟩
  | letrec =>
    cases h with
    | base hb => exact .inl hb
    | letrec h₁ h₂ =>
      exact .inr ⟨_, .letrec, h₂.subst hC _ _ (SubR.inst (.cons (fun _ => .fix h₁) .nil))⟩
  | proj hv hf =>
    cases h with
    | base hb => exact .inl hb
    | proj h₁ =>
      obtain ⟨hv₂, -, hy⟩ := Cg.value hV hv h₁
      obtain ⟨w₂, hw₂, hw⟩ := ((hy true).field _).some_left hf
      exact .inr ⟨_, .proj hv₂ hw₂, hw.lower⟩
  | update hv hw hs =>
    cases h with
    | base hb => exact .inl hb
    | update h₁ h₂ =>
      obtain ⟨hv₂, -, hy₁⟩ := Cg.value hV hv h₁
      obtain ⟨hw₂, -, hy₂⟩ := Cg.value hV hw h₂
      obtain ⟨v₂', hs₂, hr⟩ := ((hy₁ true).setField (hy₂ true) _).some_left hs
      exact .inr ⟨_, .update hv₂ hw₂ hs₂, hr.lower⟩
  | matchYes hv hm =>
    cases h with
    | base hb => exact .inl hb
    | match_ h₁ h₂ =>
      obtain ⟨hv₂, -, hy⟩ := Cg.value hV hv h₁
      cases h₂ with
      | acons hb _ =>
        obtain ⟨δ₂, hm₂, hσ⟩ := matchP_sub (hy true) hm
        exact .inr ⟨_, .matchYes hv₂ hm₂, hb.subst hC _ _ hσ⟩
      | base hb => exact (hA _ _ hb trivial).elim
  | matchNo hv hm =>
    cases h with
    | base hb => exact .inl hb
    | match_ h₁ h₂ =>
      obtain ⟨hv₂, -, hy⟩ := Cg.value hV hv h₁
      cases h₂ with
      | acons _ hr =>
        exact .inr ⟨_, .matchNo hv₂ ((Cg.matchP (hy true)).none_left hm), .match_ h₁ hr⟩
      | base hb => exact (hA _ _ hb trivial).elim
  | guardYes hv hm =>
    cases h with
    | base hb => exact .inl hb
    | match_ h₁ h₂ =>
      obtain ⟨hv₂, -, hy⟩ := Cg.value hV hv h₁
      cases h₂ with
      | aguard hg hb hr =>
        obtain ⟨δ₂, hm₂, hσ⟩ := matchP_sub (hy true) hm
        exact .inr ⟨_, .guardYes hv₂ hm₂, .match_ (hg.subst hC _ _ hσ)
          (.acons (hb.subst hC _ _ hσ) (.acons (.match_ h₁ hr) .anil))⟩
      | base hb => exact (hA _ _ hb trivial).elim
  | guardNo hv hm =>
    cases h with
    | base hb => exact .inl hb
    | match_ h₁ h₂ =>
      obtain ⟨hv₂, -, hy⟩ := Cg.value hV hv h₁
      cases h₂ with
      | aguard _ _ hr =>
        exact .inr ⟨_, .guardNo hv₂ ((Cg.matchP (hy true)).none_left hm), .match_ h₁ hr⟩
      | base hb => exact (hA _ _ hb trivial).elim
  | tryVal hv =>
    cases h with
    | base hb => exact .inl hb
    | try_ h₁ _ => exact .inr ⟨_, .tryVal (Cg.value hV hv h₁).1, h₁⟩
  | withVal hv =>
    cases h with
    | base hb => exact .inl hb
    | with_ _ h₂ => exact .inr ⟨_, .withVal (Cg.value hV hv h₂).1, h₂⟩
  | bind hv =>
    cases h with
    | base hb => exact .inl hb
    | bind h₁ => exact .inr ⟨_, .bind (Cg.value hV hv h₁).1, .econs h₁ .enil⟩
  | join h₁ h₂ =>
    cases h with
    | base hb => exact .inl hb
    | join a b => exact .inr ⟨_, .join (a.isEnv hV h₁) (b.isEnv hV h₂), a.envAppend hV h₁ b⟩
  | intEq =>
    cases h with
    | base hb => exact .inl hb
    | app h₁ h₂ =>
      cases h₂ with
      | base hb => exact (nb hb .lit).elim
      | lit =>
        cases h₁ with
        | base hb => exact (nb hb (.constApp (.inl rfl) .lit)).elim
        | app h₃ h₄ =>
          cases h₃ with
          | base hb => exact (nb hb .const).elim
          | const =>
            cases h₄ with
            | base hb => exact (nb hb .lit).elim
            | lit => exact .inr ⟨_, .intEq, .lit⟩

/-- Related stores and regions. -/
def StR (B : Flick.Tm → Flick.Tm → Prop) (Λ Λ₂ : List Rgn) (μ μ₂ : List Flick.Tm) : Prop :=
  All2 (RgR B) Λ Λ₂ ∧ All2 (Cg B false) μ μ₂

theorem RgR.new : RgR B Rgn.new Rgn.new := ⟨rfl, .nil, .nil⟩

theorem live_rel {Λ Λ₂ : List Rgn} (h : All2 (RgR B) Λ Λ₂) {p : Nat} {g : Rgn}
    (hg : Λ[p]? = some g) (hl : g.live = true) :
    ∃ g₂, Λ₂[p]? = some g₂ ∧ g₂.live = true ∧ RgR B g g₂ := by
  obtain ⟨g₂, hg₂, r⟩ := h.getElem? hg
  exact ⟨g₂, hg₂, r.1 ▸ hl, r⟩

/-- **A runtime head step of related terms.** -/
theorem rhead_sim (hV : BNoVal B) (hR : BNoRaise B) (hA : BNoAlts B) (hC : BClosed B)
    {Λ Λ' Λ₂ : List Rgn} {μ μ' μ₂ : List Flick.Tm} {r r' r₂ : Flick.Tm}
    (hh : RHead Λ μ r Λ' μ' r') (h : Cg B false r r₂) (hs : StR B Λ Λ₂ μ μ₂) :
    B r r₂ ∨ ∃ Λ₂' μ₂' r₂', RHead Λ₂ μ₂ r₂ Λ₂' μ₂' r₂' ∧ Cg B false r' r₂' ∧ StR B Λ' Λ₂' μ' μ₂' := by
  have nb : ∀ {a b : Flick.Tm}, B a b → Value a → False := fun hb hv => hV _ _ hb hv
  obtain ⟨hΛ, hμ⟩ := hs
  have setR : ∀ {Λ Λ₂ : List Rgn} {g g₂ : Rgn} (p : Nat), All2 (RgR B) Λ Λ₂ → RgR B g g₂ →
      All2 (RgR B) (setRgn Λ p g) (setRgn Λ₂ p g₂) := fun p h r => h.set p r
  cases hh with
  | refMake hv =>
    cases h with
    | base hb => exact .inl hb
    | app h₁ h₂ =>
      cases h₁ with
      | const =>
        refine .inr ⟨_, _, _, .refMake (Cg.value hV hv h₂).1, ?_, hΛ, hμ.app (.cons h₂ .nil)⟩
        rw [hμ.length]; exact .loc
      | base hb => exact (nb hb .const).elim
  | refGet hl =>
    cases h with
    | base hb => exact .inl hb
    | app h₁ h₂ =>
      cases h₁ with
      | const =>
        cases h₂ with
        | loc =>
          obtain ⟨v₂, hv₂, r⟩ := hμ.getElem? hl
          exact .inr ⟨_, _, _, .refGet hv₂, r, hΛ, hμ⟩
        | base hb => exact (nb hb .loc).elim
      | base hb => exact (nb hb .const).elim
  | refSet hv hl =>
    cases h with
    | base hb => exact .inl hb
    | app h₁ h₂ =>
      cases h₁ with
      | app h₃ h₄ =>
        cases h₃ with
        | const =>
          cases h₄ with
          | loc =>
            exact .inr ⟨_, _, _, .refSet (Cg.value hV hv h₂).1 (hμ.lt hl), .lit, hΛ, hμ.set _ h₂⟩
          | base hb => exact (nb hb .loc).elim
        | base hb => exact (nb hb .const).elim
      | base hb => exact (nb hb (.constApp (.inl rfl) .loc)).elim
  | await =>
    cases h with
    | base hb => exact .inl hb
    | await h₁ =>
      cases h₁ with
      | task he => exact .inr ⟨_, _, _, .await, he, hΛ, hμ⟩
      | base hb => exact (nb hb .task).elim
  | build hv hb =>
    cases h with
    | base hb => exact .inl hb
    | provide h₁ h₂ =>
      obtain ⟨b₂, hb₂, r⟩ := (h₁.build hV hv).some_left hb
      exact .inr ⟨_, _, _, .build (Cg.value hV hv h₁).1 hb₂, .provide r h₂, hΛ, hμ⟩
  | provideEnv hθ =>
    cases h with
    | base hb => exact .inl hb
    | provide h₁ h₂ =>
      refine .inr ⟨_, _, _, .provideEnv (h₁.isEnv hV hθ), ?_, hΛ.app (.cons RgR.new .nil), hμ⟩
      rw [hΛ.length]; exact .inside (.with_ h₁ h₂)
  | tryRaise hv =>
    cases h with
    | base hb => exact .inl hb
    | try_ h₁ h₂ =>
      cases h₁ with
      | raise h' => exact .inr ⟨_, _, _, .tryRaise (Cg.value hV hv h').1, .handle h' h₂, hΛ, hμ⟩
      | base hb => exact (hR _ _ hb).elim
  | handleYes hv hm =>
    cases h with
    | base hb => exact .inl hb
    | handle h₁ h₂ =>
      obtain ⟨hv₂, -, hy⟩ := Cg.value hV hv h₁
      cases h₂ with
      | acons hb _ =>
        obtain ⟨δ₂, hm₂, hσ⟩ := matchP_sub (hy true) hm
        exact .inr ⟨_, _, _, .handleYes hv₂ hm₂, hb.subst hC _ _ hσ, hΛ, hμ⟩
      | base hb => exact (hA _ _ hb trivial).elim
  | handleNo hv hm =>
    cases h with
    | base hb => exact .inl hb
    | handle h₁ h₂ =>
      obtain ⟨hv₂, -, hy⟩ := Cg.value hV hv h₁
      cases h₂ with
      | acons _ hr =>
        exact .inr ⟨_, _, _, .handleNo hv₂ ((Cg.matchP (hy true)).none_left hm), .handle h₁ hr,
          hΛ, hμ⟩
      | base hb => exact (hA _ _ hb trivial).elim
  | hguardYes hv hm =>
    cases h with
    | base hb => exact .inl hb
    | handle h₁ h₂ =>
      obtain ⟨hv₂, -, hy⟩ := Cg.value hV hv h₁
      cases h₂ with
      | aguard hg hb hr =>
        obtain ⟨δ₂, hm₂, hσ⟩ := matchP_sub (hy true) hm
        exact .inr ⟨_, _, _, .hguardYes hv₂ hm₂, .match_ (hg.subst hC _ _ hσ)
          (.acons (hb.subst hC _ _ hσ) (.acons (.handle h₁ hr) .anil)), hΛ, hμ⟩
      | base hb => exact (hA _ _ hb trivial).elim
  | hguardNo hv hm =>
    cases h with
    | base hb => exact .inl hb
    | handle h₁ h₂ =>
      obtain ⟨hv₂, -, hy⟩ := Cg.value hV hv h₁
      cases h₂ with
      | aguard _ _ hr =>
        exact .inr ⟨_, _, _, .hguardNo hv₂ ((Cg.matchP (hy true)).none_left hm), .handle h₁ hr,
          hΛ, hμ⟩
      | base hb => exact (hA _ _ hb trivial).elim
  | handleNil hv =>
    cases h with
    | base hb => exact .inl hb
    | handle h₁ h₂ =>
      cases h₂ with
      | anil => exact .inr ⟨_, _, _, .handleNil (Cg.value hV hv h₁).1, .raise h₁, hΛ, hμ⟩
      | base hb => exact (hA _ _ hb trivial).elim
  | scope =>
    cases h with
    | base hb => exact .inl hb
    | scope h₁ =>
      refine .inr ⟨_, _, _, .scope, ?_, hΛ.app (.cons RgR.new .nil), hμ⟩
      rw [hΛ.length]
      exact .inside (h₁.subst hC _ _ (SubR.inst (.cons (fun _ => .own) .nil)))
  | open_ hg hl =>
    cases h with
    | base hb => exact .inl hb
    | app h₁ h₂ =>
      cases h₁ with
      | const =>
        cases h₂ with
        | own =>
          obtain ⟨g₂, hg₂, hl₂, r⟩ := live_rel hΛ hg hl
          exact .inr ⟨_, _, _, .open_ hg₂ hl₂, .cap,
            setR _ hΛ ⟨r.1, r.2.1, r.2.2.app (.cons (.app .const .cap) .nil)⟩, hμ⟩
        | base hb => exact (nb hb .own).elim
      | base hb => exact (nb hb .const).elim
  | op hg hl =>
    cases h with
    | base hb => exact .inl hb
    | app h₁ h₂ =>
      cases h₁ with
      | const =>
        cases h₂ with
        | cap =>
          obtain ⟨g₂, hg₂, hl₂, -⟩ := live_rel hΛ hg hl
          exact .inr ⟨_, _, _, .op hg₂ hl₂, .lit, hΛ, hμ⟩
        | base hb => exact (nb hb .cap).elim
      | base hb => exact (nb hb .const).elim
  | finRun hg hl hw hk hf =>
    cases h with
    | base hb => exact .inl hb
    | inside h₁ =>
      obtain ⟨g₂, hg₂, hl₂, r⟩ := live_rel hΛ hg hl
      obtain ⟨fs₂, f₂, hf₂, hfs, hff⟩ := r.2.2.snoc_right hf
      have hk₂ : ∀ k ∈ g₂.kids, Final k := fun k hk' => by
        obtain ⟨k₁, hk₁, rk⟩ := r.2.1.mem hk'
        exact rk.final hV hR (hk _ hk₁)
      exact .inr ⟨_, _, _, .finRun hg₂ hl₂ (h₁.final hV hR hw) hk₂ hf₂, .inside h₁,
        setR _ hΛ ⟨r.1, r.2.1.app (.cons hff .nil), hfs⟩, hμ⟩
  | close hg hl hw hk hf =>
    cases h with
    | base hb => exact .inl hb
    | inside h₁ =>
      obtain ⟨g₂, hg₂, hl₂, r⟩ := live_rel hΛ hg hl
      have hf₂ : g₂.fins = [] := by
        have := r.2.2; rw [hf] at this
        cases h' : g₂.fins with
        | nil => rfl
        | cons => rw [h'] at this; cases this
      have hk₂ : ∀ k ∈ g₂.kids, Final k := fun k hk' => by
        obtain ⟨k₁, hk₁, rk⟩ := r.2.1.mem hk'
        exact rk.final hV hR (hk _ hk₁)
      exact .inr ⟨_, _, _, .close hg₂ hl₂ (h₁.final hV hR hw) hk₂ hf₂, h₁,
        setR _ hΛ ⟨rfl, r.2.1, .nil⟩, hμ⟩

end Flick.Elab
