import Formal.Equiv.Subst

/-!
# Contextual equivalence: related contexts

Related frames plug related terms into related terms (`FrS.plug`, `FrL.plugs`), and on the
spine of a parked body when both sides are ordered (`FrL.plugsP`). They install the same
handlers (`FrS.handles`), park the same parts and unwind to the same handler (`FrS.isCtl`,
`FrS.catches`): the cells of handlers are related one to one (`World.Bij`). A body parked at a hole
is related only to a body parked at a hole, with related and ordered frames (`Sim.plug_hole`).
-/

namespace Flick.Equiv

open Flick.Elab (All2 ORel)

variable {W : World}

/-- The related locations are a partial bijection. -/
def World.Bij (W : World) : Prop :=
  ∀ a a' b b' k₁ k₂, W.L a a' k₁ → W.L b b' k₂ → (a = b ↔ a' = b')

theorem FrS.plug {F F' : Frame} (h : FrS W F F') {a a' : Tm} (ha : Sim W .any a a') :
    Sim W .any (F.plug a) (F'.plug a') := by
  cases h <;> simp only [Frame.plug] <;> constructor <;> first | assumption | decide

theorem Frame.plug_spine {F : Frame} (hF : F.ok) {a : Tm} (ha : a.Spine) : (F.plug a).Spine := by
  cases F <;> simp only [Frame.plug, Tm.Spine] <;> first | exact ha | exact .inl ha | exact .inr ⟨hF, ha⟩

theorem plug_spine : ∀ {E : List Frame}, OkE E → ∀ {a : Tm}, a.Spine → (plug E a).Spine
  | [], _, _, ha => ha
  | _ :: _, hE, _, ha => plug_spine hE.cons.2 (Frame.plug_spine hE.cons.1 ha)

