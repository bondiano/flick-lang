import Formal.Equiv.Die

/-!
# Equivalence of every run: the left side seen from the right

The run-level theorems need the right side's steps reflected on the left (`Formal/Equiv/Refl.lean`).
This module reads the left side from the right one: a value related at `top` to a value is one
(`Sim.top_value_rev`), a frame of the right side is one of the left side (`Sim.frame_rev`), the
focus of a step of the left side has a right counterpart that is one too once the right side has
caught up (`Sim.redex_cu`), and a left thread related to a finished one is finished or steps
alone (`Sim.final_rev`).
-/

namespace Flick.Equiv

open Flick.Elab (All2 ORel)

variable {W : World}

theorem Sim.top_head_rev {x : Pos} {f f' : Tm} (h : Sim W x f f') (hx : x = .top) (hf : Value f')
    {K : Name} (hK : f'.head = .ctor K) : f.head = .ctor K := by
  induction h with
  | app _ _ _ ih₁ _ =>
    simp only [Tm.head] at hK ⊢
    cases hf with
    | ctorApp hf' _ _ => exact ih₁ hx hf' hK
    | constApp _ _ => simp [Tm.head] at hK
  | jL hx' | jR hx' | againF2 hx' | againR2 hx' => exact (hx' hx).elim
  | dropA | dropB | dropC | forceA | forceC | bornA | bornB | bornS | bornV | dead
  | o4A | o4B | o4Alloc1 | o4AllocR | o4Bind | o4Settle
  | doneF | doneR | againF1 | againF3 | againR1 | againR3 | letRaise | letUnw | letTodo => cases hx
  | o4Fix1 | o4Fix2 => simp [Tm.head] at hK
  | appK | appV | rconsK | rconsV | updK | updV => cases hx
  | ctor => exact hK
  | _ => first | (cases hf; done) | simp [Tm.head] at hK

/-- A term related at `top` to a value is a value. -/
theorem Sim.top_value_rev {x : Pos} {a b : Tm} (h : Sim W x a b) (hx : x = .top) (hb : Value b) :
    Value a := by
  induction h with
  | app _ h₁ h₂ ih₁ ih₂ =>
    subst hx
    cases hb with
    | ctorApp hf hK he => exact .ctorApp (ih₁ rfl hf) (h₁.top_head_rev rfl hf hK) (ih₂ rfl he)
    | constApp hc he =>
      cases h₁ with
      | const => exact .constApp hc (ih₂ rfl he)
      | jL hx' => exact (hx' rfl).elim
  | rcons _ _ _ ih₁ ih₂ => cases hb with
    | rcons h₁ h₂ => subst hx; exact .rcons (ih₁ rfl h₁) (ih₂ rfl h₂)
  | jL hx' | jR hx' | againF2 hx' | againR2 hx' => exact (hx' hx).elim
  | dropA | dropB | dropC | forceA | forceC | bornA | bornB | bornS | bornV | dead
  | o4A | o4B | o4Alloc1 | o4AllocR | o4Bind | o4Settle
  | doneF | doneR | againF1 | againF3 | againR1 | againR3 | letRaise | letUnw | letTodo => cases hx
  | hloc => exact .loc
  | o4Fix1 | o4Fix2 => exact .fix
  | appK | appV | rconsK | rconsV | updK | updV => cases hx
  | _ => first | (cases hb; done) | constructor

/-- **A frame of the right side** related at `top` is a frame of the left side. -/
theorem Sim.frame_rev {r a₂ : Tm} {F₂ : Frame} (h : Sim W .top r (F₂.plug a₂)) :
    ∃ F a, r = F.plug a ∧ FrS W F F₂ ∧ Sim W .any a a₂ := by
  generalize hx : Pos.top = x at h
  cases F₂ with
  | appL e₂ =>
    simp only [Frame.plug] at h
    cases h with
    | app _ h₁ h₂ => subst hx; exact ⟨.appL _, _, rfl, .appL h₂.lower, h₁.lower⟩
    | _ => first | (cases hx; done) | (subst hx; contradiction)
  | appR v₂ =>
    simp only [Frame.plug] at h
    cases h with
    | app _ h₁ h₂ => subst hx; exact ⟨.appR _, _, rfl, .appR h₁.lower, h₂.lower⟩
    | _ => first | (cases hx; done) | (subst hx; contradiction)
  | let_ b₂ =>
    simp only [Frame.plug] at h
    cases h with
    | let_ h₁ h₂ => subst hx; exact ⟨.let_ _, _, rfl, .let_ h₂, h₁⟩
    | _ => first | (cases hx; done) | (subst hx; contradiction)
  | match_ al₂ =>
    simp only [Frame.plug] at h
    cases h with
    | match_ h₁ h₂ => subst hx; exact ⟨.match_ _, _, rfl, .match_ h₂, h₁⟩
    | _ => first | (cases hx; done) | (subst hx; contradiction)
  | rconsL l r₂ =>
    simp only [Frame.plug] at h
    cases h with
    | rcons _ h₁ h₂ => subst hx; exact ⟨.rconsL _ _, _, rfl, .rconsL h₂.lower, h₁.lower⟩
    | _ => first | (cases hx; done) | (subst hx; contradiction)
  | rconsR l v₂ =>
    simp only [Frame.plug] at h
    cases h with
    | rcons _ h₁ h₂ => subst hx; exact ⟨.rconsR _ _, _, rfl, .rconsR h₁.lower, h₂.lower⟩
    | _ => first | (cases hx; done) | (subst hx; contradiction)
  | proj l =>
    simp only [Frame.plug] at h
    cases h with
    | proj h₁ => subst hx; exact ⟨.proj _, _, rfl, .proj, h₁⟩
    | _ => first | (cases hx; done) | (subst hx; contradiction)
  | updL l e₂ =>
    simp only [Frame.plug] at h
    cases h with
    | update _ h₁ h₂ => subst hx; exact ⟨.updL _ _, _, rfl, .updL h₂, h₁⟩
    | _ => first | (cases hx; done) | (subst hx; contradiction)
  | updR v₂ l =>
    simp only [Frame.plug] at h
    cases h with
    | update _ h₁ h₂ => subst hx; exact ⟨.updR _ _, _, rfl, .updR h₁, h₂⟩
    | _ => first | (cases hx; done) | (subst hx; contradiction)
  | await =>
    simp only [Frame.plug] at h
    cases h with
    | await h₁ => subst hx; exact ⟨.await, _, rfl, .await, h₁⟩
    | _ => first | (cases hx; done) | (subst hx; contradiction)
  | try_ al =>
    simp only [Frame.plug] at h
    cases h with
    | try_ h₁ h₂ => subst hx; exact ⟨.try_ _, _, rfl, .try_ h₂, h₁⟩
    | _ => first | (cases hx; done) | (subst hx; contradiction)
  | raise =>
    simp only [Frame.plug] at h
    cases h with
    | raise h₁ => subst hx; exact ⟨.raise, _, rfl, .raise, h₁⟩
    | _ => first | (cases hx; done) | (subst hx; contradiction)
  | inside p =>
    simp only [Frame.plug] at h
    cases h with
    | inside h₁ => subst hx; exact ⟨.inside _, _, rfl, .inside, h₁⟩
    | _ => first | (cases hx; done) | (subst hx; contradiction)
  | hwith p ℓ E al r =>
    simp only [Frame.plug] at h
    cases h with
    | hwith hl ha hr h₁ => subst hx; exact ⟨.hwith _ _ _ _ _, _, rfl, .hwith hl ha hr, h₁⟩
    | _ => first | (cases hx; done) | (subst hx; contradiction)
  | hctlA p ℓ κ tl E al r k =>
    simp only [Frame.plug] at h
    cases h with
    | hctl hl hk ha hr h₁ hc => subst hx; exact ⟨.hctlA _ _ _ _ _ _ _ _, _, rfl, .hctlA hl hk ha hr hc, h₁⟩
    | _ => first | (cases hx; done) | (subst hx; contradiction)
  | hresB p ℓ a =>
    simp only [Frame.plug] at h
    cases h with
    | hres hl ha h₁ => subst hx; exact ⟨.hresB _ _ _, _, rfl, .hresB hl ha, h₁⟩
    | _ => first | (cases hx; done) | (subst hx; contradiction)
  | hdone p =>
    simp only [Frame.plug] at h
    cases h with
    | hdone h₁ => subst hx; exact ⟨.hdone _, _, rfl, .hdone, h₁⟩
    | _ => first | (cases hx; done) | (subst hx; contradiction)
  | hkeep q ℓ E al =>
    simp only [Frame.plug] at h
    cases h with
    | hkeep hl ha h₁ => subst hx; exact ⟨.hkeep _ _ _ _, _, rfl, .hkeep hl ha, h₁⟩
    | _ => first | (cases hx; done) | (subst hx; contradiction)
  | hkarmA q ℓ E al k =>
    simp only [Frame.plug] at h
    cases h with
    | hkarm hl ha h₁ hc => subst hx; exact ⟨.hkarmA _ _ _ _ _, _, rfl, .hkarmA hl ha hc, h₁⟩
    | _ => first | (cases hx; done) | (subst hx; contradiction)
  | force =>
    simp only [Frame.plug] at h
    cases h with
    | force h₁ => subst hx; exact ⟨.force, _, rfl, .force, h₁⟩
    | _ => first | (cases hx; done) | (subst hx; contradiction)
  | settle ℓ =>
    simp only [Frame.plug] at h
    cases h with
    | settle hl h₁ => subst hx; exact ⟨.settle _, _, rfl, .settle hl, h₁⟩
    | _ => first | (cases hx; done) | (subst hx; contradiction)

/-- The frame of a part of a focus is ordered: the parts before it are values. -/
theorem _root_.Flick.Redex.frame_ok {r a : Tm} {F : Frame} (hr : Redex r) (h : r = F.plug a) :
    F.ok := by
  subst h
  cases F with
  | appR f => exact hr.2 (.appL a) f trivial rfl
  | rconsR l v => exact hr.2 (.rconsL l a) v trivial rfl
  | updR v l => exact hr.2 (.updL l a) v trivial rfl
  | _ => trivial

/-- **The right counterpart of a focus** is one, once the right side has caught up its parts. -/
theorem Sim.redex_cu {r r₂ : Tm} (h : Sim W .top r r₂) (hr : Redex r) (hB : ¬ W.B)
    {μ₁ : List Tm} (μ₂ : List Tm) (hst : StR W μ₁ μ₂) :
    Redex r₂ ∨ ∃ W' μ₂' r₂', RJ W μ₂ r₂ W' μ₂' r₂' ∧ r₂'.pot < r₂.pot ∧ Sim W' .any r r₂' := by
  by_cases hr₂ : Redex r₂
  · exact .inl hr₂
  · right
    have hnv : ¬ Value r₂ := fun hv => hr.1 (h.top_value_rev rfl hv)
    obtain ⟨F₂, a₂, hF₂, rfl, ha₂⟩ : ∃ F₂ a₂, F₂.ok ∧ F₂.plug a₂ = r₂ ∧ ¬ Value a₂ :=
      Classical.byContradiction fun hc => hr₂ ⟨hnv, fun F a hF ha =>
        Classical.byContradiction fun hv => hc ⟨F, a, hF, ha, hv⟩⟩
    obtain ⟨F, a, rfl, hfs, hs⟩ := h.frame_rev
    obtain ⟨W', μ₂', v₂, rj, hv₂, s⟩ :=
      hs.vcu rfl (hr.2 F a (hr.frame_ok rfl) rfl) W μ₂ (World.le.refl _) hst (fun h => (hB h).elim)
    have hlt : v₂.pot < a₂.pot := by
      rcases rj.pot with ⟨-, rfl⟩ | h
      · exact (ha₂ hv₂).elim
      · exact h
    exact ⟨W', μ₂', F₂.plug v₂, rj.frame hF₂, pot_plug_lt [F₂] hlt, (hfs.mono rj.le).plug s⟩

theorem Sim.top_const_rev {x : Pos} {c : Const} {a : Tm} (h : Sim W x a (.const c))
    (hx : x = .top) : a = .const c := by
  cases h with
  | const => rfl
  | jL hx' => exact (hx' hx).elim
  | _ => cases hx

/-- A step of the left side that reads no region, at a store. -/
def LStep (μ : List Tm) (a : Tm) : Prop := ∃ μ' a', AStep μ a μ' a'

theorem LStep.frame {μ : List Tm} {a : Tm} (h : LStep μ a) {F : Frame} (hF : F.ok) :
    LStep μ (F.plug a) :=
  let ⟨μ', _, s⟩ := h; ⟨μ', _, s.plug (E₀ := [F]) (OkE.nil.cons' hF)⟩

theorem LStep.head {μ μ' : List Tm} {a a' : Tm} (h : ∀ Λ, RHead Λ μ a Λ μ' a') : LStep μ a :=
  ⟨μ', a', [], a, a', OkE.nil, rfl, rfl, .inr h⟩

theorem LStep.pure {μ : List Tm} {a a' : Tm} (h : PHead a a') : LStep μ a :=
  ⟨μ, a', [], a, a', OkE.nil, rfl, rfl, .inl ⟨h, rfl⟩⟩

/-- **A left term related to a value** is a value, or takes a step the right side does not. -/
theorem Sim.value_rev {x : Pos} {a b : Tm} (h : Sim W x a b) (hx : x = .any) (hb : Value b)
    (hB : ¬ W.B) {μ₂ : List Tm} (μ : List Tm) (hst : StR W μ μ₂) : Value a ∨ LStep μ a := by
  induction h with
  | @app x f f' a₀ a₀' _ h₁ h₂ ih₁ ih₂ =>
    subst hx
    have hfa : Value f' ∧ Value a₀' := by cases hb with
      | ctorApp hf _ he => exact ⟨hf, he⟩
      | constApp _ he => exact ⟨.const, he⟩
    rcases ih₁ rfl hfa.1 with hf | s
    · rcases ih₂ rfl hfa.2 with ha | s
      · left
        have t₁ := h₁.value_any hf hfa.1 .top
        cases hb with
        | ctorApp hf' hK _ => exact .ctorApp hf (t₁.top_head_rev rfl hf' hK) ha
        | constApp hc _ => rw [t₁.top_const_rev rfl]; exact .constApp hc ha
      · exact .inr (s.frame (F := .appR f) hf)
    · exact .inr (s.frame (F := .appL a₀) trivial)
  | @rcons x l e e' r r' _ h₁ h₂ ih₁ ih₂ =>
    subst hx
    cases hb with
    | rcons he hr =>
      rcases ih₁ rfl he with he' | s
      · rcases ih₂ rfl hr with hr' | s
        · exact .inl (.rcons he' hr')
        · exact .inr (s.frame (F := .rconsR l e) he')
      · exact .inr (s.frame (F := .rconsL l r) trivial)
  | dropA => exact .inr ⟨_, _, [.let_ _], _, _, OkE.nil.cons' trivial, rfl, rfl, .inr fun _ => .lazy⟩
  | dropB => exact .inr (.pure (.let_ .loc))
  | forceA => exact .inr ⟨_, _, [.force], _, _, OkE.nil.cons' trivial, rfl, rfl, .inr fun _ => .lazy⟩
  | doneF _ hd =>
    obtain ⟨-, -, -, -, -, -, hμ₁, -⟩ := hst.done _ _ _ _ hd
    exact .inr (.head fun _ => .forceDone hμ₁)
  | jL _ _ _ ih =>
    subst hx
    rcases ih rfl hb with hv | s
    · exact .inr (.head fun _ => .settleVal hv)
    · exact .inr (s.frame (F := .settle _) trivial)
  | bornA hB' | bornB hB' | bornS hB' | bornV hB' | dead hB'
  | hloc hB' | o4A hB' | o4B hB' | o4Fix1 hB' | o4Fix2 hB'
  | o4Alloc1 hB' | o4AllocR hB' | o4Bind hB' | o4Settle hB' => exact (hB hB').elim
  | appK | appV | rconsK | rconsV | updK | updV => cases hx
  | _ => first | (cases hb; done) | exact .inl (by constructor)

/-- A left term related to a raised error raises one, or takes a step the right side does not. -/
theorem Sim.raise_rev {x : Pos} {a b : Tm} (h : Sim W x a b) (hx : x = .any) {v : Tm}
    (hb : b = .raise v) (hv : Value v) (hB : ¬ W.B) {μ₂ : List Tm} (μ : List Tm)
    (hst : StR W μ μ₂) : Final a ∨ LStep μ a := by
  induction h with
  | raise h' =>
    cases hb
    rcases h'.value_rev (by subst hx; rfl) hv hB μ hst with hv' | s
    · exact .inl (.inr (.inl ⟨_, hv', rfl⟩))
    · exact .inr (s.frame (F := .raise) trivial)
  | dropA => exact .inr ⟨_, _, [.let_ _], _, _, OkE.nil.cons' trivial, rfl, rfl, .inr fun _ => .lazy⟩
  | dropB => exact .inr (.pure (.let_ .loc))
  | forceA => exact .inr ⟨_, _, [.force], _, _, OkE.nil.cons' trivial, rfl, rfl, .inr fun _ => .lazy⟩
  | doneF _ _ hv' => subst hb; cases hv'
  | jL _ _ _ ih =>
    subst hx
    rcases ih rfl hb with ((hw | ⟨w, hw, rfl⟩ | ⟨p, ℓ, c, rfl⟩) | s)
    · exact .inr (.head fun _ => .settleVal hw)
    · exact .inr (.head fun _ => .settleRaise hw)
    · exact .inr (.head fun _ => .settleUnw)
    · exact .inr (s.frame (F := .settle _) trivial)
  | bornA hB' | bornB hB' | bornS hB' | bornV hB' | dead hB'
  | hloc hB' | o4A hB' | o4B hB' | o4Fix1 hB' | o4Fix2 hB'
  | o4Alloc1 hB' | o4AllocR hB' | o4Bind hB' | o4Settle hB' => exact (hB hB').elim
  | appK | appV | rconsK | rconsV | updK | updV => cases hx
  | _ => cases hb

/-- A left term related to a signal is one, or takes a step the right side does not. -/
theorem Sim.unw_rev {x : Pos} {a b : Tm} (h : Sim W x a b) (hx : x = .any) {p ℓ : Nat} {c : Tm}
    (hb : b = .unw p ℓ c) (hB : ¬ W.B) (μ : List Tm) :
    Final a ∨ LStep μ a := by
  induction h with
  | unw => exact .inl (.inr (.inr ⟨_, _, _, rfl⟩))
  | dropA => exact .inr ⟨_, _, [.let_ _], _, _, OkE.nil.cons' trivial, rfl, rfl, .inr fun _ => .lazy⟩
  | dropB => exact .inr (.pure (.let_ .loc))
  | forceA => exact .inr ⟨_, _, [.force], _, _, OkE.nil.cons' trivial, rfl, rfl, .inr fun _ => .lazy⟩
  | doneF _ _ hv' => subst hb; cases hv'
  | jL _ _ _ ih =>
    subst hx
    rcases ih rfl hb with ((hw | ⟨w, hw, rfl⟩ | ⟨q, ℓ', c', rfl⟩) | s)
    · exact .inr (.head fun _ => .settleVal hw)
    · exact .inr (.head fun _ => .settleRaise hw)
    · exact .inr (.head fun _ => .settleUnw)
    · exact .inr (s.frame (F := .settle _) trivial)
  | bornA hB' | bornB hB' | bornS hB' | bornV hB' | dead hB'
  | hloc hB' | o4A hB' | o4B hB' | o4Fix1 hB' | o4Fix2 hB'
  | o4Alloc1 hB' | o4AllocR hB' | o4Bind hB' | o4Settle hB' => exact (hB hB').elim
  | appK | appV | rconsK | rconsV | updK | updV => cases hx
  | _ => cases hb

/-- **A left thread related to a finished one** is finished, or takes a step the right side does
not. -/
theorem ThR.final_rev {μ μ₂ : List Tm} {τ : Option (Nat × Nat)} {a b : Tm} (h : ThR W μ τ a b)
    (hb : Final b) (hst : StR W μ μ₂) (hB : ¬ W.B) : Final a ∨ LStep μ a := by
  rcases h with h | ⟨E, E', n, e, e', rfl, -, hE, -, -, -, hc, -⟩
  · rcases hb with hv | ⟨v, hv, rfl⟩ | ⟨p, ℓ, c, rfl⟩
    · exact (h.value_rev rfl hv hB μ hst).imp .inl id
    · exact h.raise_rev rfl rfl hv hB μ hst
    · exact h.unw_rev rfl rfl hB μ
  · exact .inr ⟨_, _, E, _, _, hE, rfl, rfl, .inr fun _ => .forcePending hc⟩

theorem FrL.handlesE' {E E₂ : List Frame} (h : FrL W E E₂) {Ef o : Name}
    (hh : ∀ G ∈ E₂, G.handles Ef o = none) : ∀ G ∈ E, G.handles Ef o = none := by
  induction h with
  | nil => intro G hG; cases hG
  | cons hF _ ih =>
    intro G hG
    rcases List.mem_cons.mp hG with rfl | hG
    · rw [hF.handles]; exact hh _ (List.mem_cons_self ..)
    · exact ih (fun G' hG' => hh G' (List.mem_cons_of_mem _ hG')) G hG
  | junkL _ _ ih =>
    intro G hG
    rcases List.mem_cons.mp hG with rfl | hG
    · simp [Frame.handles]
    · exact ih hh G hG
  | junkR _ _ ih => exact ih (fun G' hG' => hh G' (List.mem_cons_of_mem _ hG'))
  | againF _ _ _ _ _ _ _ _ ih | againR _ _ _ _ _ _ _ _ ih =>
    intro G hG
    rcases List.mem_cons.mp hG with rfl | hG
    · simp [Frame.handles]
    · rcases List.mem_cons.mp hG with rfl | hG
      · simp [Frame.handles]
      · exact ih (fun G' hG' => hh G' (List.mem_cons_of_mem _ (List.mem_cons_of_mem _ hG'))) G hG

end Flick.Equiv
