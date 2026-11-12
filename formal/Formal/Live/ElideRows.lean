import Formal.Live.Elide
import Formal.LiveRules

/-!
# The hidden rows elision reads (regions.md §1.4)

A signature writes data with hidden storage bare; `Scheme.elide` reads its hidden row: a fresh
binder of kind `CRow` in a parameter, what the parameters before it reach in the result. `compose`
of the spec reads `∀a b s c d. (G a b)^c → (G s a)^d →R (G s b)^{c, d}`, and `leak6` returns
that result from the `scope` its second argument holds: the row holds the scope's region, which
widening never drops.
-/

namespace Flick.LiveChecks

/-- `G` written bare in a signature: the hidden row is `bare`. -/
def GB (u v : Ty) : Ty := Ty.tcon "G" [.bare, u, v]

/-- The stage of `compose`: it runs `FileSystem`. -/
def fsStage : Stage := .rt (fun x => x = .nm "FileSystem") Row.none

/-- `compose : G a b -> G s a ->{FileSystem} G s b`, `a b s` the binders `0 1 2`. -/
def composeBare : Scheme :=
  ⟨3, Facts.none, .arrow (GB (.tvar 0) (.tvar 1)) .pure
    (.arrow (GB (.tvar 2) (.tvar 0)) fsStage (GB (.tvar 2) (.tvar 1)) Caps.none) Caps.none⟩

/-- The result of `compose` as elision reads it: `(G s b)^{c, d}`. -/
def composeResult : Ty := GT (fun x => x = .v 3 ∨ x = .v 4) (.tvar 2) (.tvar 1)

/-- Its reading: the rows `c d` are the binders `3 4`, of kind `CRow`; the runtime stage needs
both live, as `Live` reads it, and the result holds `{c, d}`. -/
def composeRead : Scheme :=
  ⟨5, Facts.none,
    .arrow (Ty.tcon "G" [.tvar 3, .tvar 0, .tvar 1]) .pure
      (.arrow (Ty.tcon "G" [.tvar 4, .tvar 2, .tvar 0])
        (.rt (fun x => x = .nm "FileSystem" ∨ x = .atm (.v 3) ∨ x = .atm (.v 4)) Row.none)
        composeResult Caps.none) Caps.none⟩

theorem compose_reads_hidden_rows : Scheme.elide Caps.none composeBare = composeRead := by
  simp only [composeBare, composeRead, composeResult, GB, GT, fsStage, Ty.tcon, List.foldl]
  elide_eq

/-- `leak6` through the read signature: `compose` at `c := s`, the region `v 0` of the `scope`
that opened the file its second argument reads, returns a value whose row holds that region, so it
has no type `t.up 1` that leaves the `scope`. -/
theorem leak6_through_elision {S : Sig} (u v w t : Ty) :
    ¬ LeLive S (composeResult.substT (instT [u, v, w, .tvar 0, .caps Caps.none])) (t.up 1) :=
  fun h => Ty.up_avoid_v0 t (h.reach (.v 0) (by
    simp [composeResult, GT, Ty.tcon, Ty.substT, Ty.reach, Caps.substT, instT]))

end Flick.LiveChecks
