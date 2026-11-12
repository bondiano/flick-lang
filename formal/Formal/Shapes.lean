import Formal.VarySpine

/-!
# Kinds of types and shapes of values
-/

namespace Flick

variable {S : Sig}

/-- The outermost type former. -/
def Ty.kind : Ty → Nat
  | .tvar _ => 9
  | .base _ => 0
  | .arrow .. => 1
  | .ref _ => 2
  | .adt _ | .tapp .. => 3
  | .rnil | .rcons .. => 4
  | .eff .. => 5
  | .errors _ => 7
  | .hnd _ => 10
  | .owner _ => 11
  | .rg _ | .caps _ | .row _ | .bare => 12
  | .dict .. => 13
  | .all .. => 14

theorem Ty.tcon_kind' (T : Name) (us : List Ty) : (Ty.tcon T us).kind = 3 := by
  have : ∀ (us : List Ty) (f : Ty), f.kind = 3 → (us.foldl .tapp f).kind = 3 := by
    intro us; induction us with
    | nil => intro f h; exact h
    | cons x xs ih => intro f _; exact ih _ rfl
  exact this us _ rfl

theorem LeLive.kind {a b : Ty} (h : LeLive S a b) : a.kind = b.kind := by
  cases h with
  | refl | arrow | eff => rfl
  | hide _ _ _ _ ha hb => subst ha; subst hb; rw [Ty.tcon_kind', Ty.tcon_kind']
  | data _ _ _ _ _ ha hb => subst ha; subst hb; rw [Ty.tcon_kind', Ty.tcon_kind']

