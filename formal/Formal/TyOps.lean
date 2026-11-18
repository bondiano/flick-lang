import Formal.TyComp

/-!
# Substitution of variables and regions: rows, shifting, constructors
-/

namespace Flick

theorem Caps.substT_mono {C C' : Caps} (σ : Atom → Ty) (h : ∀ a, C a → C' a) :
    ∀ a, C.substT σ a → C'.substT σ a :=
  fun _ ⟨b, hb, h'⟩ => ⟨b, h b hb, h'⟩

theorem Row.Sub.substT {r r' : Row} (σ : Atom → Ty) (h : Row.Sub r r') :
    Row.Sub (r.substT σ) (r'.substT σ) := by
  intro x hx
  cases x with
  | nm | atm | rv | ob | obv =>
    simp only [Row.substT] at hx ⊢
    grind [Row.Sub]

theorem Row.le_nameClose (r : Row) : Row.Sub r r.nameClose := by
  intro x hx
  cases x with
  | nm n c => exact ⟨c, hx⟩
  | _ => exact hx

/-- Every instance of a name stays every instance of it under a substitution. -/
theorem Row.nameClose_substT (σ : Atom → Ty) (r : Row) :
    Row.Sub (r.nameClose.substT σ) (r.substT σ).nameClose := by
  intro x hx
  cases x with
  | nm n c =>
    rcases hx with ⟨c₀, ⟨c', h⟩, -⟩ | ⟨b, G, hb, hG, h⟩
    · exact ⟨_, .inl ⟨c', h, rfl⟩⟩
    · exact ⟨c, .inr ⟨b, G, hb, hG, h⟩⟩
  | _ => exact hx

@[simp] theorem Row.nameClose_single (E : Name) (n : Name) (c : TyClass) :
    (Row.single E).nameClose (.nm n c) ↔ n = E := by
  simp only [Row.nameClose, Row.single, Row.inst]
  constructor
  · rintro ⟨_, h⟩; cases h; rfl
  · rintro rfl; exact ⟨_, rfl⟩

theorem Stage.le_substT {φ φ' : Stage} (σ : Atom → Ty) (h : φ.le φ') :
    (φ.substT σ).le (φ'.substT σ) := by
  cases φ <;> cases φ' <;> simp_all [Stage.le, Stage.substT]
  exact ⟨h.1.substT σ, h.2.substT σ⟩

/-- A substitution keeps what the names of `H` give back to a row: a name it brings in
through a variable taken without it is one `H` names, at some argument. -/
theorem Row.back_substT (σ : Atom → Ty) (r H : Row) :
    Row.Sub ((r.back H).substT σ)
      (Row.union (H.substT σ).nameClose ((r.substT σ).back (H.substT σ))) := by
  intro x hx
  cases x with
  | nm n c =>
    rcases hx with hx | ⟨b, G', ⟨G, hb, -, hback⟩, hG', h⟩
    · exact .inr (.inl hx)
    · by_cases hG : G n
      · rcases hback n hG with h' | ⟨c', h'⟩
        · exact (hG' h').elim
        · exact .inl ⟨_, .inl ⟨c', h', rfl⟩⟩
      · exact .inr (.inr ⟨b, G, hb, hG, h⟩)
  | atm a =>
    rcases hx with ⟨b, hb, h⟩ | ⟨b, G', ⟨G, hb, -, -⟩, h⟩
    · exact .inr (.inl ⟨b, hb, h⟩)
    · exact .inr (.inr ⟨b, G, hb, h⟩)
  | ob N c a => exact .inr hx
  | obv v K a => exact .inr hx
  | rv c K =>
    obtain ⟨b, G', ⟨G, hb, hsub, hback⟩, K', hc, rfl⟩ := hx
    refine .inr ⟨fun n => G n ∨ K' n, ⟨b, G, hb, K', hc, rfl⟩, fun n hn => hn.imp (hsub n) id,
      fun n hn => ?_⟩
    rcases hn with hn | hn
    · exact (hback n hn).imp .inl fun ⟨c', h'⟩ => ⟨_, .inl ⟨c', h', rfl⟩⟩
    · exact .inl (.inr hn)

theorem Row.Sub.back_substT {ε₀ H ε' : Row} (σ : Atom → Ty)
    (h : Row.Sub ε₀ (Row.union H.nameClose (ε'.back H))) :
    Row.Sub (ε₀.substT σ)
      (Row.union (H.substT σ).nameClose ((ε'.substT σ).back (H.substT σ))) := by
  intro x hx
  have := h.substT σ x hx
  rw [Row.union_substT] at this
  exact this.elim (fun h => .inl (Row.nameClose_substT σ H x h)) (Row.back_substT σ ε' H x)

/-- A substitution keeps what the body of a handler of `E` may ask of its surroundings. -/
theorem Stage.le_back_substT {φ₀ : Stage} {E : Name} {r ε : Row} (σ : Atom → Ty)
    (h : φ₀.le (.rt (Row.union (Row.single E).nameClose (r.back (Row.single E))) ε)) :
    (φ₀.substT σ).le
      (.rt (Row.union (Row.single E).nameClose ((r.substT σ).back (Row.single E)))
        (ε.substT σ)) := by
  cases φ₀ with
  | pure => trivial
  | rt r₀ e₀ =>
    refine ⟨?_, h.2.substT σ⟩
    have := Row.Sub.back_substT σ h.1
    rwa [Row.single_substT] at this

/-- A row gives back at least itself. -/
theorem Row.le_back (r H : Row) : Row.Sub r (r.back H) := by
  intro x hx
  cases x with
  | rv a G => exact ⟨G, hx, fun _ h => h, fun _ h => .inl h⟩
  | _ => exact hx

/-- What a row gives back is in it up to the names its variables are taken without. -/
theorem Row.back_modNames {H r : Row} {x : Elem} (h : r.back H x) : r.modNames x := by
  cases x with
  | rv a G => obtain ⟨G', h, -⟩ := h; exact ⟨G', h⟩
  | _ => exact h

theorem Row.le_modNames {r : Row} {x : Elem} (h : r x) : r.modNames x := by
  cases x with
  | rv a G => exact ⟨G, h⟩
  | _ => exact h

theorem Row.modNames_mono {r r' : Row} (h : Row.Sub r r') : Row.Sub r.modNames r'.modNames := by
  intro x hx
  cases x with
  | rv a G => obtain ⟨G', hx⟩ := hx; exact ⟨G', h _ hx⟩
  | _ => exact h _ hx

theorem Row.modNames_union {r r' : Row} {x : Elem} (h : (Row.union r r').modNames x) :
    r.modNames x ∨ r'.modNames x := by
  cases x with
  | rv a G => obtain ⟨G', h⟩ := h; exact h.imp (fun h => ⟨G', h⟩) fun h => ⟨G', h⟩
  | _ => exact h

theorem Row.modNames_idem {r : Row} {x : Elem} (h : r.modNames.modNames x) : r.modNames x := by
  cases x with
  | rv a G => obtain ⟨_, G', h⟩ := h; exact ⟨G', h⟩
  | _ => exact h

/-- A substitution keeps a row up to the names its variables are taken without, but for
the names it brings in. -/
theorem Row.modNames_substT (σ : Atom → Ty) (r : Row) (x : Elem) (hx : r.modNames.substT σ x) :
    (∃ N c, x = .nm N c) ∨ (r.substT σ).modNames x := by
  cases x with
  | nm N c => exact .inl ⟨N, c, rfl⟩
  | atm a =>
    rcases hx with ⟨b, hb, h⟩ | ⟨b, G, ⟨G₂, hb⟩, h⟩
    · exact .inr (.inl ⟨b, hb, h⟩)
    · exact .inr (.inr ⟨b, G₂, hb, h⟩)
  | ob N c a => exact .inr hx
  | obv v K a => exact .inr hx
  | rv c K =>
    obtain ⟨b, G, ⟨G₂, hb⟩, K', hc, -⟩ := hx
    exact .inr ⟨_, b, G₂, hb, K', hc, rfl⟩

@[simp] theorem Row.scope_substT (σ : Atom → Ty) (s : Ty) :
    (Row.scope s).substT σ = Row.scope (s.substT σ) := by
  funext x; apply propext
  cases x <;> simp [Row.substT, Row.scope, Row.ofCaps, Ty.reach_substT]

@[simp] theorem Ty.up_zero (t : Ty) : t.up 0 = t := by
  have : shiftV 0 = Ty.ident := by funext a; cases a <;> rfl
  simp only [Ty.up_def, this]; exact t.substT_ident

@[simp] theorem List.map_up_zero (Ψ : List Ty) : Ψ.map (Ty.up 0) = Ψ := by
  induction Ψ <;> simp_all

@[simp] theorem liftT_zero (σ : Atom → Ty) : liftT 0 σ = σ := by
  funext a; cases a <;> simp [liftT]

theorem Ty.up_substT (t : Ty) (k : Nat) (ts : List Ty) (h : ts.length = k) :
    (t.up k).substT (instT ts) = t := by
  simp only [Ty.up_def, Ty.substT_comp]
  conv => rhs; rw [← t.substT_ident]
  apply Ty.substT_congr
  intro a
  cases a with
  | v i =>
    have : ¬ i + k < k := by omega
    simp [shiftV, Ty.substT, instT, this, h, Ty.ident]
  | r p => simp [shiftV, Ty.substT, instT, Ty.ident]

theorem Ty.inst_substT (t : Ty) (σ : Atom → Ty) (ts : List Ty) :
    (t.substT (liftT ts.length σ)).substT (instT (ts.map (Ty.substT σ))) =
      (t.substT (instT ts)).substT σ := by
  simp only [Ty.substT_comp]
  apply Ty.substT_congr
  intro a
  cases a with
  | v i =>
    by_cases hi : i < ts.length
    · simp [hi, Ty.substT, instT, liftT]
    · simp only [liftT, instT, hi, if_false]
      exact Ty.up_substT _ _ _ (by simp)
  | r p => simp only [liftT, instT, Ty.substT]; exact Ty.up_substT _ _ _ (by simp)

theorem Ty.up_up (t : Ty) (a b : Nat) : (t.up a).up b = t.up (a + b) := by
  simp only [Ty.up_def, Ty.substT_comp]
  apply Ty.substT_congr
  intro x
  cases x <;> simp [shiftV, Ty.substT, Nat.add_assoc]

theorem Ty.arrows_substT (ts : List Ty) (r : Ty) (σ : Atom → Ty) :
    (Ty.arrows ts r).substT σ = Ty.arrows (ts.map (Ty.substT σ)) (r.substT σ) := by
  induction ts <;> simp_all [Ty.arrows]

theorem Ty.foldl_tapp_substT (σ : Atom → Ty) :
    ∀ (us : List Ty) (f : Ty), (us.foldl .tapp f).substT σ = (us.map (Ty.substT σ)).foldl .tapp (f.substT σ)
  | [], _ => rfl
  | u :: us, f => by simp [Ty.foldl_tapp_substT σ us, Ty.substT]

theorem Ty.tcon_substT (T : Name) (σ : Atom → Ty) (us : List Ty) :
    (Ty.tcon T us).substT σ = Ty.tcon T (us.map (Ty.substT σ)) := by
  simp [Ty.tcon, Ty.foldl_tapp_substT, Ty.substT]

/-- A record type: a spine of fields ending in `rnil`. -/
def Ty.IsRec : Ty → Prop
  | .rnil => True
  | .rcons _ _ r => r.IsRec
  | _ => False

theorem Ty.labels_substT {t : Ty} (σ : Atom → Ty) (h : t.IsRec) :
    (t.substT σ).labels = t.labels := by
  induction t <;> simp_all [Ty.labels, Ty.substT, Ty.IsRec]

theorem Ty.IsRec.substT {t : Ty} (σ : Atom → Ty) (h : t.IsRec) : (t.substT σ).IsRec := by
  induction t <;> simp_all [Ty.substT, Ty.IsRec]

theorem Ty.field_substT {R : Ty} {l t} (σ : Atom → Ty) (h : R.field l = some t) :
    (R.substT σ).field l = some (t.substT σ) := by
  induction R <;> simp_all [Ty.field, Ty.substT]
  split <;> simp_all

/-- A constructor's type at the instance `us` of its parameters. -/
def Sig.ctorTy (ts : List Ty) (T : Name) (us : List Ty) : Ty :=
  Ty.arrows (ts.map (Ty.substT (instT us))) (Ty.tcon T us)

/-- The type of a constructor that builds `T rs`, at the instance `us` of its
variables. -/
def Sig.ctorTyR (ts : List Ty) (T : Name) (rs us : List Ty) : Ty :=
  Ty.arrows (ts.map (Ty.substT (instT us))) (Ty.tcon T (rs.map (Ty.substT (instT us))))

theorem idTs_inst {k : Nat} {us : List Ty} (hk : us.length = k) :
    (idTs k).map (Ty.substT (instT us)) = us := by
  apply List.ext_getElem (by simp [idTs, hk])
  intro i h₁ h₂
  simp [idTs, Ty.substT, instT, h₂]

/-- An ordinary constructor's type: it builds `T us`. -/
theorem Sig.ctorTyR_plain {ts : List Ty} {T : Name} {k : Nat} {rs us : List Ty}
    (hres : rs = idTs k) (hk : us.length = k) : Sig.ctorTyR ts T rs us = Sig.ctorTy ts T us := by
  simp only [Sig.ctorTyR, Sig.ctorTy, hres, idTs_inst hk]

theorem Sig.fields_inst_substT (S : Sig) {K k ts T} (σ : Atom → Ty) (us : List Ty)
    (hk : us.length = k) (h : S.ctor K = some (k, ts, T)) :
    (ts.map (Ty.substT (instT us))).map (Ty.substT σ) =
      ts.map (Ty.substT (instT (us.map (Ty.substT σ)))) := by
  have hc := S.ctor_closed K k ts T (liftT k σ) h (fun i hi => by simp [liftT, hi])
  conv => rhs; rw [← hc]
  simp only [List.map_map]
  apply List.map_congr_left
  intro t _
  simp only [Function.comp]
  have := Ty.inst_substT t σ us
  rw [hk] at this
  rw [this]

theorem Sig.ctorTy_substT (S : Sig) {K k ts T} (σ : Atom → Ty) (us : List Ty)
    (hk : us.length = k) (h : S.ctor K = some (k, ts, T)) :
    (Sig.ctorTy ts T us).substT σ = Sig.ctorTy ts T (us.map (Ty.substT σ)) := by
  simp only [Sig.ctorTy, Ty.arrows_substT, Ty.tcon_substT, S.fields_inst_substT σ us hk h]

theorem Sig.res_inst_substT (S : Sig) {K k ts T} (σ : Atom → Ty) (us : List Ty)
    (hk : us.length = k) (h : S.ctor K = some (k, ts, T)) :
    ((S.res K).map (Ty.substT (instT us))).map (Ty.substT σ) =
      (S.res K).map (Ty.substT (instT (us.map (Ty.substT σ)))) := by
  have hc := S.res_closed K k ts T (liftT k σ) h (fun i hi => by simp [liftT, hi])
  conv => rhs; rw [← hc]
  simp only [List.map_map]
  apply List.map_congr_left
  intro t _
  simp only [Function.comp]
  have := Ty.inst_substT t σ us
  rw [hk] at this
  rw [this]

theorem Sig.ctorTyR_substT (S : Sig) {K k ts T} (σ : Atom → Ty) (us : List Ty)
    (hk : us.length = k) (h : S.ctor K = some (k, ts, T)) :
    (Sig.ctorTyR ts T (S.res K) us).substT σ = Sig.ctorTyR ts T (S.res K) (us.map (Ty.substT σ)) := by
  simp only [Sig.ctorTyR, Ty.arrows_substT, Ty.tcon_substT, S.fields_inst_substT σ us hk h,
    S.res_inst_substT σ us hk h]

def CArg.substT (σ : Atom → Ty) (ι : CArg) : CArg :=
  ⟨ι.t.substT σ, ι.s.substT σ, ι.C.substT σ, ι.r.substT σ, ι.e.substT σ⟩

theorem Ty.reach_caps (s : Ty) (σ : Atom → Ty) : s.reach.substT σ = (s.substT σ).reach := by
  funext a; apply propext; simp [Caps.substT, Ty.reach_substT]

/-- The names of a row without row variables come from its own under a substitution. -/
theorem Row.Named.substT_nm {r : Row} (h : r.Named) (σ : Atom → Ty) (N : Name) (c : TyClass) :
    r.substT σ (.nm N c) ↔ ∃ c₀, r (.nm N c₀) ∧ c₀.substT σ = c := by
  simp only [Row.substT]; exact ⟨fun h' => h'.elim id fun ⟨b, G, hb, _⟩ => (h b G hb).elim, .inl⟩

theorem Row.Named.substT {r : Row} (h : r.Named) (σ : Atom → Ty) : (r.substT σ).Named := by
  rintro a H ⟨b, G, hb, _⟩; exact h b G hb

theorem Row.obs_substT (σ : Atom → Ty) (r : Row) (s : Ty) :
    (r.obs s).substT σ = (r.substT σ).obs (s.substT σ) := by
  funext x; apply propext
  cases x with
  | ob | obv =>
    simp only [Row.substT, Row.obs, Ty.reach_substT]
    grind
  | _ => simp [Row.substT, Row.obs]

/-- What a stage term stands for commutes with a substitution of an arrow's stage. -/
theorem Ty.asStage_substS (σ : Atom → Ty) (φ : Ty) : (φ.substS σ).asStage = φ.asStage.substT σ := by
  cases φ <;> simp [Ty.substS, Ty.isStage, Ty.asStage, Stage.substT, Ty.asRow_substT,
    Row.obs_substT, Ty.substT]

theorem Ty.asStage_substT {φ : Ty} (σ : Atom → Ty) (h : φ.isStage = true) :
    (φ.substT σ).asStage = φ.asStage.substT σ := by
  have := Ty.asStage_substS σ φ; simp only [Ty.substS, h, if_true] at this; exact this

/-- What a substituted row term stands for, read from right to left: the form the typing
rules write after a substitution. -/
theorem Row.substT_asRow (σ : Atom → Ty) (t : Ty) : t.asRow.substT σ = (t.substT σ).asRow :=
  (Ty.asRow_substT σ t).symm

/-- A substitution under one binder commutes with the shift past it. -/
theorem Ty.substT_shift_comm (t : Ty) (k : Nat) (σ : Atom → Ty) :
    (t.substT (shiftV k)).substT (liftT k σ) = (t.substT σ).substT (shiftV k) := by
  rw [← Ty.up_def, ← Ty.up_def, Ty.up_comm]

@[simp] theorem Stage.substT_rt (σ : Atom → Ty) (r e : Row) :
    (Stage.rt r e).substT σ = .rt (r.substT σ) (e.substT σ) := rfl

@[simp] theorem Stage.substT_pure (σ : Atom → Ty) : Stage.pure.substT σ = .pure := rfl

theorem Stage.substT_asStage (σ : Atom → Ty) (φ : Ty) : φ.asStage.substT σ = (φ.substS σ).asStage :=
  (Ty.asStage_substS σ φ).symm

/-- What the rows of an `Eff` stand for, as the stage of an `await`. -/
theorem Ty.asStage_rt_substT (σ : Atom → Ty) (r e : Ty) :
    (Stage.rt (r.substT σ).asRow (e.substT σ).asRow) = (Stage.rt r.asRow e.asRow).substT σ := by
  simp [Stage.substT, Ty.asRow_substT]

end Flick
