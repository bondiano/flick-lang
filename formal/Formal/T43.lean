import Formal.NeedOut

/-!
# T4.3: a service of a `provide` is used only while its region is live

A child and a finalizer of a live region `ϱ` are typed so that every use `need S q`
left at their top names an ancestor `q` of `ϱ` (`KidNeeds`): a child gets the uses
of the frames it inherits, and the obligation `Provided {S} ϱ` of its registration
made each such frame older than `ϱ` (`withs_typing`). An `ask_S` answered by a frame
of `q` leaves its use at the top of the thread or at the `inside` form of `q`
(`need_out`); both are live (`Nest`). Hence `T4_3`, and T1 with `op_H` without the
stuck use of a service.
-/

namespace Flick

variable {S : Sig}

/-- The children and finalizers of the live regions use services of `provide` regions
that are ancestors of their owner. -/
def KidNeeds (S : Sig) (Ψ : List Ty) (Λ : List Rgn) (anc : Nat → Nat → Prop) : Prop :=
  ∀ (r : Nat) (g : Rgn), Λ[r]? = some g → g.live = true → ∀ k ∈ g.kids ++ g.fins,
    ∃ t φ, Typ S Ψ Facts.age [] k (.ex t φ) ∧ ∀ N q, φ.svc (.need N (.r q)) → anc r q

theorem Stage.kill_need {φ : Stage} {n : Nat} {N : Name} {q : Nat}
    (h : (φ.substT (kill n)).svc (.need N (.r q))) : φ.svc (.need N (.r q)) := by
  cases φ with
  | pure => exact h.elim
  | rt r e => obtain ⟨b, hb, hk⟩ := h; obtain ⟨rfl, _⟩ := kill_reach hk; exact hb

theorem Stage.yf_need {φ : Stage} (h : φ.substT (kill 0) = φ) {N : Name} {q : Nat} :
    ¬ φ.svc (.need N (.r q)) := by
  intro hq
  rw [← h] at hq
  cases φ with
  | pure => exact hq.elim
  | rt r e => obtain ⟨b, -, hk⟩ := hq; exact Nat.not_lt_zero _ ((kill_reach hk).2 q rfl)

theorem KidNeeds.weaken {Ψ Λ anc} (h : KidNeeds S Ψ Λ anc) (X : List Ty) : KidNeeds S (Ψ ++ X) Λ anc :=
  fun r g hg hl k hk => let ⟨t, φ, ht, hn⟩ := h r g hg hl k hk; ⟨t, φ, ht.weakenStore X, hn⟩

