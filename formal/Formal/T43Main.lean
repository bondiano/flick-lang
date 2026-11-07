import Formal.Regions

/-!
# T4.3 and T1 with `op_H` (regions.md §5.3)
-/

namespace Flick

variable {S : Sig}

/-- The region of the handler of an operation is written in the context. -/
theorem handlerOf_has {r : Tm} {Ef o : Name} {c : OpClass} {q : Nat} :
    ∀ {E : List Frame}, handlerOf E Ef o c = some q → (plug E r).Has (.rgn q)
  | [], h => by simp [handlerOf] at h
  | F :: E, h => by
    simp only [handlerOf] at h
    cases hF : F.handles Ef o c with
    | some p =>
      rw [hF] at h; cases h
      cases F with
      | hwith p' ℓ' E' al rf =>
        simp only [Frame.handles] at hF
        split at hF
        · cases hF
          exact plug_has E (by simp [Frame.plug, Tm.Has])
        · cases hF
      | _ => simp [Frame.handles] at hF
    | none => rw [hF] at h; exact handlerOf_has (r := F.plug r) h

/-- **T4.3 (regions.md §5.3).** In every configuration a program reaches, an operation of
a thread (the main thread or a child of a live region) answered by a handler of the region
`q` finds `q` live: no fiber uses a handler after its region closed. This is the premise
G11 of T5 in the form of the liveness premise of `provide` (`runtime.md`). -/
theorem T4_3 {root e₀ t φ₀ c} (hroot : RootTy S root) (h₀ : Typ S [] Facts.none [] e₀ (.ex t φ₀))
    (hp : e₀.Below 0) (ht : t.YF 0) (hφ : φ₀.substT (kill 0) = φ₀)
    (hs : CSteps root ⟨[], [], e₀⟩ c) :
    ∀ i e E Ef o k v q, c.at i = some e → OkE E → e = plug E (.app (.const (.perform Ef o k)) v) →
      handlerOf E Ef o k = some q → Live c.rgs q := by
  obtain ⟨Ψ, anc, B, home, -, hn⟩ := reach_nest hroot h₀ hp ht hφ hs
  intro i e E Ef o k v q hi _ he hl
  exact hn.reach hi (.here (he ▸ handlerOf_has hl))

/-- **T1 with `op_H`** (regions.md §5.3, corollary). As T1, and a thread that does not
step, is not finished and does not perform an operation nothing answers waits for a child
of a region it closes. -/
theorem T1_regions {root e₀ t φ₀ c} (hroot : RootTy S root)
    (hcov : ∀ N, φ₀.svc (.nm N) → (root.svc N).isSome) (h₀ : Typ S [] Facts.none [] e₀ (.ex t φ₀))
    (hp : e₀.Below 0) (ht : t.YF 0) (hφ : φ₀.substT (kill 0) = φ₀)
    (hs : CSteps root ⟨[], [], e₀⟩ c) :
    ((Value c.main ∧ ∃ Ψ, StoreTy S Ψ c.mem ∧ Typ S Ψ Facts.age [] c.main (.ex t .pure)) ∨
      (∃ v, c.main = .raise v ∧ ∃ Ψ, Typ S Ψ Facts.age [] v (.ex (.errors φ₀.err) .pure)) ∨
      (∃ l o, Step root l c.rgs c.mem c.main o) ∨ Waits c.rgs c.main) ∧
    ∀ (p : Nat) (g : Rgn), c.rgs[p]? = some g → g.live = true → ∀ k ∈ g.kids,
      Final k ∨ (∃ l o, Step root l c.rgs c.mem k o) ∨ (∃ D, PerformStuck S root k D) ∨
        Waits c.rgs k := by
  obtain ⟨h₁, h₂⟩ := T1 hroot hcov h₀ hp ht hφ hs
  obtain ⟨hm, hk, -, -⟩ := T4 hroot h₀ hp ht hφ hs
  have waits : ∀ e, (∀ E r, OkE E → e = plug E r → ¬ DeadUse c.rgs r) →
      Blocked c.rgs e → Waits c.rgs e := by
    rintro e hd ⟨E, r, hE, he, hb⟩
    exact hb.elim (fun hw => ⟨E, r, hE, he, hw⟩) fun hb' => (hd E r hE he hb').elim
  refine ⟨h₁.imp_right (Or.imp_right (Or.imp_right (waits _ hm))), fun p g hg hl k hk' => ?_⟩
  exact (h₂ p g hg hl k hk').imp_right (Or.imp_right (Or.imp_right (waits k (hk p g hg hl k hk'))))

end Flick
