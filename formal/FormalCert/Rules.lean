import FormalCert.Tactics

/-!
# Derived rules of a certificate

Each is a theorem about `Typ` (`Formal/Rules.lean`), shaped so a certificate states the instance
it takes and proves the side conditions with the tactics of `Tactics.lean`.
-/

namespace Flick.Cert

variable {S : Sig} {Ψ : List Ty} {Θ : Facts} {Γ : List Scheme}

/-- (Var) at an instance the certificate writes out: `σ.inst ts` is `T'`, a subtype of `T`. -/
theorem typ_var {i : Nat} {σ : Scheme} {ts : List Ty} {T' T : Ty} {φ : Stage}
    (h : Γ[i]? = some σ) (hk : ts.length = σ.k) (hH : σ.Holds Θ ts)
    (he : σ.inst ts = T') (hl : LeLive T' T) : Typ S Ψ Θ Γ (.var i) (.ex T φ) :=
  .var h hk hH (he ▸ .le hl)

/-- (Glob) at an instance the certificate writes out. -/
theorem typ_glob {g : Nat} {σ : Scheme} {ts : List Ty} {T' T : Ty} {φ : Stage}
    (h : S.glob g = some σ) (hk : ts.length = σ.k) (hH : σ.Holds Θ ts)
    (he : σ.inst ts = T') (hl : LeLive T' T) : Typ S Ψ Θ Γ (.glob g) (.ex T φ) :=
  .glob h hk hH (he ▸ hl)

/-- A scheme without facts holds at every instance. -/
theorem holds_none {k : Nat} {t : Ty} (ts : List Ty) : (Scheme.mk k Facts.none t).Holds Θ ts :=
  fun _ _ h => h.elim

/-! ## Services: an operation, and `provide` lowered to a handler -/

/-- The one arm of a handler of `E` whose operation `o` the record `w`, bound outside the
handler, implements: `o x → w.o x`. -/
abbrev recArm (o : Name) : Tm := .acons (Pat.spine o [.var]) (.app (.proj (.var 1) o) (.var 0)) .anil

/-- An operation `o` of `E` called, at its arrow `a →{E} b`. -/
theorem typ_op {E o : Name} {a b T : Ty} {φ : Stage} (h : S.op E o = some (.tail, a, b))
    (hT : LeLive (.arrow a (.rt (Row.single E) Row.none) b Caps.none) T) :
    Typ S Ψ Θ Γ (.const (.perform E o .tail)) (.ex T φ) :=
  .perform (t := b) h hT

/-- `provide (Layer.succeed E w) e` as the core writes it: `let w = … in hdl E (o x → w.o x) (λv. v) e`;
the record may hold anything, as the arm reads it through a variable. -/
theorem typ_provide {E o : Name} {a b R t T : Ty} {w e : Tm} {r ε : Row} {φ₀ φ : Stage}
    (hop : S.op E o = some (.tail, a, b)) (hc : S.Complete E (recArm o))
    (hR : R.field o = some (.arrow a (.rt Row.none Row.none) b Caps.none))
    (hw : Typ S Ψ Θ Γ w (.ex R φ))
    (he : Typ S Ψ Θ (.mono R :: Γ) e (.ex t φ₀)) (hle : φ₀.le (.rt (Row.union (Row.single E) r) ε))
    (hob : φ₀.ObOut (Row.single E)) (hφ : (Stage.rt r ε).le φ) (hT : LeLive t T) :
    Typ S Ψ Θ Γ (.let_ w (.hdl E (recArm o) (.lam (.var 0)) e)) (.ex T φ) := by
  obtain ⟨ha, hb⟩ := S.op_closed E o .tail a b (shiftV 1) hop
  have hf : (R.up 1).field o = some (.arrow a (.rt Row.none Row.none) b Caps.none) := by
    rw [Ty.up_def]
    have := Ty.field_substT (shiftV 1) hR
    simpa [Ty.substT, Stage.substT, ha, hb, Row.none_substT, Caps.none_substT] using this
  refine .let_ hw (.hdl (ψ := .pure) (C := t.reach) (t' := t) ?_ ?_ id_typ trivial he hle hob
    ⟨⟨o, _, _, _, hop⟩, fun hn => (hn hc).elim⟩ hφ hT)
  · refine .hatail hop ?_ .hanil
    refine .app (a := a) (ψ := .rt Row.none Row.none) (t := b) (C := Caps.none) ?_ ?_ ?_ .refl
    · refine .proj (R := R.up 1) ?_ hf .refl
      refine typ_var (ts := []) (T' := R.up 1) rfl rfl ?_ ?_ .refl
      · intro C s h
        obtain ⟨_, _, h, _⟩ := h
        exact h.elim
      · simp [Scheme.up, Ty.up_def]
    · exact typ_var (ts := []) (T' := a) rfl rfl (holds_none _) (Scheme.inst_mono a) .refl
    · simp only [Stage.use, Stage.le, Stage.substT, Row.Sub]
      exact ⟨fun n h => by simp [Row.union, Row.none, Row.ofCaps, Caps.none] at h; cases n <;> simp_all,
        fun _ h => h.elim⟩
  · simp [Tm.NoIns, Tm.Has]
end Flick.Cert
