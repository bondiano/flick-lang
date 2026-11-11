import Formal.KidRow

/-!
# The typing of a child in the handlers of its context
-/

namespace Flick

variable {S : Sig} {Ψ : List Ty}

theorem Frame.provides_nonatm {F : Frame} {y : Elem} (hy : ∀ a, y ≠ .atm a)
    (h : Frame.provides F y) : F.dom y := by
  rcases h with h | ⟨_, -, rfl⟩
  · exact h
  · exact (hy _ rfl).elim

/-- A use stage adds only region atoms to the row it uses. -/
theorem Stage.use_le_rows {r ε r' ε' : Row} {C C' : Caps}
    (h : (Stage.use (.rt r ε) C).le (Stage.use (.rt r' ε') C')) :
    ∀ y, (∀ a, y ≠ .atm a) → r y → r' y := by
  intro y hy hr
  rcases h.1 y (.inl hr) with h' | h'
  · exact h'
  · cases y with
    | atm a => exact (hy a rfl).elim
    | _ => exact h'.elim

theorem Stage.use_eq_rows {r ε R ε' : Row} {C : Caps}
    (h : Stage.use (.rt r ε) C = .rt R ε') : ∀ y, (∀ a, y ≠ .atm a) → R y → r y := by
  intro y hy hR
  simp only [Stage.use, Stage.rt.injEq] at h; obtain ⟨rfl, -⟩ := h
  rcases hR with h | h
  · exact h
  · cases y with
    | atm a => exact (hy a rfl).elim
    | _ => exact h.elim

/-- The work of a child in the handlers of its context. When `Q`, every region atom of its row
is `Old` and in the stage of the context or provided by one of its frames, or is an atom of the
arms of a handler the child keeps. -/
theorem withs_typing_atm {x r : Tm} {o : Nat} (Q : Prop) (Old : Atom → Prop)
    (mk : ∀ {Θ' t' φ'}, Typ S Ψ Θ' [] r (.ex t' φ') → Entails Θ' Facts.age →
      ∃ tx R εx, Typ S Ψ Facts.age [] x (.ex tx (.rt R εx)) ∧ KidRow S o φ' [] R ∧
        (Q → ∀ a, R (.atm a) → Old a ∧ φ'.svc (.atm a)))
    (hnp : ∀ {Ψ' Θ' t'}, ¬ Typ S Ψ' Θ' [] r (.ex t' .pure)) (hv : ¬ Value r) :
    ∀ (E : List Frame) {b : Nat} {Θ : Facts} {t : Ty} {φ : Stage}, OkE E →
      Typ S Ψ Θ [] (plug E r) (.ex t φ) → Entails Θ Facts.age → AscE b E →
      ∃ t' R' ε', Typ S Ψ Facts.age [] (withs o E x) (.ex t' (.rt R' ε')) ∧ KidRow S o φ E R' ∧
        (Q → ∀ a, R' (.atm a) → (Old a ∧ (φ.svc (.atm a) ∨ ∃ F ∈ E, Frame.provides F (.atm a))) ∨
          KeptAtm Ψ o E a) := by
  refine list_rev_ind ?_ ?_
  · intro b Θ t φ _ h hΘ _
    obtain ⟨tx, R, εx, h₁, h₂, h₃⟩ := mk h hΘ
    exact ⟨tx, R, εx, h₁, h₂, fun hQ a ha => .inl ⟨(h₃ hQ a ha).1, .inl (h₃ hQ a ha).2⟩⟩
  · intro E₀ G ih b Θ t φ hE h hΘ hA
    obtain ⟨hE₀, hG⟩ := OkE.of_append hE
    rw [plug_append] at h
    simp only [plug] at h
    have hnv := not_value_plugs hv E₀
    have hA₀ := hA.of_append
    rw [withs_append]
    have mem : ∀ {p}, .inside p ∈ E₀ → .inside p ∈ E₀ ++ [G] := fun h => List.mem_append_left _ h
    have mem' : ∀ {F}, F ∈ E₀ → F ∈ E₀ ++ [G] := fun h => List.mem_append_left _ h
    have self : G ∈ E₀ ++ [G] := by simp
    have nopure : ∀ {k Θ' t'}, ¬ Typ S (Ψ.map (Ty.up k)) Θ' [] (plug E₀ r) (.ex t' .pure) := by
      intro k Θ' t' h'
      obtain ⟨_, _, _, _, hr, rel, _⟩ := ctx_typing _ hE₀ hv h'
      exact hnp (rel.pure rfl ▸ hr)
    -- an atom of the stage of the hole that the frame passes on
    have lift : ∀ {φ₀ : Stage} {Q' : Prop} {R' : Row},
        (∀ a, φ₀.svc (.atm a) → φ.svc (.atm a) ∨ Frame.provides G (.atm a)) →
        (Q' → ∀ a, R' (.atm a) → (Old a ∧ (φ₀.svc (.atm a) ∨ ∃ F ∈ E₀, Frame.provides F (.atm a))) ∨
          KeptAtm Ψ o E₀ a) →
        Q' → ∀ a, R' (.atm a) → (Old a ∧ (φ.svc (.atm a) ∨ ∃ F ∈ E₀ ++ [G], Frame.provides F (.atm a))) ∨
          KeptAtm Ψ o (E₀ ++ [G]) a := by
      intro φ₀ Q' R' hφ₀ hat hQ a ha
      rcases hat hQ a ha with ⟨ho, h' | ⟨F, hF, hd⟩⟩ | hk
      · rcases hφ₀ a h' with h'' | h''
        · exact .inl ⟨ho, .inl h''⟩
        · exact .inl ⟨ho, .inr ⟨G, self, h''⟩⟩
      · exact .inl ⟨ho, .inr ⟨F, mem' hF, hd⟩⟩
      · exact .inr (hk.mono fun _ => mem')
    cases G with
    | hwith q ℓ E' al rf =>
      cases h with
      | hwith hℓ hal _ _ hb hrow hc hφ _ =>
        obtain ⟨t', R', ε', hw, hk, hat⟩ := ih hE₀ hb hΘ hA₀
        have up : ∀ y, _ → φ.svc y := fun y h => Stage.le_svc hφ y h
        have hl := lift (fun a h' => .inl (up _ (Stage.le_svc hrow _ h'))) hat
        simp only [Frame.wrap]
        split
        · rename_i hq
          obtain ⟨hR, hk'⟩ := kid_kept (G := .hwith q ℓ E' al rf) hΘ rfl trivial rfl hA hq hrow
            (fun _ _ h => h) up hk
          refine ⟨t', _, Row.union ε' _, .hkeep hℓ (hal.cut hΘ) hw
            ⟨hR, fun _ h => .inl h⟩ ⟨hc.1, fun hn => Row.kid_out _ (hc.2 hn)⟩
            (fun hn => .inr (.inr ⟨rfl, hn⟩)) ⟨Row.kid_out, fun _ h => .inr h⟩ (Stage.le_refl _) .refl, hk',
            fun hQ a ha => ?_⟩
          rcases ha with ha | ha
          · exact hl hQ a ha
          · exact .inr ⟨_, self, q, ℓ, _, _, _, rfl, hq, hℓ, ha⟩
        · rename_i hq
          exact ⟨t', R', ε', hw, kid_dropped (G := .hwith q ℓ E' al rf) hΘ rfl trivial hA hq hrow up hk, hl⟩
      | errVal hv' | allI hv' => exact (not_value_plug hnv _ hv').elim
    | hkeep q ℓ E' al =>
      cases h with
      | hkeep hℓ hal hb hrow hc hnt hle hφ _ =>
        obtain ⟨t', R', ε', hw, hk, hat⟩ := ih hE₀ hb hΘ hA₀
        have up : ∀ y, _ → φ.svc y := fun y h => Stage.le_svc hφ y h
        have hl := lift (fun a h' => .inl (up _ (Stage.le_svc hrow _ h'))) hat
        simp only [Frame.wrap]
        split
        · rename_i hq
          obtain ⟨hR, hk'⟩ := kid_kept (G := .hkeep q ℓ E' al) (rk := fun y => _ ∨ (∀ a, y ≠ .atm a) ∧ _)
            hΘ rfl trivial rfl hA hq hrow (fun y hy h => .inr ⟨hy, h⟩)
            (fun y h => h.elim (fun h => up _ (hle.1 y h)) fun h => up _ h.2) hk
          refine ⟨t', _, Row.union ε' _, .hkeep hℓ (hal.cut hΘ) hw
            ⟨hR, fun _ h => .inl h⟩
            ⟨hc.1, fun hn => Row.kid_out _ (.inr ⟨(fun _ h => by cases h), hc.2 hn⟩)⟩
            (fun hn => Row.kid_out _ (.inr ⟨(fun _ h => by cases h), hnt hn⟩))
            ⟨fun y hy => Row.kid_out y (.inl hy), fun y hy => .inr (hle.2 y hy)⟩ (Stage.le_refl _) .refl, hk',
            fun hQ a ha => ?_⟩
          rcases ha with ha | ha | ⟨hn, -⟩
          · exact hl hQ a ha
          · exact .inr ⟨_, self, q, ℓ, _, _, _, rfl, hq, hℓ, ha⟩
          · exact (hn a rfl).elim
        · rename_i hq
          exact ⟨t', R', ε', hw, kid_dropped (G := .hkeep q ℓ E' al) hΘ rfl trivial hA hq hrow up hk, hl⟩
      | errVal hv' | allI hv' => exact (not_value_plug hnv _ hv').elim
    | inside p =>
      cases h with
      | inside hF hb hrow _ _ hφ _ =>
        obtain ⟨t', R', ε', hw, ⟨h1, h2⟩, hat⟩ := ih hE₀ hb (hΘ.inside_age hF) hA₀
        have up : ∀ y, _ → φ.svc y := fun y h => Stage.le_svc hφ y h
        refine ⟨t', R', ε', hw, ⟨fun N a hy => ?_, fun N hy => ?_⟩, lift (fun a h' => ?_) hat⟩
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
        · rcases Stage.le_svc hrow _ h' with h₁ | h₁ | h₁
          · exact .inr (Frame.provides_of ⟨p, rfl, h₁⟩)
          · exact .inl (up _ h₁)
          · exact h₁.elim
      | errVal hv' | allI hv' => exact (not_value_plug hnv _ hv').elim
    | _ =>
      obtain ⟨k, Θ', t₁, φ₁, h₁, rel, hk, -⟩ := frame_typing hnv _ hG h
      rcases hk with rfl | ⟨rfl, -, hΘ'⟩
      · exact (nopure h₁).elim
      · rw [hΘ' (fun h => h.elim)] at h₁
        simp only [List.map_up_zero] at h₁
        obtain ⟨t', R', ε', hw, ⟨h1, h2⟩, hat⟩ := ih hE₀ h₁ hΘ hA₀
        have up : ∀ y, φ₁.svc y → (∀ a, y ≠ .atm a) → φ.svc.modNames y := fun y h' hy =>
          (rel.svc _ h').resolve_right fun h₂ => by
            have h₃ := Frame.provides_nonatm hy h₂
            cases y <;> simp [Frame.dom, Frame.installs] at h₃
        refine ⟨t', R', ε', hw, ⟨fun N a hy => ?_, fun N hy => ?_⟩, lift (fun a h' => rel.svc _ h') hat⟩
        · rcases h1 N a hy with h' | ⟨p, rfl, hp⟩ | ⟨F, hF', x⟩
          · exact .inl (up _ h' fun _ h => by cases h)
          · exact .inr (.inl ⟨p, rfl, mem hp⟩)
          · exact .inr (.inr ⟨F, mem' hF', x⟩)
        · obtain ⟨a₁, a₂⟩ := h2 N hy
          refine ⟨?_, ?_⟩
          · rcases a₁ with a₁ | ⟨hn, F, hF', hI⟩
            · exact .inl (up _ a₁ fun _ h => by cases h)
            · exact .inr ⟨hn, F, mem' hF', hI⟩
          · rcases a₂ with h₂ | h₂ | ⟨F, hF', x⟩
            · exact .inl (up _ h₂ fun _ h => by cases h)
            · exact .inr (.inl (mem h₂))
            · exact .inr (.inr ⟨F, mem' hF', x⟩)

/-- The work of a child in the handlers of its context. -/
theorem withs_typing {x r : Tm} {o : Nat}
    (mk : ∀ {Θ' t' φ'}, Typ S Ψ Θ' [] r (.ex t' φ') → Entails Θ' Facts.age →
      ∃ tx R εx, Typ S Ψ Facts.age [] x (.ex tx (.rt R εx)) ∧ KidRow S o φ' [] R)
    (hnp : ∀ {Ψ' Θ' t'}, ¬ Typ S Ψ' Θ' [] r (.ex t' .pure)) (hv : ¬ Value r)
    (E : List Frame) {b : Nat} {Θ : Facts} {t : Ty} {φ : Stage} (hE : OkE E)
    (h : Typ S Ψ Θ [] (plug E r) (.ex t φ)) (hΘ : Entails Θ Facts.age) (hA : AscE b E) :
    ∃ t' R' ε', Typ S Ψ Facts.age [] (withs o E x) (.ex t' (.rt R' ε')) ∧ KidRow S o φ E R' :=
  let ⟨t', R', ε', h₁, h₂, _⟩ := withs_typing_atm False (fun _ => True)
    (fun h' hΘ' => let ⟨tx, R, εx, k₁, k₂⟩ := mk h' hΘ'; ⟨tx, R, εx, k₁, k₂, False.elim⟩) hnp hv E hE h hΘ hA
  ⟨t', R', ε', h₁, h₂⟩
/-- The work a finalizer or a child is given, awaited: its obligations are in the
stage of the registration (regions.md §2.7). -/
theorem fin_await_atm {Θ : Facts} {c : Const} {p : Nat} {w : Tm} {T : Ty} {φ : Stage}
    (h : Typ S Ψ Θ [] (.app (.app (.const c) (.own p)) w) (.ex T φ))
    (hc : c = .addFin ∨ c = .spawn) (hΘ : Entails Θ Facts.age) (hv : Value w) :
    ∃ tx R εx, Typ S Ψ Facts.age [] (.await w) (.ex tx (.rt R εx)) ∧ KidRow S p φ [] R ∧
      (∀ a, R (.atm a) → (a = .r p ∨ ∀ x, a = .r x → x < p) ∧ φ.svc (.atm a)) := by
  cases h with
  | app hf hw hψ _ =>
    cases hf with
    | app hc' ho _ hT₁ =>
      cases hc' with
      | perform => rcases hc with h | h <;> cases h
      | const hok hT' =>
        rcases hc with rfl | rfl
        · obtain ⟨_, _, _, _, h, la₀, _, lt, -⟩ := hT'.arrow_left
          cases h; cases LeLive.eq_owner la₀
          obtain ⟨q, hq, hs⟩ := canonical_owner .own ho
          cases hq
          obtain ⟨_, _, _, _, h, la, lφ, _, -⟩ := (lt.trans hT₁).arrow_left
          cases h
          obtain ⟨r', e', t', C', rfl, hle, -, -⟩ := LeLive.eff_right la
          obtain ⟨R, ε, hR⟩ := Stage.use_rt r' e' C'
          have hr' := fun (y : Elem) (hy : ∀ a, y ≠ .atm a) (h : R y) =>
            Stage.use_le_rows hle y hy (Stage.use_eq_rows hR y hy h)
          have up := fun y h => Stage.le_svc (Stage.le_trans (Stage.le_use _ _)
            (Stage.le_trans lφ hψ)) y h
          have hk : Typ S Ψ Facts.age [] (.await w) (.ex t' (.rt R ε)) := by
            rw [← hR]; exact .await ((hw.valueAny hv _).cut hΘ) (Stage.le_refl _) LeLive.refl
          refine ⟨_, R, ε, hk, ⟨fun N a ha => .inl (up _ (.inr (.inl (hr' _ (fun _ h => by cases h) ha)))),
            fun N ha => ⟨.inl (up _ (.inr (.inl (hr' _ (fun _ h => by cases h) ha)))),
              .inl (up _ (.inr (.inr ⟨hr' _ (fun _ h => by cases h) ha, by rw [hs]; rfl⟩)))⟩⟩, fun a ha => ?_⟩
          obtain ⟨-, hkeep, hsub⟩ := hok
          rw [hs] at hkeep
          have h₁ : (Stage.use (.rt r' e') C').svc (.atm a) := by rw [hR]; exact ha
          rcases Stage.use_svc _ _ (Stage.le_svc hle _ h₁) with h₂ | ⟨b, hb, he⟩
          · exact ⟨keep_old hΘ hkeep h₂, up _ (.inr (.inl h₂))⟩
          · cases he
            have hr := hsub (.atm _) hb
            exact ⟨keep_old hΘ hkeep hr, up _ (.inr (.inl hr))⟩
        · obtain ⟨_, _, _, _, h, la₀, _, lt, -⟩ := hT'.arrow_left
          cases h; cases LeLive.eq_owner la₀
          obtain ⟨q, hq, hs⟩ := canonical_owner .own ho
          cases hq
          obtain ⟨_, _, _, _, h, la, lφ, _, -⟩ := (lt.trans hT₁).arrow_left
          cases h
          obtain ⟨r', e', t', C', rfl, hle, -, -⟩ := LeLive.eff_right la
          obtain ⟨R, ε, hR⟩ := Stage.use_rt r' e' C'
          have hr' := fun (y : Elem) (hy : ∀ a, y ≠ .atm a) (h : R y) =>
            Stage.use_le_rows hle y hy (Stage.use_eq_rows hR y hy h)
          have up := fun y h => Stage.le_svc (Stage.le_trans (Stage.le_use _ _)
            (Stage.le_trans lφ hψ)) y h
          have hk : Typ S Ψ Facts.age [] (.await w) (.ex t' (.rt R ε)) := by
            rw [← hR]; exact .await ((hw.valueAny hv _).cut hΘ) (Stage.le_refl _) LeLive.refl
          refine ⟨_, R, ε, hk, ⟨fun N a ha => ?_, fun N ha => ?_⟩, fun a ha => ?_⟩
          · rcases hr' _ (fun _ h => by cases h) ha with h' | h'
            · simp [Row.scope, Row.ofCaps] at h'
            · exact .inl (up _ (.inl h'))
          · rcases hr' _ (fun _ h => by cases h) ha with h' | h'
            · simp [Row.scope, Row.ofCaps] at h'
            · exact ⟨.inl (up _ (.inl h')), .inl (up _ (.inr ⟨h', by rw [hs]; rfl⟩))⟩
          · obtain ⟨-, hkeep, hsub⟩ := hok
            rw [hs] at hkeep
            have h₁ : (Stage.use (.rt r' e') C').svc (.atm a) := by rw [hR]; exact ha
            rcases Stage.use_svc _ _ (Stage.le_svc hle _ h₁) with h₂ | ⟨b, hb, he⟩
            · rcases h₂ with h₂ | h₂
              · have ha' : a = .r p := by simpa [Row.scope, Row.ofCaps, hs, Ty.reach] using h₂
                subst ha'
                exact ⟨.inl rfl, Stage.le_svc (Stage.le_trans lφ hψ) _ (by rw [hs]; exact Or.inr rfl)⟩
              · exact ⟨keep_old hΘ hkeep h₂, up _ (.inl h₂)⟩
            · cases he
              have hr := hsub (.atm _) hb
              exact ⟨keep_old hΘ hkeep hr, up _ (.inl hr)⟩
  | errVal hv' | allI hv' => exact (fin_not_value hc hv').elim

/-- The work a finalizer or a child is given, awaited. -/
theorem fin_await {Θ : Facts} {c : Const} {p : Nat} {w : Tm} {T : Ty} {φ : Stage}
    (h : Typ S Ψ Θ [] (.app (.app (.const c) (.own p)) w) (.ex T φ))
    (hc : c = .addFin ∨ c = .spawn) (hΘ : Entails Θ Facts.age) (hv : Value w) :
    ∃ tx R εx, Typ S Ψ Facts.age [] (.await w) (.ex tx (.rt R εx)) ∧ KidRow S p φ [] R :=
  let ⟨tx, R, εx, h₁, h₂, _⟩ := fin_await_atm h hc hΘ hv
  ⟨tx, R, εx, h₁, h₂⟩

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
      have h₂ := Frame.provides_nonatm (fun _ h => by cases h) h₂
      cases G <;> simp only [Frame.dom, Frame.installs] at h₂
      · subst h₂
        cases h with
        | hwith _ _ _ _ _ _ hc hφ _ =>
          refine Stage.le_svc hφ _ (hc.2 fun hcomp => ?_)
          have h₁ := hcomp o c a b hop
          simp [Frame.handles, h₁] at hhG
        | errVal hv' | allI hv' => exact (not_value_plug hnv _ hv').elim
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
        | errVal hv' | allI hv' => exact (not_value_plug hnv _ hv').elim

end Flick
