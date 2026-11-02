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

/-- **⌈·⌉ commutes with substitution**: `⌈τ[σ]⌉ = ⋃_{b ∈ ⌈τ⌉} ⌈σ b⌉`. -/
theorem Ty.reach_substT (σ : Atom → Ty) (t : Ty) (a : Atom) :
    (t.substT σ).reach a ↔ ∃ b, t.reach b ∧ (σ b).reach a := by
  induction t generalizing a with
  | tvar n => simp [Ty.substT]
  | rg p => simp [Ty.substT]
  | caps C => simp [Ty.substT, Ty.reach, Caps.substT]
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
  | errors e => simp [Ty.substT, Ty.reach, Row.substT]
  | hnd s ih | owner s ih => simp only [Ty.substT, Ty.reach, ih]
  | base | adt | rnil | layer | env | dict => simp [Ty.substT, Ty.reach, Caps.none]

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
  | nm => rfl
  | atm a | ob _ a | need _ a =>
    simp only [Row.substT, Ty.reach_substT]
    constructor
    · rintro ⟨b, ⟨c, hc, hb⟩, h⟩; exact ⟨c, hc, b, hb, h⟩
    · rintro ⟨c, hc, b, hb, h⟩; exact ⟨b, ⟨c, hc, hb⟩, h⟩

