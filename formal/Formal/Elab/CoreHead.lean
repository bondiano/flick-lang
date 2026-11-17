import Formal.Elab.CorePHead

/-!
# λFlick⁺ as a `Target`: runtime head steps of related terms

A runtime head step of a term related by a congruence rule is matched by the same rule on the
other side, with related results, store and regions (`rhead_sim`). The other side needs no step
before it: the values at the redex are related to values (`Cg.value`), and the cells a step
reads to cells of the same form (`Formal/Elab/CoreCell`). A base pair at the redex itself is left
to the caller.
-/

namespace Flick.Elab

variable {B : Flick.Tm → Flick.Tm → Prop}

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
theorem rhead_sim (hV : BNoVal B) (hR : BNoRaise B) (hA : BNoAlts B) (hC : BClosed B) (hP : BPlain B)
    (hL : BNoCell B) {Λ Λ' Λ₂ : List Rgn} {μ μ' μ₂ : List Flick.Tm} {r r' r₂ : Flick.Tm}
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
  | refGet hl hn =>
    cases h with
    | base hb => exact .inl hb
    | app h₁ h₂ =>
      cases h₁ with
      | const =>
        cases h₂ with
        | loc =>
          obtain ⟨v₂, hv₂, r⟩ := hμ.getElem? hl
          exact .inr ⟨_, _, _, .refGet hv₂ fun hc => hn ((r.cell hL).2 hc), r, hΛ, hμ⟩
        | base hb => exact (nb hb .loc).elim
      | base hb => exact (nb hb .const).elim
  | refGetCell hl hc =>
    cases h with
    | base hb => exact .inl hb
    | app h₁ h₂ =>
      cases h₁ with
      | const =>
        cases h₂ with
        | loc =>
          obtain ⟨c₂, hc₂, r⟩ := hμ.getElem? hl
          exact .inr ⟨_, _, _, .refGetCell hc₂ ((r.cell hL).1 hc), .todo, hΛ, hμ⟩
        | base hb => exact (nb hb .loc).elim
      | base hb => exact (nb hb .const).elim
  | lazy =>
    cases h with
    | base hb => exact .inl hb
    | lazy he =>
      refine .inr ⟨_, _, _, .lazy, ?_, hΛ, hμ.app (.cons (.lazy he) .nil)⟩
      rw [hμ.length]; exact .loc
  | forcePending hl =>
    cases h with
    | base hb => exact .inl hb
    | force h₁ =>
      cases h₁ with
      | loc =>
        obtain ⟨c₂, hc₂, r⟩ := hμ.getElem? hl
        obtain ⟨e₂, rfl, he⟩ := r.lazy_left hL
        exact .inr ⟨_, _, _, .forcePending hc₂, .settle he, hΛ, hμ.set _ .anil⟩
      | base hb => exact (nb hb .loc).elim
  | forceDone hl =>
    cases h with
    | base hb => exact .inl hb
    | force h₁ =>
      cases h₁ with
      | loc =>
        obtain ⟨c₂, hc₂, r⟩ := hμ.getElem? hl
        obtain ⟨o₂, rfl, ho⟩ := r.done_left hL
        exact .inr ⟨_, _, _, .forceDone hc₂, ho, hΛ, hμ⟩
      | base hb => exact (nb hb .loc).elim
  | forceRunning hl hp hd =>
    cases h with
    | base hb => exact .inl hb
    | force h₁ =>
      cases h₁ with
      | loc =>
        obtain ⟨c₂, hc₂, r⟩ := hμ.getElem? hl
        refine .inr ⟨_, _, _, .forceRunning hc₂ (fun e h => ?_) (fun o h => ?_), .todo, hΛ, hμ⟩
        · subst h; obtain ⟨e', rfl⟩ := r.lazy_right hL; exact hp e' rfl
        · subst h; obtain ⟨o', rfl⟩ := r.done_right hL; exact hd o' rfl
      | base hb => exact (nb hb .loc).elim
  | settleVal hv =>
    cases h with
    | base hb => exact .inl hb
    | settle h₁ =>
      exact .inr ⟨_, _, _, .settleVal (Cg.value hV hv h₁).1, h₁, hΛ, hμ.settleCell hA hP _ (.await h₁)⟩
  | settleRaise hv =>
    cases h with
    | base hb => exact .inl hb
    | settle h₁ =>
      cases h₁ with
      | raise h₂ =>
        exact .inr ⟨_, _, _, .settleRaise (Cg.value hV hv h₂).1, .raise h₂, hΛ,
          hμ.settleCell hA hP _ (.await (.raise h₂))⟩
      | base hb => exact (hR.1 _ _ hb).elim
  | settleUnw =>
    cases h with
    | base hb => exact .inl hb
    | settle h₁ =>
      cases h₁ with
      | unw h₂ => exact .inr ⟨_, _, _, .settleUnw, .unw h₂, hΛ, hμ⟩
      | base hb => exact (hR.2 _ _ _ _ hb).elim
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
      | base hb => exact (nb hb (.constApp rfl .loc)).elim
  | scopeRun hv =>
    cases h with
    | base hb => exact .inl hb
    | app h₁ h₂ =>
      cases h₁ with
      | const =>
        exact .inr ⟨_, _, _, .scopeRun (Cg.value hV hv h₂).1, .scope (.app (h₂.rename hC _) .var), hΛ, hμ⟩
      | base hb => exact (nb hb .const).elim
  | await =>
    cases h with
    | base hb => exact .inl hb
    | await h₁ =>
      cases h₁ with
      | task he => exact .inr ⟨_, _, _, .await, he, hΛ, hμ⟩
      | base hb => exact (nb hb .task).elim
  | hkarmRaise hE hk hv =>
    subst hk
    cases h with
    | base hb => exact .inl hb
    | hkarm h₁ h₃ h₄ =>
      obtain ⟨Ei₂, rfl, hfr⟩ := Cg.plug_hole hP h₄
      cases h₃ with
      | raise h' =>
        exact .inr ⟨_, _, _, .hkarmRaise (FrR.okE hV hfr hE) rfl (Cg.value hV hv h').1,
          .hkeep h₁ (FrR.plugs hfr (.unw (.raise h'))), hΛ, hμ⟩
      | base hb => exact (hR.1 _ _ hb).elim
  | hkarmUnw hE hk =>
    subst hk
    cases h with
    | base hb => exact .inl hb
    | hkarm h₁ h₃ h₄ =>
      obtain ⟨Ei₂, rfl, hfr⟩ := Cg.plug_hole hP h₄
      cases h₃ with
      | unw h' => exact .inr ⟨_, _, _, .hkarmUnw (FrR.okE hV hfr hE) rfl,
          .hkeep h₁ (FrR.plugs hfr (.unw h')), hΛ, hμ⟩
      | base hb => exact (hR.2 _ _ _ _ hb).elim
  | hctlRaise hE hk hv =>
    subst hk
    cases h with
    | base hb => exact .inl hb
    | hctl h₁ h₂ h₃ h₄ =>
      obtain ⟨Ei₂, rfl, hfr⟩ := Cg.plug_hole hP h₄
      cases h₃ with
      | raise h' =>
        exact .inr ⟨_, _, _, .hctlRaise (FrR.okE hV hfr hE) rfl (Cg.value hV hv h').1,
          .hwith h₁ h₂ (FrR.plugs hfr (.unw (.raise h'))), hΛ, hμ⟩
      | base hb => exact (hR.1 _ _ hb).elim
  | hctlUnw hE hk =>
    subst hk
    cases h with
    | base hb => exact .inl hb
    | hctl h₁ h₂ h₃ h₄ =>
      obtain ⟨Ei₂, rfl, hfr⟩ := Cg.plug_hole hP h₄
      cases h₃ with
      | unw h' => exact .inr ⟨_, _, _, .hctlUnw (FrR.okE hV hfr hE) rfl,
          .hwith h₁ h₂ (FrR.plugs hfr (.unw (.unw h'))), hΛ, hμ⟩
      | base hb => exact (hR.2 _ _ _ _ hb).elim
  | hresRaise hA ha hv =>
    subst ha
    cases h with
    | base hb => exact .inl hb
    | hres h₁ h₂ =>
      obtain ⟨A₂, rfl, hfr⟩ := Cg.plug_hole hP h₁
      cases h₂ with
      | raise h' =>
        exact .inr ⟨_, _, _, .hresRaise (FrR.okE hV hfr hA) rfl (Cg.value hV hv h').1,
          .hdone (FrR.plugs hfr (.raise h')), hΛ, hμ⟩
      | base hb => exact (hR.1 _ _ hb).elim
  | hresUnw hA ha =>
    subst ha
    cases h with
    | base hb => exact .inl hb
    | hres h₁ h₂ =>
      obtain ⟨A₂, rfl, hfr⟩ := Cg.plug_hole hP h₁
      cases h₂ with
      | unw h' => exact .inr ⟨_, _, _, .hresUnw (FrR.okE hV hfr hA) rfl, .hdone (FrR.plugs hfr (.unw h')),
          hΛ, hμ⟩
      | base hb => exact (hR.2 _ _ _ _ hb).elim
  | hwithCatch =>
    cases h with
    | base hb => exact .inl hb
    | hwith _ _ h₃ =>
      cases h₃ with
      | unw h' => exact .inr ⟨_, _, _, .hwithCatch, h', hΛ, hμ⟩
      | base hb => exact (hR.2 _ _ _ _ hb).elim
  | hdl =>
    cases h with
    | base hb => exact .inl hb
    | hdl h₁ h₂ h₃ =>
      refine .inr ⟨_, _, _, .hdl, ?_, hΛ.app (.cons RgR.new .nil), hμ.app (.cons (Cg.refl false _) .nil)⟩
      rw [hΛ.length, hμ.length]
      exact .inside (.hwith h₁ h₂ h₃)
  | tryRaise hv =>
    cases h with
    | base hb => exact .inl hb
    | try_ h₁ h₂ =>
      cases h₁ with
      | raise h' => exact .inr ⟨_, _, _, .tryRaise (Cg.value hV hv h').1, .handle h' h₂, hΛ, hμ⟩
      | base hb => exact (hR.1 _ _ hb).elim
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
