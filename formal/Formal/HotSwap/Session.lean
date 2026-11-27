import Formal.HotSwap.Migrate

/-!
# Live sessions and unaffected progress under hot swap (Theorem 3)
-/

namespace Flick.HotSwap

/-! ## 5. Live sessions and Progress unaffected (Theorem 3) -/

/-- State of a live session: current signature `sig`, current global/root table
`root`, and current running configuration `cfg`. -/
structure SessionState where
  sig : Sig
  root : Roots
  cfg : Cfg

/-- Well-typed invariant of a live session at store typing `Ψ`, program type
`t`, and stage `φ₀`: roots are typed, the configuration is typed at `Ψ`, and
the main thread holds no kept handler. -/
structure SessionOk (t : Ty) (φ₀ : Stage) (s : SessionState) (Ψ : List Ty) : Prop where
  root : RootTy s.sig s.root
  cfg : CfgTy s.sig Ψ s.cfg t φ₀
  noKept : ¬ s.cfg.main.Has .kept

/-- Initial live session from a well-typed source program `e₀ : t ! φ₀`. -/
theorem SessionOk.init {S₀ : Sig} {root₀ : Roots} {e₀ t φ₀}
    (hroot : RootTy S₀ root₀) (h₀ : Typ S₀ [] Facts.none [] e₀ (.ex t φ₀))
    (hp : e₀.Below 0) :
    SessionOk t φ₀ ⟨S₀, root₀, ⟨[], [], e₀⟩⟩ [] where
  root := hroot
  cfg := CfgTy.init h₀ hp
  noKept := fun h => let ⟨p, hp'⟩ := (Tm.kept_ins h).2; Nat.not_lt_zero _ (hp p hp')