theorem Shp.le {a b : Ty} {e : Tm}
    (h : LeLive S a b) (hs : Shp S true a e) : Shp S true b e := by
  cases h with
  | refl => exact hs
  | arrow | eff => exact .hidden (by simp [Ty.Opaque, Ty.hd])
  | hide hi hl heq hsub ha hb =>
    cases hs with
    | hidden ho =>
      subst ha; subst hb
      exact .hidden (by rw [Ty.Opaque, Ty.tcon_hd] at ho ⊢; exact ho)
    | adt hU hUh hTh hB =>
      subst ha; subst hb
      exact .adt hU hUh (by rw [Ty.tcon_hd] at hTh ⊢; exact hTh)
        fun K hK => Sig.builds_vary (.of_hide hi hl heq hsub) (hB K hK)
    | lit | rnil | rcons =>
      have := congrArg Ty.kind ha
      rw [Ty.tcon_kind'] at this
      simp [Ty.kind] at this
  | data hl hr hp hn hi ha hb =>
    cases hs with
    | hidden ho =>
      subst ha; subst hb
      exact .hidden (by rw [Ty.Opaque, Ty.tcon_hd] at ho ⊢; exact ho)
    | adt hU hUh hTh hB =>
      subst ha; subst hb
      exact .adt hU hUh (by rw [Ty.tcon_hd] at hTh ⊢; exact hTh)
        fun K hK => Sig.builds_vary ⟨hl, hr, hp, hn, hi⟩ (hB K hK)
    | lit | rnil | rcons =>
      have := congrArg Ty.kind ha
      rw [Ty.tcon_kind'] at this
      simp [Ty.kind] at this

theorem Ty.opaque_of_kind {t : Ty} (h : t.kind = 1) : t.Opaque := by
  cases t <;> simp [Ty.kind] at h <;> simp [Ty.Opaque, Ty.hd]

theorem Ty.tcon_kind (T : Name) (us : List Ty) : (Ty.tcon T us).kind = 3 := by
  have : ∀ (us : List Ty) (f : Ty), f.kind = 3 → (us.foldl .tapp f).kind = 3 := by
    intro us; induction us with
    | nil => intro f h; exact h
    | cons x xs ih => intro f _; exact ih _ rfl
  exact this us _ rfl

theorem Ty.arrows_kind (ts : List Ty) (T : Name) (us : List Ty) :
    (Ty.arrows ts (.tcon T us)).kind = 1 ∨ (Ty.arrows ts (.tcon T us)).kind = 3 := by
  cases ts with
  | nil => exact .inr (Ty.tcon_kind T us)
  | cons => exact .inl rfl

theorem Ty.kind_of_hd {U : Ty} {E : Name} (h : U.hd = .adt E) : U.kind = 3 := by
  cases U <;> simp [Ty.hd, Ty.kind] at h ⊢

theorem Shp.ctor_instR {ts : List Ty} {T₀ : Name} {rs us : List Ty} :
    (Ty.arrows ts (.tcon T₀ rs)).substT (instT us) = Sig.ctorTyR ts T₀ rs us := by
  simp only [Ty.arrows_substT, Ty.tcon_substT, Sig.ctorTyR]

theorem Shp.ctor_inst {k ts T₀} {us : List Ty} (hk : us.length = k) :
    (Ty.arrows ts (.tcon T₀ (idTs k))).substT (instT us) = Sig.ctorTy ts T₀ us := by
  have : (idTs k).map (Ty.substT (instT us)) = us := by
    apply List.ext_getElem (by simp [idTs, hk])
    intro i h₁ h₂
    simp [idTs, Ty.substT, instT, h₂]
  simp only [Ty.arrows_substT, Ty.tcon_substT, Sig.ctorTy, this]

theorem Const.ty_kind (c : Const) (ι : CArg) : (c.ty S ι).kind = 1 := by
  obtain ⟨_, _, _, _, h⟩ := Const.ty_isArrow c ι S
  rw [h]; rfl

/-- The result of a binary constant after one argument. -/
theorem Const.ty_res {c : Const} {ι : CArg} (h : c.arity = 2) :
    ∃ x y, c.ty S ι = .arrow x .pure y Caps.none ∧ y.kind = 1 := by
  cases c <;> simp [Const.arity] at h <;> exact ⟨_, _, rfl, rfl⟩

theorem Const.res_kind {c : Const} {ι a ψ t C T'} (hc : c.arity = 2)
    (h : LeLive S (c.ty S ι) (.arrow a ψ t C)) (hT : LeLive S t T') : T'.kind = 1 := by
  obtain ⟨x, y, hxy, hk⟩ := Const.ty_res (S := S) (ι := ι) hc
  rw [hxy] at h
  obtain ⟨_, _, _, _, h', _, _, lt, -⟩ := h.arrow_left
  cases h'; rw [← hT.kind, ← lt.kind, hk]

theorem Typ.constApp_kind {Ψ Θ Γ c w T φ} (h : Typ S Ψ Θ Γ (.app (.const c) w) (.ex T φ))
    (hc : c.arity = 2) (hA : ¬ T.isAll) : T.kind = 1 := by
  cases h with
  | allI => exact (hA trivial).elim
  | app hf _ _ hT => cases hf with
    | const _ hT' => exact Const.res_kind hc hT' hT
    | perform => simp [Const.arity] at hc
  | errVal _ h' hU _ =>
    exfalso
    cases h' with
    | app hf _ _ hT =>
      cases hf with
      | const _ hT' =>
        have := Const.res_kind hc hT' hT
        rw [Ty.kind_of_hd hU] at this
        cases this
      | perform => simp [Const.arity] at hc
    | errVal _ _ _ _ => simp [Ty.hd] at hU
    | allI => simp [Ty.hd] at hU

theorem LeLive.opaque {a b : Ty} (h : LeLive S a b) (ha : a.Opaque) : b.Opaque := by
  cases h with
  | refl => exact ha
  | arrow => simp [Ty.Opaque, Ty.hd]
  | eff => simp [Ty.Opaque, Ty.hd]
  | hide _ _ _ _ ha' hb => subst ha'; subst hb; rw [Ty.Opaque, Ty.tcon_hd] at ha ⊢; exact ha
  | data _ _ _ _ _ ha' hb => subst ha'; subst hb; rw [Ty.Opaque, Ty.tcon_hd] at ha ⊢; exact ha

theorem Shp.false_arrows {U e} (h : Shp S false U e) :
    ∃ K k ts T ts', e.head = .ctor K ∧ S.ctor K = some (k, ts, T) ∧
      U = .arrows ts' (.tcon T (S.res K)) := by
  generalize hf : false = fl at h
  induction h with
  | head hK => exact ⟨_, _, _, _, _, rfl, hK, rfl⟩
  | app _ _ ih _ =>
    obtain ⟨K, k, ts, T, ts', hh, hK, hU⟩ := ih hf
    cases ts' with
    | nil => exact absurd hU.symm (Ty.tcon_not_arrow _ _ _ _ _ _)
    | cons a ts' => cases hU; exact ⟨K, k, ts, T, ts', hh, hK, rfl⟩
  | _ => cases hf

theorem Sig.res_congr {K k ts T} (hK : S.ctor K = some (k, ts, T)) {ρ ρ' : Atom → Ty}
    (h : ∀ i, i < k → ρ (.v i) = ρ' (.v i)) :
    (S.res K).map (Ty.substT ρ) = (S.res K).map (Ty.substT ρ') := by
  let σ : Atom → Ty := fun a => match a with
    | .v i => if i < k then .tvar i else .rnil
    | .r _ => .rnil
  have hσ := S.res_closed K k ts T σ hK fun i hi => by simp [σ, hi]
  rw [← hσ]; simp only [List.map_map, Function.comp_def, Ty.substT_comp]
  congr 1; funext x; apply Ty.substT_congr; intro a
  cases a with
  | v i => by_cases hi : i < k <;> simp [σ, hi, Ty.substT, h i]
  | r p => simp [σ, Ty.substT]

/-- A constructor builds each of its instances. -/
theorem Sig.builds_inst {K k ts T} (hK : S.ctor K = some (k, ts, T)) (us : List Ty) :
    S.Builds K (Ty.tcon T ((S.res K).map (Ty.substT (instT us)))) := by
  intro k' ts' T' _ hK' _ _
  rw [hK] at hK'; cases hK'
  have hl : ((List.range k).map fun i => instT us (.v i)).length = k := by simp
  have hres : (S.res K).map (Ty.substT (instT ((List.range k).map fun i => instT us (.v i)))) =
      (S.res K).map (Ty.substT (instT us)) :=
    Sig.res_congr hK fun i hi => by simp [instT, hi]
  refine ⟨instT ((List.range k).map fun i => instT us (.v i)), ?_⟩
  rw [Ty.up_substT _ k _ hl, Ty.tcon_substT, hres]

theorem Shp.full_of {U T e} {us : List Ty} (h : Shp S false U e) (hl : LeLive S (U.substT (instT us)) T) :
    Shp S true T e := by
  obtain ⟨K, _, _, T₀, ts', hh, hK, rfl⟩ := h.false_arrows
  cases ts' with
  | nil =>
    simp only [Ty.arrows, Ty.tcon_substT] at hl
    have hB := Sig.builds_inst hK us
    have key : ∀ X, LeLive S X T →
        X = Ty.tcon T₀ ((S.res K).map (Ty.substT (instT us))) → Shp S true T e := by
      intro X hl hX
      cases hl with
      | refl =>
        subst hX
        refine .adt h (Ty.tcon_hd _ _) (Ty.tcon_hd _ _) fun K' hh' => ?_
        rw [hh] at hh'; cases hh'
        exact hB
      | arrow => exact absurd hX Ty.arrow_ne_tcon
      | eff => exact absurd hX Ty.eff_ne_tcon
      | hide hi hlen heq hsub ha hb =>
        obtain ⟨rfl, rfl⟩ := Ty.tcon_eq (ha.symm.trans hX)
        subst hb
        refine .adt h (Ty.tcon_hd _ _) (Ty.tcon_hd _ _) fun K' hh' => ?_
        rw [hh] at hh'; cases hh'
        exact Sig.builds_vary (.of_hide hi hlen heq hsub) hB
      | data hlen hr hp hn hi ha hb =>
        obtain ⟨rfl, rfl⟩ := Ty.tcon_eq (ha.symm.trans hX)
        subst hb
        refine .adt h (Ty.tcon_hd _ _) (Ty.tcon_hd _ _) fun K' hh' => ?_
        rw [hh] at hh'; cases hh'
        exact Sig.builds_vary ⟨hlen, hr, hp, hn, hi⟩ hB
    exact key _ hl rfl
  | cons _ _ => exact .hidden (hl.opaque (by simp [Ty.arrows, Ty.substT, Ty.Opaque, Ty.hd]))

/-- The spine clause of `Typ.shape`: a constructor spine has the shape of its
declaration, at an instance below its type, or is an error at `Errors ε`. -/
def SpineShp (S : Sig) (Ψ : List Ty) (Θ : Facts) (Γ : List Scheme) (e : Tm) (T : Ty)
    (φ : Stage) : Prop :=
  (∃ U us, LeLive S (U.substT (instT us)) T ∧ Shp S false U e) ∨
    ∃ ε E U, T = .errors ε ∧ ε (.nm E) ∧ U.hd = .adt E ∧ Shp S true U e ∧
      Typ S Ψ Θ Γ e (.ex U φ) ∧ Row.Sub (Row.err E U) ε

theorem Typ.shape {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) : Value e → ∀ {T φ}, J = .ex T φ →
    Shp S true T e ∧ (∀ K, e.head = .ctor K → ¬ T.isAll → SpineShp S Ψ Θ Γ e T φ) := by
  induction h with
  | lam _ _ hT | fix _ _ hT | task _ _ _ hT | perform _ hT | kont _ _ hT =>
    intro _ _ _ hJ; cases hJ
    exact ⟨.hidden (hT.opaque (by simp [Ty.Opaque, Ty.hd])), by simp [Tm.head]⟩
  | const _ hT =>
    intro _ _ _ hJ; cases hJ
    exact ⟨.hidden (Ty.opaque_of_kind (by rw [← hT.kind]; exact Const.ty_kind _ _)),
      by simp [Tm.head]⟩
  | loc | cap | own | drec =>
    intro _ _ _ hJ; cases hJ; exact ⟨.hidden (by simp [Ty.Opaque, Ty.hd]), by simp [Tm.head]⟩
  | lit => intro _ _ _ hJ; cases hJ; exact ⟨.lit, by simp [Tm.head]⟩
  | rnil => intro _ _ _ hJ; cases hJ; exact ⟨.rnil, by simp [Tm.head]⟩
  | rcons _ _ _ _ ih₁ ih₂ =>
    intro hv _ _ hJ; cases hJ
    cases hv with
    | rcons hv₁ hv₂ => exact ⟨.rcons (ih₁ hv₁ rfl).1 (ih₂ hv₂ rfl).1, by simp [Tm.head]⟩
  | ctor hK hk _ hT =>
    intro _ _ _ hJ; cases hJ
    have hh := Shp.head (S := S) hK
    exact ⟨hh.full_of (by rw [Shp.ctor_instR]; exact hT),
      fun _ _ _ => .inl ⟨_, _, by rw [Shp.ctor_instR]; exact hT, hh⟩⟩
  | allI =>
    intro _ _ _ hJ; cases hJ
    exact ⟨.hidden (by simp [Ty.Opaque, Ty.hd]), fun _ _ h => (h trivial).elim⟩
  | errVal hv' h' hU hE ih =>
    intro _ _ _ hJ; cases hJ
    refine ⟨.hidden (by simp [Ty.Opaque, Ty.hd]),
      fun K hK _ => .inr ⟨_, _, _, rfl, hE _ (.inl rfl), hU, (ih hv' rfl).1, h', hE⟩⟩
  | app hf _ _ hT ih₁ ih₂ =>
    intro hv _ _ hJ; cases hJ
    cases hv with
    | ctorApp hvf hK hve =>
      rcases (ih₁ hvf rfl).2 _ hK (by simp [Ty.isAll]) with ⟨U, us, hU, hs⟩ | ⟨_, _, _, h, _⟩
      · obtain ⟨_, _, _, T₀, ts', _, _, hU'⟩ := hs.false_arrows
        subst hU'
        cases ts' with
        | nil =>
          simp only [Ty.arrows, Ty.tcon_substT] at hU
          have := hU.kind
          rw [Ty.tcon_kind] at this; cases this
        | cons g ts'' =>
          simp only [Ty.arrows, Ty.substT, Stage.substT, Caps.none_substT] at hU
          obtain ⟨_, _, _, _, he, la, _, lt, -⟩ := hU.arrow_left
          cases he
          have hfield : Shp S true g _ := Shp.of_substT (Shp.le la (ih₂ hve rfl).1)
          have hs' : Shp S false _ _ := .app hs hfield
          refine ⟨hs'.full_of (us := us) (lt.trans hT), fun _ _ _ => .inl ⟨_, us, lt.trans hT, hs'⟩⟩
      · cases h
    | constApp hc _ =>
      refine ⟨.hidden (Ty.opaque_of_kind ?_), by simp [Tm.head]⟩
      cases hf with
      | const _ hT' => exact Const.res_kind hc hT' hT
      | perform => simp [Const.arity] at hc
  | _ => intro hv _ _ hJ; cases hJ <;> cases hv

end Flick
