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

/-! ### A hidden row without constructors (`@stores`)

`FieldCodec` of std has no constructors: foreign code makes its values and keeps the functions
it is given, which `@stores` says. `Sig.hidden_ok` speaks only of the constructors of a type, so a
signature that builds no such type may give it a hidden row, and the reading above applies to it
as to `G`. -/

/-- A signature that builds no `T`, and hides nothing of it, with the hidden row of `T` first. -/
def storesSig (S : Sig) (T : Name) (hT : ∀ K k ts, S.ctor K ≠ some (k, ts, T))
    (hS : S.hidden T = none) : Sig :=
  { S with
    hidden := fun N => if N = T then some 0 else S.hidden N
    hidden_ok := by
      intro N i h K k ts hK
      by_cases hN : N = T
      · subst hN; exact absurd hK (hT K k ts)
      · rw [if_neg hN] at h
        obtain ⟨h1, h2, h3, h4⟩ := S.hidden_ok N i h K k ts hK
        refine ⟨h1, h2, h3, fun f hf => ?_⟩
        rcases h4 f hf with h | h | h | ⟨T', us, j, he, hj, hus⟩
        · exact Or.inl h
        · exact Or.inr (Or.inl h)
        · exact Or.inr (Or.inr (Or.inl h))
        · have hT' : T' ≠ T := by rintro rfl; rw [hS] at hj; cases hj
          exact Or.inr (Or.inr (Or.inr ⟨T', us, j, he, by simp [hT', hj], hus⟩)) }

theorem storesSig_hidden (S : Sig) (T : Name) (hT hS) :
    (storesSig S T hT hS).hidden T = some 0 := by
  simp [storesSig]

/-- `FieldCodec a` written bare. -/
def FB (u : Ty) : Ty := Ty.tcon "FieldCodec" [.bare, u]

/-- `on_missing : (Unit -> a)^c -> FieldCodec a -> FieldCodec a`, the rows of services left out,
`a c` the binders `0 1`. -/
def onMissingBare : Scheme :=
  ⟨2, Facts.none, .arrow (.arrow (.base .unit) .pure (.tvar 0) (fun x => x = .v 1)) .pure
    (.arrow (FB (.tvar 0)) .pure (FB (.tvar 0)) Caps.none) Caps.none⟩

/-- The result of `on_missing` as elision reads it: `(FieldCodec a)^{c, d}`. -/
def onMissingResult : Ty := Ty.tcon "FieldCodec" [.caps (fun x => x = .v 1 ∨ x = .v 2), .tvar 0]

/-- Its reading: the row `d` of the codec it takes is the binder `2`, and the result holds it
and the `c` of the default it keeps. -/
def onMissingRead : Scheme :=
  ⟨3, Facts.none, .arrow (.arrow (.base .unit) .pure (.tvar 0) (fun x => x = .v 1)) .pure
    (.arrow (Ty.tcon "FieldCodec" [.tvar 2, .tvar 0]) .pure onMissingResult Caps.none) Caps.none⟩

theorem on_missing_reads_hidden_rows :
    Scheme.elide (fun x => x = .v 1) onMissingBare = onMissingRead := by
  simp only [onMissingBare, onMissingRead, onMissingResult, FB, Ty.tcon, List.foldl]
  elide_eq
  -- the default's qualifier names a binder of the scheme, which the fresh row leaves in place
  all_goals
    funext x
    rcases x with n | p <;>
      simp [Caps.substT, liftT, shiftV, Ty.reach, Caps.none, Caps.fresh, Atom.shiftFrom] <;>
      omega

/-- A codec whose default holds the region `v 0` of a `scope` holds it too, so it has no type
`t.up 1` that leaves the `scope`: the compiler reports `FLICK-G001` there. -/
theorem on_missing_scope_rejected {S : Sig} (u d t : Ty) :
    ¬ LeLive S (onMissingResult.substT (instT [u, .tvar 0, d])) (t.up 1) :=
  fun h => Ty.up_avoid_v0 t (h.reach (.v 0) (by
    simp [onMissingResult, Ty.tcon, Ty.substT, Ty.reach, Caps.substT, instT]))

end Flick.LiveChecks
