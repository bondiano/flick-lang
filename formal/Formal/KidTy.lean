import Formal.Asc

/-!
# A child keeps the `with` frames of the place that registered it

`Fiber.spawn` and `Scope.add_finalizer` wrap the work in the `with` frames of their
context (runtime.md §3.5). The work is typed with the services and obligations of
the task; a `with` frame answers the services it has and discharges their
obligations. An obligation that the context discharged at a frame, it discharges in
the child too; one that a region of the context discharged at its end meets, in the
child, frames older than that region (`AscE.with_inside`), which outlive it.
-/

namespace Flick

variable {S : Sig} {Ψ : List Ty}

theorem list_rev_ind {α : Type} {P : List α → Prop} (h₀ : P []) (h₁ : ∀ E x, P E → P (E ++ [x])) :
    ∀ l, P l := by
  have : ∀ l : List α, P l.reverse := by
    intro l
    induction l with
    | nil => exact h₀
    | cons x l ih => simpa using h₁ _ x ih
  intro l; simpa using this l.reverse

theorem withs_append {x : Tm} {F : Frame} : ∀ {E : List Frame}, withs (E ++ [F]) x = F.wrap (withs E x)
  | [] => rfl
  | G :: E => by simp only [List.cons_append, withs]; exact withs_append (E := E) (x := G.wrap x)

theorem OkE.of_append {E : List Frame} {F : Frame} (h : OkE (E ++ [F])) : OkE E ∧ F.ok :=
  ⟨fun G hG => h G (List.mem_append_left _ hG), h F (by simp)⟩

theorem ascBE_append {b : Nat} {F : Frame} : ∀ {E : List Frame}, ascBE b (E ++ [F]) = ascBE (F.ascB b) E
  | [] => rfl
  | G :: E => by simp only [List.cons_append, ascBE]; rw [ascBE_append]

theorem AscE.of_append {b : Nat} {F : Frame} : ∀ {E : List Frame}, AscE b (E ++ [F]) → AscE (F.ascB b) E
  | [], _ => trivial
  | G :: E, h => by
    simp only [List.cons_append, AscE] at h ⊢
    rw [ascBE_append] at h
    exact ⟨AscE.of_append h.1, h.2⟩

/-- Facts of an `inside` form follow the order of creation. -/
theorem Entails.inside_age {Θ F : Facts} {p : Nat} (hΘ : Entails Θ Facts.age)
    (hF : ∀ C s, F C s → s = .rg p ∧ ∀ q, C (.r q) → q < p) : Entails (Facts.union Θ F) Facts.age := by
  intro C s h k hk
  rcases h k hk with h' | ⟨C', h₁ | h₁, h₂⟩
  · exact .inl h'
  · exact hΘ C' s (fun y hy => .inr ⟨C', h₁, hy⟩) k h₂
  · obtain ⟨rfl, hC⟩ := hF C' s h₁
    exact .inr ⟨C', fun q p' hq hp' => by cases hp'; exact hC q hq, h₂⟩

theorem Outl.age_lt {q p : Nat} (h : q < p) : Outl Facts.age (· = .r q) (Atom.r p).ty :=
  fun k hk => .inr ⟨(· = .r q), fun q' p' hq hp => by subst hk; cases hq; cases hp; exact h, hk⟩

theorem Outl.age_le {q o : Nat} (h : Outl Facts.age (· = .r q) (Atom.r o).ty) : q ≤ o := by
  rcases h (.r q) rfl with h | ⟨C', h₁, h₂⟩
  · simp [Atom.ty] at h; omega
  · exact Nat.le_of_lt (h₁ q o h₂ rfl)

/-- What the frames of the place say of the work wrapped in them: its obligations are
those of the place or of a region of the place that discharged them; its services
reach the place with the obligation for the owner `o`; the uses of a service of a
`provide` region `q` reach the place or the `inside` form of `q`, and `q` is not
younger than `o`. -/
def KidRow (o : Nat) (φ : Stage) (E : List Frame) (R : Row) : Prop :=
  (∀ N a, R (.ob N a) → φ.svc (.ob N a) ∨ ∃ p, a = .r p ∧ .inside p ∈ E) ∧
  (∀ N, R (.nm N) → φ.svc (.nm N) ∧ (φ.svc (.ob N (.r o)) ∨ .inside o ∈ E)) ∧
  (∀ N c, R (.need N c) → ∃ q, c = .r q ∧ (φ.svc (.need N (.r q)) ∨ .inside q ∈ E) ∧ q ≤ o)