/-- Normal execution steps preserve `SessionOk` and monotonically extend `Ψ`. -/
theorem SessionOk.exec {t φ₀ s Ψ l c'}
    (ht : t.YF 0) (hφ : φ₀.substT (kill 0) = φ₀)
    (hok : SessionOk t φ₀ s Ψ) (hcs : CStep s.root l s.cfg c') :
    ∃ X, SessionOk t φ₀ ⟨s.sig, s.root, c'⟩ (Ψ ++ X) := by
  cases s with
  | mk sig root cfg =>
    obtain ⟨X, hc', _⟩ := cstep_typed hok.root ht hφ hok.cfg hcs
    refine ⟨X, hok.root, hc', ?_⟩
    cases hcs with
    | main hs => exact fun h => hok.noKept (hs.kept hok.root.below hok.cfg.2.2.2.2.2.1.mem h)
    | kid => exact hok.noKept

/-- Multi-step execution preserves `SessionOk` and monotonically extends `Ψ`. -/
theorem SessionOk.execs {t φ₀ s Ψ c'}
    (ht : t.YF 0) (hφ : φ₀.substT (kill 0) = φ₀)
    (hok : SessionOk t φ₀ s Ψ) (hcs : CSteps s.root s.cfg c') :
    ∃ X, SessionOk t φ₀ ⟨s.sig, s.root, c'⟩ (Ψ ++ X) := by
  cases s with
  | mk sig root cfg =>
    dsimp only at hok hcs ⊢
    induction hcs generalizing Ψ with
    | refl => exact ⟨[], by simpa using hok⟩
    | step h₁ _ ih =>
      obtain ⟨X₁, hok₁⟩ := hok.exec ht hφ h₁
      obtain ⟨X₂, hok₂⟩ := ih hok₁
      exact ⟨X₁ ++ X₂, by simpa [List.append_assoc] using hok₂⟩

/-- A compatible swap preserves `SessionOk` at the exact same store typing `Ψ`. -/
theorem SessionOk.swap {t φ₀ s Ψ S' root'}
    (hok : SessionOk t φ₀ s Ψ) (hcompat : SigSwapCompat s.sig S')
    (hroot' : RootTy S' root') :
    SessionOk t φ₀ ⟨S', root', s.cfg⟩ Ψ where
  root := hroot'
  cfg := cfgTy_of_swapCompat hcompat hok.cfg
  noKept := hok.noKept

/-- A well-typed atomic migration (commit or rollback) preserves `SessionOk`,
monotonically extending `Ψ` on commit and leaving `s.cfg` and `Ψ` completely
unchanged on rollback. -/
theorem SessionOk.migrate {t φ₀ s Ψ ms out c'}
    (hok : SessionOk t φ₀ s Ψ)
    (hms : ∀ m ∈ ms, CellMigrationOk s.sig Ψ m)
    (hstep : MigrateStep s.root ms s.cfg out c') :
    ∃ X, SessionOk t φ₀ ⟨s.sig, s.root, c'⟩ (Ψ ++ X) ∧
      (out = .rolledBack → c' = s.cfg ∧ X = []) := by
  cases out with
  | committed =>
    obtain ⟨hc', _, _, _⟩ := migration_preserves_cfgTy hok.root hok.cfg hms hstep
    cases hstep
    exact ⟨ms.map (·.newTy), ⟨hok.root, hc', hok.noKept⟩, fun h => by cases h⟩
  | rolledBack =>
    obtain ⟨hEq, hc'⟩ := migration_rollback_atomic hok.cfg hstep
    subst hEq
    exact ⟨[], ⟨hok.root, by simpa using hc', hok.noKept⟩, fun _ => ⟨rfl, rfl⟩⟩

/-- **Theorem 3a (Old frames remain well typed against the extended store typing).**
Any old frame or closed/open term `e` well typed under `(S, Ψ)` remains well
typed under `(S', Ψ ++ X)` after any compatible swap (`SigSwapCompat S S'`) and
any store-extending migration appending `X` (holding both old cell versions in
`Ψ` and new migrated cell versions in `X`). -/
theorem old_frame_well_typed_under_extended_store {S S' : Sig} {Ψ X : List Ty} {Θ Γ e J}
    (hcompat : SigSwapCompat S S') (h : Typ S Ψ Θ Γ e J) :
    Typ S' (Ψ ++ X) Θ Γ e J ∧ ∀ (i : Nat), i < Ψ.length → (Ψ ++ X)[i]? = Ψ[i]? :=
  ⟨(typ_of_swapCompat hcompat h).weakenStore X, fun _ hi => List.getElem?_append_left hi⟩

/-- **Theorem 3b (Progress is unaffected by hot swaps and migrations).**
For any live session state `s` satisfying `SessionOk t φ₀ s Ψ` (whether reached
through execution, compatible swaps, committed migrations extending `Ψ` with
both versions, or rolled-back migrations), if root services cover `φ₀`:
- the main thread is a value (typed at `t` in `Ψ`), raises an error within
  `φ₀.err`, makes a `Step` under `s.root`, or is `Blocked` (waiting on a child
  or closed region) — never stuck;
- every child of a live region is `Final`, makes a `Step` under `s.root`, is
  `PerformStuck`, or is `Blocked` — never stuck. -/
theorem hotSwap_progress_unaffected {t φ₀ s Ψ}
    (hok : SessionOk t φ₀ s Ψ)
    (hcov : ∀ N c, φ₀.svc (.nm N c) → (s.root.svc N).isSome) :
    ((Value s.cfg.main ∧ StoreTy s.sig Ψ s.cfg.mem ∧
        Typ s.sig Ψ Facts.age [] s.cfg.main (.ex t .pure)) ∨
      (∃ v, s.cfg.main = .raise v ∧ ∃ ε, Row.Sub ε.asRow φ₀.err ∧
        Typ s.sig Ψ Facts.age [] v (.ex (.errors ε) .pure)) ∨
      (∃ l o, Step s.root l s.cfg.rgs s.cfg.mem s.cfg.main o) ∨
      Blocked s.cfg.rgs s.cfg.main) ∧
    ∀ (p : Nat) (g : Rgn), s.cfg.rgs[p]? = some g → g.live = true → ∀ k ∈ g.kids,
      Final k ∨ (∃ l o, Step s.root l s.cfg.rgs s.cfg.mem k o) ∨
        (∃ D, PerformStuck s.sig s.root k D) ∨ Blocked s.cfg.rgs k := by
  obtain ⟨hμ, h, hΛ, -, -, -, _, _, hcg⟩ := hok.cfg
  refine ⟨?_, fun p g hg _ k hk => ?_⟩
  · rcases progress (Λ := s.cfg.rgs) (root := s.root) hok.root h hμ.1 rfl rfl with hv | hn
    · exact .inl ⟨hv, hμ, h.valueAny hv _⟩
    cases hn with
    | raise h' =>
      obtain ⟨v, hv, he⟩ := h'
      obtain ⟨ε, hε, hv'⟩ := raise_errors hv (he ▸ h)
      exact .inr (.inl ⟨v, he, ε, hε, hv'⟩)
    | step hstep => exact .inr (.inr (.inl hstep))
    | perform h' =>
      rcases h' with ⟨_, Ef, o, c', _, a, b, _, _, _, hop, _, hna, _, hD⟩ |
        ⟨E, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, _, he⟩
      · obtain ⟨w, hw⟩ := Option.isSome_iff_exists.mp (hcov Ef _ hD)
        exact (hna (root_answers hok.root hw hop)).elim
      · exact (hok.noKept (he ▸ plug_has E (by simp [Tm.Has]))).elim
    | blocked hb => exact .inr (.inr (.inr hb))
    | unw h' =>
      obtain ⟨_, _, _, he⟩ := h'
      exact (Nat.not_lt_zero _ (he ▸ hcg.main).1).elim
  · obtain ⟨t', φ', hk'⟩ := (hΛ p g hg).1 k hk
    rcases progress (Λ := s.cfg.rgs) (root := s.root) hok.root hk' hμ.1 rfl rfl with hv | hn
    · exact .inl (.inl hv)
    cases hn with
    | raise h' => exact .inl (.inr (.inl h'))
    | step hstep => exact .inr (.inl hstep)
    | perform hp => exact .inr (.inr (.inl ⟨_, hp⟩))
    | blocked hb => exact .inr (.inr (.inr hb))
    | unw h' => exact .inl (.inr (.inr h'))


end Flick.HotSwap
