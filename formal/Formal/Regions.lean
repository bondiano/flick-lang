import Formal.NestMain

/-!
# T4: regions (regions.md §5.3)

Every configuration a program reaches is typed (T1) and satisfies `⊢ Λ` (`Nest`).
Hence the main thread, the children and the finalizers of the live regions reach
through the store only live regions (T4.2: when `π` closes, nothing that can still
step reaches it), no thread uses a region that is not live (T4.1), and T1 holds
with `op_H`: a thread that does not step waits for a child of a region it closes.
-/

namespace Flick

variable {S : Sig}

theorem Nest.cstep {root : Name → Option Tm} {c c' : Cfg} {l Ψ X anc B home t φ}
    (hroot : RootTy S root) (hc : CfgTy S Ψ c t φ) (hc' : CfgTy S (Ψ ++ X) c' t φ)
    (hn : Nest Ψ c anc B home) (hs : CStep root l c c') :
    ∃ anc' B' home', Nest (Ψ ++ X) c' anc' B' home' ∧ ∀ q, q ≠ c.rgs.length → anc' q = anc q := by
  obtain ⟨hμ, h, hΛ, -, hΨ, -⟩ := hc
  cases hs with
  | main hs => exact hn.step (i := none) hroot rfl h hμ hc'.1 hΨ hs
  | kid hg hl hj hs =>
    obtain ⟨t', φ', hk⟩ := (hΛ _ _ hg).1 _ (List.mem_of_getElem? hj)
    exact hn.step hroot (Cfg.at_some.mpr ⟨_, hg, hl, hj⟩) hk hμ hc'.1 hΨ hs

theorem steps_nest {root : Name → Option Tm} {c c' : Cfg} (hroot : RootTy S root)
    (hs : CSteps root c c') :
    ∀ {Ψ t φ anc B home}, t.YF 0 → φ.substT (kill 0) = φ → CfgTy S Ψ c t φ → Nest Ψ c anc B home →
      ∃ X anc' B' home', CfgTy S (Ψ ++ X) c' t φ ∧ Nest (Ψ ++ X) c' anc' B' home' := by
  induction hs with
  | refl => intro Ψ _ _ anc B home _ _ hc hn; exact ⟨[], anc, B, home, by simpa using hc, by simpa using hn⟩
  | step h₁ _ ih =>
    intro Ψ t φ anc B home ht hφ hc hn
    obtain ⟨X₁, hc₁⟩ := cstep_typed hroot ht hφ hc h₁
    obtain ⟨anc₁, B₁, home₁, hn₁, -⟩ := hn.cstep hroot hc hc₁ h₁
    obtain ⟨X₂, anc₂, B₂, home₂, hc₂, hn₂⟩ := ih ht hφ hc₁ hn₁
    exact ⟨X₁ ++ X₂, anc₂, B₂, home₂, by simpa using hc₂, by simpa using hn₂⟩

/-- `⊢ Λ` in every configuration a program reaches. The ancestors `anc` of the
live regions are live (`Nest.live`): this is Л5, a region closes only after every
region created in its body or in its children (`Nest.nested`). -/
theorem reach_nest {root e₀ t φ₀ c} (hroot : RootTy S root) (h₀ : Typ S [] Facts.none [] e₀ (.ex t φ₀))
    (hp : e₀.Below 0) (ht : t.YF 0) (hφ : φ₀.substT (kill 0) = φ₀)
    (hs : CSteps root ⟨[], [], e₀⟩ c) :
    ∃ Ψ anc B home, CfgTy S Ψ c t φ₀ ∧ Nest Ψ c anc B home := by
  obtain ⟨X, anc, B, home, hc, hn⟩ := steps_nest hroot hs ht hφ (CfgTy.init h₀ hp) (Nest.init (Ψ := []) h₀ hp)
  exact ⟨_, anc, B, home, hc, hn⟩

/-- A step that needs a region that is not live (regions.md §3.2): `open_H`, `op_H`,
a finalizer or a child registered, or a region closed. -/
def DeadUse (Λ : List Rgn) (r : Tm) : Prop :=
  ∃ p, Dead Λ p ∧ (r = .app (.const .open_) (.own p) ∨ r = .app (.const .op) (.cap p) ∨
    (∃ t, r = .app (.app (.const .addFin) (.own p)) t) ∨
    (∃ t, r = .app (.app (.const .spawn) (.own p)) t) ∨ ∃ w, r = .inside p w ∧ Final w)

/-- A finished region body that waits for a child. -/
def Waits (Λ : List Rgn) (e : Tm) : Prop :=
  ∃ E r, OkE E ∧ e = plug E r ∧
    ∃ p w g, r = .inside p w ∧ Final w ∧ Λ[p]? = some g ∧ g.live = true ∧ ∃ k ∈ g.kids, ¬ Final k

/-- **T4 (regions).** In every configuration a program reaches:

1. no thread (the main thread or a child of a live region) is at a step that needs
   a region that is not live, in particular `op_H` on a closed region;
2. the main thread, the children and the finalizers of the live regions reach
   through the store only live regions: after `π` closes, nothing that can step
   reaches `π`. -/
theorem T4 {root e₀ t φ₀ c} (hroot : RootTy S root) (h₀ : Typ S [] Facts.none [] e₀ (.ex t φ₀))
    (hp : e₀.Below 0) (ht : t.YF 0) (hφ : φ₀.substT (kill 0) = φ₀)
    (hs : CSteps root ⟨[], [], e₀⟩ c) :
    (∀ E r, OkE E → c.main = plug E r → ¬ DeadUse c.rgs r) ∧
    (∀ (p : Nat) (g : Rgn), c.rgs[p]? = some g → g.live = true → ∀ k ∈ g.kids, ∀ E r, OkE E → k = plug E r →
      ¬ DeadUse c.rgs r) ∧
    (∀ p, Reaches c.mem c.main p → Live c.rgs p) ∧
    ∀ (r : Nat) (g : Rgn), c.rgs[r]? = some g → g.live = true → ∀ k ∈ g.kids ++ g.fins, ∀ p,
      Reaches c.mem k p → Live c.rgs p := by
  obtain ⟨Ψ, anc, B, home, -, hn⟩ := reach_nest hroot h₀ hp ht hφ hs
  refine ⟨fun E r _ he hd => hn.not_dead (i := none) (by rw [← he]; rfl) hd,
    fun p g hg hl k hk E r _ he hd => ?_, fun p h => hn.reach (i := none) rfl h,
    fun r g hg hl k hk p h => ?_⟩
  · obtain ⟨j, hj⟩ := List.mem_iff_getElem?.mp hk
    exact hn.not_dead (i := some (p, j)) (Cfg.at_some.mpr ⟨g, hg, hl, he ▸ hj⟩) hd
  · rcases List.mem_append.mp hk with hk | hk
    · obtain ⟨j, hj⟩ := List.mem_iff_getElem?.mp hk
      exact hn.reach (Cfg.at_some.mpr ⟨g, hg, hl, hj⟩) h
    · exact hn.reach_fin hg hl hk h

end Flick
