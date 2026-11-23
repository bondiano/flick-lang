import Formal.Equiv.CatchUp

/-!
# Contextual equivalence: the right side reaches a value

When the left side holds a value, a raised error or a signal, the right side reaches the same
form by steps that touch only its own cells (`Sim.vcu`, `Sim.rcu`, `Sim.ucu`): it does the work a
rewrite the other way round put there, and leaves the frames of its own cells.
-/

namespace Flick.Equiv

open Flick.Elab (All2)

variable {W : World}

theorem Sim.top_const {x : Pos} {c : Const} {b : Tm} (h : Sim W x (.const c) b) (hx : x = .top) :
    b = .const c := by
  cases h with
  | const => rfl
  | jR hx' => exact (hx' hx).elim
  | _ => cases hx

/-- **The right side reaches a value** related to a value of the left side. -/
theorem Sim.vcu_sz : ∀ (n : Nat) {W : World} {x : Pos} {a b : Tm}, b.sz ≤ n →
    Sim W x a b → x = .any → Value a →
    ∀ (W' : World) {μ₁ : List Tm} (μ : List Tm), W.le W' → StR W' μ₁ μ →
      (W'.B → a.Closed ∧ b.Closed) →
      ∃ W'' μ' b', RJ W' μ b W'' μ' b' ∧ Value b' ∧ Sim W'' .any a b' := by
  intro n
  induction n with
  | zero =>
    intro W x a b hsz
    cases b <;> simp [Tm.sz] at hsz
  | succ n ih =>
    intro W x a b hsz h hx ha W' μ₁ μ hW hst hcl
    cases h with
    | @app x f f' a₀ a₀' hx' h₁ h₂ =>
      subst hx
      simp only [Tm.sz] at hsz
      have hfa : Value f ∧ Value a₀ := by cases ha with
        | ctorApp hf _ he => exact ⟨hf, he⟩
        | constApp _ he => exact ⟨.const, he⟩
      have hcl₁ : W'.B → f.Closed ∧ f'.Closed := fun hB =>
        let ⟨hca, hcb⟩ := hcl hB
        ⟨fun i hi => hca i (.inl hi), fun i hi => hcb i (.inl hi)⟩
      obtain ⟨W₁, μ₁', f₁, r₁, v₁, s₁⟩ := ih (by omega) h₁ rfl hfa.1 W' μ hW hst hcl₁
      have hcl₂ : W₁.B → a₀.Closed ∧ a₀'.Closed := fun hB =>
        let ⟨hca, hcb⟩ := hcl (r₁.le.B.2 hB)
        ⟨fun i hi => hca i (.inr hi), fun i hi => hcb i (.inr hi)⟩
      obtain ⟨W₂, μ₂, a₁, r₂, v₂, s₂⟩ :=
        ih (by omega) h₂ rfl hfa.2 W₁ μ₁' (hW.trans r₁.le) (r₁.st _ hst) hcl₂
      refine ⟨W₂, μ₂, .app f₁ a₁, (r₁.frame (F := .appL a₀') trivial).trans
        (r₂.frame (F := .appR f₁) v₁), ?_, .app (by decide) (s₁.mono r₂.le) s₂⟩
      cases ha with
      | ctorApp hf hK he => exact .ctorApp v₁ ((s₁.value_any hf v₁ .top).top_head rfl hf hK) v₂
      | constApp hc he =>
        rw [(s₁.value_any .const v₁ .top).top_const rfl]
        exact .constApp hc v₂
    | @rcons x l e e' r r' hx' h₁ h₂ =>
      subst hx
      simp only [Tm.sz] at hsz
      cases ha with
      | rcons he hr =>
        have hcl₁ : W'.B → e.Closed ∧ e'.Closed := fun hB =>
          let ⟨hca, hcb⟩ := hcl hB
          ⟨fun i hi => hca i (.inl hi), fun i hi => hcb i (.inl hi)⟩
        obtain ⟨W₁, μ₁', e₁, r₁, v₁, s₁⟩ := ih (by omega) h₁ rfl he W' μ hW hst hcl₁
        have hcl₂ : W₁.B → r.Closed ∧ r'.Closed := fun hB =>
          let ⟨hca, hcb⟩ := hcl (r₁.le.B.2 hB)
          ⟨fun i hi => hca i (.inr hi), fun i hi => hcb i (.inr hi)⟩
        obtain ⟨W₂, μ₂, r₂', r₂, v₂, s₂⟩ :=
          ih (by omega) h₂ rfl hr W₁ μ₁' (hW.trans r₁.le) (r₁.st _ hst) hcl₂
        exact ⟨W₂, μ₂, .rcons l e₁ r₂', (r₁.frame (F := .rconsL l r') trivial).trans
          (r₂.frame (F := .rconsR l e₁) v₁), .rcons v₁ v₂, .rcons (by decide) (s₁.mono r₂.le) s₂⟩
    | @dropC e c c' b' hb hn h' =>
      subst hb
      simp only [Tm.sz, sz_rename] at hsz
      have rj := RJ.drop (W := W') μ e c'
      have hcl₁ : (W'.addJ₂ μ.length).B → a.Closed ∧ c'.Closed := fun hB =>
        let ⟨hca, hcb⟩ := hcl hB; ⟨hca, rj.cl hcb⟩
      obtain ⟨W₁, μ₁', b₁, r₁, v₁, s₁⟩ :=
        ih (by omega) h' rfl ha (W'.addJ₂ μ.length) (μ ++ [.lazy e])
          (hW.trans (World.le_addJ₂ _ _)) (rj.st _ hst) hcl₁
      exact ⟨W₁, μ₁', b₁, rj.trans r₁, v₁, s₁⟩
    | @forceC e e' hn h' =>
      simp only [Tm.sz] at hsz
      have hcl₁ : (W'.addJ₂ μ.length).B → a.Closed ∧ e'.Closed := fun hB =>
        let ⟨hca, hcb⟩ := hcl hB; ⟨hca, hcb⟩
      have rj := RJ.start (W := W') μ e'
      obtain ⟨W₁, μ₁', b₁, r₁, v₁, s₁⟩ :=
        ih (by omega) h' rfl ha (W'.addJ₂ μ.length) _ (hW.trans (World.le_addJ₂ _ _))
          (rj.st _ hst) hcl₁
      exact ⟨W₁, _, b₁, rj.trans ((r₁.frame (F := .settle μ.length) trivial).trans
        (RJ.settleV (r₁.le.J₂ _ (.inr rfl)) v₁)), v₁, s₁⟩
    | @jR x n a a' hx' hn h' =>
      subst hx
      simp only [Tm.sz] at hsz
      have hcl₁ : W'.B → a.Closed ∧ a'.Closed := fun hB =>
        let ⟨hca, hcb⟩ := hcl hB; ⟨hca, hcb⟩
      obtain ⟨W₁, μ₁', b₁, r₁, v₁, s₁⟩ := ih (by omega) h' rfl ha W' μ hW hst hcl₁
      exact ⟨W₁, _, b₁, (r₁.frame (F := .settle n) trivial).trans
        (RJ.settleV (r₁.le.J₂ _ (hW.J₂ _ hn)) v₁), v₁, s₁⟩
    | @o4A e e' b₀ b₀' f br hB hf hbr hpe hpe' he hb hne hnb =>
      subst hf hbr
      have hB' : W'.B := hW.B.1 hB
      have ⟨hca, hcb⟩ := hcl hB'
      have hce : e.Closed := closed_fix_let_lazy_up2 hca
      have hce' : e'.Closed := closed_let_lazy hcb
      have hpe₁ : PureEval W'.R e := (hW.R ▸ hpe).eval hce
      have hpe₁' : PureEval W'.R e' := (hW.R ▸ hpe').eval hce'
      have rj := RJ.o4A (W := W') μ (b' := b₀') hB' hce' hpe₁'
      have hW' := hW.trans (World.le_addHR W' μ.length e')
      exact ⟨W'.addHR μ.length e', μ ++ [.lazy e'], .fix (b₀'.subst (inst [.loc μ.length])), rj,
        .fix, .o4Fix1 hB' (.inr ⟨rfl, rfl⟩) rfl rfl hce hpe₁ (he.mono hW') (hb.mono hW') hnb⟩
    | @o4AllocR l e e' s c c' hB hl hs hcl' hpe' he hc =>
      subst hs
      simp only [Tm.sz] at hsz
      have hB' : W'.B := hW.B.1 hB
      have rj := RJ.o4AllocR (W := W') μ (c' := c') hB' (hW.HL _ _ hl) hcl' (hW.R ▸ hpe') (he.mono hW)
      have hW₁ := hW.trans rj.le
      have hsub : SubR ((W'.addHR μ.length e').addH l μ.length) (inst [.loc l]) (inst [.loc μ.length]) := by
        intro i; cases i with
        | zero =>
          exact ⟨fun y _ => .hloc hB' (.inr ⟨rfl, rfl⟩), by simp [inst, Tm.NoIns, Tm.Has],
            .inr .loc, .inr .loc⟩
        | succ k =>
          exact ⟨fun _ hy => .var hy, by simp [inst, Tm.NoIns, Tm.Has],
            .inl ⟨k, rfl⟩, .inl ⟨k, rfl⟩⟩
      have hcl₁ : ((W'.addHR μ.length e').addH l μ.length).B →
          (c.subst (inst [.loc l])).Closed ∧ (c'.subst (inst [.loc μ.length])).Closed := fun hB'' =>
        let ⟨hca, hcb⟩ := hcl hB''; ⟨hca, rj.cl hcb⟩
      have hsz' : (c'.subst (inst [.loc μ.length])).sz ≤ n := by rw [sz_subst_loc]; omega
      obtain ⟨W₁, μ₁', b₁, r₁, v₁, s₁⟩ :=
        ih hsz' ((hc.mono hW₁).subst _ _ hsub) rfl ha _ (μ ++ [.lazy e'])
          (World.le.refl _) (rj.st _ hst) hcl₁
      exact ⟨W₁, μ₁', b₁, rj.trans r₁, v₁, s₁⟩
    | doneR _ hd _ _ =>
      obtain ⟨-, -, hv', -, hcv', -, -, hl', hvv⟩ := hst.done _ _ _ _ (hW.D _ _ _ _ hd)
      exact ⟨W', μ, _, RJ.doneR hl' hv' hcv', hv', hvv⟩
    | dropA | dropB | forceA | jL | bornV | dead | bornA | bornB
    | o4B | o4Alloc1 | o4Bind | o4Settle
    | doneF | againF1 | againF2 | againF3 | againR1 | againR2 | againR3
    | letRaise | letUnw | letTodo => cases ha
    | bornS _ _ _ hs => rcases hs with rfl | rfl | rfl | rfl | rfl <;> cases ha
    | appK | appV | rconsK | rconsV | updK | updV => cases hx
    | hloc hB hH => exact ⟨W', μ, _, RJ.refl _ _, .loc, .hloc (hW.B.1 hB) (hW.H _ _ hH)⟩
    | o4Fix1 hB hr hf hf' hce hpe he hb hnb =>
      exact ⟨W', μ, _, RJ.refl _ _, .fix,
        .o4Fix1 (hW.B.1 hB) (hW.HR _ _ hr) hf hf' hce (hW.R ▸ hpe) (he.mono hW) (hb.mono hW) hnb⟩
    | o4Fix2 hB hl hf hf' hce' hpe' he hb hne' hnb =>
      exact ⟨W', μ, _, RJ.refl _ _, .fix,
        .o4Fix2 (hW.B.1 hB) (hW.HL _ _ hl) hf hf' hce' (hW.R ▸ hpe') (he.mono hW) (hb.mono hW) hne' hnb⟩
    | _ =>
      first
        | (cases ha; done)
        | exact ⟨W', μ, _, RJ.refl _ _, by constructor, Sim.mono hW (by constructor <;> assumption)⟩

theorem Sim.vcu {x : Pos} {a b : Tm} (h : Sim W x a b) (hx : x = .any) (ha : Value a)
    (W' : World) {μ₁ : List Tm} (μ : List Tm) (hW : W.le W') (hst : StR W' μ₁ μ)
    (hcl : W'.B → a.Closed ∧ b.Closed) :
    ∃ W'' μ' b', RJ W' μ b W'' μ' b' ∧ Value b' ∧ Sim W'' .any a b' :=
  Sim.vcu_sz b.sz (Nat.le_refl _) h hx ha W' μ hW hst hcl

/-- **The right side reaches a raised error** related to one of the left side. -/
theorem Sim.rcu_sz : ∀ (n : Nat) {W : World} {x : Pos} {r b : Tm}, b.sz ≤ n →
    Sim W x r b → x = .any → ∀ {v : Tm}, r = .raise v → Value v →
    ∀ (W' : World) {μ₁ : List Tm} (μ : List Tm), W.le W' → StR W' μ₁ μ →
      (W'.B → r.Closed ∧ b.Closed) →
      ∃ W'' μ' v', RJ W' μ b W'' μ' (.raise v') ∧ Value v' ∧ Sim W'' .any v v' := by
  intro n
  induction n with
  | zero =>
    intro W x r b hsz
    cases b <;> simp [Tm.sz] at hsz
  | succ n ih =>
    intro W x r b hsz h hx v hr hv W' μ₁ μ hW hst hcl
    cases h with
    | raise h' =>
      cases hr; subst hx
      obtain ⟨W₁, μ₁', b₁, r₁, v₁, s₁⟩ := h'.vcu rfl hv W' μ hW hst hcl
      exact ⟨W₁, μ₁', b₁, r₁.frame (F := .raise) trivial, v₁, s₁⟩
    | @dropC e c c' b' hb hn h' =>
      subst hb
      simp only [Tm.sz, sz_rename] at hsz
      have rj := RJ.drop (W := W') μ e c'
      have hcl₁ : (W'.addJ₂ μ.length).B → r.Closed ∧ c'.Closed := fun hB =>
        let ⟨hca, hcb⟩ := hcl hB; ⟨hca, rj.cl hcb⟩
      obtain ⟨W₁, μ₁', b₁, r₁, v₁, s₁⟩ :=
        ih (by omega) h' rfl hr hv (W'.addJ₂ μ.length) (μ ++ [.lazy e])
          (hW.trans (World.le_addJ₂ _ _)) (rj.st _ hst) hcl₁
      exact ⟨W₁, μ₁', b₁, rj.trans r₁, v₁, s₁⟩
    | @forceC e e' hn h' =>
      simp only [Tm.sz] at hsz
      have rj := RJ.start (W := W') μ e'
      obtain ⟨W₁, μ₁', b₁, r₁, v₁, s₁⟩ :=
        ih (by omega) h' rfl hr hv (W'.addJ₂ μ.length) _
          (hW.trans (World.le_addJ₂ _ _)) (rj.st _ hst) hcl
      exact ⟨W₁, _, b₁, rj.trans ((r₁.frame (F := .settle μ.length) trivial).trans
        (RJ.settleR (r₁.le.J₂ _ (.inr rfl)) v₁)), v₁, s₁⟩
    | @jR x n a a' hx' hn h' =>
      subst hx
      simp only [Tm.sz] at hsz
      obtain ⟨W₁, μ₁', b₁, r₁, v₁, s₁⟩ := ih (by omega) h' rfl hr hv W' μ hW hst hcl
      exact ⟨W₁, _, b₁, (r₁.frame (F := .settle n) trivial).trans
        (RJ.settleR (r₁.le.J₂ _ (hW.J₂ _ hn)) v₁), v₁, s₁⟩
    | @o4AllocR l e e' s c c' hB hl hs hcl' hpe' he hc =>
      subst hs
      simp only [Tm.sz] at hsz
      have hB' : W'.B := hW.B.1 hB
      have rj := RJ.o4AllocR (W := W') μ (c' := c') hB' (hW.HL _ _ hl) hcl' (hW.R ▸ hpe') (he.mono hW)
      have hW₁ := hW.trans rj.le
      have hsub : SubR ((W'.addHR μ.length e').addH l μ.length) (inst [.loc l]) (inst [.loc μ.length]) := by
        intro i; cases i with
        | zero =>
          exact ⟨fun y _ => .hloc hB' (.inr ⟨rfl, rfl⟩), by simp [inst, Tm.NoIns, Tm.Has],
            .inr .loc, .inr .loc⟩
        | succ k =>
          exact ⟨fun _ hy => .var hy, by simp [inst, Tm.NoIns, Tm.Has],
            .inl ⟨k, rfl⟩, .inl ⟨k, rfl⟩⟩
      have hcl₁ : ((W'.addHR μ.length e').addH l μ.length).B →
          (c.subst (inst [.loc l])).Closed ∧ (c'.subst (inst [.loc μ.length])).Closed := fun hB'' =>
        let ⟨hca, hcb⟩ := hcl hB''; ⟨hca, rj.cl hcb⟩
      have hsz' : (c'.subst (inst [.loc μ.length])).sz ≤ n := by rw [sz_subst_loc]; omega
      obtain ⟨W₁, μ₁', b₁, r₁, v₁, s₁⟩ :=
        ih hsz' ((hc.mono hW₁).subst _ _ hsub) rfl hr hv _ (μ ++ [.lazy e'])
          (World.le.refl _) (rj.st _ hst) hcl₁
      exact ⟨W₁, μ₁', b₁, rj.trans r₁, v₁, s₁⟩
    | doneR _ _ hvd => subst hr; cases hvd
    | bornS _ _ _ hs => subst hr; rcases hs with h | h | h | h | h <;> cases h
    | _ => first | cases hr | cases hx | (rename_i h; exact (h rfl).elim)

theorem Sim.rcu {x : Pos} {r b : Tm} (h : Sim W x r b) (hx : x = .any) {v : Tm} (hr : r = .raise v)
    (hv : Value v) (W' : World) {μ₁ : List Tm} (μ : List Tm) (hW : W.le W') (hst : StR W' μ₁ μ)
    (hcl : W'.B → r.Closed ∧ b.Closed) :
    ∃ W'' μ' v', RJ W' μ b W'' μ' (.raise v') ∧ Value v' ∧ Sim W'' .any v v' :=
  Sim.rcu_sz b.sz (Nat.le_refl _) h hx hr hv W' μ hW hst hcl

/-- **The right side reaches a signal** related to one of the left side. -/
theorem Sim.ucu_sz : ∀ (n : Nat) {W : World} {x : Pos} {r b : Tm}, b.sz ≤ n →
    Sim W x r b → x = .any → ∀ {p ℓ : Nat} {a : Tm}, r = .unw p ℓ a →
    ∀ (W' : World) {μ₁ : List Tm} (μ : List Tm), W.le W' → StR W' μ₁ μ →
      (W'.B → r.Closed ∧ b.Closed) →
      ∃ W'' μ' ℓ' k a', RJ W' μ b W'' μ' (.unw p ℓ' a') ∧ W''.L ℓ ℓ' k ∧ Sim W'' .any a a' := by
  intro n
  induction n with
  | zero =>
    intro W x r b hsz
    cases b <;> simp [Tm.sz] at hsz
  | succ n ih =>
    intro W x r b hsz h hx p ℓ a hr W' μ₁ μ hW hst hcl
    cases h with
    | unw hl h' =>
      cases hr
      exact ⟨W', μ, _, _, _, RJ.refl _ _, hW.L _ _ _ hl, h'.mono hW⟩
    | @dropC e c c' b' hb hn h' =>
      subst hb
      simp only [Tm.sz, sz_rename] at hsz
      have rj := RJ.drop (W := W') μ e c'
      have hcl₁ : (W'.addJ₂ μ.length).B → r.Closed ∧ c'.Closed := fun hB =>
        let ⟨hca, hcb⟩ := hcl hB; ⟨hca, rj.cl hcb⟩
      obtain ⟨W₁, μ₁', ℓ', k, a', r₁, hl, s₁⟩ :=
        ih (by omega) h' rfl hr (W'.addJ₂ μ.length) (μ ++ [.lazy e])
          (hW.trans (World.le_addJ₂ _ _)) (rj.st _ hst) hcl₁
      exact ⟨W₁, μ₁', ℓ', k, a', rj.trans r₁, hl, s₁⟩
    | @forceC e e' hn h' =>
      simp only [Tm.sz] at hsz
      have rj := RJ.start (W := W') μ e'
      obtain ⟨W₁, μ₁', ℓ', k, a', r₁, hl, s₁⟩ :=
        ih (by omega) h' rfl hr (W'.addJ₂ μ.length) _
          (hW.trans (World.le_addJ₂ _ _)) (rj.st _ hst) hcl
      exact ⟨W₁, _, ℓ', k, a', rj.trans ((r₁.frame (F := .settle μ.length) trivial).trans
        (RJ.settleU (r₁.le.J₂ _ (.inr rfl)))), hl, s₁⟩
    | @jR x n a₀ a' hx' hn h' =>
      subst hx
      simp only [Tm.sz] at hsz
      obtain ⟨W₁, μ₁', ℓ', k, a'', r₁, hl, s₁⟩ := ih (by omega) h' rfl hr W' μ hW hst hcl
      exact ⟨W₁, _, ℓ', k, a'', (r₁.frame (F := .settle n) trivial).trans
        (RJ.settleU (r₁.le.J₂ _ (hW.J₂ _ hn))), hl, s₁⟩
    | @o4AllocR l e e' s c c' hB hl hs hcl' hpe' he hc =>
      subst hs
      simp only [Tm.sz] at hsz
      have hB' : W'.B := hW.B.1 hB
      have rj := RJ.o4AllocR (W := W') μ (c' := c') hB' (hW.HL _ _ hl) hcl' (hW.R ▸ hpe') (he.mono hW)
      have hW₁ := hW.trans rj.le
      have hsub : SubR ((W'.addHR μ.length e').addH l μ.length) (inst [.loc l]) (inst [.loc μ.length]) := by
        intro i; cases i with
        | zero =>
          exact ⟨fun y _ => .hloc hB' (.inr ⟨rfl, rfl⟩), by simp [inst, Tm.NoIns, Tm.Has],
            .inr .loc, .inr .loc⟩
        | succ k =>
          exact ⟨fun _ hy => .var hy, by simp [inst, Tm.NoIns, Tm.Has],
            .inl ⟨k, rfl⟩, .inl ⟨k, rfl⟩⟩
      have hcl₁ : ((W'.addHR μ.length e').addH l μ.length).B →
          (c.subst (inst [.loc l])).Closed ∧ (c'.subst (inst [.loc μ.length])).Closed := fun hB'' =>
        let ⟨hca, hcb⟩ := hcl hB''; ⟨hca, rj.cl hcb⟩
      have hsz' : (c'.subst (inst [.loc μ.length])).sz ≤ n := by rw [sz_subst_loc]; omega
      obtain ⟨W₁, μ₁', ℓ', k, a'', r₁, hll, s₁⟩ :=
        ih hsz' ((hc.mono hW₁).subst _ _ hsub) rfl hr _ (μ ++ [.lazy e'])
          (World.le.refl _) (rj.st _ hst) hcl₁
      exact ⟨W₁, μ₁', ℓ', k, a'', rj.trans r₁, hll, s₁⟩
    | doneR _ _ hvd => subst hr; cases hvd
    | bornS _ _ _ hs => subst hr; rcases hs with h | h | h | h | h <;> cases h
    | _ => first | cases hr | cases hx | (rename_i h; exact (h rfl).elim)

theorem Sim.ucu {x : Pos} {r b : Tm} (h : Sim W x r b) (hx : x = .any) {p ℓ : Nat} {a : Tm}
    (hr : r = .unw p ℓ a) (W' : World) {μ₁ : List Tm} (μ : List Tm) (hW : W.le W')
    (hst : StR W' μ₁ μ) (hcl : W'.B → r.Closed ∧ b.Closed) :
    ∃ W'' μ' ℓ' k a', RJ W' μ b W'' μ' (.unw p ℓ' a') ∧ W''.L ℓ ℓ' k ∧ Sim W'' .any a a' :=
  Sim.ucu_sz b.sz (Nat.le_refl _) h hx hr W' μ hW hst hcl

end Flick.Equiv
