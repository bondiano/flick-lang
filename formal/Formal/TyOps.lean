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

theorem Stage.le_substT {φ φ' : Stage} (σ : Atom → Ty) (h : φ.le φ') :
    (φ.substT σ).le (φ'.substT σ) := by
  cases φ <;> cases φ' <;> simp_all [Stage.le, Stage.substT]
  exact ⟨h.1.substT σ, h.2.substT σ⟩

/-- A substitution keeps what the names of `H` give back to a row: a name it brings in
through a variable taken without it is one `H` names. -/
theorem Row.back_substT (σ : Atom → Ty) (r H : Row) :
    Row.Sub ((r.back H).substT σ) (Row.union (H.substT σ) ((r.substT σ).back (H.substT σ))) := by
  intro x hx
  cases x with
  | nm n =>
    rcases hx with hx | ⟨b, G', ⟨G, hb, -, hback⟩, hG', h⟩
    · exact .inr (.inl hx)
    · by_cases hG : G n
      · rcases hback n hG with h' | h'
        · exact (hG' h').elim
        · exact .inl (.inl h')
      · exact .inr (.inr ⟨b, G, hb, hG, h⟩)
  | atm a =>
    rcases hx with ⟨b, hb, h⟩ | ⟨b, G', ⟨G, hb, -, -⟩, h⟩
    · exact .inr (.inl ⟨b, hb, h⟩)
    · exact .inr (.inr ⟨b, G, hb, h⟩)
  | ob N a => exact .inr hx
  | obv v K a => exact .inr hx
  | rv c K =>
    obtain ⟨b, G', ⟨G, hb, hsub, hback⟩, K', hc, rfl⟩ := hx
    refine .inr ⟨fun n => G n ∨ K' n, ⟨b, G, hb, K', hc, rfl⟩, fun n hn => hn.imp (hsub n) id,
      fun n hn => ?_⟩
    rcases hn with hn | hn
    · exact (hback n hn).imp .inl .inl
    · exact .inl (.inr hn)

theorem Row.Sub.back_substT {ε₀ H ε' : Row} (σ : Atom → Ty)
    (h : Row.Sub ε₀ (Row.union H (ε'.back H))) :
    Row.Sub (ε₀.substT σ) (Row.union (H.substT σ) ((ε'.substT σ).back (H.substT σ))) := by
  intro x hx
  have := h.substT σ x hx
  rw [Row.union_substT] at this
  exact this.elim .inl (Row.back_substT σ ε' H x)

/-- A substitution keeps what the body of a handler of `E` may ask of its surroundings. -/
theorem Stage.le_back_substT {φ₀ : Stage} {E : Name} {r ε : Row} (σ : Atom → Ty)
    (h : φ₀.le (.rt (Row.union (Row.single E) (r.back (Row.single E))) ε)) :
    (φ₀.substT σ).le
      (.rt (Row.union (Row.single E) ((r.substT σ).back (Row.single E))) (ε.substT σ)) := by
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
    (∃ N, x = .nm N) ∨ (r.substT σ).modNames x := by
  cases x with
  | nm N => exact .inl ⟨N, rfl⟩
  | atm a =>
    rcases hx with ⟨b, hb, h⟩ | ⟨b, G, ⟨G₂, hb⟩, h⟩
    · exact .inr (.inl ⟨b, hb, h⟩)
    · exact .inr (.inr ⟨b, G₂, hb, h⟩)
  | ob N a => exact .inr hx
  | obv v K a => exact .inr hx
  | rv c K =>
    obtain ⟨b, G, ⟨G₂, hb⟩, K', hc, -⟩ := hx
    exact .inr ⟨_, b, G₂, hb, K', hc, rfl⟩

@[simp] theorem Row.scope_substT (σ : Atom → Ty) (s : Ty) :
    (Row.scope s).substT σ = Row.scope (s.substT σ) := by
  funext x; apply propext
  cases x <;> simp [Row.substT, Row.scope, Row.ofCaps, Ty.reach_substT]

@[simp] theorem Caps.none_substT (σ : Atom → Ty) : Caps.none.substT σ = Caps.none := by
  funext a; simp [Caps.substT, Caps.none]

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
  induction ts <;> simp_all [Ty.arrows, Ty.substT, Stage.substT]

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

/-- The names of a row without row variables stay the same under a substitution. -/
theorem Row.Named.substT_nm {r : Row} (h : r.Named) (σ : Atom → Ty) (N : Name) :
    r.substT σ (.nm N) ↔ r (.nm N) := by
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

end Flick