theorem Sim.top_head {x : Pos} {f f' : Tm} (h : Sim W x f f') (hx : x = .top) (hf : Value f)
    {K : Name} (hK : f.head = .ctor K) : f'.head = .ctor K := by
  induction h with
  | app _ _ _ ih₁ _ =>
    simp only [Tm.head] at hK ⊢
    cases hf with
    | ctorApp hf' _ _ => exact ih₁ hx hf' hK
    | constApp _ _ => simp [Tm.head] at hK
  | jL hx' | jR hx' | againF2 hx' | againR2 hx' => exact (hx' hx).elim
  | dropA | dropB | dropC | forceA | forceC | bornA | bornB | bornS | bornV | dead
  | o4A | o4B | o4Alloc1 | o4AllocR | o4Bind | o4Settle
  | doneF | doneR | againF1 | againF3 | againR1 | againR3
  | letRaise | letUnw | letTodo => cases hx
  | appK | appV | rconsK | rconsV | updK | updV => cases hx
  | ctor => exact hK
  | _ => first | (cases hf; done) | simp [Tm.head] at hK

/-- A value related at `top` is related to a value. -/
theorem Sim.top_value {x : Pos} {a b : Tm} (h : Sim W x a b) (hx : x = .top) (ha : Value a) :
    Value b := by
  induction h with
  | app _ h₁ h₂ ih₁ ih₂ =>
    subst hx
    cases ha with
    | ctorApp hf hK he => exact .ctorApp (ih₁ rfl hf) (h₁.top_head rfl hf hK) (ih₂ rfl he)
    | constApp hc he =>
      cases h₁ with
      | const => exact .constApp hc (ih₂ rfl he)
      | jR hx' => exact (hx' rfl).elim
  | rcons _ _ _ ih₁ ih₂ => cases ha with
    | rcons h₁ h₂ => subst hx; exact .rcons (ih₁ rfl h₁) (ih₂ rfl h₂)
  | jL hx' | jR hx' | againF2 hx' | againR2 hx' => exact (hx' hx).elim
  | dropA | dropB | dropC | forceA | forceC | bornA | bornB | bornS | bornV | dead
  | o4A | o4B | o4Alloc1 | o4AllocR | o4Bind | o4Settle
  | doneF | doneR | againF1 | againF3 | againR1 | againR3
  | letRaise | letUnw | letTodo => cases hx
  | appK | appV | rconsK | rconsV | updK | updV => cases hx
  | hloc => exact .loc
  | o4Fix1 | o4Fix2 => exact .fix
  | _ => first | (cases ha; done) | constructor

theorem FrS.plugP {F F' : Frame} (h : FrS W F F') (hF : F.ok) (hF' : F'.ok) {a a' : Tm}
    (hs : a.Spine) (ha : Sim W .park a a') : Sim W .park (F.plug a) (F'.plug a') := by
  cases h with
  | appL he => exact .appK hs ha he
  | appR hv => exact .appV hF (hv.value_any hF hF' _) ha
  | rconsL hr => exact .rconsK hs ha hr
  | rconsR hv => exact .rconsV hF (hv.value_any hF hF' _) ha
  | updL he => exact .updK hs ha he
  | updR hv => exact .updV hF (hv.value_any hF hF' _) ha
  | _ => simp only [Frame.plug]; constructor <;> assumption

theorem FrL.plugs {E E' : List Frame} (h : FrL W E E') :
    ∀ {a a' : Tm}, Sim W .any a a' → Sim W .any (plug E a) (plug E' a') := by
  induction h with
  | nil => exact id
  | cons hF _ ih => exact fun ha => ih (hF.plug ha)
  | junkL hn _ ih => exact fun ha => ih (.jL (by decide) hn ha)
  | junkR hn _ ih => exact fun ha => ih (.jR (by decide) hn ha)
  | againF hB hl hc hσ hσn hσv hσv' _ ih =>
    exact fun ha => ih (.againF2 (by decide) hB hl ha hc hσ hσn hσv hσv')
  | againR hB hl hc hσ hσn hσv hσv' _ ih =>
    exact fun ha => ih (.againR2 (by decide) hB hl ha hc hσ hσn hσv hσv')

theorem FrL.plugsP {E E' : List Frame} (h : FrL W E E') (hE : OkE E) (hE' : OkE E') :
    ∀ {a a' : Tm}, a.Spine → Sim W .park a a' → Sim W .park (plug E a) (plug E' a') := by
  induction h with
  | nil => exact fun _ h => h
  | cons hF _ ih =>
    obtain ⟨h₁, h₂⟩ := hE.cons
    obtain ⟨h₁', h₂'⟩ := hE'.cons
    exact fun hs ha => ih h₂ h₂' (Frame.plug_spine h₁ hs) (hF.plugP h₁ h₁' hs ha)
  | junkL hn _ ih =>
    exact fun hs ha => ih hE.cons.2 hE' (by simpa [Frame.plug, Tm.Spine] using hs)
      (.jL (by decide) hn ha)
  | junkR hn _ ih => exact fun hs ha => ih hE hE'.cons.2 hs (.jR (by decide) hn ha)
  | againF hB hl hc hσ hσn hσv hσv' _ ih =>
    exact fun hs ha => ih hE.cons.2.cons.2 hE'.cons.2.cons.2
      (by simpa [Frame.plug, Tm.Spine] using hs) (.againF2 (by decide) hB hl ha hc hσ hσn hσv hσv')
  | againR hB hl hc hσ hσn hσv hσv' _ ih =>
    exact fun hs ha => ih hE.cons.2.cons.2 hE'.cons.2.cons.2
      (by simpa [Frame.plug, Tm.Spine] using hs) (.againR2 (by decide) hB hl ha hc hσ hσn hσv hσv')

theorem FrS.isTry {F F' : Frame} (h : FrS W F F') : F.isTry ↔ F'.isTry := by
  cases h <;> simp [Frame.isTry]

theorem FrS.isInside {F F' : Frame} (h : FrS W F F') : F.isInside ↔ F'.isInside := by
  cases h <;> simp [Frame.isInside]

theorem FrS.isCtl {F F' : Frame} (h : FrS W F F') : F.isCtl ↔ F'.isCtl := by
  cases h <;> simp [Frame.isCtl]

theorem FrS.resumes {F F' : Frame} (h : FrS W F F') (p : Nat) : F.resumes p ↔ F'.resumes p := by
  cases h <;> simp [Frame.resumes]

theorem FrS.catches (hB : W.Bij) {F F' : Frame} (h : FrS W F F') {p ℓ ℓ' : Nat} {k : Kd}
    (hl : W.L ℓ ℓ' k) : F.catches p ℓ ↔ F'.catches p ℓ' := by
  cases h with
  | hwith hl' =>
    simp only [Frame.catches]
    rw [hB _ _ _ _ _ _ hl' hl]
  | _ => simp [Frame.catches]

/-- Two arms: related, or no run takes them (`Sim.aconsD`). -/
def ArmR (W : World) (b b' : Tm) : Prop :=
  Sim W .any b b' ∨ ((b.LzTop .any ∨ b'.LzTop .any) ∧ b'.NoIns)

theorem ArmR.mono {W' : World} (hW : W.le W') {b b' : Tm} (h : ArmR W b b') : ArmR W' b b' :=
  h.imp (·.mono hW) id

theorem armOf_eq {o : Name} {x : Pos} {al al' : Tm} (h : Sim W x al al') (hx : x = .top) :
    ORel (ArmR W) (al.armOf o) (al'.armOf o) := by
  induction h with
  | @acons _ p _ _ _ _ hb _ _ ihr =>
    simp only [Tm.armOf]
    by_cases hc : p.headName = some o
    · simp only [hc, if_true]; exact .inl hb
    · simp only [hc, if_false]; exact ihr hx
  | @aconsD _ p _ _ _ _ hd hn _ ihr =>
    simp only [Tm.armOf]
    by_cases hc : p.headName = some o
    · simp only [hc, if_true]; exact .inr ⟨hd, hn⟩
    · simp only [hc, if_false]; exact ihr hx
  | jL hx' | jR hx' | againF2 hx' | againR2 hx' => exact (hx' hx).elim
  | dropA | dropB | dropC | forceA | forceC | bornA | bornB | bornS | bornV | dead
  | o4A | o4B | o4Alloc1 | o4AllocR | o4Bind | o4Settle
  | doneF | doneR | againF1 | againF3 | againR1 | againR3
  | letRaise | letUnw | letTodo => cases hx
  | _ => simp [Tm.armOf, ORel]

theorem FrS.handles {F F' : Frame} (h : FrS W F F') (E o : Name) :
    F.handles E o = F'.handles E o := by
  cases h with
  | hwith _ hal =>
    have := armOf_eq (o := o) hal rfl
    simp only [Frame.handles]
    revert this
    cases (_ : Tm).armOf o <;> cases (_ : Tm).armOf o <;> simp [ORel]
  | hkeep _ hal =>
    have := armOf_eq (o := o) hal rfl
    simp only [Frame.handles]
    revert this
    cases (_ : Tm).armOf o <;> cases (_ : Tm).armOf o <;> simp [ORel]
  | _ => simp [Frame.handles]

theorem FrL.handlesE {E E₂ : List Frame} (h : FrL W E E₂) {Ef o : Name}
    (hh : ∀ G ∈ E, G.handles Ef o = none) : ∀ G ∈ E₂, G.handles Ef o = none := by
  induction h with
  | nil => intro G hG; cases hG
  | cons hF _ ih =>
    intro G hG
    rcases List.mem_cons.mp hG with rfl | hG
    · rw [← hF.handles]; exact hh _ (List.mem_cons_self ..)
    · exact ih (fun G' hG' => hh G' (List.mem_cons_of_mem _ hG')) G hG
  | junkL _ _ ih => exact ih (fun G' hG' => hh G' (List.mem_cons_of_mem _ hG'))
  | junkR _ _ ih =>
    intro G hG
    rcases List.mem_cons.mp hG with rfl | hG
    · simp [Frame.handles]
    · exact ih hh G hG
  | againF _ _ _ _ _ _ _ _ ih | againR _ _ _ _ _ _ _ _ ih =>
    intro G hG
    rcases List.mem_cons.mp hG with rfl | hG
    · simp [Frame.handles]
    · rcases List.mem_cons.mp hG with rfl | hG
      · simp [Frame.handles]
      · exact ih (fun G' hG' => hh G' (List.mem_cons_of_mem _ (List.mem_cons_of_mem _ hG'))) G hG

theorem FrL.resumesE {E E₂ : List Frame} (h : FrL W E E₂) {p : Nat}
    (hn : ∀ G ∈ E, ¬ G.resumes p) : ∀ G ∈ E₂, ¬ G.resumes p := by
  induction h with
  | nil => intro G hG; cases hG
  | cons hF _ ih =>
    intro G hG
    rcases List.mem_cons.mp hG with rfl | hG
    · exact fun h => hn _ (List.mem_cons_self ..) ((hF.resumes p).2 h)
    · exact ih (fun G' hG' => hn G' (List.mem_cons_of_mem _ hG')) G hG
  | junkL _ _ ih => exact ih (fun G' hG' => hn G' (List.mem_cons_of_mem _ hG'))
  | junkR _ _ ih =>
    intro G hG
    rcases List.mem_cons.mp hG with rfl | hG
    · simp [Frame.resumes]
    · exact ih hn G hG
  | againF _ _ _ _ _ _ _ _ ih | againR _ _ _ _ _ _ _ _ ih =>
    intro G hG
    rcases List.mem_cons.mp hG with rfl | hG
    · simp [Frame.resumes]
    · rcases List.mem_cons.mp hG with rfl | hG
      · simp [Frame.resumes]
      · exact ih (fun G' hG' => hn G' (List.mem_cons_of_mem _ (List.mem_cons_of_mem _ hG'))) G hG

/-- The handlers a child inherits from related contexts. -/
theorem FrL.withs {E E' : List Frame} (h : FrL W E E') (p : Nat) :
    ∀ {a a' : Tm}, Sim W .any a a' → Sim W .any (withs p E a) (withs p E' a') := by
  induction h with
  | nil => exact id
  | cons hF _ ih =>
    intro a a' ha
    refine ih ?_
    cases hF <;> simp only [Frame.wrap] <;> first
      | exact ha
      | (split <;> first | exact .hkeep ‹_› ‹_› ha | exact ha)
  | junkL _ _ ih => intro a a' ha; exact ih ha
  | junkR _ _ ih => intro a a' ha; exact ih ha
  | againF _ _ _ _ _ _ _ _ ih | againR _ _ _ _ _ _ _ _ ih =>
    intro a a' ha; exact ih ha

/-- Split the head frame of the left context when it is no cell's frame. -/
theorem FrL.split_head {F : Frame} (hF : ∀ n, F ≠ .settle n) :
    ∀ {B E₂ : List Frame}, FrL W (F :: B) E₂ →
      ∃ A₂ F₂ B₂, E₂ = A₂ ++ F₂ :: B₂ ∧ FrL W [] A₂ ∧ FrS W F F₂ ∧ FrL W B B₂ := by
  intro B E₂ h
  generalize hl : F :: B = l at h
  induction h with
  | nil => cases hl
  | cons hF' h' => cases hl; exact ⟨[], _, _, rfl, .nil, hF', h'⟩
  | junkL _ _ => cases hl; exact (hF _ rfl).elim
  | junkR hn _ ih =>
    obtain ⟨A₂, F₂, B₂, rfl, hA, hF₂, hB⟩ := ih hl
    exact ⟨.settle _ :: A₂, F₂, B₂, rfl, .junkR hn hA, hF₂, hB⟩
  | againF | againR => cases hl; exact (hF _ rfl).elim

private theorem FrL.split_left_aux {F : Frame} (hF : ∀ n, F ≠ .settle n) (hFl : ∀ b, F ≠ .let_ b) :
    ∀ (m : Nat) {A B E₂ : List Frame}, A.length ≤ m → FrL W (A ++ F :: B) E₂ →
      ∃ A₂ F₂ B₂, E₂ = A₂ ++ F₂ :: B₂ ∧ FrL W A A₂ ∧ FrS W F F₂ ∧ FrL W B B₂ := by
  intro m
  induction m with
  | zero =>
    intro A B E₂ hm h
    cases A with
    | nil => exact FrL.split_head hF h
    | cons => simp at hm
  | succ m ih =>
    intro A B E₂ hm h
    cases A with
    | nil => exact FrL.split_head hF h
    | cons G A' =>
      simp only [List.cons_append] at h
      generalize hl : G :: (A' ++ F :: B) = l at h
      induction h with
      | nil => cases hl
      | cons hG h' =>
        cases hl
        obtain ⟨A₂, F₂, B₂, rfl, hA, hF₂, hB⟩ := ih (by simp at hm; omega) h'
        exact ⟨_ :: A₂, F₂, B₂, rfl, .cons hG hA, hF₂, hB⟩
      | junkL hn h' =>
        cases hl
        obtain ⟨A₂, F₂, B₂, rfl, hA, hF₂, hB⟩ := ih (by simp at hm; omega) h'
        exact ⟨A₂, F₂, B₂, rfl, .junkL hn hA, hF₂, hB⟩
      | junkR hn _ ih' =>
        obtain ⟨A₂, F₂, B₂, rfl, hA, hF₂, hB⟩ := ih' hl
        exact ⟨.settle _ :: A₂, F₂, B₂, rfl, .junkR hn hA, hF₂, hB⟩
      | againF hB hll hc hσ hσn hσv hσv' h' =>
        injection hl with hG heq
        subst hG
        cases A' with
        | nil => simp only [List.nil_append, List.cons.injEq] at heq; exact (hFl _ heq.1).elim
        | cons G' A'' =>
          simp only [List.cons_append, List.cons.injEq] at heq
          obtain ⟨rfl, heq'⟩ := heq
          subst heq'
          obtain ⟨A₂, F₂, B₂, rfl, hA, hF₂, hB'⟩ := ih (by simp at hm; omega) h'
          exact ⟨.settle _ :: .let_ _ :: A₂, F₂, B₂, rfl,
            .againF hB hll hc hσ hσn hσv hσv' hA, hF₂, hB'⟩
      | againR hB hll hc hσ hσn hσv hσv' h' =>
        injection hl with hG heq
        subst hG
        cases A' with
        | nil => simp only [List.nil_append, List.cons.injEq] at heq; exact (hFl _ heq.1).elim
        | cons G' A'' =>
          simp only [List.cons_append, List.cons.injEq] at heq
          obtain ⟨rfl, heq'⟩ := heq
          subst heq'
          obtain ⟨A₂, F₂, B₂, rfl, hA, hF₂, hB'⟩ := ih (by simp at hm; omega) h'
          exact ⟨.settle _ :: .let_ _ :: A₂, F₂, B₂, rfl,
            .againR hB hll hc hσ hσn hσv hσv' hA, hF₂, hB'⟩

/-- A split of the left context at a frame that is neither a cell's frame nor a `let`. -/
theorem FrL.split_left {F : Frame} (hF : ∀ n, F ≠ .settle n) (hFl : ∀ b, F ≠ .let_ b)
    {A B E₂ : List Frame} (h : FrL W (A ++ F :: B) E₂) :
    ∃ A₂ F₂ B₂, E₂ = A₂ ++ F₂ :: B₂ ∧ FrL W A A₂ ∧ FrS W F F₂ ∧ FrL W B B₂ :=
  FrL.split_left_aux hF hFl A.length (Nat.le_refl _) h

/-- Split the head frame of the right context when it is no cell's frame. -/
theorem FrL.split_head_right {F₂ : Frame} (hF : ∀ n, F₂ ≠ .settle n) :
    ∀ {B₂ E : List Frame}, FrL W E (F₂ :: B₂) →
      ∃ A F B, E = A ++ F :: B ∧ FrL W A [] ∧ FrS W F F₂ ∧ FrL W B B₂ := by
  intro B₂ E h
  generalize hl : F₂ :: B₂ = l at h
  induction h with
  | nil => cases hl
  | cons hF' h' => cases hl; exact ⟨[], _, _, rfl, .nil, hF', h'⟩
  | junkL hn _ ih =>
    obtain ⟨A, F, B, rfl, hA, hF₁, hB⟩ := ih hl
    exact ⟨.settle _ :: A, F, B, rfl, .junkL hn hA, hF₁, hB⟩
  | junkR _ _ => cases hl; exact (hF _ rfl).elim
  | againF | againR => cases hl; exact (hF _ rfl).elim

private theorem FrL.split_right_aux {F₂ : Frame} (hF : ∀ n, F₂ ≠ .settle n) (hFl : ∀ b, F₂ ≠ .let_ b) :
    ∀ (m : Nat) {A₂ B₂ E : List Frame}, A₂.length ≤ m → FrL W E (A₂ ++ F₂ :: B₂) →
      ∃ A F B, E = A ++ F :: B ∧ FrL W A A₂ ∧ FrS W F F₂ ∧ FrL W B B₂ := by
  intro m
  induction m with
  | zero =>
    intro A₂ B₂ E hm h
    cases A₂ with
    | nil => exact FrL.split_head_right hF h
    | cons => simp at hm
  | succ m ih =>
    intro A₂ B₂ E hm h
    cases A₂ with
    | nil => exact FrL.split_head_right hF h
    | cons G A₂' =>
      simp only [List.cons_append] at h
      generalize hl : G :: (A₂' ++ F₂ :: B₂) = l at h
      induction h with
      | nil => cases hl
      | cons hG h' =>
        cases hl
        obtain ⟨A, F, B, rfl, hA, hF₁, hB⟩ := ih (by simp at hm; omega) h'
        exact ⟨_ :: A, F, B, rfl, .cons hG hA, hF₁, hB⟩
      | junkL hn _ ih' =>
        obtain ⟨A, F, B, rfl, hA, hF₁, hB⟩ := ih' hl
        exact ⟨.settle _ :: A, F, B, rfl, .junkL hn hA, hF₁, hB⟩
      | junkR hn h' =>
        cases hl
        obtain ⟨A, F, B, rfl, hA, hF₁, hB⟩ := ih (by simp at hm; omega) h'
        exact ⟨A, F, B, rfl, .junkR hn hA, hF₁, hB⟩
      | againF hB hll hc hσ hσn hσv hσv' h' =>
        injection hl with hG heq
        subst hG
        cases A₂' with
        | nil => simp only [List.nil_append, List.cons.injEq] at heq; exact (hFl _ heq.1).elim
        | cons G' A₂'' =>
          simp only [List.cons_append, List.cons.injEq] at heq
          obtain ⟨rfl, heq'⟩ := heq
          subst heq'
          obtain ⟨A, F, B, rfl, hA, hF₁, hB'⟩ := ih (by simp at hm; omega) h'
          exact ⟨.settle _ :: .let_ _ :: A, F, B, rfl,
            .againF hB hll hc hσ hσn hσv hσv' hA, hF₁, hB'⟩
      | againR hB hll hc hσ hσn hσv hσv' h' =>
        injection hl with hG heq
        subst hG
        cases A₂' with
        | nil => simp only [List.nil_append, List.cons.injEq] at heq; exact (hFl _ heq.1).elim
        | cons G' A₂'' =>
          simp only [List.cons_append, List.cons.injEq] at heq
          obtain ⟨rfl, heq'⟩ := heq
          subst heq'
          obtain ⟨A, F, B, rfl, hA, hF₁, hB'⟩ := ih (by simp at hm; omega) h'
          exact ⟨.settle _ :: .let_ _ :: A, F, B, rfl,
            .againR hB hll hc hσ hσn hσv hσv' hA, hF₁, hB'⟩

/-- A split of the right context at a frame that is neither a cell's frame nor a `let`. -/
theorem FrL.split_right {F₂ : Frame} (hF : ∀ n, F₂ ≠ .settle n) (hFl : ∀ b, F₂ ≠ .let_ b)
    {A₂ B₂ E : List Frame} (h : FrL W E (A₂ ++ F₂ :: B₂)) :
    ∃ A F B, E = A ++ F :: B ∧ FrL W A A₂ ∧ FrS W F F₂ ∧ FrL W B B₂ :=
  FrL.split_right_aux hF hFl A₂.length (Nat.le_refl _) h

theorem FrL.append {A A₂ B B₂ : List Frame} (h₁ : FrL W A A₂) (h₂ : FrL W B B₂) :
    FrL W (A ++ B) (A₂ ++ B₂) := by
  induction h₁ with
  | nil => exact h₂
  | cons hF _ ih => exact .cons hF ih
  | junkL hn _ ih => exact .junkL hn ih
  | junkR hn _ ih => exact .junkR hn ih
  | againF hB hl hc hσ hσn hσv hσv' _ ih => exact .againF hB hl hc hσ hσn hσv hσv' ih
  | againR hB hl hc hσ hσn hσv hσv' _ ih => exact .againR hB hl hc hσ hσn hσv hσv' ih

end Flick.Equiv
