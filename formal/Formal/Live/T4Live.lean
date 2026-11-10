import Formal.Live.LSafe
import Formal.Live.LNestMain
import Formal.Live

/-!
# T4′ of the live discipline

The invariant of a reachable configuration is the typing of the configuration, the stages of
the children and finalizers of each region `p` held to ancestors of `p` (`LCfgTy`), and the
run-time forms of the configuration (`LNest`). The two are kept together: the typing supplies
the facts about names a step of the forms needs (the owner given work is a region of the hole,
the row of a new handler's arms names regions of its place), and the forms supply the place
at which a step registers work. With them no thread uses a region that is not live and every
operation is answered by a handler of a live region: `T4'_live`.
-/

namespace Flick

variable {S : Sig} {Ψ : List Ty} {anc : Nat → Nat → Prop}

theorem Step.fresh {root l Λ μ e Λ' μ' e'} (hs : Step root l Λ μ e (.cfg Λ' μ' e')) :
    ∀ g, Λ'[Λ.length]? = some g → g.kids = [] ∧ g.fins = [] := by
  intro g hg
  cases hs with
  | rt _ hh =>
    cases hh <;> simp_all [setRgn, Rgn.new]
    all_goals (subst hg; exact ⟨rfl, rfl⟩)
  | _ => simp_all [setRgn]

/-- The region a step may create holds no work yet. -/
theorem CStep.fresh {root l c c'} (hs : CStep root l c c') :
    ∀ g, c'.rgs[c.rgs.length]? = some g → g.kids = [] ∧ g.fins = [] := by
  intro g hg
  cases hs with
  | main hs => exact hs.fresh g hg
  | kid _ _ _ hs =>
    obtain ⟨g₀, hg₀, h₁, h₂⟩ := setKid_get hg
    obtain ⟨hk, hf⟩ := hs.fresh g₀ hg₀
    by_cases hgg : g = g₀
    · subst hgg; exact ⟨hk, hf⟩
    · have hq := Classical.byContradiction fun hq => hgg (h₂ hq)
      rw [h₁ hq]; simp [hk, hf]

theorem LRgs.reanc {Ψ anc anc' Λ n} (h : LRgs S Ψ anc Λ)
    (hn : ∀ g, Λ[n]? = some g → g.kids = [] ∧ g.fins = []) (ha : ∀ q, q ≠ n → anc' q = anc q) :
    LRgs S Ψ anc' Λ := fun p g hg => by
  by_cases hp : p = n
  · subst hp; obtain ⟨hk, hf⟩ := hn g hg; simp [hk, hf]
  · rw [ha p hp]; exact h p g hg

/-- Every thread of a typed configuration runs at a stage of the regions of its base. -/
theorem LCfgTy.thread {c t φ i e} (hc : LCfgTy S Ψ c t φ anc) (hφ : φ.substT (kill 0) = φ)
    (hi : c.at i = some e) :
    ∃ t' ψ, TypM S .live Ψ Facts.age [] e (.ex t' ψ) ∧ ∀ x, ψ.svc (.atm (.r x)) → baseOf anc i x := by
  rcases i with _ | ⟨p, j⟩
  · simp only [Cfg.at, Option.some.injEq] at hi; subst hi
    exact ⟨t, φ, hc.1.2.1, fun x hx => (Stage.no_atm_zero hφ x hx).elim⟩
  · obtain ⟨g, hg, -, hj⟩ := Cfg.at_some.mp hi
    exact (hc.2 p g hg).1 e (List.mem_of_getElem? hj)

/-- The owner a thread gives work to is a region of its hole. -/
theorem LNest.own_hole {c home i e t'} {ψ : Stage} (hn : LNest Ψ c anc home) (hi : c.at i = some e)
    (h : TypM S .live Ψ Facts.age [] e (.ex t' ψ)) (hψ : ∀ x, ψ.svc (.atm (.r x)) → baseOf anc i x) :
    ∀ E c' p w, OkE E → e = plug E (.app (.app (.const c') (.own p)) w) →
      (c' = .addFin ∨ c' = .spawn) → holeE (baseOf anc i) E p := by
  intro E c' p w hE he hc
  subst he
  obtain ⟨K, Θ', t₁, ψ₁, hr, -, hx⟩ := hole_atm hE (fin_not_value hc) h
  have hP := hn.place hi hE hψ
  exact (hx p (fin_atm hc hr)).elim (hP.2.1 p) fun ⟨F, hF, hd⟩ => hP.2.2.1 F hF p hd

/-- The arms of a handler just installed name regions of its place or its own region. -/
theorem LNest.new_cell {c home i E Ef al rf e₀ n ℓ t' Ψ'} {ψ : Stage} (hn : LNest Ψ c anc home)
    (hi : c.at i = some (plug E (.hdl Ef al rf e₀))) (hE : OkE E)
    (h : TypM S .live Ψ' Facts.age [] (plug E (.inside n (.hwith n ℓ Ef al rf e₀))) (.ex t' ψ))
    (hψ : ∀ x, ψ.svc (.atm (.r x)) → baseOf anc i x) :
    ∃ r ε t, Ψ'[ℓ]? = some (.arrow (.base .unit) (.rt r ε) t Caps.none) ∧
      ∀ x, r (.atm (.r x)) → holeE (baseOf anc i) E x ∨ x = n := by
  obtain ⟨r, ε, t, hℓ, hx⟩ := hwith_cell hE h
  refine ⟨r, ε, t, hℓ, fun x hr => ?_⟩
  rcases hx x hr with h₁ | h₁ | ⟨F, hF, h₁ | h₁⟩
  · exact .inl (holeE_iff.mpr (.inl (hψ x h₁)))
  · exact .inr h₁
  · subst h₁; exact .inl (holeE_iff.mpr (.inr hF))
  · exact .inl (hn.frame_reg hi hE F hF (Frame.rgn_reg h₁))

