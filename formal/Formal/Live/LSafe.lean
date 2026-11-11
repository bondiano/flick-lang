import Formal.Live.LCfg
import Formal.Live.LNest
import Formal.Regions

/-!
# What the live invariant gives a thread: its place, and no use of a dead region

A thread of a configuration with `LNest` whose stage has its region atoms among the regions
of its base is at a `Place` at each of its holes. Every region its redex needs is in the stage
of the redex, hence of the thread or of a frame, hence live; so is the region of every form of
a handler and of every `inside` form of the thread.
-/

namespace Flick

variable {S : Sig} {Ψ : List Ty} {anc : Nat → Nat → Prop}

theorem Frame.rgn_reg {F : Frame} {x : Nat} (h : F.rgn = some x) : F.reg = some x := by
  cases F <;> simp_all [Frame.rgn, Frame.reg]

theorem Frame.cell_reg {F : Frame} {q ℓ : Nat} (h : F.cell = some (q, ℓ)) : F.reg = some q := by
  cases F <;> simp_all [Frame.cell, Frame.reg]

theorem handlerOf_reg {Ef o : Name} {c : OpClass} {q : Nat} :
    ∀ {E : List Frame}, handlerOf E Ef o c = some q → ∃ F ∈ E, F.reg = some q
  | [], h => by simp [handlerOf] at h
  | F :: E, h => by
    simp only [handlerOf] at h
    cases hF : F.handles Ef o c with
    | some p =>
      rw [hF] at h; cases h
      refine ⟨F, by simp, ?_⟩
      cases F <;> simp only [Frame.handles] at hF <;> (try split at hF) <;> simp_all [Frame.reg]
    | none =>
      rw [hF] at h
      obtain ⟨G, hG, hr⟩ := handlerOf_reg h
      exact ⟨G, by simp [hG], hr⟩

/-- A thread with stage atoms in its base is at a place at each of its holes. -/
theorem LNest.place {c home i E r ψ} (hn : LNest Ψ c anc home) (hi : c.at i = some (plug E r))
    (hE : OkE E) (hψ : ∀ x, ψ.svc (.atm (.r x)) → baseOf anc i x) :
    Place Ψ anc ψ E (holeE (baseOf anc i) E) := by
  refine ⟨hn.hole_chain hi hE, fun x hx => holeE_iff.mpr (.inl (hψ x hx)),
    fun F hF x hx => ?_, fun F hF q ℓ hc => ⟨hn.frame_reg hi hE F hF (Frame.cell_reg hc), ?_⟩⟩
  · rcases hx with rfl | hx
    · exact holeE_iff.mpr (.inr hF)
    · exact hn.frame_reg hi hE F hF (Frame.rgn_reg hx)
  · intro r' ε' t' hℓ x hx
    obtain ⟨r₀, ε₀, t₀, h₀, ha⟩ := hn.frame_cell hi hE F hF hc
    rw [hℓ] at h₀; cases h₀
    exact ha x hx

/-- What the stage of a hole of a thread holds is live. -/
theorem LNest.atm_live {c home i E r x} {ψ : Stage} (hn : LNest Ψ c anc home)
    (hi : c.at i = some (plug E r)) (hE : OkE E)
    (hψ : ∀ x, ψ.svc (.atm (.r x)) → baseOf anc i x)
    (hx : ψ.svc (.atm (.r x)) ∨ ∃ F ∈ E, F = .inside x ∨ F.rgn = some x) : Live c.rgs x :=
  have hP := hn.place hi hE hψ
  hn.hole_live hi (hx.elim (hP.2.1 x) fun ⟨F, hF, hd⟩ => hP.2.2.1 F hF x hd)

theorem Live.not_dead {Λ : List Rgn} {p : Nat} (h : Live Λ p) (hd : Dead Λ p) : False := by
  obtain ⟨g, hg, hl⟩ := h; rw [hd g hg] at hl; cases hl

/-- A typed thread of the live discipline whose stage holds only regions of its base uses no
region that is not live (T4.1). -/
theorem LNest.not_dead {c home i E r t} {ψ : Stage} (hn : LNest Ψ c anc home)
    (hi : c.at i = some (plug E r)) (hE : OkE E)
    (h : Typ S Ψ Facts.age [] (plug E r) (.ex t ψ))
    (hψ : ∀ x, ψ.svc (.atm (.r x)) → baseOf anc i x) : ¬ DeadUse c.rgs r := by
  rintro ⟨p, hd, hr⟩
  have byAtm : ¬ Value r → (∀ {Ψ' Θ t' ψ'}, Typ S Ψ' Θ [] r (.ex t' ψ') → ψ'.svc (.atm (.r p))) →
      False := fun hv hat => by
    obtain ⟨K, Θ', t', ψ', hr', -, hx⟩ := hole_atm hE hv h
    exact (hn.atm_live hi hE hψ (hx p (hat hr'))).not_dead hd
  rcases hr with rfl | rfl | ⟨w, rfl⟩ | ⟨w, rfl⟩ | ⟨w, rfl, -⟩ | ⟨ℓ, Ef, al, Ei, o, v, rfl, -⟩
  · exact byAtm (fun hv => by cases hv <;> simp_all [Const.arity, Tm.head]) open_atm
  · exact byAtm (fun hv => by cases hv <;> simp_all [Const.arity, Tm.head]) op_atm
  · exact byAtm (fin_not_value (.inl rfl)) (fin_atm (.inl rfl))
  · exact byAtm (fin_not_value (.inr rfl)) (fin_atm (.inr rfl))
  · exact ((hn.thread i _ hi).2 p (plug_has E (by simp [Tm.Has]))).2.not_dead hd
  · have hv : ¬ Value (Tm.hkeep p ℓ Ef al (plug Ei (.app (.const (.perform Ef o .tail)) v))) :=
      fun hv => by cases hv
    have hw := (fw_of_plug hv hE (hn.thread i _ hi).1).2
    exact (hn.hole_live hi hw.1).not_dead hd

/-- An operation of a thread is answered by a handler of a live region (T4.3). -/
theorem LNest.handler_live {c home i E Ef o k v q} (hn : LNest Ψ c anc home)
    (hi : c.at i = some (plug E (.app (.const (.perform Ef o k)) v))) (hE : OkE E)
    (hq : handlerOf E Ef o k = some q) : Live c.rgs q := by
  obtain ⟨F, hF, hr⟩ := handlerOf_reg hq
  exact hn.hole_live hi (hn.frame_reg hi hE F hF hr)

end Flick