theorem Stage.substT_comp (φ : Stage) (σ σ' : Atom → Ty) :
    (φ.substT σ).substT σ' = φ.substT fun a => (σ a).substT σ' := by
  cases φ <;> simp [Stage.substT, Row.substT_comp]

theorem Caps.substT_ident (C : Caps) : C.substT Ty.ident = C := by
  funext a; apply propext; simp [Caps.substT]

theorem Row.substT_ident (r : Row) : r.substT Ty.ident = r := by
  funext x; apply propext; cases x <;> simp [Row.substT]

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
  | nm => exact h _ hx
  | atm a | ob _ a | need _ a => obtain ⟨b, hb, h'⟩ := hx; exact ⟨b, h _ hb, h'⟩

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
  cases x with
  | nm => rfl
  | atm a | ob _ a | need _ a =>
    simp only [Row.substT, Row.union]
    constructor
    · rintro ⟨b, hb | hb, h⟩
      · exact .inl ⟨b, hb, h⟩
      · exact .inr ⟨b, hb, h⟩
    · rintro (⟨b, hb, h⟩ | ⟨b, hb, h⟩)
      · exact ⟨b, .inl hb, h⟩
      · exact ⟨b, .inr hb, h⟩

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

def CArg.substT (σ : Atom → Ty) (ι : CArg) : CArg :=
  ⟨ι.t.substT σ, ι.s.substT σ, ι.C.substT σ, ι.r.substT σ, ι.e.substT σ, ι.o.substT σ,
    ι.r'.substT σ, ι.e'.substT σ, ι.o'.substT σ⟩

theorem Ty.reach_caps (s : Ty) (σ : Atom → Ty) : s.reach.substT σ = (s.substT σ).reach := by
  funext a; apply propext; simp [Caps.substT, Ty.reach_substT]

@[simp] theorem Row.obs_substT (σ : Atom → Ty) (r : Row) (s : Ty) :
    (r.obs s).substT σ = (r.substT σ).obs (s.substT σ) := by
  funext x; apply propext
  cases x with
  | ob n a =>
    simp only [Row.substT, Row.obs, Ty.reach_substT]
    constructor
    · rintro ⟨b, ⟨h₁, h₂⟩, h₃⟩; exact ⟨h₁, b, h₂, h₃⟩
    · rintro ⟨h₁, b, h₂, h₃⟩; exact ⟨b, ⟨h₁, h₂⟩, h₃⟩
  | _ => simp [Row.substT, Row.obs]

theorem Const.ty_substT (S : Sig) (c : Const) (ι : CArg) (σ : Atom → Ty) :
    (c.ty S ι).substT σ = c.ty S (ι.substT σ) := by
  cases c <;> simp [Const.ty, Ty.substT, Stage.substT, S.impl_closed, CArg.substT, Ty.reach_caps]

theorem Facts.substT_comp (Θ : Facts) (σ σ' : Atom → Ty) :
    (Θ.substT σ).substT σ' = Θ.substT fun a => (σ a).substT σ' := by
  funext C s; apply propext
  simp only [Facts.substT]
  constructor
  · rintro ⟨C₁, s₁, ⟨C₀, s₀, h, rfl, rfl⟩, rfl, rfl⟩
    exact ⟨C₀, s₀, h, Caps.substT_comp _ _ _, Ty.substT_comp _ _ _⟩
  · rintro ⟨C₀, s₀, h, rfl, rfl⟩
    exact ⟨_, _, ⟨C₀, s₀, h, rfl, rfl⟩, (Caps.substT_comp _ _ _).symm, (Ty.substT_comp _ _ _).symm⟩

theorem Facts.substT_ident (Θ : Facts) : Θ.substT Ty.ident = Θ := by
  funext C s; apply propext
  simp only [Facts.substT]
  constructor
  · rintro ⟨C₀, s₀, h, rfl, rfl⟩; rwa [Caps.substT_ident, Ty.substT_ident]
  · intro h; exact ⟨C, s, h, (Caps.substT_ident C).symm, (Ty.substT_ident s).symm⟩

@[simp] theorem Facts.union_substT (Θ Θ' : Facts) (σ : Atom → Ty) :
    (Facts.union Θ Θ').substT σ = Facts.union (Θ.substT σ) (Θ'.substT σ) := by
  funext C s; apply propext
  simp only [Facts.substT, Facts.union]
  constructor
  · rintro ⟨C₀, s₀, h | h, rfl, rfl⟩
    · exact .inl ⟨C₀, s₀, h, rfl, rfl⟩
    · exact .inr ⟨C₀, s₀, h, rfl, rfl⟩
  · rintro (⟨C₀, s₀, h, rfl, rfl⟩ | ⟨C₀, s₀, h, rfl, rfl⟩)
    · exact ⟨C₀, s₀, .inl h, rfl, rfl⟩
    · exact ⟨C₀, s₀, .inr h, rfl, rfl⟩

@[simp] theorem Facts.none_substT (σ : Atom → Ty) : Facts.none.substT σ = Facts.none := by
  funext C s; simp [Facts.substT, Facts.none]

/-- `Outlives` is kept by a substitution. -/
theorem Outl.substT {Θ C s} (σ : Atom → Ty) (h : Outl Θ C s) :
    Outl (Θ.substT σ) (C.substT σ) (s.substT σ) := by
  intro k ⟨b, hb, hk⟩
  rcases h b hb with h | ⟨C', h₁, h₂⟩
  · exact .inl ((Ty.reach_substT σ s k).mpr ⟨b, h, hk⟩)
  · exact .inr ⟨C'.substT σ, ⟨C', s, h₁, rfl, rfl⟩, b, h₂, hk⟩

theorem Outl.mono {Θ Θ' C s} (h : Outl Θ C s) (hΘ : ∀ C s, Θ C s → Θ' C s) : Outl Θ' C s :=
  fun k hk => (h k hk).imp id fun ⟨C', h₁, h₂⟩ => ⟨C', hΘ _ _ h₁, h₂⟩

theorem Const.ok_substT {Θ c ι} (σ : Atom → Ty) (h : Const.ok Θ c ι) :
    Const.ok (Θ.substT σ) c (ι.substT σ) := by
  cases c <;> simp only [Const.ok, CArg.substT] at h ⊢
  · exact fun N ⟨h₁, h₂⟩ => h N ⟨h₁, h₂⟩
  · exact ⟨h.1.substT σ, fun N a ⟨b, hb, _⟩ => h.2 N b hb⟩
  · have := h.1.substT σ
    refine ⟨fun k hk => this k ?_, fun N a ⟨b, hb, _⟩ => h.2 N b hb⟩
    rcases hk with ⟨b, hb, h'⟩ | hk
    · exact ⟨b, .inl hb, h'⟩
    · obtain ⟨b, hb, h'⟩ := (Ty.reach_substT σ _ k).mp hk
      exact ⟨b, .inr hb, h'⟩

theorem Const.ok_mono {Θ Θ' c ι} (h : Const.ok Θ c ι)
    (hΘ : ∀ C s, Outl Θ C s → Outl Θ' C s) : Const.ok Θ' c ι := by
  cases c <;> simp only [Const.ok] at h ⊢ <;> first | exact h | exact hΘ _ _ h | exact ⟨hΘ _ _ h.1, h.2⟩

def Scheme.substT (σ : Atom → Ty) (s : Scheme) : Scheme :=
  ⟨s.k, s.cs.substT (liftT s.k σ), s.t.substT (liftT s.k σ)⟩

def Scheme.up (k : Nat) (s : Scheme) : Scheme := s.substT (shiftV k)

/-- `σ[ᾱ := τ̄]`. -/
def Scheme.inst (s : Scheme) (ts : List Ty) : Ty := s.t.substT (instT ts)

/-- The facts of a scheme at an instance hold. -/
def Scheme.Holds (Θ : Facts) (s : Scheme) (ts : List Ty) : Prop :=
  ∀ C r, s.cs C r → Outl Θ (C.substT (instT ts)) (r.substT (instT ts))

theorem Scheme.substT_comp (s : Scheme) (σ σ' : Atom → Ty) :
    (s.substT σ).substT σ' = s.substT fun a => (σ a).substT σ' := by
  simp only [Scheme.substT, Ty.substT_comp, Facts.substT_comp, Scheme.mk.injEq, true_and]
  constructor <;> congr 1 <;> funext a <;> exact liftT_comp _ _ _ _

theorem Scheme.substT_ident (s : Scheme) : s.substT Ty.ident = s := by
  simp [Scheme.substT, liftT_ident, Ty.substT_ident, Facts.substT_ident]

theorem Scheme.substT_congr {s : Scheme} {σ σ' : Atom → Ty} (h : ∀ a, σ a = σ' a) :
    s.substT σ = s.substT σ' := by
  rw [funext h]

@[simp] theorem Scheme.up_zero (s : Scheme) : s.up 0 = s := by
  have : shiftV 0 = Ty.ident := by funext a; cases a <;> rfl
  simp only [Scheme.up, this]; exact s.substT_ident

theorem Scheme.up_comm (s : Scheme) (k : Nat) (σ : Atom → Ty) :
    (s.up k).substT (liftT k σ) = (s.substT σ).up k := by
  simp only [Scheme.up, Scheme.substT_comp]
  apply Scheme.substT_congr
  intro a
  cases a with
  | v i =>
    have : ¬ i + k < k := by omega
    simp [shiftV, Ty.substT, liftT, Ty.up, this]
  | r p => simp [shiftV, Ty.substT, liftT, Ty.up]

theorem Scheme.up_substT (s : Scheme) (k : Nat) (ts : List Ty) (h : ts.length = k) :
    (s.up k).substT (instT ts) = s := by
  simp only [Scheme.up, Scheme.substT_comp]
  conv => rhs; rw [← s.substT_ident]
  apply Scheme.substT_congr
  intro a
  cases a with
  | v i =>
    have : ¬ i + k < k := by omega
    simp [shiftV, Ty.substT, instT, h, this, Ty.ident]
  | r p => simp [shiftV, Ty.substT, instT, Ty.ident]

@[simp] theorem List.map_sup_zero (Γ : List Scheme) : Γ.map (Scheme.up 0) = Γ := by
  induction Γ <;> simp_all

@[simp] theorem Scheme.mono_k (t : Ty) : (Scheme.mono t).k = 0 := rfl
@[simp] theorem Scheme.mono_t (t : Ty) : (Scheme.mono t).t = t := rfl
@[simp] theorem Scheme.mono_cs (t : Ty) : (Scheme.mono t).cs = Facts.none := rfl

@[simp] theorem Scheme.mono_substT (t : Ty) (σ : Atom → Ty) :
    (Scheme.mono t).substT σ = Scheme.mono (t.substT σ) := by
  simp [Scheme.substT, Scheme.mono]

@[simp] theorem Scheme.inst_mono (t : Ty) : (Scheme.mono t).inst [] = t := by
  simp only [Scheme.inst, Scheme.mono]
  conv => rhs; rw [← t.substT_ident]
  apply Ty.substT_congr; intro a; cases a <;> simp [instT, Ty.ident]

theorem Scheme.inst_substT (s : Scheme) (σ : Atom → Ty) (ts : List Ty) (h : ts.length = s.k) :
    (s.substT σ).inst (ts.map (Ty.substT σ)) = (s.inst ts).substT σ := by
  simp only [Scheme.substT, Scheme.inst]
  rw [← h]; exact Ty.inst_substT _ _ _

theorem Caps.inst_substT (C : Caps) (σ : Atom → Ty) (ts : List Ty) :
    (C.substT (liftT ts.length σ)).substT (instT (ts.map (Ty.substT σ))) =
      (C.substT (instT ts)).substT σ := by
  rw [Caps.substT_comp, Caps.substT_comp]
  congr 1; funext a
  have := Ty.inst_substT (Ty.ident a) σ ts
  cases a <;> simpa [Ty.substT, Ty.ident] using this

theorem Scheme.Holds.substT {Θ : Facts} {s : Scheme} {ts : List Ty} (σ : Atom → Ty) (h : s.Holds Θ ts) (hk : ts.length = s.k) :
    (s.substT σ).Holds (Θ.substT σ) (ts.map (Ty.substT σ)) := by
  intro C r ⟨C₀, r₀, h₀, hC, hr⟩
  subst hC hr
  have := (h C₀ r₀ h₀).substT σ
  rw [← hk, Caps.inst_substT, Ty.inst_substT]
  exact this

end Flick