theorem LInv.main {root Ψ Λ μ e Λ' μ' e' t φ l anc home} (hroot : RootTyM S .live root)
    (ht : t.YF 0) (hφ : φ.substT (kill 0) = φ)
    (hc : LCfgTy S Ψ ⟨Λ, μ, e⟩ t φ anc) (hn : LNest Ψ ⟨Λ, μ, e⟩ anc home)
    (hs : Step root l Λ μ e (.cfg Λ' μ' e')) :
    ∃ X anc' home', LCfgTy S (Ψ ++ X) ⟨Λ', μ', e'⟩ t φ anc' ∧
      LNest (Ψ ++ X) ⟨Λ', μ', e'⟩ anc' home' := by
  have hi : (⟨Λ, μ, e⟩ : Cfg).at none = some e := rfl
  have hψ : ∀ x, φ.svc (.atm (.r x)) → baseOf anc none x :=
    fun x hx => (Stage.no_atm_zero hφ x hx).elim
  obtain ⟨X, hc', -⟩ := hc.main hroot ht hφ hn.self
    (fun E r hE he _ => ⟨_, hn.place (r := r) (by rw [← he]; exact hi) hE hψ⟩) hs
  obtain ⟨hμ, h, -, -, -, -, _hs, _cell, hcg⟩ := hc.1
  obtain ⟨anc', home', hn', hag⟩ := hn.step (X := X) hroot hi hμ hc'.1.1 hcg.main
    (hn.own_hole hi h hψ)
    (fun E _ _ _ _ hE he he' => hn.new_cell (he ▸ hi) hE (he' ▸ hc'.1.2.1) hψ) hs
  exact ⟨X, anc', home', ⟨hc'.1, hc'.2.reanc hs.fresh hag⟩, hn'⟩

theorem LInv.kid {root Ψ Λ μ e p g j k Λ' μ' k' t φ l anc home} (hroot : RootTyM S .live root)
    (hc : LCfgTy S Ψ ⟨Λ, μ, e⟩ t φ anc) (hn : LNest Ψ ⟨Λ, μ, e⟩ anc home)
    (hg : Λ[p]? = some g) (hl : g.live = true) (hj : g.kids[j]? = some k)
    (hs : Step root l Λ μ k (.cfg Λ' μ' k')) :
    ∃ X anc' home', LCfgTy S (Ψ ++ X) ⟨setKid Λ' p j k', μ', e⟩ t φ anc' ∧
      LNest (Ψ ++ X) ⟨setKid Λ' p j k', μ', e⟩ anc' home' := by
  have hi : (⟨Λ, μ, e⟩ : Cfg).at (some (p, j)) = some k := Cfg.at_some.mpr ⟨g, hg, hl, hj⟩
  obtain ⟨X, hc', -, t₁, ψ₁, hk₁, hψ₁⟩ := hc.kid hroot hn.self hg hj
    (fun _ _ _ hψ E r hE he _ =>
      ⟨_, hn.place (i := some (p, j)) (r := r) (by rw [← he]; exact hi) hE hψ⟩) hs
  obtain ⟨t₀, ψ₀, hk, hψ₀⟩ := (hc.2 p g hg).1 k (List.mem_of_getElem? hj)
  obtain ⟨hμ, -, -, -, -, -, _hs, _cell, hcg⟩ := hc.1
  obtain ⟨m, -, hkc⟩ := hcg.rgs _ _ hg _ (List.mem_append_left _ (List.mem_of_getElem? hj))
  obtain ⟨anc', home', hn', hag⟩ := hn.step (X := X) hroot hi hμ hc'.1.1 hkc
    (hn.own_hole (i := some (p, j)) hi hk hψ₀)
    (fun E _ _ _ _ hE he he' => hn.new_cell (i := some (p, j)) (he ▸ hi) hE (he' ▸ hk₁) hψ₁) hs
  exact ⟨X, anc', home', ⟨hc'.1, hc'.2.reanc (CStep.kid (e := e) hg hl hj hs).fresh hag⟩, hn'⟩

theorem LInv.steps {root c c'} (hroot : RootTyM S .live root)
    (hs : CSteps root c c') :
    ∀ {Ψ t φ anc home}, t.YF 0 → φ.substT (kill 0) = φ → LCfgTy S Ψ c t φ anc →
      LNest Ψ c anc home → ∃ Ψ' anc' home', LCfgTy S Ψ' c' t φ anc' ∧ LNest Ψ' c' anc' home' := by
  induction hs with
  | refl => intro Ψ _ _ anc home _ _ hc hn; exact ⟨Ψ, anc, home, hc, hn⟩
  | step h₁ _ ih =>
    intro Ψ t φ anc home ht hφ hc hn
    cases h₁ with
    | main hs =>
      obtain ⟨_, _, _, hc₁, hn₁⟩ := LInv.main hroot ht hφ hc hn hs
      exact ih ht hφ hc₁ hn₁
    | kid hg hl hj hs =>
      obtain ⟨_, _, _, hc₁, hn₁⟩ := LInv.kid hroot hc hn hg hl hj hs
      exact ih ht hφ hc₁ hn₁

/-- **T4′ of the live discipline.** A program typed by use reaches no configuration where a
thread is at a step that needs a region that is not live, or where an operation is answered by
a handler of a region that ended. -/
theorem T4'_live {root : Roots} (hroot : RootTyM S .live root) :
    SafeUse root (fun e t φ => TypM S .live [] Facts.none [] e (.ex t φ)) := by
  intro e₀ t φ₀ c h₀ hp ht hφ hs
  obtain ⟨Ψ, anc, home, hc, hn⟩ := LInv.steps hroot hs ht hφ
    (LCfgTy.init h₀ hp (fun q x => x = q)) (LNest.init hp)
  refine ⟨fun E r hE he => ?_, fun p g hg hl k hk E r hE he => ?_,
    fun i e E Ef o k v q hi hE he hq => ?_⟩
  · have hi : c.at none = some (plug E r) := by rw [← he]; rfl
    obtain ⟨_, _, h, hψ⟩ := hc.thread hφ hi
    exact hn.not_dead hi hE h hψ
  · obtain ⟨j, hj⟩ := List.mem_iff_getElem?.mp hk
    have hi : c.at (some (p, j)) = some (plug E r) := Cfg.at_some.mpr ⟨g, hg, hl, he ▸ hj⟩
    obtain ⟨_, _, h, hψ⟩ := hc.thread hφ hi
    exact hn.not_dead hi hE h hψ
  · subst he; exact hn.handler_live hi hE hq

end Flick
