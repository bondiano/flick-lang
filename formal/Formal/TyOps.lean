import Formal.Syntax

/-!
# Substitution of variables and regions: shifting, lifting under a scheme,
instantiation; how `⌈·⌉` commutes with a substitution (regions.md §2.1)
-/

namespace Flick

/-- The renaming `i ↦ i + k` as a substitution; run-time regions stay. -/
def shiftV (k : Nat) : Atom → Ty
  | .v i => .tvar (i + k)
  | .r p => .rg p

/-- Shift the free variables by `k`. -/
def Ty.up (k : Nat) (t : Ty) : Ty := t.substT (shiftV k)

/-- A substitution under the `k` binders of a scheme. -/
def liftT (k : Nat) (σ : Atom → Ty) : Atom → Ty
  | .v i => if i < k then .tvar i else (σ (.v (i - k))).up k
  | .r p => (σ (.r p)).up k

/-- Instantiate the binders of a scheme with `ts`. -/
def instT (ts : List Ty) : Atom → Ty
  | .v i => if h : i < ts.length then ts[i] else .tvar (i - ts.length)
  | .r p => .rg p

@[simp] theorem Ty.reach_tvar (n : Nat) (a : Atom) : (Ty.tvar n).reach a ↔ a = .v n := Iff.rfl
@[simp] theorem Ty.reach_rg (p : Nat) (a : Atom) : (Ty.rg p).reach a ↔ a = .r p := Iff.rfl

@[simp] theorem Ty.reach_ident (b a : Atom) : (Ty.ident b).reach a ↔ a = b := by
  cases b <;> simp [Ty.ident]

/-- `⌈τ⌉` is what `τ` stands for in a row, as atoms and as row variables. -/
theorem Ty.reach_asRow (t : Ty) (a : Atom) : t.reach a ↔ t.asRow (.atm a) ∨ t.asRow (.rv a) := by
  cases t <;> simp [Ty.asRow, Ty.reach, Row.plain, Row.ofCaps]

/-- **⌈·⌉ commutes with substitution**: `⌈τ[σ]⌉ = ⋃_{b ∈ ⌈τ⌉} ⌈σ b⌉`. -/
theorem Ty.reach_substT (σ : Atom → Ty) (t : Ty) (a : Atom) :
    (t.substT σ).reach a ↔ ∃ b, t.reach b ∧ (σ b).reach a := by
  induction t generalizing a with
  | tvar n => simp [Ty.substT]
  | rg p => simp [Ty.substT]
  | caps C => simp [Ty.substT, Ty.reach, Caps.substT]
  | row r =>
    simp only [Ty.substT, Ty.reach, Row.substT]
    grind [Ty.reach_asRow]
  | arrow x φ y C ihx ihy =>
    simp only [Ty.substT, Ty.reach, Caps.substT, ihx, ihy]
    constructor
    · rintro (⟨b, hb, h⟩ | ⟨b, hb, h⟩ | ⟨b, hb, h⟩)
      · exact ⟨b, .inl hb, h⟩
      · exact ⟨b, .inr (.inl hb), h⟩
      · exact ⟨b, .inr (.inr hb), h⟩
    · rintro ⟨b, hb | hb | hb, h⟩
      · exact .inl ⟨b, hb, h⟩
      · exact .inr (.inl ⟨b, hb, h⟩)
      · exact .inr (.inr ⟨b, hb, h⟩)
  | ref t ih => simp only [Ty.substT, Ty.reach, ih]
  | tapp f x ihf ihx | rcons _ f x ihf ihx =>
    simp only [Ty.substT, Ty.reach, ihf, ihx]
    constructor
    · rintro (⟨b, hb, h⟩ | ⟨b, hb, h⟩)
      · exact ⟨b, .inl hb, h⟩
      · exact ⟨b, .inr hb, h⟩
    · rintro ⟨b, hb | hb, h⟩
      · exact .inl ⟨b, hb, h⟩
      · exact .inr ⟨b, hb, h⟩
  | eff r e t C ih =>
    simp only [Ty.substT, Ty.reach, Caps.substT, ih]
    constructor
    · rintro (⟨b, hb, h⟩ | ⟨b, hb, h⟩)
      · exact ⟨b, .inl hb, h⟩
      · exact ⟨b, .inr hb, h⟩
    · rintro ⟨b, hb | hb, h⟩
      · exact .inl ⟨b, hb, h⟩
      · exact .inr ⟨b, hb, h⟩
  | errors e =>
    simp only [Ty.substT, Ty.reach, Row.substT]
    grind [Ty.reach_asRow]
  | hnd s ih | owner s ih => simp only [Ty.substT, Ty.reach, ih]
  | base | adt | rnil | layer | env | dict => simp [Ty.substT, Ty.reach, Caps.none]

