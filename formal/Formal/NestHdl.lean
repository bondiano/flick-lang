import Formal.NestStep

/-!
# The steps of handlers keep `⊢ Λ`: the frames they move and the cells they add

A step of a handler replaces its redex by one with the same `inside` forms: a parked
body goes back under its handler, an arm waits beside the resumed body. A new cell of
a handler or of `resume` holds `lam todo`, which no term names as a location.
-/

namespace Flick

variable {S : Sig} {Ψ : List Ty} {B anc : Nat → Nat → Prop}

theorem Tm.Within.congrA {e : Tm} {A A' : Nat → Prop} (h : e.Within Ψ B anc A) (hA : ∀ x, A x ↔ A' x) :
    e.Within Ψ B anc A' := by
  have : A = A' := funext fun x => propext (hA x)
  exact this ▸ h

/-- A redex replaced by one with the same `inside` forms, at the place of its hole. -/
theorem Nest.redex' {c : Cfg} {home i E r r'} (hn : Nest Ψ c anc B home) (hi : c.at i = some (plug E r))
    (hE : OkE E) (hv : ¬ Value r)
    (hfr : r.Within Ψ B anc (holeE (baseOf anc i) E) → ∀ q, r'.Has (.frame q) ↔ r.Has (.frame q))
    (h' : r.Within Ψ B anc (holeE (baseOf anc i) E) → r'.Within Ψ B anc (holeE (baseOf anc i) E)) :
    Nest Ψ ((⟨c.rgs, c.mem, c.main⟩ : Cfg).put i (plug E r')) anc B home := by
  have hw := (hn.thread i _ hi).1
  obtain ⟨-, hrW⟩ := within_of_plug hv hE hw
  refine hn.put_local hi (fun _ _ h => h) (fun _ _ h => h)
    (hw.replug hv hE (h' hrW) fun x hx => .inl ((hfr hrW x).mp hx)) (fun q => ?_) hn.cells
  rw [plug_has_iff, plug_has_iff (x := r), hfr hrW]

/-- A new cell that holds what names no region and no location. -/
theorem Nest.extend_store {c : Cfg} {home} {t₀ : Ty} {v₀ : Tm} (hn : Nest Ψ c anc B home)
    (hμ : c.mem.length = Ψ.length) (hv₀ : v₀.NoIns) (hr₀ : ∀ p, ¬ v₀.Has (.rgn p))
    (hl₀ : ∀ l, ¬ v₀.Has (.loc l)) : Nest (Ψ ++ [t₀]) ⟨c.rgs, c.mem ++ [v₀], c.main⟩ anc B home := by
  have hl : ∀ A l, LocOk Ψ B A l → LocOk (Ψ ++ [t₀]) B A l := fun A l ⟨t, ht, hB⟩ =>
    ⟨t, by rw [List.getElem?_append_left (getElem?_lt ht)]; exact ht, hB⟩
  have hh : ∀ p ℓ, HCell Ψ p ℓ → HCell (Ψ ++ [t₀]) p ℓ := fun _ _ h => h.ext
  have := hn.put_local (i := none) (e := c.main) (e' := c.main) (μ' := c.mem ++ [v₀]) rfl hl hh
    (((hn.thread none c.main rfl).1).mono hl hh fun _ _ => rfl) (fun _ => Iff.rfl) fun l v hv => ?_
  · exact this
  by_cases hlt : l < c.mem.length
  · rw [List.getElem?_append_left hlt] at hv
    obtain ⟨h₁, h₂⟩ := hn.cells l v hv
    exact ⟨h₁.mono hl hh fun _ _ => rfl, h₂⟩
  · rw [List.getElem?_append_right (by omega)] at hv
    cases hj : l - c.mem.length with
    | zero =>
      simp [hj] at hv; subst hv
      exact ⟨Tm.Within.of_has hv₀ (fun p h => (hr₀ p h).elim) fun l h => (hl₀ l h).elim, hv₀⟩
    | succ => simp [hj] at hv

theorem lam_todo_sites : (Tm.lam .todo).NoIns ∧ (∀ p, ¬ (Tm.lam .todo).Has (.rgn p)) ∧
    ∀ l, ¬ (Tm.lam .todo).Has (.loc l) := by
  refine ⟨?_, ?_, ?_⟩ <;> simp [Tm.NoIns, Tm.Has]

/-- A value of a type without regions is at any place. -/
theorem Tm.Within.moveC {Θ : Facts} {v : Tm} {T : Ty} {φ : Stage} {A₁ A₂ : Nat → Prop}
    (hv : Value v) (ht : Typ S Ψ Θ [] v (.ex T φ)) (h : v.Within Ψ B anc A₁) (hT : T.YF 0) :
    v.Within Ψ B anc A₂ :=
  h.moveT hv ht fun x hx => absurd (hT.reach hx) (Nat.not_lt_zero _)

theorem Ty.closed_YF {T : Ty} (h : ∀ σ, T.substT σ = T) : T.YF 0 := h _

/-- The arm of an operation is at the place of the arms. -/
theorem Tm.Within.armOf {o : Name} {b : Tm} {A : Nat → Prop} :
    ∀ {al : Tm}, al.Within Ψ B anc A → al.armOf o = some b → b.Within Ψ B anc A
  | .acons _ b' r, h, ha => by
    simp only [Tm.armOf] at ha
    split at ha
    · cases ha; exact h.1
    · exact Tm.Within.armOf h.2 ha
  | .anil, _, ha | .var _, _, ha | .lam _, _, ha | .app _ _, _, ha | .let_ _ _, _, ha
  | .letrec _ _, _, ha | .fix _, _, ha | .lit _, _, ha | .loc _, _, ha | .const _, _, ha
  | .ctor _, _, ha | .rnil, _, ha | .rcons _ _ _, _, ha | .proj _ _, _, ha | .update _ _ _, _, ha
  | .match_ _ _, _, ha | .aguard _ _ _ _, _, ha | .todo, _, ha | .task _, _, ha | .await _, _, ha
  | .raise _, _, ha | .try_ _ _, _, ha | .handle _ _, _, ha | .provide _ _, _, ha
  | .with_ _ _ _, _, ha | .enil, _, ha | .econs _ _ _, _, ha | .bind _ _, _, ha | .join _ _, _, ha
  | .scope _, _, ha | .inside _ _, _, ha | .cap _, _, ha | .own _, _, ha | .drec _ _, _, ha
  | .glob _, _, ha | .hdl _ _ _ _, _, ha | .hwith _ _ _ _ _ _, _, ha | .hctl _ _ _ _ _ _ _ _ _, _, ha
  | .hres _ _ _ _, _, ha | .hdone _ _, _, ha | .unw _ _ _, _, ha | .kont _ _, _, ha
  | .hole _, _, ha => by simp [Tm.armOf] at ha

/-- An arm instantiated with values, at a place where both are. -/
theorem Tm.Within.ofInst {b : Tm} {δ : List Tm} {A : Nat → Prop} (hb : b.NoIns) (h : b.Within Ψ B anc A)
    (hδ : ∀ w ∈ δ, w.NoIns ∧ w.Within Ψ B anc A) : (b.subst (Flick.inst δ)).Within Ψ B anc A := by
  have hn : (b.subst (Flick.inst δ)).NoIns := fun hi => by
    rcases subst_inst_has hi with h' | ⟨w, hw, h'⟩
    · exact hb h'
    · exact (hδ w hw).1 h'
  refine Tm.Within.of_has hn (fun p hp => ?_) fun l hl => ?_
  · rcases subst_inst_has hp with h' | ⟨w, hw, h'⟩
    · exact h.rgn0 hb h'
    · exact (hδ w hw).2.rgn0 (hδ w hw).1 h'
  · rcases subst_inst_has hl with h' | ⟨w, hw, h'⟩
    · exact h.loc0 hb h'
    · exact (hδ w hw).2.loc0 (hδ w hw).1 h'

end Flick
