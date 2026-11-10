import Formal.T43Main
import Formal.Live.T4Live

/-!
# T1 with `op_H` for typing by use

As `T1_regions`: a thread that does not step and is not finished waits for a child of a
region it closes, or is a child that performs an operation only the frame of its handler in
a parent runs. That no thread is at a use of a closed region is T4.1 of the live discipline
(`T4'`), not T4.
-/

namespace Flick

variable {S : Sig}

/-- **T1 with `op_H`** of the live discipline (regions.md §5.3, corollary). -/
theorem T1_regions {root e₀ t φ₀ c} (hroot : RootTy S root)
    (hcov : ∀ N, φ₀.svc (.nm N) → (root.svc N).isSome)
    (h₀ : Typ S [] Facts.none [] e₀ (.ex t φ₀))
    (hp : e₀.Below 0) (ht : t.YF 0) (hφ : φ₀.substT (kill 0) = φ₀)
    (hs : CSteps root ⟨[], [], e₀⟩ c) :
    ((Value c.main ∧ ∃ Ψ, StoreTy S Ψ c.mem ∧ Typ S Ψ Facts.age [] c.main (.ex t .pure)) ∨
      (∃ v, c.main = .raise v ∧ ∃ Ψ, Typ S Ψ Facts.age [] v (.ex (.errors φ₀.err) .pure)) ∨
      (∃ l o, Step root l c.rgs c.mem c.main o) ∨ Waits c.rgs c.main) ∧
    ∀ (p : Nat) (g : Rgn), c.rgs[p]? = some g → g.live = true → ∀ k ∈ g.kids,
      Final k ∨ (∃ l o, Step root l c.rgs c.mem k o) ∨ PerformStuck S root k (Row.nonTail S) ∨
        Waits c.rgs k := by
  obtain ⟨h₁, -⟩ := T1 hroot hcov h₀ hp ht hφ hs
  obtain ⟨hm, hk, -⟩ := T4' hroot e₀ t φ₀ c h₀ hp ht hφ hs
  have waits : ∀ e, (∀ E r, OkE E → e = plug E r → ¬ DeadUse c.rgs r) →
      Blocked c.rgs e → Waits c.rgs e := by
    rintro e hd ⟨E, r, hE, he, hb⟩
    exact hb.elim (fun hw => ⟨E, r, hE, he, hw⟩) fun hb' => (hd E r hE he hb').elim
  refine ⟨h₁.imp_right (Or.imp_right (Or.imp_right (waits _ hm))), fun p g hg hl k hk' => ?_⟩
  obtain ⟨Ψ, hc, hkc⟩ := steps_cov hroot hs ht hφ (fun N h => .inl (hcov N h)) (CfgTy.init h₀ hp)
    (fun _ _ h => by simp at h)
  obtain ⟨t', φ', hkt, hkn⟩ := hkc p g hg k (List.mem_append_left _ hk')
  rcases progress (Λ := c.rgs) (root := root) hroot hkt hc.1.1 rfl rfl with hv | hn
  · exact .inl (.inl hv)
  cases hn with
  | raise h' => exact .inl (.inr (.inl h'))
  | step hstep => exact .inr (.inl hstep)
  | blocked hb => exact .inr (.inr (.inr (waits k (hk p g hg hl k hk') hb)))
  | unw h' => exact .inl (.inr (.inr h'))
  | perform hps =>
    obtain ⟨E, Ef, o, c', v, a, b, hE, hv, he, hop, hh, hna, hD⟩ := hps
    refine .inr (.inr (.inl ⟨E, Ef, o, c', v, a, b, hE, hv, he, hop, hh, hna, ?_⟩))
    rcases hkn Ef hD with h | h
    · obtain ⟨w, hw⟩ := Option.isSome_iff_exists.mp h
      exact (hna (root_answers hroot hw hop)).elim
    · exact h

end Flick