theorem Row.ofCaps_substT (C : Caps) (σ : Atom → Ty) :
    (Row.ofCaps C).substT σ = Row.ofCaps (C.substT σ) := by
  funext x; cases x <;> simp [Row.substT, Row.ofCaps, Caps.substT]

theorem Ty.asRow_tvar (n : Nat) : (Ty.tvar n).asRow = fun x => x = .rv (.v n) := rfl
theorem Ty.asRow_rg (p : Nat) : (Ty.rg p).asRow = fun x => x = .rv (.r p) := rfl
theorem Ty.asRow_row (r : Row) : (Ty.row r).asRow = r.plain := rfl

@[simp] theorem Ty.asRow_ob (t : Ty) (N : Name) (a : Atom) : ¬ t.asRow (.ob N a) := by
  cases t <;> simp [Ty.asRow, Row.plain, Row.ofCaps]

@[simp] theorem Ty.asRow_need (t : Ty) (N : Name) (a : Atom) : ¬ t.asRow (.need N a) := by
  cases t <;> simp [Ty.asRow, Row.plain, Row.ofCaps]

@[simp] theorem Ty.asRow_obv (t : Ty) (v a : Atom) : ¬ t.asRow (.obv v a) := by
  cases t <;> simp [Ty.asRow, Row.plain, Row.ofCaps]

