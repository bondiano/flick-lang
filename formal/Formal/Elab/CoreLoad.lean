import Formal.Elab.CoreSim

/-!
# λFlick⁺ as a `Target`: `≈` dictionaries are related

`≈` relates open dictionaries too, through `trans` and junk intermediate terms. Its
congruence closure of closed pairs, `DSim`, is transitive, so `Eqv ⇒ DSim` when the slots of
the instances name only their premises (`SlotsBelow`, `eqv_dsim`). `DTyp P Δ d T` is the typing
of a dictionary by the laws of `Target`; a closed one is typed in the empty context, and the
core types its translation at the pure stage (`DTyp.core`). Then `DSim` pairs of typed
dictionaries translate to related core terms (`dsim_cg`): a closed pair is a base pair `DP`,
with the same values by `eqv_sem`.
-/

namespace Flick.Elab

variable {P : Prog}

/-- `k` occurs in a dictionary. -/
def Dict.Occ : Dict → Nat → Prop
  | .var k, j => k = j
  | .slot d _, j => d.Occ j
  | .ap d e, j => d.Occ j ∨ e.Occ j
  | _, _ => False

def Dict.NoVar (d : Dict) : Prop := ∀ k, ¬ d.Occ k

/-- The congruence closure of closed `≈` pairs. -/
inductive DSim (P : Prog) : Dict → Dict → Prop
  | var {k} : DSim P (.var k) (.var k)
  | slot {d d' i} : DSim P d d' → DSim P (.slot d i) (.slot d' i)
  | inst {I} : DSim P (.inst I) (.inst I)
  | strct {C τ} : DSim P (.strct C τ) (.strct C τ)
  | prim {p q} : DSim P (.prim p q) (.prim p q)
  | ap {d d' e e'} : DSim P d d' → DSim P e e' → DSim P (.ap d e) (.ap d' e')
  | base {d d'} : d.NoVar → d'.NoVar → Eqv P d d' → DSim P d d'

theorem DSim.refl (d : Dict) : DSim P d d := by
  induction d <;> constructor <;> assumption

theorem DSim.eqv {d d' : Dict} (h : DSim P d d') : Eqv P d d' := by
  induction h with
  | slot _ ih => exact .slot ih
  | ap _ _ ih₁ ih₂ => exact .ap ih₁ ih₂
  | base _ _ h => exact h
  | _ => exact .refl

theorem DSim.occ {d d' : Dict} (h : DSim P d d') (k : Nat) : d.Occ k ↔ d'.Occ k := by
  induction h with
  | slot _ ih => exact ih
  | ap _ _ ih₁ ih₂ => exact or_congr ih₁ ih₂
  | base h₁ h₂ => exact ⟨fun h => (h₁ _ h).elim, fun h => (h₂ _ h).elim⟩
  | _ => exact Iff.rfl

theorem DSim.symm {d d' : Dict} (h : DSim P d d') : DSim P d' d := by
  induction h with
  | slot _ ih => exact .slot ih
  | ap _ _ ih₁ ih₂ => exact .ap ih₁ ih₂
  | base h₁ h₂ h => exact .base h₂ h₁ h.symm
  | _ => constructor

theorem DSim.trans {d e f : Dict} (h₁ : DSim P d e) (h₂ : DSim P e f) : DSim P d f := by
  induction h₁ generalizing f with
  | var => exact h₂
  | inst => exact h₂
  | strct => exact h₂
  | prim => exact h₂
  | base hd he h => exact .base hd (fun k hk => he k ((h₂.occ k).2 hk)) (h.trans h₂.eqv)
  | slot h ih =>
    cases h₂ with
    | slot h₂ => exact .slot (ih h₂)
    | base he hf h' =>
      exact .base (fun k hk => he k (((DSim.slot h).occ k).1 hk)) hf ((DSim.slot h).eqv.trans h')
  | ap h h' ih ih' =>
    cases h₂ with
    | ap h₃ h₄ => exact .ap (ih h₃) (ih' h₄)
    | base he hf hq =>
      exact .base (fun k hk => he k (((DSim.ap h h').occ k).1 hk)) hf ((DSim.ap h h').eqv.trans hq)

/-- The slots of the instances name only their premises. -/
def SlotsBelow (P : Prog) : Prop :=
  ∀ {I : Nat} {ins : Inst} {i : Nat} {s : Dict} {k : Nat}, P.insts I = some ins →
    ins.slots[i]? = some s → Dict.Occ s k →
    k < ins.ctx.length

theorem apps_noVar {f : Dict} {ds : List Dict} (hf : f.NoVar) (hds : ∀ d ∈ ds, d.NoVar) :
    (apps f ds).NoVar := by
  induction ds generalizing f with
  | nil => exact hf
  | cons d ds ih =>
    exact ih (fun k hk => hk.elim (hf k) (hds d (by simp) k)) (fun e he => hds e (by simp [he]))

theorem ent_noVar {qs : List Cn} {ds : List Dict} (h : Ent P [] qs ds) : ∀ d ∈ ds, d.NoVar := by
  induction h with
  | nil => simp
  | cons _ _ ih₁ ih₂ =>
    intro d hd
    rcases List.mem_cons.mp hd with rfl | hd
    · exact ih₁ _ (by simp)
    · exact ih₂ _ hd
  | given hc => exact (hc.not_nil).elim
  | prim => intro d hd; simp at hd; subst hd; exact fun _ h => h
  | strct _ _ _ ih => intro d hd; simp at hd; subst hd; exact apps_noVar (fun _ h => h) ih
  | inst _ _ _ _ ih => intro d hd; simp at hd; subst hd; exact apps_noVar (fun _ h => h) ih

theorem subst_noVar {s : Dict} {ds : List Dict} (hs : ∀ k, s.Occ k → k < ds.length)
    (hds : ∀ d ∈ ds, d.NoVar) : (s.subst (args ds)).NoVar := by
  induction s with
  | var k =>
    have hk := hs k rfl
    simp only [Dict.subst, args, getD_lt hk]
    exact hds _ (List.getElem_mem hk)
  | slot d i ih => exact ih hs
  | ap d e ih₁ ih₂ =>
    intro k hk
    exact hk.elim (ih₁ (fun j hj => hs j (.inl hj)) k) (ih₂ (fun j hj => hs j (.inr hj)) k)
  | _ => exact fun _ h => h

/-- **`≈` is the congruence closure of its closed pairs.** -/
theorem eqv_dsim (hS : SlotsBelow P) {d d' : Dict} (h : Eqv P d d') : DSim P d d' := by
  induction h with
  | refl => exact .refl _
  | symm _ ih => exact ih.symm
  | trans _ _ ih₁ ih₂ => exact ih₁.trans ih₂
  | slot _ ih => exact .slot ih
  | ap _ _ ih₁ ih₂ => exact .ap ih₁ ih₂
  | instSlot hI hsl hds =>
    refine .base (apps_noVar (fun _ h => h) (ent_noVar hds)) (subst_noVar (fun k hk => ?_)
      (ent_noVar hds)) (.instSlot hI hsl hds)
    rw [hds.get.1, List.length_map]; exact hS hI hsl hk
  | strctSlot hC hS' hf hds =>
    refine .base (apps_noVar (fun _ h => h) (ent_noVar hds))
      (apps_noVar (fun _ h => h) fun d hd => ?_) (.strctSlot hC hS' hf hds)
    obtain ⟨d₀, hd₀, rfl⟩ := List.mem_map.mp hd
    exact ent_noVar hds d₀ hd₀
  | strctSub hC hf => exact .base (fun _ h => h) (fun _ h => h) (.strctSub hC hf)
  | primSub hq hp => exact .base (fun _ h => h) (fun _ h => h) (.primSub hq hp)

/-- The typing of a dictionary by the laws of `Target`, in a context of premises. -/
inductive DTyp (P : Prog) : List Cn → Dict → TTy → Prop
  | var {Δ k q} : Δ[k]? = some q → DTyp P Δ (.var k) (.dict q)
  | slot {Δ d q i S} : DTyp P Δ d (.dict q) → P.super q i = some S → DTyp P Δ (.slot d i) (.dict S)
  | ap {Δ f x a t q} : DTyp P Δ f (.arr .P a t q) → DTyp P Δ x a → DTyp P Δ (.ap f x) t
  | inst {Δ I ins σ} : P.insts I = some ins →
      DTyp P Δ (.inst I)
        (dicts .cnil (ins.ctx.map (·.subst σ)) (.dict ⟨ins.cls, ins.head.map (·.subst σ)⟩))
  | strct {Δ C τ ts θ} : P.structC C = true → P.fields τ = some ts →
      DTyp P Δ (.strct C τ)
        (dicts .cnil (ts.map fun t => ⟨C, [t.subst θ]⟩) (.dict ⟨C, [τ.subst θ]⟩))
  | prim {Δ q p θ} : P.primC q.cls = true → P.prims q = some p →
      DTyp P Δ (.prim p q) (.dict (q.subst θ))

theorem DTyp.target (X : Target P) {Δ : List Cn} {d : Dict} {T : TTy} (h : DTyp P Δ d T)
    (Γ : List TSch) (φ : Stage) : X.Has Γ Δ (.dict d) T φ := by
  induction h with
  | var hk => exact X.dvar hk
  | slot _ hs ih => exact X.slot ih hs
  | ap _ _ ih₁ ih₂ => exact X.dap ih₁ ih₂
  | inst hI => exact X.inst hI
  | strct h₁ h₂ => exact X.strct h₁ h₂
  | prim h₁ h₂ => exact X.prim h₁ h₂

theorem DTyp.strengthen {Δ : List Cn} {d : Dict} {T : TTy} (h : DTyp P Δ d T) (hd : d.NoVar) :
    DTyp P [] d T := by
  induction h with
  | var => exact (hd _ rfl).elim
  | slot _ hs ih => exact .slot (ih hd) hs
  | ap _ _ ih₁ ih₂ => exact .ap (ih₁ fun k h => hd k (.inl h)) (ih₂ fun k h => hd k (.inr h))
  | inst hI => exact .inst hI
  | strct h₁ h₂ => exact .strct h₁ h₂
  | prim h₁ h₂ => exact .prim h₁ h₂

variable {Sg : Sig} {N : Names} {root : Roots}

theorem trDt_noVar {d : Dict} (hd : d.NoVar) (δ : Nat → Nat) : trDt N δ d = trDt N id d := by
  induction d with
  | var k => exact (hd k rfl).elim
  | slot d i ih => simp only [trDt, ih hd]
  | ap d e ih₁ ih₂ =>
    simp only [trDt, ih₁ fun k h => hd k (.inl h), ih₂ fun k h => hd k (.inr h)]
  | _ => rfl

theorem trDt_noIns {δ : Nat → Nat} {d : Dict} : (trDt N δ d).NoIns := by
  induction d <;> simp_all [trDt, Flick.Tm.NoIns, Flick.Tm.Has]

theorem trDt_noAlts {δ : Nat → Nat} {d : Dict} : ¬ IsAlts (trDt N δ d) := by
  cases d <;> simp [trDt, IsAlts]

theorem trDt_closed {d : Dict} (hd : d.NoVar) (δ : Nat → Nat) : (trDt N δ d).Closed := by
  induction d with
  | var k => exact (hd k rfl).elim
  | slot d i ih => exact ih hd
  | ap d e ih₁ ih₂ =>
    exact fun i h => h.elim (ih₁ (fun k h => hd k (.inl h)) i) (ih₂ (fun k h => hd k (.inr h)) i)
  | _ => exact fun _ h => h

theorem trDt_shape (δ : Nat → Nat) (d : Dict) : ¬ Value (trDt N δ d) ∧
    (∀ K, (trDt N δ d).head ≠ .ctor K) ∧ (∀ c, trDt N δ d ≠ .const c) ∧
    ∀ c x, trDt N δ d ≠ .app (.const c) x := by
  induction d with
  | ap d e ih₁ _ =>
    simp only [trDt]
    generalize trDt N δ d = A at ih₁
    generalize trDt N δ e = B
    refine ⟨fun hv => ?_, fun K => by simpa [Flick.Tm.head] using ih₁.2.1 K,
      fun c => by simp, fun c x h => ?_⟩
    · cases hv with
      | ctorApp _ hK => exact ih₁.2.1 _ hK
      | constApp => exact ih₁.2.2.1 _ rfl
    · simp only [Flick.Tm.app.injEq] at h; exact ih₁.2.2.1 c h.1
  | _ =>
    refine ⟨fun hv => ?_, fun K => by simp [trDt, Flick.Tm.head], fun c => by simp [trDt],
      fun c x => by simp [trDt]⟩
    simp only [trDt] at hv; cases hv

/-- A closed typed dictionary: its translation is typed in the core at the pure stage. -/
theorem DTyp.core (hR : Real P Sg N) {d : Dict} {T : TTy} (h : DTyp P [] d T) :
    Typ Sg [] Facts.none [] (trDt N id d) (.ex (trTT N T) .pure) := by
  obtain ⟨e', he, ht⟩ := DTyp.target (typingTarget hR) h [] .P [] Facts.none [] id id (Corr.nil id id)
  cases he with
  | dict hd => rw [← hd.eq]; exact ht

/-- **`DSim` pairs of typed dictionaries translate to related terms.** -/
theorem dsim_cg (hR : Real P Sg N) (hRun : RealRun P N root) {d d' : Dict} (h : DSim P d d') :
    ∀ {Δ T}, DTyp P Δ d T → ∀ δ, Cg (DP Sg root) false (trDt N δ d) (trDt N δ d') := by
  induction h with
  | var => intro _ _ _ _; exact .var
  | inst => intro _ _ _ _; exact .glob
  | strct => intro _ _ _ _; exact .glob
  | prim => intro _ _ _ _; exact .glob
  | slot _ ih => intro Δ T ht δ; cases ht with
    | slot ht _ => exact .proj (ih ht δ)
  | ap _ _ ih₁ ih₂ => intro Δ T ht δ; cases ht with
    | ap h₁ h₂ => exact .app (ih₁ h₁ δ) (ih₂ h₂ δ)
  | base hd hd' heq =>
    intro Δ T ht δ
    rw [trDt_noVar hd, trDt_noVar hd']
    exact .base ⟨(trDt_shape id _).1, trDt_closed hd' id, ⟨trDt_noIns, trDt_noIns, trDt_noAlts⟩,
      eqv_sem hRun heq,
      Facts.none, _, DTyp.core hR (ht.strengthen hd)⟩

end Flick.Elab
