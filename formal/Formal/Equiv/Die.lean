import Formal.Equiv.Every

/-!
# Equivalence of every run: a defect answered by a defect

A thread of the left side steps to a defect at `todo` or at a `resume` it may not take
(`Step.todo`, `Step.resumeDie`). Without the pairs of O1b (`World.B`), no special pair holds
such a focus (`Act.not_todo`, `Act.simple`), so a congruence relates it once the right side has
caught up, and the right side steps to a defect there too (`die_sim`): `todo` is `todo`, and a
`resume` the right side could take is one the left side could (`FrL.resumable`). With O1b's pairs
this fails: `dead` relates a left thread that forced its own running cell with a right one that
goes on.
-/

namespace Flick.Equiv

variable {W : World}

/-- No special pair of O1a or O1c holds a `todo`. -/
theorem Act.not_todo {a b : Tm} (h : Act W a b) (hB : ¬ W.B) {E : List Frame}
    (hE : OkE E) (ha : a = plug E .todo) : False := by
  cases h with
  | dropA =>
    obtain ⟨-, h⟩ := decomp_eq (E := [.let_ _]) (lazy_redex _) todo_redex
      (OkE.nil.cons' trivial) hE ha
    cases h
  | dropB =>
    obtain ⟨-, h⟩ := decomp_eq (E := []) (let_redex .loc) todo_redex OkE.nil hE ha
    cases h
  | forceA =>
    obtain ⟨-, h⟩ := decomp_eq (E := [.force]) (lazy_redex _) todo_redex
      (OkE.nil.cons' trivial) hE ha
    cases h
  | bornA hB' | bornB hB' | bornS hB' | bornV hB' | dead hB' => exact hB hB'

theorem FrL.resumesE' {E E₂ : List Frame} (h : FrL W E E₂) {p : Nat}
    (hn : ∀ G ∈ E₂, ¬ G.resumes p) : ∀ G ∈ E, ¬ G.resumes p := by
  induction h with
  | nil => intro G hG; cases hG
  | cons hF _ ih =>
    intro G hG
    rcases List.mem_cons.mp hG with rfl | hG
    · exact fun h => hn _ (List.mem_cons_self ..) ((hF.resumes p).1 h)
    · exact ih (fun G' hG' => hn G' (List.mem_cons_of_mem _ hG')) G hG
  | junkL _ _ ih =>
    intro G hG
    rcases List.mem_cons.mp hG with rfl | hG
    · simp [Frame.resumes]
    · exact ih hn G hG
  | junkR _ _ ih => exact ih (fun G' hG' => hn G' (List.mem_cons_of_mem _ hG'))

/-- A `resume` the right side may take is one the left side may take. -/
theorem FrL.resumable {E E₂ : List Frame} (h : FrL W E E₂) (hB : W.Bij) {p ℓ ℓ' : Nat} {k : Kd}
    (hl : W.L ℓ ℓ' k) (hr : Resumable E₂ p ℓ') : Resumable E p ℓ := by
  obtain ⟨A₂, E₂', ℓh, Ef, al, r, c, rfl, hA⟩ := hr
  obtain ⟨A, F, B, rfl, hA', hF, -⟩ := h.split_right (fun n h => by cases h)
  cases hF with
  | hctlA _ hk =>
    rcases hk with ⟨h, -⟩ | ⟨-, k', hκ⟩
    · cases h
    · obtain rfl := (hB _ _ _ _ _ _ hκ hl).2 rfl
      exact ⟨A, B, _, _, _, _, _, rfl, hA'.resumesE' hA⟩

/-- **A thread that steps to a defect** on the left is answered by a right one that does, after
it caught up, when the pairs of O1b are absent. -/
theorem die_sim {root : Roots} {l : Lbl} {Λ Λ₂ : List Rgn} {μ μ₂ : List Tm}
    {τ : Option (Nat × Nat)} {t t₂ : Tm} (ht : ThR W μ τ t t₂) (hst : StR W μ μ₂) (hB : ¬ W.B)
    (hs : Step root l Λ μ t .die) :
    ∃ W' μ₂' t₂' l', RJ W μ₂ t₂ W' μ₂' t₂' ∧ Step root l' Λ₂ μ₂' t₂' .die ∧ Sim W' .any t t₂' := by
  rcases ht with ht | ⟨E, E', n, e, e', rfl, rfl, hE, -, -, -, -, -⟩
  · cases hs with
    | todo hE =>
      rcases ht.dec todo_redex.1 (fun _ _ h => by cases h) (fun _ _ _ h => by cases h)
          (fun h => h) _ _ rfl hE μ₂ with
        ⟨W', μ', E₀, E₁, a, b, E₂, hj, rfl, rfl, -, -, hact⟩ | ⟨W', μ', E₂, r₂, hj, hfr, hE₂, hs⟩ |
        ⟨_, _, _, _, _, _, _, _, _, h', _, _⟩
      · exact (hact.not_todo (fun h => hB (hj.le.B.2 h))
          (fun F hF => hE F (List.mem_append_left _ hF)) rfl).elim
      · have hr := hfr.plugs hs.lower
        rw [hs.inv_todo] at hj hr
        exact ⟨_, _, _, _, hj, .todo hE₂, hr⟩
      · cases h'
    | resumeDie hE hu hnr =>
      rcases ht.dec (Flick.Elab.kont_redex hu).1
          (fun _ _ h => Redex.app (Flick.Elab.kont_redex hu) h) (fun _ _ _ h => by cases h)
          (fun h => h) _ _ rfl hE μ₂ with
        ⟨W', μ', E₀, E₁, a, b, E₂, hj, rfl, rfl, -, -, hact⟩ | ⟨W', μ', E₂, r₂, hj, hfr, hE₂, hs⟩ |
        ⟨_, _, _, _, _, _, _, _, _, h', _, _⟩
      · have := hact.simple (hj.st _ hst) kont_not_value rfl
        simp [Tm.Simple] at this
      · have hrel := hfr.plugs hs.lower
        obtain ⟨f₂, u₂, rfl, h₁, h₂⟩ := hs.inv_app
        obtain ⟨ℓ', k, rfl, hl⟩ := h₁.inv_kont
        exact ⟨_, _, _, _, hj,
          .resumeDie hE₂ (h₂.top_value rfl hu) fun hr => hnr (hfr.resumable (hj.st _ hst).bij hl hr),
          hrel⟩
      · cases h'
  · exfalso
    generalize ht' : plug E (.force (.loc n)) = t at hs
    cases hs with
    | todo hE' =>
      obtain ⟨-, h⟩ := decomp_eq todo_redex (force_redex n) hE' hE ht'.symm
      cases h
    | resumeDie hE' hu =>
      obtain ⟨-, h⟩ := decomp_eq (Flick.Elab.kont_redex hu) (force_redex n) hE' hE ht'.symm
      cases h

/-- **A main thread that steps to a defect** on the left is answered by a right one. -/
theorem dies_sim {root : Roots} {c₁ c₂ : Cfg} (hr : CfgR W c₁ c₂) (hB : ¬ W.B)
    (hd : MainDies root c₁) : ∃ c₂', CSteps root c₂ c₂' ∧ MainDies root c₂' := by
  obtain ⟨l, hd⟩ := hd
  obtain ⟨W', μ₂', t₂', l', hk, hd', -⟩ := die_sim (Λ₂ := c₂.rgs) hr.1 hr.2.1 hB hd
  exact ⟨⟨c₂.rgs, μ₂', t₂'⟩, hk.ksteps.main c₂.rgs, l', hd'⟩

end Flick.Equiv
