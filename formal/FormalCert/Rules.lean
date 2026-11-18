import FormalCert.Tactics

/-!
# Derived rules of a certificate

Each is a theorem about `Typ` (`Formal/Rules.lean`), shaped so a certificate states the instance
it takes and proves the side conditions with the tactics of `Tactics.lean`.
-/

namespace Flick.Cert

variable {S : Sig} {Ψ : List Ty} {Θ : Facts} {Γ : List Scheme}

/-- (Var) at an instance the certificate writes out: `σ.inst ts` is a subtype of `T'`, the type
the model gives the instance, a subtype of `T`. The model keeps rows as sets, so `T'` writes a
row the instance writes as a union with an instantiated variable, `ρ[ρ := ∅] ∪ C`, as `C`;
the two read alike, and live subtyping compares rows by what they stand for. -/
theorem typ_var {i : Nat} {σ : Scheme} {ts : List Ty} {T' T : Ty} {φ : Stage}
    (h : Γ[i]? = some σ) (hk : ts.length = σ.k) (hH : σ.Holds Θ ts)
    (he : LeLive S (σ.inst ts) T') (hl : LeLive S T' T) : Typ S Ψ Θ Γ (.var i) (.ex T φ) :=
  .var h hk hH (.le (he.trans hl))

/-- (Glob) at an instance the certificate writes out, as (Var). -/
theorem typ_glob {g : Nat} {σ : Scheme} {ts : List Ty} {T' T : Ty} {φ : Stage}
    (h : S.glob g = some σ) (hk : ts.length = σ.k) (hH : σ.Holds Θ ts)
    (he : LeLive S (σ.inst ts) T') (hl : LeLive S T' T) : Typ S Ψ Θ Γ (.glob g) (.ex T φ) :=
  .glob h hk hH (he.trans hl)

/-- A scheme without facts holds at every instance. -/
theorem holds_none {k : Nat} {t : Ty} (ts : List Ty) : (Scheme.mk k Facts.none t).Holds Θ ts :=
  fun _ _ h => h.elim

/-! ## Services: an operation, and `provide` lowered to a handler -/

/-- The one arm of a handler of `E` whose operation `o` the record `w`, bound outside the
handler, implements: `o x → w.o x`. -/
abbrev recArm (o : Name) : Tm := .acons (Pat.spine o [.var]) (.app (.proj (.var 1) o) (.var 0)) .anil

/-- An operation `o` of a service `E` without parameters called, at its arrow `a →{E} b`. -/
theorem typ_op {E o : Name} {a b T : Ty} {φ : Stage} (h0 : S.arity E = 0)
    (h : S.op E o = some (.tail, a, b))
    (hT : LeLive S (.arrow a (.rt (.rnm E (.adt E)) .cnil) b Caps.none) T) :
    Typ S Ψ Θ Γ (.const (.perform E o .tail)) (.ex T φ) := by
  obtain ⟨ha, hb⟩ := S.op_closed E o .tail a b (instT []) h fun _ hi => by omega
  exact .perform (t := b) (us := []) h0.symm h (by rw [ha, hb]; exact hT)

/-- `provide (Layer.succeed E w) e` as the core writes it: `let w = … in hdl E (o x → w.o x) (λv. v) e`;
the record may hold anything, as the arm reads it through a variable. The rows outside, `r` and
`ε`, are row terms: the handler's value and its arms are typed at them. `E` has no parameters,
so the handler answers each of its arguments. -/
theorem typ_provide {E o : Name} {a b R t T r ε : Ty} {w e : Tm} {φ₀ φ : Stage} (h0 : S.arity E = 0)
    (hop : S.op E o = some (.tail, a, b)) (hc : S.Complete E (recArm o))
    (hR : R.field o = some (.arrow a (.rt .cnil .cnil) b Caps.none))
    (hw : Typ S Ψ Θ Γ w (.ex R φ))
    (he : Typ S Ψ Θ (.mono R :: Γ) e (.ex t φ₀))
    (hle : φ₀.le (.rt (Row.union (Row.single E) r.asRow) ε.asRow))
    (hob : φ₀.ObOut (Row.single E)) (hφ : (Stage.rt r.asRow ε.asRow).le φ) (hT : LeLive S t T) :
    Typ S Ψ Θ Γ (.let_ w (.hdl E (recArm o) (.lam (.var 0)) e)) (.ex T φ) := by
  obtain ⟨ha, hb⟩ := S.op_closed E o .tail a b (shiftV 1) hop fun _ hi => by omega
  obtain ⟨ha₀, hb₀⟩ := S.op_closed E o .tail a b (instT []) hop fun _ hi => by omega
  have hU : ∀ c, S.answers E [] c := fun _ => .inl h0
  have hf : (R.up 1).field o = some (.arrow a (.rt .cnil .cnil) b Caps.none) := by
    rw [Ty.up_def]
    have := Ty.field_substT (shiftV 1) hR
    simpa [Ty.substT, Ty.isStage, ha, hb, Caps.none_substT] using this
  have hB : Row.Sub (Row.union (Row.single E) r.asRow) (Row.hBody E (S.answers E []) r.asRow) := by
    intro x hx
    rcases hx with hx | hx
    · cases x <;> simp [Row.single, Row.inst] at hx
      exact .inl ⟨hx.1, hU _⟩
    · exact Row.hBody_of hU hx
  refine .let_ hw (.hdl (ψ := .pure) (C := t.reach) (t' := t) (us := []) h0.symm ?_ ?_ id_typ trivial
    he (Stage.le_rt_sub hle hB) hob ⟨⟨o, _, _, _, hop⟩, fun hn => (hn hc).elim⟩ hφ hT)
  · refine .hatail (us := []) h0.symm hop ?_ .hanil
    simp only [ha₀, hb₀]
    refine .app (a := a) (ψ := .rt .cnil .cnil) (t := b) (C := Caps.none) ?_ ?_ ?_ .refl
    · refine .proj (R := R.up 1) ?_ hf .refl
      refine typ_var (ts := []) (T' := R.up 1) rfl rfl ?_ ?_ .refl
      · intro C s h
        obtain ⟨_, _, h, _⟩ := h
        exact h.elim
      · exact le_of_eq (by simp [Scheme.up, Ty.up_def])
    · exact typ_var (ts := []) (T' := a) rfl rfl (holds_none _) (le_of_eq (Scheme.inst_mono a)) .refl
    · simp only [Ty.asStage, Stage.use, Stage.le, Row.Sub]
      refine ⟨fun n h => ?_, fun n h => ?_⟩ <;> cases n <;>
        simp [Row.union, Ty.asRow, Row.none, Row.ofCaps, Caps.none] at h
  · simp [Tm.NoIns, Tm.Has]
/-! ## Dictionaries -/

/-- (Dproj) of a method or a superclass of no variables of its own, at the dictionary's
arguments. -/
theorem typ_dproj {e : Tm} {c k : Nat} {us : List Ty} {fs : List (Label × Scheme)} {l : Label}
    {t T' T : Ty} {φ : Stage}
    (he : Typ S Ψ Θ Γ e (.ex (.dict c (Ty.spine us)) φ)) (hd : S.dict c = some (k, fs))
    (hk : us.length = k) (hl : fs.lookup l = some ⟨0, Facts.none, t⟩)
    (hi : t.substT (instT us) = T') (hT : LeLive S T' T) :
    Typ S Ψ Θ Γ (.proj e l) (.ex T φ) := by
  refine .dproj (ts := []) he hd hk hl rfl ?_ ?_
  · rintro _ _ ⟨_, _, h, _⟩; exact h.elim
  · have hid : ∀ t : Ty, t.substT (instT []) = t := fun t => by
      conv => rhs; rw [← t.substT_ident]
      exact Ty.substT_congr (fun a => by cases a <;> simp [instT, Ty.ident])
    simpa [Scheme.substT, Scheme.inst, hid, ← hi] using hT

/-! ## Regions -/

/-- The fact a `scope` gives its body: what is outside outlives the region. -/
theorem scope_single {C₀ : Caps} (h : ¬ C₀ (.v 0)) :
    ∀ C s, Facts.single C₀ (.tvar 0) C s → s = .tvar 0 ∧ ¬ C (.v 0) := by
  rintro C s ⟨rfl, rfl⟩
  exact ⟨rfl, h⟩

/-- (Scope) with the type of its body written in the region's binder, `t.up 1` as `t'`. -/
theorem typ_scope {F : Facts} {φ' : Stage} {r ε : Row} {t t' T : Ty} {e : Tm} {φ : Stage}
    (hn : e.NoIns) (hF : ∀ C s, F C s → s = .tvar 0 ∧ ¬ C (.v 0))
    (he : Typ S (Ψ.map (Ty.up 1)) (Facts.union (Θ.substT (shiftV 1)) F)
      (.mono (.owner (.tvar 0)) :: Γ.map (Scheme.up 1)) e (.ex t' φ'))
    (ht : t.up 1 = t')
    (hle : φ'.le (.rt (Row.union (Row.scope (.tvar 0))
      (Row.union (r.substT (shiftV 1)) (Row.disch (.v 0) (r.substT (shiftV 1))))) (ε.substT (shiftV 1))))
    (hφ : (Stage.rt r ε).le φ) (hT : LeLive S t T) : Typ S Ψ Θ Γ (.scope e) (.ex T φ) :=
  .scope hn hF (ht ▸ he) hle hφ hT

end Flick.Cert
