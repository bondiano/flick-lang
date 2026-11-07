import Formal.KidRow

/-!
# The typing of a child in the handlers of its context
-/

namespace Flick

variable {S : Sig} {Ψ : List Ty}

/-- The work of a child in the handlers of its context. -/
theorem withs_typing {x r : Tm} {o : Nat}
    (mk : ∀ {Θ' t' φ'}, Typ S Ψ Θ' [] r (.ex t' φ') → Entails Θ' Facts.age →
      ∃ tx R εx, Typ S Ψ Facts.age [] x (.ex tx (.rt R εx)) ∧ KidRow S o φ' [] R)
    (hnp : ∀ {Ψ' Θ' t'}, ¬ Typ S Ψ' Θ' [] r (.ex t' .pure)) (hv : ¬ Value r) :
    ∀ (E : List Frame) {b : Nat} {Θ : Facts} {t : Ty} {φ : Stage}, OkE E →
      Typ S Ψ Θ [] (plug E r) (.ex t φ) → Entails Θ Facts.age → AscE b E →
      ∃ t' R' ε', Typ S Ψ Facts.age [] (withs o E x) (.ex t' (.rt R' ε')) ∧ KidRow S o φ E R' := by
  refine list_rev_ind ?_ ?_
  · intro b Θ t φ _ h hΘ _
    exact mk h hΘ
  · intro E₀ G ih b Θ t φ hE h hΘ hA
    obtain ⟨hE₀, hG⟩ := OkE.of_append hE
    rw [plug_append] at h
    simp only [plug] at h
    have hnv := not_value_plugs hv E₀
    have hA₀ := hA.of_append
    rw [withs_append]
    have mem : ∀ {p}, .inside p ∈ E₀ → .inside p ∈ E₀ ++ [G] := fun h => List.mem_append_left _ h
    have mem' : ∀ {F}, F ∈ E₀ → F ∈ E₀ ++ [G] := fun h => List.mem_append_left _ h
    have nopure : ∀ {k Θ' t'}, ¬ Typ S (Ψ.map (Ty.up k)) Θ' [] (plug E₀ r) (.ex t' .pure) := by
      intro k Θ' t' h'
      obtain ⟨_, _, _, _, hr, rel, _⟩ := ctx_typing _ hE₀ hv h'
      exact hnp (rel.pure rfl ▸ hr)
    cases G with
    | hwith q ℓ E' al rf =>
      cases h with
      | hwith hℓ hal _ _ hb hrow hc hφ _ =>
        obtain ⟨t', R', ε', hw, hk⟩ := ih hE₀ hb hΘ hA₀
        have up : ∀ y, _ → φ.svc y := fun y h => Stage.le_svc hφ y h
        simp only [Frame.wrap]
        split
        · rename_i hq
          obtain ⟨hR, hk'⟩ := kid_kept (G := .hwith q ℓ E' al rf) hΘ rfl trivial rfl hA hq hrow up hk
          refine ⟨t', _, Row.union ε' _, .hkeep hℓ (hal.cut hΘ) hw
            ⟨hR, fun _ h => .inl h⟩ ⟨hc.1, fun hn => Row.kid_out _ (hc.2 hn)⟩
            (fun hn => .inr (.inr ⟨rfl, hn⟩)) ⟨Row.kid_out, fun _ h => .inr h⟩ (Stage.le_refl _) .refl, hk'⟩
        · rename_i hq
          exact ⟨t', R', ε', hw, kid_dropped (G := .hwith q ℓ E' al rf) hΘ rfl trivial hA hq hrow up hk⟩
      | errVal hv' => exact (not_value_plug hnv _ hv').elim
    | hkeep q ℓ E' al =>
      cases h with
      | hkeep hℓ hal hb hrow hc hnt hle hφ _ =>
        obtain ⟨t', R', ε', hw, hk⟩ := ih hE₀ hb hΘ hA₀
        have up : ∀ y, _ → φ.svc y := fun y h => Stage.le_svc hφ y h
        simp only [Frame.wrap]
        split
        · rename_i hq
          obtain ⟨hR, hk'⟩ := kid_kept (G := .hkeep q ℓ E' al) hΘ rfl trivial rfl hA hq hrow up hk
          refine ⟨t', _, Row.union ε' _, .hkeep hℓ (hal.cut hΘ) hw
            ⟨hR, fun _ h => .inl h⟩ ⟨hc.1, fun hn => Row.kid_out _ (hc.2 hn)⟩
            (fun hn => Row.kid_out _ (hnt hn))
            ⟨fun y hy => Row.kid_out y (hle.1 y hy), fun y hy => .inr (hle.2 y hy)⟩ (Stage.le_refl _) .refl, hk'⟩
        · rename_i hq
          exact ⟨t', R', ε', hw, kid_dropped (G := .hkeep q ℓ E' al) hΘ rfl trivial hA hq hrow up hk⟩
      | errVal hv' => exact (not_value_plug hnv _ hv').elim
    | inside p =>
      cases h with
      | inside hF hb hrow _ _ hφ _ =>
        obtain ⟨t', R', ε', hw, h1, h2⟩ := ih hE₀ hb (hΘ.inside_age hF) hA₀
        have up : ∀ y, _ → φ.svc y := fun y h => Stage.le_svc hφ y h
        refine ⟨t', R', ε', hw, fun N a hy => ?_, fun N hy => ?_⟩
        · rcases h1 N a hy with h' | ⟨p', rfl, hp⟩ | ⟨F, hF', x⟩
          · rcases Stage.le_svc hrow _ h' with h₁ | h₁ | h₁
            · exact h₁.elim
            · exact .inl (up _ h₁)
            · exact .inr (.inl ⟨p, h₁.1, by simp⟩)
          · exact .inr (.inl ⟨p', rfl, mem hp⟩)
          · exact .inr (.inr ⟨F, mem' hF', x⟩)
        · obtain ⟨a₁, a₂⟩ := h2 N hy
          refine ⟨?_, ?_⟩
          · rcases a₁ with a₁ | ⟨hn, F, hF', hI⟩
            · rcases Stage.le_svc hrow _ a₁ with h' | h' | h'
              · exact h'.elim
              · exact .inl (up _ h')
              · exact h'.elim
            · exact .inr ⟨hn, F, mem' hF', hI⟩
          · rcases a₂ with h₂ | h₂ | ⟨F, hF', x⟩
            · rcases Stage.le_svc hrow _ h₂ with h' | h' | h'
              · exact h'.elim
              · exact .inl (up _ h')
              · cases h'.1; exact .inr (.inl (by simp))
            · exact .inr (.inl (mem h₂))
            · exact .inr (.inr ⟨F, mem' hF', x⟩)
      | errVal hv' => exact (not_value_plug hnv _ hv').elim
    | _ =>
      obtain ⟨k, Θ', t₁, φ₁, h₁, rel, hk, -⟩ := frame_typing hnv _ hG h
      rcases hk with rfl | ⟨rfl, -, hΘ'⟩
      · exact (nopure h₁).elim
      · rw [hΘ' (fun h => h.elim)] at h₁
        simp only [List.map_up_zero] at h₁
        obtain ⟨t', R', ε', hw, h1, h2⟩ := ih hE₀ h₁ hΘ hA₀
        have up : ∀ y, φ₁.svc y → φ.svc y := fun y h' =>
          (rel.svc _ h').resolve_right fun h₂ => by
            cases y <;> simp [Frame.dom, Frame.installs] at h₂
        refine ⟨t', R', ε', hw, fun N a hy => ?_, fun N hy => ?_⟩
        · rcases h1 N a hy with h' | ⟨p, rfl, hp⟩ | ⟨F, hF', x⟩
          · exact .inl (up _ h')
          · exact .inr (.inl ⟨p, rfl, mem hp⟩)
          · exact .inr (.inr ⟨F, mem' hF', x⟩)
        · obtain ⟨a₁, a₂⟩ := h2 N hy
          refine ⟨?_, ?_⟩
          · rcases a₁ with a₁ | ⟨hn, F, hF', hI⟩
            · exact .inl (up _ a₁)
            · exact .inr ⟨hn, F, mem' hF', hI⟩
          · rcases a₂ with h₂ | h₂ | ⟨F, hF', x⟩
            · exact .inl (up _ h₂)
            · exact .inr (.inl (mem h₂))
            · exact .inr (.inr ⟨F, mem' hF', x⟩)

/-- The work a finalizer or a child is given, awaited: its obligations are in the
stage of the registration (regions.md §2.7). -/
theorem fin_await {Θ : Facts} {c : Const} {p : Nat} {w : Tm} {T : Ty} {φ : Stage}
    (h : Typ S Ψ Θ [] (.app (.app (.const c) (.own p)) w) (.ex T φ))
    (hc : c = .addFin ∨ c = .spawn) (hΘ : Entails Θ Facts.age) (hv : Value w) :
    ∃ tx R εx, Typ S Ψ Facts.age [] (.await w) (.ex tx (.rt R εx)) ∧ KidRow S p φ [] R := by
  cases h with
  | app hf hw hψ _ =>
    cases hf with
    | app hc' ho _ hT₁ =>
      cases hc' with
      | perform => rcases hc with h | h <;> cases h
      | const hok hT' =>
        rcases hc with rfl | rfl
        · obtain ⟨_, _, _, _, h, la₀, _, lt, -⟩ := hT'.arrow_left
          cases h; cases la₀
          obtain ⟨q, hq, hs⟩ := canonical_owner .own ho
          cases hq
          obtain ⟨_, _, _, _, h, la, lφ, _, -⟩ := (lt.trans hT₁).arrow_left
          cases h
          obtain ⟨r', e', t', C', rfl, hr', -, -, -⟩ := la.eff_right
          have up := fun y h => Stage.le_svc (Stage.le_trans lφ hψ) y h
          refine ⟨_, _, _, .await ((hw.valueAny hv _).cut hΘ) (Stage.le_refl _) .refl,
            fun N a ha => .inl (up _ (.inr (.inl (hr' _ ha)))), fun N ha => ⟨.inl (up _ (.inr (.inl (hr' _ ha)))),
              .inl (up _ (.inr (.inr ⟨hr' _ ha, by rw [hs]; rfl⟩)))⟩⟩
        · obtain ⟨_, _, _, _, h, la₀, _, lt, -⟩ := hT'.arrow_left
          cases h; cases la₀
          obtain ⟨q, hq, hs⟩ := canonical_owner .own ho
          cases hq
          obtain ⟨_, _, _, _, h, la, lφ, _, -⟩ := (lt.trans hT₁).arrow_left
          cases h
          obtain ⟨r', e', t', C', rfl, hr', -, -, -⟩ := la.eff_right
          have up := fun y h => Stage.le_svc (Stage.le_trans lφ hψ) y h
          have inr : ∀ y, r' y → (∃ a, y = .atm a) ∨ _ := fun y h => (hr' y h).imp
            (fun h' => by cases y <;> simp_all [Row.scope, Row.ofCaps]) id
          refine ⟨_, _, _, .await ((hw.valueAny hv _).cut hΘ) (Stage.le_refl _) .refl,
            fun N a ha => ?_, fun N ha => ?_⟩
          · rcases inr _ ha with ⟨_, h'⟩ | h'
            · cases h'
            · exact .inl (up _ (.inl h'))
          · rcases inr _ ha with ⟨_, h'⟩ | h'
            · cases h'
            · exact ⟨.inl (up _ (.inl h')), .inl (up _ (.inr ⟨h', by rw [hs]; rfl⟩))⟩
  | errVal hv' => exact (fin_not_value hc hv').elim

/-- An operation at the hole of a context that no frame answers has its effect in the stage of
the context: a handler without an arm for it leaves its effect in the row outside. -/
theorem perform_out {Ef o : Name} {c : OpClass} {r : Tm}
    (hask : ∀ {Ψ' Θ' t' φ'}, Typ S Ψ' Θ' [] r (.ex t' φ') → φ'.svc (.nm Ef)) (hv : ¬ Value r)
    (hop : ∃ a b, S.op Ef o = some (c, a, b)) :
    ∀ (E : List Frame) {Ψ : List Ty} {Θ : Facts} {t : Ty} {φ : Stage}, OkE E →
      (∀ F ∈ E, F.handles Ef o c = none) → Typ S Ψ Θ [] (plug E r) (.ex t φ) → φ.svc (.nm Ef) := by
  refine list_rev_ind ?_ ?_
  · intro Ψ Θ t φ _ _ h
    exact hask h
  · intro E₀ G ih Ψ Θ t φ hE hh h
    obtain ⟨hE₀, hG⟩ := OkE.of_append hE
    rw [plug_append] at h
    simp only [plug] at h
    have hnv := not_value_plugs hv E₀
    have hh₀ : ∀ F ∈ E₀, F.handles Ef o c = none := fun F hF => hh F (List.mem_append_left _ hF)
    have hhG : G.handles Ef o c = none := hh G (by simp)
    obtain ⟨k, Θ', t₁, φ₁, h₁, rel, -⟩ := frame_typing hnv _ hG h
    have hn := ih hE₀ hh₀ h₁
    rcases rel.svc _ hn with h₂ | h₂
    · exact h₂
    · obtain ⟨a, b, hop⟩ := hop
      cases G <;> simp only [Frame.dom, Frame.installs] at h₂
      · subst h₂
        cases h with
        | hwith _ _ _ _ _ _ hc hφ _ =>
          refine Stage.le_svc hφ _ (hc.2 fun hcomp => ?_)
          have h₁ := hcomp o c a b hop
          simp [Frame.handles, h₁] at hhG
        | errVal hv' => exact (not_value_plug hnv _ hv').elim
      · subst h₂
        cases h with
        | hkeep _ _ _ _ hc hnt _ hφ _ =>
          refine Stage.le_svc hφ _ ?_
          by_cases hct : c = .tail
          · subst hct
            refine hc.2 fun hcomp => ?_
            have h₁ := hcomp o _ a b hop
            simp [Frame.handles, h₁] at hhG
          · exact hnt ⟨o, c, a, b, hop, hct⟩
        | errVal hv' => exact (not_value_plug hnv _ hv').elim

end Flick
