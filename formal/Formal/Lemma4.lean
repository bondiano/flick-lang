import Formal.Fresh
import Formal.Soundness

/-!
# L4 and L6: what a value reaches through the store, and the types of old cells

`reach_μ(e)` of regions.md §5.2 is `Reaches μ e`: the run-time regions `e` writes
down (in `cap π`, `own π`, `inside π e` and inside the bodies of `λ` and `task`),
and those of the values in the cells of its locations, transitively. In the
semantics by substitution this is reachability.
-/

namespace Flick

variable {S : Sig}

/-- `π ∈ reach_μ(e)`. -/
inductive Reaches (μ : List Tm) : Tm → Nat → Prop
  | here : e.Has (.rgn p) → Reaches μ e p
  | cell : e.Has (.loc l) → μ[l]? = some v → Reaches μ v p → Reaches μ e p

/-- **L4.** If `·;Ψ ⊢ v : τ` and `⊢ μ : Ψ`, then `reach_μ(v) ⊆ ⌈τ⌉`. The cases of
`regions.md` §5.2 are those of `Typ.holds`; a location reaches the cell's value,
which is typed at `Ψ(ℓ)` exactly, so it reaches within `⌈Ψ(ℓ)⌉ ⊆ ⌈τ⌉`. -/
theorem lemma_4 {Ψ μ p} (hμ : StoreTy S Ψ μ) {e : Tm} (hr : Reaches μ e p) :
    ∀ {Θ τ φ}, Value e → Typ S Ψ Θ [] e (.ex τ φ) → τ.reach (.r p) := by
  induction hr with
  | here hp =>
    intro _ _ _ hv h
    exact (h.holds hv rfl).1 _ (.inr (.inl ⟨_, hp, rfl⟩))
  | cell hl hv' _ ih =>
    intro _ _ _ hv h
    have hlt := h.locs _ hl
    obtain ⟨hvv, hvt⟩ := hμ.2 _ _ _ hv' (List.getElem?_eq_getElem hlt)
    exact (h.holds hv rfl).1 _ (.inr (.inr ⟨_, _, hl, List.getElem?_eq_getElem hlt, ih hvv hvt⟩))

/-- A cell reaches no more than its type. -/
theorem lemma_4_cell {Ψ : List Ty} {μ : List Tm} {l : Nat} {v : Tm} {t : Ty} {p : Nat} (hμ : StoreTy S Ψ μ) (hv : μ[l]? = some v)
    (ht : Ψ[l]? = some t) (hr : Reaches μ v p) : t.reach (.r p) :=
  let ⟨hvv, hvt⟩ := hμ.2 l v t hv ht
  lemma_4 hμ hr hvv hvt

/-- **L6 (old cells).** In every configuration reached by a program, the type of each
cell names only regions that already exist, and later configurations keep the type
of every cell: a region created after a cell is not in its type. -/
theorem lemma_6 {root e₀ t φ₀ c} (hroot : RootTy S root) (h₀ : Typ S [] Facts.none [] e₀ (.ex t φ₀))
    (hp : e₀.Below 0) (ht : t.YF 0) (hφ : φ₀.substT (kill 0) = φ₀)
    (hs : CSteps root ⟨[], [], e₀⟩ c) :
    ∃ Ψ, CfgTy S Ψ c t φ₀ ∧ (∀ u ∈ Ψ, ∀ p, c.rgs.length ≤ p → u.substT (kill p) = u) ∧
      ∀ {c'}, CSteps root c c' → ∃ X, CfgTy S (Ψ ++ X) c' t φ₀ := by
  obtain ⟨Ψ, hc⟩ := steps_typed hroot hs ht hφ (CfgTyM.init h₀ hp)
  simp only [List.nil_append] at hc
  exact ⟨Ψ, hc, fun u hu _ hle => (hc.2.2.2.2.1 u hu).mono hle,
    fun hs' => steps_typed hroot hs' ht hφ hc⟩

end Flick