theorem KidNeeds.keep {Ψ Λ Λ' anc anc'} (hk : KidNeeds S Ψ Λ anc)
    (hag : ∀ q, q ≠ Λ.length → anc' q = anc q)
    (h : ∀ (r : Nat) (g : Rgn), Λ'[r]? = some g → g.live = true → ∀ k ∈ g.kids ++ g.fins,
      (∃ g₀, Λ[r]? = some g₀ ∧ g₀.live = true ∧ k ∈ g₀.kids ++ g₀.fins) ∨
      ∃ t φ, Typ S Ψ Facts.age [] k (.ex t φ) ∧ ∀ N q, φ.svc (.need N (.r q)) → anc' r q) :
    KidNeeds S Ψ Λ' anc' := by
  intro r g hg hl k hk'
  rcases h r g hg hl k hk' with ⟨g₀, h₀, hl₀, hk₀⟩ | h'
  · obtain ⟨t, φ, ht, hn⟩ := hk r g₀ h₀ hl₀ k hk₀
    refine ⟨t, φ, ht, fun N q hq => ?_⟩
    rw [hag r (Nat.ne_of_lt (getElem?_lt h₀))]; exact hn N q hq
  · exact h'

theorem KidNeeds.setKid {Ψ Λ anc p j k} (h : KidNeeds S Ψ Λ anc)
    (hk : ∃ t φ, Typ S Ψ Facts.age [] k (.ex t φ) ∧ ∀ N q, φ.svc (.need N (.r q)) → anc p q) :
    KidNeeds S Ψ (setKid Λ p j k) anc := by
  intro q g hq hl k' hk'
  obtain ⟨g₀, hg₀, h₁, h₂⟩ := setKid_get hq
  by_cases hqp : q = p
  · subst hqp
    rw [h₁ rfl] at hk' hl
    rcases List.mem_append.mp hk' with hk' | hk'
    · obtain ⟨i, hi, rfl⟩ := List.getElem_of_mem hk'
      rw [List.getElem_set]
      split
      · exact hk
      · exact h q g₀ hg₀ hl _ (List.mem_append_left _ (List.getElem_mem _))
    · exact h q g₀ hg₀ hl _ (List.mem_append_right _ hk')
  · rw [h₂ hqp] at hk' hl; exact h q g₀ hg₀ hl k' hk'

/-- A step of a thread keeps the uses of the children and finalizers among the
ancestors of their owner. -/
theorem KidNeeds.step {root : Roots} {Λ : List Rgn} {μ : List Tm} {m : Tm}
    {i : Option (Nat × Nat)} {e : Tm} {l : Lbl} {Λ' : List Rgn} {μ' : List Tm} {e' : Tm}
    {Ψ : List Ty} {anc anc' B : Nat → Nat → Prop} {home : Nat → Option (Nat × Nat)} {t : Ty} {φ : Stage}
    (hn : Nest Ψ ⟨Λ, μ, m⟩ anc B home) (hi : (⟨Λ, μ, m⟩ : Cfg).at i = some e)
    (he : Typ S Ψ Facts.age [] e (.ex t φ)) (hneed : ∀ N q, φ.svc (.need N (.r q)) → baseOf anc i q)
    (hA : e.Asc 0) (hk : KidNeeds S Ψ Λ anc) (hag : ∀ q, q ≠ Λ.length → anc' q = anc q)
    (hs : Step root l Λ μ e (.cfg Λ' μ' e')) : KidNeeds S Ψ Λ' anc' := by
  have same : KidNeeds S Ψ Λ anc' := hk.keep hag fun r g hg hl k hk' => .inl ⟨g, hg, hl, hk'⟩
  have setold : ∀ {p g g'}, Λ[p]? = some g → g'.live = g.live →
      (∀ k ∈ g'.kids ++ g'.fins, k ∈ g.kids ++ g.fins ∨
        ∃ t φ, Typ S Ψ Facts.age [] k (.ex t φ) ∧ ∀ N q, φ.svc (.need N (.r q)) → anc' p q) →
      KidNeeds S Ψ (setRgn Λ p g') anc' := by
    intro p g g' hg hl hnew
    refine hk.keep hag fun r g'' hr hl'' k hk' => ?_
    rcases setRgn_get hg hr with ⟨rfl, rfl⟩ | ⟨_, hr⟩
    · rcases hnew k hk' with h | h
      · exact .inl ⟨g, hg, hl ▸ hl'', h⟩
      · exact .inr h
    · exact .inl ⟨g'', hr, hl'', hk'⟩
  have newkid : ∀ {E c p w}, OkE E → e = plug E (.app (.app (.const c) (.own p)) w) →
      (c = .addFin ∨ c = .spawn) → Value w →
      ∃ t φ, Typ S Ψ Facts.age [] (withs E (.await w)) (.ex t φ) ∧
        ∀ N q, φ.svc (.need N (.r q)) → anc' p q := by
    intro E c p w hE he' hc hv
    subst he'
    have hnv := fin_not_value (w₁ := .own p) (w₂ := w) hc
    obtain ⟨hWE, hrW⟩ := within_of_plug hnv hE (hn.thread i _ hi).1
    obtain ⟨hAE, -⟩ := asc_of_plug hnv hE hA
    obtain ⟨t', R', ε', hk', -, -, h3⟩ := withs_typing (o := p)
      (fun h' hΘ => fin_await h' hc hΘ hv) (fun h' => fin_not_pure h' hc) hnv _ hE he
      (Entails.refl _) hAE
    refine ⟨t', _, hk', fun N q hq => ?_⟩
    obtain ⟨q', hq', h₁, hle⟩ := h3 N _ hq
    simp only [Atom.r.injEq] at hq'
    subst hq'
    have hp : holeE (baseOf anc i) E p := hrW.1.2.1
    have hqH : holeE (baseOf anc i) E q := by
      rcases h₁ with h₁ | h₁
      · exact holeE_iff.mpr (.inl (hneed N q h₁))
      · exact holeE_iff.mpr (.inr h₁)
    have hc' := hn.hole_chain hi hWE p hp
    have hpL : p ≠ Λ.length := Nat.ne_of_lt (hn.hole_live hi hp).lt
    rw [hag p hpL, hc']
    exact ⟨hqH, hle⟩
  cases hs with
  | rt hE hh =>
    cases hh with
    | scope | provideEnv =>
      refine hk.keep hag fun r g hg hl k hk' => ?_
      by_cases hr : r < Λ.length
      · rw [List.getElem?_append_left hr] at hg; exact .inl ⟨g, hg, hl, hk'⟩
      · rw [List.getElem?_append_right (by omega)] at hg
        cases hj : r - Λ.length with
        | zero => simp [hj] at hg; subst hg; simp [Rgn.new] at hk'
        | succ => simp [hj] at hg
    | open_ hg =>
      refine setold hg rfl fun k hk' => ?_
      simp only [List.mem_append] at hk'
      rcases hk' with hk' | hk' | hk'
      · exact .inl (List.mem_append_left _ hk')
      · exact .inl (List.mem_append_right _ hk')
      · simp at hk'; subst hk'
        exact .inr ⟨_, .rt Row.none Row.none, .app (.const (ι := { (default : CArg) with s := .rg _ })
          trivial .refl) .cap (Stage.le_refl _) .refl, fun N q h => by simp [Stage.svc, Row.none] at h⟩
    | finRun hg _ _ _ hfs =>
      refine setold hg rfl fun k hk' => .inl ?_
      simp only [List.mem_append, hfs] at hk' ⊢
      rcases hk' with (hk' | hk') | hk'
      · exact .inl hk'
      · simp at hk'; subst hk'; simp
      · simp [hk']
    | close hg =>
      refine hk.keep hag fun r g'' hr hl'' k hk' => ?_
      rcases setRgn_get hg hr with ⟨rfl, rfl⟩ | ⟨_, hr⟩
      · cases hl''
      · exact .inl ⟨g'', hr, hl'', hk'⟩
    | _ => exact same
  | addFin hE hg _ hv =>
    refine setold hg rfl fun k hk' => ?_
    simp only [List.mem_append] at hk'
    rcases hk' with hk' | hk' | hk'
    · exact .inl (List.mem_append_left _ hk')
    · exact .inl (List.mem_append_right _ hk')
    · simp at hk'; subst hk'; exact .inr (newkid hE rfl (.inl rfl) hv)
  | spawn hE hg _ hv =>
    refine setold hg rfl fun k hk' => ?_
    simp only [List.mem_append] at hk'
    rcases hk' with (hk' | hk') | hk'
    · exact .inl (List.mem_append_left _ hk')
    · simp at hk'; subst hk'; exact .inr (newkid hE rfl (.inr rfl) hv)
    · exact .inl (List.mem_append_right _ hk')
  | _ => exact same

end Flick
