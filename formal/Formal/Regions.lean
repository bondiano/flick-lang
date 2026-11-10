import Formal.Soundness

/-!
# What a thread does with a region (regions.md §5.3)

A step that needs a region that is not live (`DeadUse`), and a finished region body that waits
for a child (`Waits`). T4′ (`Live/T4Live.lean`) shows that no thread of a program typed by use
reaches a `DeadUse`; it gives up T4.2 of the capture discipline, that nothing that can still
step reaches a closed region.
-/

namespace Flick

/-- A step that needs a region that is not live (regions.md §3.2): `open_H`, `op_H`,
a finalizer or a child registered, or a region closed. -/
def DeadUse (Λ : List Rgn) (r : Tm) : Prop :=
  ∃ p, Dead Λ p ∧ (r = .app (.const .open_) (.own p) ∨ r = .app (.const .op) (.cap p) ∨
    (∃ t, r = .app (.app (.const .addFin) (.own p)) t) ∨
    (∃ t, r = .app (.app (.const .spawn) (.own p)) t) ∨ (∃ w, r = .inside p w ∧ Final w) ∨
    ∃ ℓ Ef al Ei o v, r = .hkeep p ℓ Ef al (plug Ei (.app (.const (.perform Ef o .tail)) v)) ∧
      OkE Ei ∧ ∀ G ∈ Ei, G.handles Ef o .tail = none)

/-- A finished region body that waits for a child. -/
def Waits (Λ : List Rgn) (e : Tm) : Prop :=
  ∃ E r, OkE E ∧ e = plug E r ∧
    ∃ p w g, r = .inside p w ∧ Final w ∧ Λ[p]? = some g ∧ g.live = true ∧ ∃ k ∈ g.kids, ¬ Final k

end Flick
