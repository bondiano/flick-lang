import Formal.RT.Agree

/-! Preservation of the invariants about frames: who holds an owner. -/

-- The step tactics share one `simp` set across ~50 rules; not every rule needs every lemma.
set_option linter.unusedSimpArgs false

namespace Flick.RT

variable {T V : Type} {K : Core T V} {L : Lib V} {C C' : Conf T V}

set_option maxHeartbeats 1000000 in
theorem step_frames [DecidableEq V] (h : Hyp L) (I : Inv C) (hs : Step K L C C') :
    ∀ j φ, C'.Φ j = some (.run φ) → ∀ fr ∈ φ.stack, FrOk C' j fr := by
  have hfr0 := I.frames
  cases hs
  case fiber f φ φ' hf hl =>
    refine frames_of (C := C) (agree_same (C := C) rfl rfl) fun j ψ h' fr hfr => ?_
    fiber_cases h'
    · rcases hl.frames fr hfr with hm | ⟨h1, h2⟩
      · exact .inr ⟨_, hf, hm, fun _ _ h => h, hfr0 _ _ hf _ hm⟩
      · exact .inl (FrOk.triv h1 h2)
    · exact .inr ⟨_, h', hfr, fun _ _ h => h, hfr0 _ _ h' _ hfr⟩
  case done =>
    refine frames_of (C := C) (agree_same (C := C) rfl rfl) ?_
    shape_tac
  case make =>
    refine frames_of (C := C) (agree_same (C := C) rfl rfl) ?_
    shape_tac
  case modify =>
    refine frames_of (C := C) (agree_same (C := C) rfl rfl) ?_
    shape_tac
  case modWrite =>
    refine frames_of (C := C) (agree_same (C := C) rfl rfl) ?_
    shape_tac
  case spawnClosed =>
    refine frames_of (C := C) (agree_same (C := C) rfl rfl) ?_
    shape_tac
  case addNow =>
    refine frames_of (C := C) (agree_same (C := C) rfl rfl) ?_
    shape_tac
  case addDrop =>
    refine frames_of (C := C) (agree_same (C := C) rfl rfl) ?_
    shape_tac
  case acqNow =>
    refine frames_of (C := C) (agree_same (C := C) rfl rfl) ?_
    shape_tac
  case acqDrop =>
    refine frames_of (C := C) (agree_same (C := C) rfl rfl) ?_
    shape_tac
  case acqLost =>
    refine frames_of (C := C) (agree_same (C := C) rfl rfl) ?_
    shape_tac
  case interrupt =>
    refine frames_of (C := C) (agree_same (C := C) rfl rfl) ?_
    shape_tac
  case req =>
    refine frames_of (C := C) (agree_same (C := C) rfl rfl) ?_
    shape_tac
  case extU =>
    refine frames_of (C := C) (agree_same (C := C) rfl rfl) ?_
    shape_tac
  case grunClosed =>
    refine frames_of (C := C) (agree_same (C := C) rfl rfl) ?_
    shape_tac
  case use =>
    refine frames_of (C := C) (agree_same (C := C) rfl rfl) ?_
    shape_tac
  case groupClosed =>
    refine frames_of (C := C) (agree_same (C := C) rfl rfl) ?_
    shape_tac
  case grun =>
    refine frames_of (C := C) (agree_same (C := C) rfl rfl) ?_
    shape_tac
  case parStart =>
    refine frames_of (C := C) (agree_same (C := C) rfl rfl) ?_
    shape_tac
  case spawn =>
    refine frames_of (C := C) (agree_upd (C := C) ‹C.Λ _ = some _› (by simp) (by rfl) rfl (M_same rfl)) ?_
    shape_tac
  case addFin =>
    refine frames_of (C := C) (agree_upd (C := C) ‹C.Λ _ = some _› (by simp) (by rfl) rfl (M_same rfl)) ?_
    shape_tac
  case acqLive =>
    refine frames_of (C := C) (agree_upd (C := C) ‹C.Λ _ = some _› (by simp) (by rfl) rfl (M_same rfl)) ?_
    shape_tac
  case hookCode =>
    refine frames_of (C := C) (agree_upd (C := C) ‹C.Λ _ = some _› (by simp) (by rfl) rfl (M_same rfl)) ?_
    shape_tac
  case hookStop =>
    refine frames_of (C := C) (agree_upd (C := C) ‹C.Λ _ = some _› (by simp) (by rfl) rfl (M_same rfl)) ?_
    shape_tac
  case stopDone =>
    refine frames_of (C := C) (agree_upd (C := C) ‹C.Λ _ = some _› (by simp) (by rfl) rfl (M_same rfl)) ?_
    shape_tac
  case kids =>
    refine frames_of (C := C) (agree_upd (C := C) ‹C.Λ _ = some _› (by simp) (by rfl) rfl (M_same rfl)) ?_
    shape_tac
  case kidsDone =>
    refine frames_of (C := C) (agree_upd (C := C) ‹C.Λ _ = some _› (by simp) (by rfl) rfl (M_same rfl)) ?_
    shape_tac
  case finCode =>
    refine frames_of (C := C) (agree_upd (C := C) ‹C.Λ _ = some _› (by simp) (by rfl) rfl (M_same rfl)) ?_
    shape_tac
  case absorbH =>
    refine frames_of (C := C) (agree_upd (C := C) ‹C.Λ _ = some _› (by simp) (by rfl) rfl (M_same rfl)) ?_
    shape_tac
  case absorbF =>
    refine frames_of (C := C) (agree_upd (C := C) ‹C.Λ _ = some _› (by simp) (by rfl) rfl (M_same rfl)) ?_
    shape_tac
  case ext =>
    refine frames_of (C := C) (agree_upd (C := C) ‹C.Λ _ = some _› (by simp) (by rfl) rfl (M_same rfl)) ?_
    shape_tac
  case group =>
    refine frames_of (C := C) (agree_upd (C := C) ‹C.Λ _ = some _› (by simp) (by rfl) rfl (M_same rfl)) ?_
    shape_tac
  case doneW =>
    refine frames_of (C := C) (agree_updO (C := C) (fun _ => by rfl) (fun _ => by rfl) rfl rfl) ?_
    shape_tac
  case doneWF =>
    refine frames_of (C := C) (agree_updO (C := C) (fun _ => by rfl) (fun _ => by rfl) rfl rfl) ?_
    shape_tac
  case settle =>
    refine frames_of (C := C) (agree_updO (C := C) (fun _ => by rfl) (fun _ => by rfl) rfl rfl) ?_
    shape_tac
  case watchOk =>
    refine frames_of (C := C) (agree_updO (C := C) (fun _ => by rfl) (fun _ => by rfl) rfl rfl) ?_
    shape_tac
  case watchFail =>
    refine frames_of (C := C) (agree_updO (C := C) (fun _ => by rfl) (fun _ => by rfl) rfl rfl) ?_
    shape_tac
  case fins =>
    refine frames_of (C := C) (agree_upd (C := C) ‹C.Λ _ = some _›
      (by simp [‹_ = St.closing›]) (by rfl) rfl (M_same rfl)) ?_
    shape_tac
  case closed =>
    refine frames_of (C := C) (agree_upd (C := C) ‹C.Λ _ = some _›
      (by simp [‹_ = St.finalizing›]) (by rfl) rfl (M_same rfl)) ?_
    shape_tac
  case provHit f φ πp m Ly c Ls b m' e o mm hf hc hmemo hlk hb ho hst hm =>
    obtain ⟨mm₀, hm₀, he₀⟩ := lookup_lt hlk
    refine frames_of (C := C) (agree_upd (C := C) ho (by simp) (by rfl) rfl (M_setEnt rfl ?_)) ?_
    · intro mm' σ hm' he'; rw [hm₀] at hm'; cases hm'; rw [he₀] at he'; cases he'; cases hb
    · shape_tac
  case finObs f φ π s o i m Ly fs mm e hf hc hs ho hph hfs hm he hobs hst =>
    refine frames_of (C := C) (agree_upd (C := C) ho (by simp) (by rfl) rfl (M_setEnt rfl ?_)) ?_
    · intro mm' σ hm' he'; rw [hm] at hm'; cases hm'; rw [he] at he'; cases he'; rfl
    · shape_tac
  case scope f φ e e' b hf hc hst =>
    obtain ⟨-, hΛn, -, -, -⟩ := I.fresh C.n (Nat.le_refl _)
    have hn0 : C.n ≠ 0 := fun h0 => by
      obtain ⟨o, ho, _⟩ := I.root; rw [← h0, hΛn] at ho; cases ho
    refine frames_of (C := C) (agree_ext (fun π o h => ?_) (M_same rfl)) fun j ψ h' fr hfr => ?_
    · have : π ≠ C.n := fun e => by subst e; rw [hΛn] at h; cases h
      simp [upd, this, h]
    · fiber_cases h'
      · simp only [List.mem_cons] at hfr
        rcases hfr with rfl | rfl | hm
        · exact .inl ⟨fun π env he => (by
            cases he; exact ⟨hn0, Owner.fresh (.ok K.unit) _, (by simp [upd]), rfl, rfl⟩),
            fun _ _ _ _ _ _ he => (by cases he), fun _ he => (by cases he)⟩
        · exact .inl (FrOk.triv rfl fun _ h => by cases h)
        · exact .inr ⟨_, hf, hm, fun _ _ h => h, hfr0 _ _ hf _ hm⟩
      · exact .inr ⟨_, h', hfr, fun _ _ h => h, hfr0 _ _ h' _ hfr⟩
  case provide f φ e e' Ls b hf hc hst =>
    obtain ⟨-, hΛn, -, -, hMn⟩ := I.fresh C.n (Nat.le_refl _)
    obtain ⟨-, -, -, -, hMn'⟩ := I.fresh (C.n + 1) (by omega)
    have hn0 : C.n ≠ 0 := fun h0 => by
      obtain ⟨o, ho, _⟩ := I.root; rw [← h0, hΛn] at ho; cases ho
    refine frames_of (C := C) (agree_ext (fun π o h => ?_) (fun m mm Ly σ hm he => ?_))
      fun j ψ h' fr hfr => ?_
    · have : π ≠ C.n := fun e => by subst e; rw [hΛn] at h; cases h
      simp [upd, this, h]
    · have : m ≠ C.n + 1 := fun e => by subst e; rw [hMn'] at hm; cases hm
      exact ⟨mm, by simp [upd, this, hm], he⟩
    · fiber_cases h'
      · simp only [List.mem_cons] at hfr
        rcases hfr with rfl | rfl | hm
        · exact .inl ⟨fun π env he => (by
            cases he; exact ⟨hn0, Owner.fresh (.ok K.unit) _, (by simp [upd]), rfl, rfl⟩),
            fun _ _ _ _ _ _ he => (by cases he), fun _ he => (by cases he)⟩
        · exact .inl (FrOk.triv rfl fun _ h => by cases h)
        · exact .inr ⟨_, hf, hm, fun _ _ h => h, hfr0 _ _ hf _ hm⟩
      · exact .inr ⟨_, h', hfr, fun _ _ h => h, hfr0 _ _ h' _ hfr⟩
  case provBuild f φ πp m Ly c Ls b hf hc hlk hsome =>
    obtain ⟨-, hΛn, -, -, -⟩ := I.fresh C.n (Nat.le_refl _)
    have hn0 : C.n ≠ 0 := fun h0 => by
      obtain ⟨o, ho, _⟩ := I.root; rw [← h0, hΛn] at ho; cases ho
    obtain ⟨mm₀, hm₀⟩ := Option.isSome_iff_exists.1 hsome
    refine frames_of (C := C) (agree_ext (fun π o h => ?_) (M_setEnt rfl ?_))
      fun j ψ h' fr hfr => ?_
    · have : π ≠ C.n := fun e => by subst e; rw [hΛn] at h; cases h
      simp [upd, this, h]
    · intro mm σ hm he
      rw [hm₀] at hm; cases hm
      rw [lookup_none (hlk h.g7d) (Nat.lt_of_le_of_lt (Nat.zero_le _) (I.ltM hm₀)) hm₀] at he
      cases he
    · fiber_cases h'
      · simp only [List.mem_cons] at hfr
        rcases hfr with rfl | hm
        · refine .inl ⟨fun _ _ he => (by cases he), fun _ _ _ _ _ _ he => ?_, fun _ he => (by cases he)⟩
          cases he
          exact ⟨hn0, ⟨Owner.fresh (.ok K.unit) _, (by simp [upd]), rfl, rfl⟩,
            { mm₀ with ents := upd mm₀.ents Ly (some ⟨.building, C.n, []⟩) },
            (by simp [setEnt, hm₀]), (by simp)⟩
        · exact .inr ⟨_, hf, hm, fun _ _ h => h, hfr0 _ _ hf _ hm⟩
      · exact .inr ⟨_, h', hfr, fun _ _ h => h, hfr0 _ _ h' _ hfr⟩
  case close f φ w π env s o hf hc hs ho hst =>
    have hπ0 := (hfr0 f φ hf (.inside π env) (by simp [hs])).inside.1
    refine frames_of (C := C) (bad := (· = π)) ⟨fun π₁ o₁ h₁ => ?_, fun m mm Ly σ _ hm he =>
      ⟨mm, hm, he⟩⟩ fun j ψ h' fr hfr => ?_
    · by_cases e : π₁ = π
      · subst e; rw [ho] at h₁; cases h₁
        exact ⟨{ o with st := .closing, x := w, ph := .hooks false }, by simp [upd],
          fun h => absurd hst h, fun h => absurd rfl h⟩
      · exact ⟨o₁, by simp [upd, e, h₁], id, fun _ => ⟨Iff.rfl, rfl⟩⟩
    · fiber_cases h'
      · simp only [List.mem_cons] at hfr
        rcases hfr with rfl | hm
        · exact .inl ⟨fun _ _ he => (by cases he), fun _ _ _ _ _ _ he => (by cases he),
            fun _ he => (by cases he; exact ⟨hπ0, _, (by simp [upd]; rfl), (by simp)⟩)⟩
        · exact .inr ⟨_, hf, by rw [hs]; exact List.mem_cons_of_mem _ hm,
            fun _ h1 h2 => held_only_self I hf hs rfl fr hm (h2 ▸ h1),
            hfr0 _ _ hf _ (by rw [hs]; exact List.mem_cons_of_mem _ hm)⟩
      · exact .inr ⟨_, h', hfr, fun _ h1 h2 => held_only' I hf hs rfl h' ‹_› fr hfr (h2 ▸ h1),
          hfr0 _ _ h' _ hfr⟩
  case buildFail f φ πp m Ly σ Ls b s c oσ hf hc hs hσ hσo =>
    have hσ0 := (hfr0 f φ hf (.build πp m Ly σ Ls b) (by simp [hs])).build.1
    refine frames_of (C := C) (bad := (· = σ)) ⟨fun π₁ o₁ h₁ => ?_, fun m' mm Ly' σ' hb hm he => ?_⟩
      fun j ψ h' fr hfr => ?_
    · by_cases e : π₁ = σ
      · subst e; rw [hσ] at h₁; cases h₁
        exact ⟨{ oσ with st := .closing, x := .fail c, ph := .hooks false }, by simp [upd],
          fun h => absurd hσo h, fun h => absurd rfl h⟩
      · exact ⟨o₁, by simp [upd, e, h₁], id, fun _ => ⟨Iff.rfl, rfl⟩⟩
    · exact M_setEnt_bad (bad := (· = σ)) rfl rfl
        (hfr0 f φ hf (.build πp m Ly σ Ls b) (by simp [hs])).build.2.2 m' mm Ly' σ' hb hm he
    · fiber_cases h'
      · simp only [List.mem_cons] at hfr
        rcases hfr with rfl | hm
        · exact .inl ⟨fun _ _ he => (by cases he), fun _ _ _ _ _ _ he => (by cases he),
            fun _ he => (by cases he; exact ⟨hσ0, _, (by simp [upd]; rfl), (by simp)⟩)⟩
        · exact .inr ⟨_, hf, by rw [hs]; exact List.mem_cons_of_mem _ hm,
            fun _ h1 h2 => held_only_self I hf hs rfl fr hm (h2 ▸ h1),
            hfr0 _ _ hf _ (by rw [hs]; exact List.mem_cons_of_mem _ hm)⟩
      · exact .inr ⟨_, h', hfr, fun _ h1 h2 => held_only' I hf hs rfl h' ‹_› fr hfr (h2 ▸ h1),
          hfr0 _ _ h' _ hfr⟩
  case buildOk f φ πp m Ly σ Ls b s v o mm e hf hc hs ho hst hm he =>
    have hb := (hfr0 f φ hf (.build πp m Ly σ Ls b) (by simp [hs])).build.2.2
    refine frames_of (C := C) (bad := (· = σ))
      ⟨fun π₁ o₁ h₁ => ?_, M_setEnt_bad rfl rfl hb⟩ fun j ψ h' fr hfr => ?_
    · by_cases e₂ : π₁ = πp
      · subst e₂; rw [ho] at h₁; cases h₁
        exact ⟨{ o with F := ⟨C.n, .obs m Ly⟩ :: o.F }, by simp [upd], id,
          fun _ => ⟨Iff.rfl, rfl⟩⟩
      · exact ⟨o₁, by simp [upd, e₂, h₁], id, fun _ => ⟨Iff.rfl, rfl⟩⟩
    · fiber_cases h'
      · exact .inr ⟨_, hf, by rw [hs]; exact List.mem_cons_of_mem _ hfr,
          fun _ h1 h2 => held_only_self I hf hs rfl fr hfr (h2 ▸ h1),
          hfr0 _ _ hf _ (by rw [hs]; exact List.mem_cons_of_mem _ hfr)⟩
      · exact .inr ⟨_, h', hfr, fun _ h1 h2 => held_only' I hf hs rfl h' ‹_› fr hfr (h2 ▸ h1),
          hfr0 _ _ h' _ hfr⟩
  case finObsLast f φ π s o i m Ly fs mm e oσ hf hc hs ho hph hfs hm he hobs hne hσ hσo hst hb =>
    obtain ⟨hσ0, -, hnh⟩ := I.ents m mm Ly e hm he
    have hnh := hnh hb
    refine frames_of (C := C) (bad := (· = e.σ))
      ⟨fun π₁ o₁ h₁ => ?_, fun m₁ mm₁ Ly₁ σ₁ _ => M_setEnt rfl (fun mm' σ' hm' he' => ?_) m₁ mm₁ Ly₁ σ₁⟩
      fun j ψ h' fr hfr => ?_
    · by_cases e₁ : π₁ = e.σ
      · subst e₁; rw [hσ] at h₁; cases h₁
        exact ⟨_, (by simp [upd] <;> rfl), fun h => absurd hσo h, fun h => absurd rfl h⟩
      · by_cases e₂ : π₁ = π
        · subst e₂; rw [ho] at h₁; cases h₁
          exact ⟨{ o with fs := fs }, by simp [upd, e₁], id, fun _ => ⟨Iff.rfl, rfl⟩⟩
        · exact ⟨o₁, by simp [upd, e₁, e₂, h₁], id, fun _ => ⟨Iff.rfl, rfl⟩⟩
    · rw [hm] at hm'; cases hm'; rw [he] at he'; cases he'; cases hb
    · fiber_cases h'
      · simp only [List.mem_cons] at hfr
        rcases hfr with rfl | hm₁
        · exact .inl ⟨fun _ _ he => (by cases he), fun _ _ _ _ _ _ he => (by cases he),
            fun _ he => (by cases he; exact ⟨hσ0, _, (by simp [upd]; rfl), (by simp)⟩)⟩
        · exact .inr ⟨_, hf, hm₁, fun _ h1 h2 => hnh _ _ hf _ hm₁ (h2 ▸ h1), hfr0 _ _ hf _ hm₁⟩
      · exact .inr ⟨_, h', hfr, fun _ h1 h2 => hnh _ _ h' _ hfr (h2 ▸ h1), hfr0 _ _ h' _ hfr⟩

end Flick.RT