/-- What a type stands for in a row commutes with substitution. -/
theorem Ty.asRow_substT (σ : Atom → Ty) (t : Ty) : (t.substT σ).asRow = t.asRow.substT σ := by
  have hc : ∀ t' : Ty, t'.asRow = Row.ofCaps t'.reach → (t'.substT σ).asRow = Row.ofCaps (t'.substT σ).reach →
      (t'.substT σ).asRow = t'.asRow.substT σ := by
    intro t' h₁ h₂
    rw [h₁, h₂, Row.ofCaps_substT]; congr 1; funext a; apply propext; exact Ty.reach_substT σ t' a
  cases t with
  | tvar n =>
    funext x; apply propext; cases x <;> simp [Ty.substT, Ty.asRow_tvar, Row.substT]
  | rg p =>
    funext x; apply propext; cases x <;> simp [Ty.substT, Ty.asRow_rg, Row.substT]
  | row r =>
    funext x; apply propext; cases x <;> simp [Ty.substT, Ty.asRow_row, Row.substT, Row.plain]
  | _ => exact hc _ rfl rfl

/-- A substitution that brings no name into a row: the instance of each row variable has
no service and no error. -/
def NoNames (σ : Atom → Ty) : Prop := ∀ b N, ¬ (σ b).asRow (.nm N)

theorem Row.substT_nm {σ : Atom → Ty} (h : NoNames σ) (r : Row) (N : Name) :
    r.substT σ (.nm N) ↔ r (.nm N) :=
  ⟨fun h' => h'.elim id fun ⟨b, _, hb⟩ => (h b N hb).elim, .inl⟩

theorem Caps.substT_comp (C : Caps) (σ σ' : Atom → Ty) :
    (C.substT σ).substT σ' = C.substT fun a => (σ a).substT σ' := by
  funext a; apply propext
  simp only [Caps.substT, Ty.reach_substT]
  constructor
  · rintro ⟨b, ⟨c, hc, hb⟩, h⟩; exact ⟨c, hc, b, hb, h⟩
  · rintro ⟨c, hc, b, hb, h⟩; exact ⟨b, ⟨c, hc, hb⟩, h⟩

theorem Row.substT_comp (r : Row) (σ σ' : Atom → Ty) :
    (r.substT σ).substT σ' = r.substT fun a => (σ a).substT σ' := by
  funext x; apply propext
  cases x with
  | need _ a =>
    simp only [Row.substT, Ty.reach_substT]
    constructor
    · rintro ⟨b, ⟨c, hc, hb⟩, h⟩; exact ⟨c, hc, b, hb, h⟩
    · rintro ⟨c, hc, b, hb, h⟩; exact ⟨b, ⟨c, hc, hb⟩, h⟩
  | _ =>
    simp only [Row.substT, Ty.reach_substT, Ty.asRow_substT]
    grind

theorem Stage.substT_comp (φ : Stage) (σ σ' : Atom → Ty) :
    (φ.substT σ).substT σ' = φ.substT fun a => (σ a).substT σ' := by
  cases φ <;> simp [Stage.substT, Row.substT_comp]

theorem Caps.substT_ident (C : Caps) : C.substT Ty.ident = C := by
  funext a; apply propext; simp [Caps.substT]

theorem Ty.asRow_ident (b : Atom) : (Ty.ident b).asRow = fun x => x = .rv b := by
  cases b <;> rfl

theorem Row.substT_ident (r : Row) : r.substT Ty.ident = r := by
  funext x; apply propext; cases x <;> simp [Row.substT, Ty.asRow_ident]

theorem Stage.substT_ident (φ : Stage) : φ.substT Ty.ident = φ := by
  cases φ <;> simp [Stage.substT, Row.substT_ident]

theorem Ty.substT_comp (t : Ty) (σ σ' : Atom → Ty) :
    (t.substT σ).substT σ' = t.substT fun a => (σ a).substT σ' := by
  induction t <;>
    simp_all [Ty.substT, Caps.substT_comp, Row.substT_comp, Stage.substT_comp]

theorem Ty.substT_ident (t : Ty) : t.substT Ty.ident = t := by
  induction t <;>
    simp_all [Ty.substT, Ty.ident, Caps.substT_ident, Row.substT_ident, Stage.substT_ident]

theorem Ty.substT_congr {t : Ty} {σ σ' : Atom → Ty} (h : ∀ a, σ a = σ' a) :
    t.substT σ = t.substT σ' := by
  rw [funext h]

theorem Caps.substT_mono {C C' : Caps} (σ : Atom → Ty) (h : ∀ a, C a → C' a) :
    ∀ a, C.substT σ a → C'.substT σ a :=
  fun _ ⟨b, hb, h'⟩ => ⟨b, h b hb, h'⟩

theorem Row.Sub.substT {r r' : Row} (σ : Atom → Ty) (h : Row.Sub r r') :
    Row.Sub (r.substT σ) (r'.substT σ) := by
  intro x hx
  cases x with
  | need _ a => obtain ⟨b, hb, h'⟩ := hx; exact ⟨b, h _ hb, h'⟩
  | nm | atm | rv | ob | obv =>
    simp only [Row.substT] at hx ⊢
    grind [Row.Sub]

theorem Stage.le_substT {φ φ' : Stage} (σ : Atom → Ty) (h : φ.le φ') :
    (φ.substT σ).le (φ'.substT σ) := by
  cases φ <;> cases φ' <;> simp_all [Stage.le, Stage.substT]
  exact ⟨h.1.substT σ, h.2.substT σ⟩

@[simp] theorem Row.none_substT (σ : Atom → Ty) : Row.none.substT σ = Row.none := by
  funext x; cases x <;> simp [Row.substT, Row.none]

@[simp] theorem Row.single_substT (σ : Atom → Ty) (N : Name) :
    (Row.single N).substT σ = Row.single N := by
  funext x; cases x <;> simp [Row.substT, Row.single]

@[simp] theorem Row.union_substT (σ : Atom → Ty) (r r' : Row) :
    (Row.union r r').substT σ = Row.union (r.substT σ) (r'.substT σ) := by
  funext x; apply propext
  cases x <;> simp only [Row.substT, Row.union] <;> grind

@[simp] theorem Row.scope_substT (σ : Atom → Ty) (s : Ty) :
    (Row.scope s).substT σ = Row.scope (s.substT σ) := by
  funext x; apply propext
  cases x <;> simp [Row.substT, Row.scope, Row.ofCaps, Ty.reach_substT]

@[simp] theorem Caps.none_substT (σ : Atom → Ty) : Caps.none.substT σ = Caps.none := by
  funext a; simp [Caps.substT, Caps.none]

@[simp] theorem Ty.up_zero (t : Ty) : t.up 0 = t := by
  have : shiftV 0 = Ty.ident := by funext a; cases a <;> rfl
  simp only [Ty.up, this]; exact t.substT_ident

@[simp] theorem List.map_up_zero (Ψ : List Ty) : Ψ.map (Ty.up 0) = Ψ := by
  induction Ψ <;> simp_all

@[simp] theorem liftT_zero (σ : Atom → Ty) : liftT 0 σ = σ := by
  funext a; cases a <;> simp [liftT]

theorem liftT_ident (k : Nat) : liftT k Ty.ident = Ty.ident := by
  funext a
  cases a with
  | v i =>
    simp only [liftT]
    split
    · rfl
    · simp only [Ty.ident, Ty.up, Ty.substT, shiftV]; congr 1; omega
  | r p => simp [liftT, Ty.up, Ty.substT, shiftV, Ty.ident]

theorem Ty.up_comm (t : Ty) (k : Nat) (σ : Atom → Ty) :
    (t.up k).substT (liftT k σ) = (t.substT σ).up k := by
  simp only [Ty.up, Ty.substT_comp]
  apply Ty.substT_congr
  intro a
  cases a with
  | v i =>
    have : ¬ i + k < k := by omega
    simp [shiftV, Ty.substT, liftT, this, Ty.up]
  | r p => simp [shiftV, Ty.substT, liftT, Ty.up]

theorem liftT_comp (k : Nat) (σ σ' : Atom → Ty) (a : Atom) :
    (liftT k σ a).substT (liftT k σ') = liftT k (fun b => (σ b).substT σ') a := by
  cases a with
  | v i =>
    by_cases hi : i < k
    · simp [liftT, hi, Ty.substT]
    · simp only [liftT, hi, if_false]; exact Ty.up_comm _ k σ'
  | r p => simp only [liftT]; exact Ty.up_comm _ k σ'

theorem Ty.up_substT (t : Ty) (k : Nat) (ts : List Ty) (h : ts.length = k) :
    (t.up k).substT (instT ts) = t := by
  simp only [Ty.up, Ty.substT_comp]
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
  simp only [Ty.up, Ty.substT_comp]
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
  ⟨ι.t.substT σ, ι.s.substT σ, ι.C.substT σ, ι.r.substT σ, ι.e.substT σ, ι.o.substT σ,
    ι.r'.substT σ, ι.e'.substT σ, ι.o'.substT σ⟩

theorem Ty.reach_caps (s : Ty) (σ : Atom → Ty) : s.reach.substT σ = (s.substT σ).reach := by
  funext a; apply propext; simp [Caps.substT, Ty.reach_substT]

/-- The names of a row without row variables stay the same under a substitution. -/
theorem Row.Named.substT_nm {r : Row} (h : r.Named) (σ : Atom → Ty) (N : Name) :
    r.substT σ (.nm N) ↔ r (.nm N) := by
  simp only [Row.substT]; exact ⟨fun h' => h'.elim id fun ⟨b, hb, _⟩ => (h b hb).elim, .inl⟩

theorem Row.Named.substT {r : Row} (h : r.Named) (σ : Atom → Ty) : (r.substT σ).Named := by
  rintro a ⟨b, hb, _⟩; exact h b hb

theorem Row.obs_substT (σ : Atom → Ty) (r : Row) (s : Ty) :
    (r.obs s).substT σ = (r.substT σ).obs (s.substT σ) := by
  funext x; apply propext
  cases x with
  | ob | obv =>
    simp only [Row.substT, Row.obs, Ty.reach_substT]
    grind
  | _ => simp [Row.substT, Row.obs]

theorem Const.ty_substT (S : Sig) (c : Const) (ι : CArg) (σ : Atom → Ty) :
    (c.ty S ι).substT σ = c.ty S (ι.substT σ) := by
  cases c
  case addFin | spawn =>
    simp [Const.ty, Ty.substT, Stage.substT, CArg.substT, Ty.reach_caps, Row.obs_substT σ]
  case perform E o _ =>
    rcases h : S.op E o with _ | ⟨c, a, b⟩
    · simp [Const.ty, h, Ty.substT, Stage.substT]
    · obtain ⟨ha, hb⟩ := S.op_closed E o c a b σ h
      cases c <;> simp [Const.ty, h, Ty.substT, Stage.substT, CArg.substT, ha, hb]
  all_goals simp [Const.ty, Ty.substT, Stage.substT, S.impl_closed, CArg.substT]

end Flick
