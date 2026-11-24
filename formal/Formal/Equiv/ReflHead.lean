import Formal.Equiv.RInv
import Formal.Equiv.TypInv

/-!
# Equivalence of every run: a head step of the right side, on the left

When the right side takes a head step at a focus related to the left one at `top`, the left side
takes one too (`phead_rev`, `rhead_rev`), or, for a region that closes or runs a finalizer, one of
the left children of the region steps alone: the right ones are finished, and a left child
related to a finished one is finished or steps (`ThR.final_rev`). Parked bodies of the left side
come from its typing (`TypedT`); stores and regions are related (`StR`, `RgsR`).
-/

namespace Flick.Equiv

open Flick.Elab (All2 ORel)

variable {W : World}

theorem ORel.some_right {α β : Type} {R : α → β → Prop} {o : Option α} {o' : Option β} {b : β}
    (h : ORel R o o') (hb : o' = some b) : ∃ a, o = some a := by
  subst hb; cases o <;> simp_all [ORel]

/-- **A pure head step** of the right side at the focus is one of the left side. -/
theorem phead_rev {S : Sig} {r r₂ r₂' : Tm} (hs : Sim W .top r r₂) (hr : Redex r)
    (hty : TypedT S r) (hh : PHead r₂ r₂') : ∃ r', PHead r r' := by
  cases hh with
  | beta =>
    obtain ⟨f, a, rfl, hf, -⟩ := hs.rinv_app
    obtain ⟨b, rfl⟩ := hf.rinv_lam
    exact ⟨_, .beta (Redex.app hr rfl).2⟩
  | fix =>
    obtain ⟨f, a, rfl, hf, -⟩ := hs.rinv_app
    obtain ⟨g, rfl⟩ := hf.rinv_fix
    exact ⟨_, .fix (Redex.app hr rfl).2⟩
  | let_ =>
    obtain ⟨e, b, rfl, -⟩ := hs.rinv_let
    exact ⟨_, .let_ (hr.2 (.let_ b) e trivial rfl)⟩
  | letrec => obtain ⟨f, b, rfl⟩ := hs.rinv_letrec; exact ⟨_, .letrec⟩
  | proj hv hf =>
    obtain ⟨e, rfl, he⟩ := hs.rinv_proj
    have hev := hr.2 (.proj _) e trivial rfl
    obtain ⟨w, hw⟩ := ORel.some_right ((he.value_any hev hv .top).field rfl _) hf
    exact ⟨_, .proj hev hw⟩
  | update hv hw hset =>
    obtain ⟨e, a, rfl, he, ha⟩ := hs.rinv_update
    have hev := hr.2 (.updL _ a) e trivial rfl
    have hav := hr.2 (.updR e _) a hev rfl
    obtain ⟨w, hw'⟩ :=
      ORel.some_right (Sim.setField (he.value_any hev hv .top) (ha.value_any hav hw .top) _) hset
    exact ⟨_, .update hev hav hw'⟩
  | matchYes | matchNo =>
    obtain ⟨e, al, rfl, -, hal⟩ := hs.rinv_match
    obtain ⟨b, rr, rfl⟩ := hal.rinv_acons
    have hev := hr.2 (.match_ _) e trivial rfl
    cases hm : matchP _ e with
    | none => exact ⟨_, .matchNo hev hm⟩
    | some δ => exact ⟨_, .matchYes hev hm⟩
  | guardYes | guardNo =>
    obtain ⟨e, al, rfl, -, hal⟩ := hs.rinv_match
    obtain ⟨g, b, rr, rfl⟩ := hal.rinv_aguard
    have hev := hr.2 (.match_ _) e trivial rfl
    cases hm : matchP _ e with
    | none => exact ⟨_, .guardNo hev hm⟩
    | some δ => exact ⟨_, .guardYes hev hm⟩
  | tryVal =>
    obtain ⟨e, al, rfl, -, -⟩ := hs.rinv_try
    exact ⟨_, .tryVal (hr.2 (.try_ al) e trivial rfl)⟩
  | intEq =>
    obtain ⟨f, x, rfl, hf, hx⟩ := hs.rinv_app
    obtain ⟨f₁, y, rfl, hf₁, hy⟩ := hf.rinv_app
    rw [hf₁.top_const_rev rfl, hy.rinv_lit, hx.rinv_lit]
    exact ⟨_, .intEq⟩
  | funcRef =>
    obtain ⟨f, a, rfl, hf, -⟩ := hs.rinv_app
    rw [hf.top_const_rev rfl]
    exact ⟨_, .funcRef (Redex.app hr rfl).2⟩
  | hwithVal =>
    obtain ⟨ℓ, k, al, rr, e, rfl, -⟩ := hs.rinv_hwith
    exact ⟨_, .hwithVal (hr.2 (.hwith _ _ _ _ _) e trivial rfl)⟩
  | hctlTail | hctlDisc =>
    obtain ⟨ℓ, k, κ, al, rr, a, c, rfl, -⟩ := hs.rinv_hctl
    obtain ⟨_, _, _, _, ht⟩ := hty
    obtain ⟨Ei, hEi, rfl⟩ := ht.hctl_parked rfl
    first
      | exact ⟨_, .hctlTail hEi rfl (hr.2 (.hctlA _ _ _ _ _ _ _ _) a trivial rfl)⟩
      | exact ⟨_, .hctlDisc hEi rfl (hr.2 (.hctlA _ _ _ _ _ _ _ _) a trivial rfl)⟩
  | hresVal =>
    obtain ⟨ℓ, k, a, b, rfl, -⟩ := hs.rinv_hres
    obtain ⟨_, _, _, _, ht⟩ := hty
    obtain ⟨A, hA, rfl⟩ := ht.hres_parked rfl
    exact ⟨_, .hresVal hA rfl (hr.2 (.hresB _ _ _) b trivial rfl)⟩
  | hdoneVal =>
    obtain ⟨a, rfl, -⟩ := hs.rinv_hdone
    exact ⟨_, .hdoneVal (hr.2 (.hdone _) a trivial rfl)⟩
  | hkeepVal =>
    obtain ⟨ℓ, k, al, e, rfl, -⟩ := hs.rinv_hkeep
    exact ⟨_, .hkeepVal (hr.2 (.hkeep _ _ _ _) e trivial rfl)⟩
  | hkarmVal =>
    obtain ⟨ℓ, k, al, a, c, rfl, -⟩ := hs.rinv_hkarm
    obtain ⟨_, _, _, _, ht⟩ := hty
    obtain ⟨Ei, hEi, rfl⟩ := ht.hkarm_parked rfl
    exact ⟨_, .hkarmVal hEi rfl (hr.2 (.hkarmA _ _ _ _ _) a trivial rfl)⟩

theorem RgsR.get' {μ : List Tm} {τ : Option (Nat × Nat)} {Λ Λ₂ : List Rgn} (h : RgsR W μ τ Λ Λ₂)
    {p : Nat} {g₂ : Rgn} (hg₂ : Λ₂[p]? = some g₂) : ∃ g, Λ[p]? = some g ∧ RgR W μ τ p g g₂ := by
  have hp : p < Λ.length := h.1 ▸ (List.getElem?_eq_some_iff.mp hg₂).1
  obtain ⟨g, hg⟩ : ∃ g, Λ[p]? = some g := ⟨_, List.getElem?_eq_getElem hp⟩
  exact ⟨g, hg, h.2 p g g₂ hg hg₂⟩

/-- A region of the right side whose children are finished: the left children are, or one of
them steps alone. -/
theorem RgR.kids_rev {μ μ₂ : List Tm} {p : Nat} {g g₂ : Rgn} (hr : RgR W μ none p g g₂)
    (hk : ∀ k ∈ g₂.kids, Final k) (hst : StR W μ μ₂) (hB : ¬ W.B) :
    (∀ k ∈ g.kids, Final k) ∨ ∃ k ∈ g.kids, LStep μ k := by
  by_cases h : ∃ k ∈ g.kids, LStep μ k
  · exact .inr h
  · refine .inl fun k hk' => ?_
    obtain ⟨j, hj⟩ := List.mem_iff_getElem?.mp hk'
    have hj₂ : j < g₂.kids.length := hr.len ▸ Flick.Elab.lt_of_getElem? hj
    have hk₂ := List.getElem?_eq_getElem hj₂
    rcases (hr.kids j k _ (by simp) hj hk₂).final_rev (hk _ (List.mem_of_getElem? hk₂)) hst hB with
      hf | hs
    · exact hf
    · exact (h ⟨k, hk', hs⟩).elim

theorem All2.snoc_left {α β : Type} {R : α → β → Prop} {l : List α} {m : List β} {b : β}
    (h : All2 R l (m ++ [b])) : ∃ l' a, l = l' ++ [a] := by
  have := Flick.Elab.All2.length h
  rcases List.eq_nil_or_concat l with rfl | ⟨l', a, rfl⟩
  · simp at this
  · exact ⟨l', a, by simp⟩

theorem All2.nil_right {α β : Type} {R : α → β → Prop} {l : List α} (h : All2 R l []) : l = [] := by
  cases h; rfl

/-- What the left side does at a head step of the right side at its focus: a head step, or a
step of one of its children alone. -/
def HMoves (Λ : List Rgn) (μ : List Tm) (r : Tm) : Prop :=
  (∃ Λ' μ' r', RHead Λ μ r Λ' μ' r') ∨
    ∃ (p : Nat) (g : Rgn), Λ[p]? = some g ∧ g.live = true ∧ ∃ k ∈ g.kids, LStep μ k

/-- **A runtime head step** of the right side at the focus is one of the left side, or a child of
the left side steps alone. -/
theorem rhead_rev {Λ Λ₂ Λ₂' : List Rgn} {μ μ₂ μ₂' : List Tm} {r r₂ r₂' : Tm}
    (hs : Sim W .top r r₂) (hr : Redex r) (hr₂ : Redex r₂) (hst : StR W μ μ₂)
    (hrg : RgsR W μ none Λ Λ₂) (hB : ¬ W.B) (hh : RHead Λ₂ μ₂ r₂ Λ₂' μ₂' r₂') :
    HMoves Λ μ r := by
  have nv : ∀ {F : Frame} {a : Tm}, ¬ Value a → r₂ = F.plug a → F.ok → False :=
    fun ha he hF => ha (hr₂.2 _ _ hF he.symm)
  -- the left cell of a related location
  have cell : ∀ {l l' : Nat} {k : Kd}, W.L l l' k → ∃ c, μ[l]? = some c := fun hl => by
    obtain ⟨c, -, hc, -, -⟩ := hst.cell _ _ _ hl
    exact ⟨c, hc⟩
  cases hh with
  | refMake =>
    obtain ⟨f, a, rfl, hf, -⟩ := hs.rinv_app
    rw [hf.top_const_rev rfl]
    exact Or.inl ⟨_, _, _, .refMake (Redex.app hr rfl).2⟩
  | refGet | refGetCell =>
    obtain ⟨f, a, rfl, hf, ha⟩ := hs.rinv_app
    rw [hf.top_const_rev rfl]
    obtain ⟨l, k, rfl, hl⟩ := ha.rinv_loc hB
    obtain ⟨c, hc⟩ := cell hl
    by_cases hic : c.IsCell
    · exact Or.inl ⟨_, _, _, .refGetCell hc hic⟩
    · exact Or.inl ⟨_, _, _, .refGet hc hic⟩
  | refSet =>
    obtain ⟨f, a, rfl, hf, -⟩ := hs.rinv_app
    obtain ⟨f₁, y, rfl, hf₁, hy⟩ := hf.rinv_app
    rw [hf₁.top_const_rev rfl]
    obtain ⟨l, k, rfl, hl⟩ := hy.rinv_loc hB
    exact Or.inl ⟨_, _, _, .refSet (Redex.app hr rfl).2 (hst.lt₁ hl)⟩
  | await =>
    obtain ⟨e, rfl, he⟩ := hs.rinv_await
    have hev := hr.2 .await e trivial rfl
    obtain ⟨e₀, rfl⟩ := (he.value_any hev .task .top).rinv_task
    exact Or.inl ⟨_, _, _, .await⟩
  | lazy => obtain ⟨e, rfl⟩ := hs.rinv_lazy; exact Or.inl ⟨_, _, _, .lazy⟩
  | forcePending | forceDone | forceRunning =>
    obtain ⟨e, rfl, he⟩ := hs.rinv_force
    have hev := hr.2 .force e trivial rfl
    obtain ⟨l, k, rfl, hl⟩ := (he.value_any hev .loc .top).rinv_loc hB
    obtain ⟨c, hc⟩ := cell hl
    by_cases h₁ : ∃ e', c = .lazy e'
    · obtain ⟨e', rfl⟩ := h₁; exact Or.inl ⟨_, _, _, .forcePending hc⟩
    · by_cases h₂ : ∃ o, c = .done o
      · obtain ⟨o, rfl⟩ := h₂; exact Or.inl ⟨_, _, _, .forceDone hc⟩
      · exact Or.inl ⟨_, _, _, .forceRunning hc (fun e h => h₁ ⟨e, h⟩) (fun o h => h₂ ⟨o, h⟩)⟩
  | settleVal =>
    obtain ⟨ℓ, a, rfl, -⟩ := hs.rinv_settle
    exact Or.inl ⟨_, _, _, .settleVal (hr.2 (.settle _) a trivial rfl)⟩
  | settleRaise => exact (nv (F := .settle _) raise_not_value rfl trivial).elim
  | settleUnw => exact (nv (F := .settle _) (fun h => by cases h) rfl trivial).elim
  | tryRaise => exact (nv (F := .try_ _) raise_not_value rfl trivial).elim
  | hctlRaise => exact (nv (F := .hctlA _ _ _ _ _ _ _ _) raise_not_value rfl trivial).elim
  | hctlUnw => exact (nv (F := .hctlA _ _ _ _ _ _ _ _) (fun h => by cases h) rfl trivial).elim
  | hresRaise => exact (nv (F := .hresB _ _ _) raise_not_value rfl trivial).elim
  | hresUnw => exact (nv (F := .hresB _ _ _) (fun h => by cases h) rfl trivial).elim
  | hkarmRaise => exact (nv (F := .hkarmA _ _ _ _ _) raise_not_value rfl trivial).elim
  | hkarmUnw => exact (nv (F := .hkarmA _ _ _ _ _) (fun h => by cases h) rfl trivial).elim
  | hwithCatch => exact (nv (F := .hwith _ _ _ _ _) (fun h => by cases h) rfl trivial).elim
  | handleYes hv | handleNo hv | hguardYes hv | hguardNo hv | handleNil hv =>
    obtain ⟨v, al, rfl, hv', hal⟩ := hs.rinv_handle
    have hvv := hv'.top_value_rev rfl hv
    first
      | (obtain ⟨b, rr, rfl⟩ := hal.rinv_acons
         cases hm : matchP _ v with
         | none => exact Or.inl ⟨_, _, _, .handleNo hvv hm⟩
         | some δ => exact Or.inl ⟨_, _, _, .handleYes hvv hm⟩)
      | (obtain ⟨g, b, rr, rfl⟩ := hal.rinv_aguard
         cases hm : matchP _ v with
         | none => exact Or.inl ⟨_, _, _, .hguardNo hvv hm⟩
         | some δ => exact Or.inl ⟨_, _, _, .hguardYes hvv hm⟩)
      | (rw [hal.rinv_anil]; exact Or.inl ⟨_, _, _, .handleNil hvv⟩)
  | scopeRun =>
    obtain ⟨f, a, rfl, hf, -⟩ := hs.rinv_app
    rw [hf.top_const_rev rfl]
    exact Or.inl ⟨_, _, _, .scopeRun (Redex.app hr rfl).2⟩
  | scope => obtain ⟨e, rfl⟩ := hs.rinv_scope; exact Or.inl ⟨_, _, _, .scope⟩
  | open_ hg hl | op hg hl =>
    obtain ⟨f, a, rfl, hf, ha⟩ := hs.rinv_app
    rw [hf.top_const_rev rfl]
    obtain ⟨g, hg', hR⟩ := hrg.get' hg
    first
      | (rw [ha.rinv_own]; exact Or.inl ⟨_, _, _, .open_ hg' (hR.live ▸ hl)⟩)
      | (rw [ha.rinv_cap]; exact Or.inl ⟨_, _, _, .op hg' (hR.live ▸ hl)⟩)
  | finRun hg hl hw hk hfs | close hg hl hw hk hfs =>
    obtain ⟨w, rfl, -⟩ := hs.rinv_inside
    have hwv : Final w := .inl (hr.2 (.inside _) w trivial rfl)
    obtain ⟨g, hg', hR⟩ := hrg.get' hg
    have hl' : g.live = true := hR.live ▸ hl
    rcases hR.kids_rev hk hst hB with hk' | hs'
    · first
        | (obtain ⟨fs', f', hfs'⟩ := All2.snoc_left (hfs ▸ hR.fins)
           exact Or.inl ⟨_, _, _, .finRun hg' hl' hwv hk' hfs'⟩)
        | exact Or.inl ⟨_, _, _, .close hg' hl' hwv hk' (All2.nil_right (hfs ▸ hR.fins))⟩
    · exact Or.inr ⟨_, g, hg', hl', hs'⟩
  | hdl => obtain ⟨al, rr, e, rfl⟩ := hs.rinv_hdl; exact Or.inl ⟨_, _, _, .hdl⟩

end Flick.Equiv