/-- The work of a child in the `with` frames of its context. -/
theorem withs_typing {x r : Tm} {o : Nat}
    (mk : ∀ {Θ' t' φ'}, Typ S Ψ Θ' [] r (.ex t' φ') → Entails Θ' Facts.age →
      ∃ tx R εx, Typ S Ψ Facts.age [] x (.ex tx (.rt R εx)) ∧ KidRow o φ' [] R)
    (hnp : ∀ {Ψ' Θ' t'}, ¬ Typ S Ψ' Θ' [] r (.ex t' .pure)) (hv : ¬ Value r) :
    ∀ (E : List Frame) {b : Nat} {Θ : Facts} {t : Ty} {φ : Stage}, OkE E →
      Typ S Ψ Θ [] (plug E r) (.ex t φ) → Entails Θ Facts.age → AscE b E →
      ∃ t' R' ε', Typ S Ψ Facts.age [] (withs E x) (.ex t' (.rt R' ε')) ∧ KidRow o φ E R' := by
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
    have nopure : ∀ {k Θ' t'}, ¬ Typ S (Ψ.map (Ty.up k)) Θ' [] (plug E₀ r) (.ex t' .pure) := by
      intro k Θ' t' h'
      obtain ⟨_, _, _, _, hr, rel, _⟩ := ctx_typing _ hE₀ hv h'
      exact hnp (rel.pure rfl ▸ hr)
    cases G with
    | with_ q θ =>
      cases h with
      | @with_ _ _ _ _ ρ _ _ φ₀ _ r_out ε _ hθv hθ hb hrow hφ =>
        obtain ⟨t', R', ε', hw, h1, h2, h3⟩ := ih hE₀ hb hΘ hA₀
        have up : ∀ y, r_out y → φ.svc y := fun y h => Stage.le_svc hφ _ h
        have qle : ∀ N, ρ (.nm N) → R' (.nm N) → q ≤ o := fun N hN hy => by
          rcases (h2 N hy).2 with h' | h'
          · exact Outl.age_le (hΘ _ _ ((Stage.le_svc hrow _ h').1 hN))
          · exact Nat.le_of_lt (hA.with_inside h')
        let R'' : Row := fun y => match y with
          | .nm N => ¬ ρ (.nm N) ∧ R' (.nm N)
          | .ob N a => ¬ ρ (.nm N) ∧ R' (.ob N a)
          | .need N c => R' (.need N c) ∨ (c = .r q ∧ ρ (.nm N) ∧ R' (.nm N))
          | .atm a => R' (.atm a)
          | .rv a => R' (.rv a)
          | .obv v a => R' (.obv v a)
        refine ⟨t', R'', ε', .with_ hθv (hθ.cut hΘ) hw (Stage.le_of_svc (Stage.le_refl _) fun y hy => ?_)
          (Stage.le_refl _), fun N a hy => ?_, fun N hy => ?_, fun N c hy => ?_⟩
        · cases y with
          | nm N => exact ⟨fun h => .inr ⟨rfl, h, hy⟩, fun h => ⟨h, hy⟩⟩
          | ob N a =>
            refine ⟨fun h => ?_, fun h => ⟨h, hy⟩⟩
            rcases h1 N a hy with h' | ⟨p, rfl, hp⟩
            · exact hΘ _ _ ((Stage.le_svc hrow _ h').1 h)
            · exact Outl.age_lt (hA.with_inside hp)
          | need N c => exact .inl hy
          | atm a | rv a | obv _ a => exact hy
        · obtain ⟨hN, hy⟩ := hy
          rcases h1 N a hy with h' | ⟨p, rfl, hp⟩
          · exact .inl (up _ ((Stage.le_svc hrow _ h').2 hN))
          · exact .inr ⟨p, rfl, mem hp⟩
        · obtain ⟨hN, hy⟩ := hy
          obtain ⟨h₁, h₂⟩ := h2 N hy
          refine ⟨up _ ((Stage.le_svc hrow _ h₁).2 hN), ?_⟩
          rcases h₂ with h₂ | h₂
          · exact .inl (up _ ((Stage.le_svc hrow _ h₂).2 hN))
          · exact .inr (mem h₂)
        · rcases hy with hy | ⟨rfl, hN, hy⟩
          · obtain ⟨q', rfl, h₁, h₂⟩ := h3 N c hy
            refine ⟨q', rfl, ?_, h₂⟩
            rcases h₁ with h₁ | h₁
            · exact .inl (up _ (Stage.le_svc hrow _ h₁))
            · exact .inr (mem h₁)
          · exact ⟨q, rfl, .inl (up _ ((Stage.le_svc hrow _ (h2 N hy).1).1 hN)), qle N hN hy⟩
      | errVal hv' => exact (not_value_plug hnv _ hv').elim
    | inside p =>
      cases h with
      | inside hF hb hrow _ _ hφ _ =>
        obtain ⟨t', R', ε', hw, h1, h2, h3⟩ := ih hE₀ hb (hΘ.inside_age hF) hA₀
        have up : ∀ y, _ → φ.svc y := fun y h => Stage.le_svc hφ y h
        refine ⟨t', R', ε', hw, fun N a hy => ?_, fun N hy => ?_, fun N c hy => ?_⟩
        · rcases h1 N a hy with h' | ⟨p', rfl, hp⟩
          · rcases Stage.le_svc hrow _ h' with h₁ | h₁ | h₁ | h₁
            · exact h₁.elim
            · exact .inl (up _ h₁)
            · exact .inr ⟨p, h₁.1, by simp⟩
            · exact h₁.elim
          · exact .inr ⟨p', rfl, mem hp⟩
        · obtain ⟨h₁, h₂⟩ := h2 N hy
          refine ⟨?_, ?_⟩
          · rcases Stage.le_svc hrow _ h₁ with h' | h' | h' | h'
            · exact h'.elim
            · exact up _ h'
            · exact h'.elim
            · exact h'.elim
          · rcases h₂ with h₂ | h₂
            · rcases Stage.le_svc hrow _ h₂ with h' | h' | h' | h'
              · exact h'.elim
              · exact .inl (up _ h')
              · cases h'.1; exact .inr (by simp)
              · exact h'.elim
            · exact .inr (mem h₂)
        · obtain ⟨q, rfl, h₁, h₂⟩ := h3 N c hy
          refine ⟨q, rfl, ?_, h₂⟩
          rcases h₁ with h₁ | h₁
          · rcases Stage.le_svc hrow _ h₁ with h' | h' | h' | h'
            · exact h'.elim
            · exact .inl (up _ h')
            · exact h'.elim
            · cases h'; exact .inr (by simp)
          · exact .inr (mem h₁)
      | errVal hv' => exact (not_value_plug hnv _ hv').elim
    | _ =>
      obtain ⟨k, Θ', t₁, φ₁, h₁, rel, hk, -, -⟩ := frame_typing hnv _ hG h
      rcases hk with rfl | ⟨rfl, -, hΘ'⟩
      · exact (nopure h₁).elim
      · rw [hΘ' (fun h => h.elim)] at h₁
        simp only [List.map_up_zero] at h₁
        obtain ⟨t', R', ε', hw, h1, h2, h3⟩ := ih hE₀ h₁ hΘ hA₀
        have up : ∀ y, φ₁.svc y → φ.svc y := fun y h' =>
          (rel.svc _ h').resolve_right fun h₂ => by
            cases y <;> simp [Frame.dom, Frame.lookup] at h₂
        refine ⟨t', R', ε', hw, fun N a hy => ?_, fun N hy => ?_, fun N c hy => ?_⟩
        · rcases h1 N a hy with h' | ⟨p, rfl, hp⟩
          · exact .inl (up _ h')
          · exact .inr ⟨p, rfl, mem hp⟩
        · obtain ⟨h₁', h₂⟩ := h2 N hy
          exact ⟨up _ h₁', h₂.imp (up _) mem⟩
        · obtain ⟨q, rfl, h₁', h₂⟩ := h3 N c hy
          exact ⟨q, rfl, h₁'.imp (up _) mem, h₂⟩

/-- The work a finalizer or a child is given, awaited: its obligations are in the
stage of the registration (regions.md §2.7). -/
theorem fin_await {Θ : Facts} {c : Const} {p : Nat} {w : Tm} {T : Ty} {φ : Stage}
    (h : Typ S Ψ Θ [] (.app (.app (.const c) (.own p)) w) (.ex T φ))
    (hc : c = .addFin ∨ c = .spawn) (hΘ : Entails Θ Facts.age) (hv : Value w) :
    ∃ tx R εx, Typ S Ψ Facts.age [] (.await w) (.ex tx (.rt R εx)) ∧ KidRow p φ [] R := by
  cases h with
  | app hf hw hψ _ =>
    cases hf with
    | app hc' ho _ hT₁ =>
      cases hc' with
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
            fun N a ha => .inl (up _ (.inr (.inl (hr' _ ha)))), fun N ha => ⟨up _ (.inr (.inl (hr' _ ha))),
              .inl (up _ (.inr (.inr ⟨hr' _ ha, by rw [hs]; rfl⟩)))⟩, fun N c ha => (hok.2 N c (hr' _ ha)).elim⟩
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
            fun N a ha => ?_, fun N ha => ?_, fun N c ha => ?_⟩
          · rcases inr _ ha with ⟨_, h'⟩ | h'
            · cases h'
            · exact .inl (up _ (.inl h'))
          · rcases inr _ ha with ⟨_, h'⟩ | h'
            · cases h'
            · exact ⟨up _ (.inl h'), .inl (up _ (.inr ⟨h', by rw [hs]; rfl⟩))⟩
          · rcases inr _ ha with ⟨_, h'⟩ | h'
            · cases h'
            · exact (hok.2 N c h').elim
  | errVal hv' => exact (fin_not_value hc hv').elim

end Flick
