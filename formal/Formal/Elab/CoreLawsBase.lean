import Formal.Elab.CoreTr
import Formal.SubAdm

/-!
# λFlick⁺ as a `Target`: the typing and its laws

`Has Γ Δ e t φ`: in every core context that corresponds to `Γ` and `Δ` (`Corr`), `e` is a
core term typed at `⟦t⟧ ! ⟦φ⟧`, for every store typing `Ψ` and facts `Θ`.

`Real P S N` is what the core's signature `S` has to declare for the program: a record type
`D_C` for each class and each number of parameters, with a superclass slot `N.slot i` of the
superclass's dictionary type and, for the class's own parameters, a field `N.meth m` for each
method `m`, with the method's scheme, its own variables bound first; the schemes of the
globals of instances, constants of Γ₀ and the compiler's dictionaries; the constructor
`Some`. Each law of `Target` below is one rule of the core (`CoreT6a.lean` assembles them).
-/

namespace Flick.Elab

/-- `∀β̄. D_ctx →P D_head` of an instance. -/
def instTy (ins : Inst) : TTy := dicts .cnil ins.ctx (.dict ⟨ins.cls, ins.head⟩)

/-- `D_{C τ₁} →P … →P D_{C τ}` of a structural dictionary. -/
def strctTy (C : Nat) (τ : Ty) (ts : List Ty) : TTy :=
  dicts .cnil (ts.map fun t => ⟨C, [t]⟩) (.dict ⟨C, [τ]⟩)

/-- The type of a method after its class constraint. -/
def methTy (rest : List Cn) (t : Ty) : TTy := dicts .cnil rest (.mono t)

/-- The variables of a method scheme in the order of a field: its own `kk` variables first,
then the `p` class parameters. -/
def rot (p kk : Nat) : Atom → Flick.Ty
  | .v j => if j < p then .tvar (j + kk) else if j < p + kk then .tvar (j - p) else .tvar j
  | .r q => .rg q

variable (P : Prog) (S : Sig) (N : Names)

/-- Value bounds that mention only the `k` variables of their scheme. -/
def BdClosed (k : Nat) (bd : List (Ty × Ty)) : Prop :=
  ∀ σ σ' : Nat → Ty, (∀ j, j < k → σ j = σ' j) → bd.map (Bound.subst σ) = bd.map (Bound.subst σ')

/-- What the core's signature declares for the program. -/
structure Real : Prop where
  dict : ∀ C n, ∃ fs, S.dict (N.cls C n) = some (n, fs)
  slot : ∀ {C n fs i Sup}, S.dict (N.cls C n) = some (n, fs) → P.super ⟨C, vars n⟩ i = some Sup →
    fs.lookup (N.slot i) = some ⟨0, Facts.none, trD N Sup⟩
  meth : ∀ {m C s}, P.methods m = some (C, s) → ∃ p rest, s.ctx = ⟨C, vars p⟩ :: rest ∧
    p ≤ s.k ∧ (methTy rest s.ty).Closed s.k ∧ BdClosed s.k s.bd ∧
    ∀ fs, S.dict (N.cls C p) = some (p, fs) →
      fs.lookup (N.meth m) = some ⟨s.k - p, (trBd N s.bd).substT (rot p (s.k - p)),
        (trTT N (methTy rest s.ty)).substT (rot p (s.k - p))⟩
  inst : ∀ {I ins}, P.insts I = some ins → ∃ k,
    S.glob (N.inst I) = some ⟨k, Facts.none, trTT N (instTy ins)⟩ ∧ (instTy ins).Closed k
  const : ∀ {c s}, P.consts c = some s →
    S.glob (N.const c) = some ⟨s.k, trBd N s.bd, trTT N (dicts .cnil s.ctx (.mono s.ty))⟩
  strct : ∀ {C τ ts}, P.structC C = true → P.fields τ = some ts → ∃ k,
    S.glob (N.strct C τ) = some ⟨k, Facts.none, trTT N (strctTy C τ ts)⟩ ∧
      (strctTy C τ ts).Closed k
  prim : ∀ {q p}, P.primC q.cls = true → P.prims q = some p → ∃ k,
    S.glob (N.prim p q) = some ⟨k, Facts.none, trD N q⟩ ∧ (TTy.dict q).Closed k
  some : S.ctor N.some = some (1, [.tvar 0], N.con 3)
  /-- `Some` builds `Option a`: it is an ordinary constructor. -/
  some_res : S.res N.some = idTs 1
  /-- A constructor of the program is one of the core: its dictionaries first, then its
  fields, building `T ρ̄` of its source type. -/
  ctor : ∀ {K c}, P.ctors K = Option.some c →
    S.ctor (N.ctor K) = Option.some (c.k, c.ctx.map (trD N) ++ c.fields.map (trT N), N.con c.T) ∧
      S.res (N.ctor K) = c.res.map (trT N) ∧ c.T ≠ 2 ∧ c.T ≠ 4
  /-- The fields of a constructor reach no more than the type it builds (`Sig.Holds`), the
  premise of (Ctor). -/
  ctor_holds : ∀ {K c}, P.ctors K = Option.some c → ∀ us : List Flick.Ty, us.length = c.k →
    Sig.Holds (c.ctx.map (trD N) ++ c.fields.map (trT N)) (N.con c.T) (c.res.map (trT N)) us
  /-- In a program with data types, type constructors have names apart. -/
  con_inj : ∀ {K c}, P.ctors K = Option.some c → ∀ {c₁ c₂ : Nat}, c₁ ≠ c₂ →
    trT N (.con c₁) ≠ trT N (.con c₂)
  /-- Non-invariant data-constructor variances match the core signature's `S.var`. -/
  dataVar : ∀ T j, P.dataVar T j ≠ .inv → S.var (N.con T) j = trVar (P.dataVar T j) := by
    intro _ _ h; exact absurd rfl h
  /-- Non-invariant class-parameter variances match the core signature's `S.dictVar`. -/
  clsVar : ∀ C n j, P.clsVar C j ≠ .inv → S.dictVar (N.cls C n) j = trVar (P.clsVar C j) := by
    intro _ _ _ h; exact absurd rfl h

/-- `Γ, ⟦Δ⟧ ⊢⁺ e : t ! φ` in the core. -/
def Has (Γ : List TSch) (Δ : List Cn) (e : Tm) (t : TTy) (φ : Stage) : Prop :=
  ∀ Ψ Θ Γc γ δ, Corr N Γc Γ Δ γ δ →
    ∃ e', Tr N γ δ e e' ∧ Typ S Ψ Θ Γc e' (.ex (trTT N t) (trStage N φ))

variable {P S N}

theorem holds_none {k : Nat} {t : Flick.Ty} {σ : Atom → Flick.Ty} {Θ : Facts} {ts : List Flick.Ty} :
    ((⟨k, Facts.none, t⟩ : Flick.Scheme).substT σ).Holds Θ ts := by
  intro C r h; simp [Flick.Scheme.substT, Facts.none_substT, Facts.none] at h

/-- (Inst) of the value bounds in the core: the facts of a translated scheme hold at the
translation of an instance where the bounds hold. -/
theorem holds_trBd {k : Nat} {bd : List (Ty × Ty)} {t : Flick.Ty} {ts : List Ty} {Θ : Facts}
    (h : Bounds.Hold (bd.map (Bound.subst (inst ts)))) :
    (⟨k, trBd N bd, t⟩ : Flick.Scheme).Holds Θ (ts.map (trT N)) :=
  trBd_holds N (fun a => (trσ_inst N ts a).symm) h Θ

theorem holds_trS {s : TSch} {ts : List Ty} {Θ : Facts}
    (h : Bounds.Hold (s.bd.map (Bound.subst (inst ts)))) : (trS N s).Holds Θ (ts.map (trT N)) :=
  trFacts_holds N (fun a => (trσ_inst N ts a).symm) h Θ

theorem substT_instT_nil (t : Flick.Ty) : t.substT (instT []) = t := by
  have := Flick.Ty.up_substT t 0 [] rfl; rwa [Flick.Ty.up_zero] at this

theorem trTT_inst_tr (t : TTy) (ts : List Ty) :
    (trTT N t).substT (instT (ts.map (trT N))) = trTT N (t.subst (inst ts)) := by
  rw [trTT_subst]; exact Flick.Ty.substT_congr fun a => (trσ_inst N ts a).symm

/-- An atom of `⌈⟦q⟧⌉` for a qualifier `q` the stage `R r e` keeps live is in the row `⟦r⟧`
(`trT_keeps`). -/
theorem trT_kept {r e q : Ty} (hk : (Stage.R r e).Keeps q) {a : Atom}
    (ha : (trT N q).reach a) : (trT N r).asRow (.atm a) := by
  obtain ⟨n, rfl, hn⟩ := (trT_reach N q a).1 ha
  exact trT_keeps N (hk n hn)

/-- A call of an arrow at `ψ` with the qualifier `q` runs at the stage `φ` that keeps `q`
live: the atoms of `⌈⟦q⟧⌉` are in the row of `⟦φ⟧`, and so are those of `Live C` in the row
of `ψ`, which `φ` keeps live too. -/
theorem trStage_use_le {ψ φ : Stage} {q : Ty} (h : ψ.Applies φ q) :
    (Stage.use (trStage N ψ) (trT N q).reach).le (trStage N φ) := by
  rcases h with rfl | ⟨hk, rfl | ⟨C, r, e, rfl, rfl, hC⟩⟩
  · trivial
  · cases ψ with
    | P => trivial
    | R r e =>
      refine ⟨fun x hx => ?_, fun _ h => h⟩
      rcases hx with hx | hx
      · exact hx
      · cases x with
        | atm a => exact trT_kept hk hx
        | _ => exact hx.elim
  · refine ⟨fun x hx => ?_, fun _ h => h⟩
    cases x with
    | atm a =>
      rcases hx with ((hx | hx) | hx) | hx
      · exact trT_kept hC hx
      · exact hx.elim
      · exact hx
      · exact trT_kept hk hx
    | nm n =>
      rcases hx with (hx | hx) | hx
      · exact hx.elim
      · exact hx
      · exact hx.elim
    | rv v =>
      rcases hx with (hx | hx) | hx
      · exact hx.elim
      · exact hx
      · exact hx.elim
    | ob n c a => rcases hx with hx | hx <;> first | exact hx.elim | exact (Flick.Ty.asRow_ob _ _ _ _ hx).elim
    | obv v G a => rcases hx with hx | hx <;> first | exact hx.elim | exact (Flick.Ty.asRow_obv _ _ _ _ hx).elim

/-- A row `⟦t⟧` holds no obligation. -/
theorem trT_asRow_ob (t : Flick.Ty) (n : Flick.Name) (c : TyClass) (a : Atom) :
    ¬ t.asRow (.ob n c a) := by
  exact Flick.Ty.asRow_ob t n c a

theorem trT_asRow_obv (t : Flick.Ty) (v a : Atom) (G : Flick.Name → Prop) :
    ¬ t.asRow (.obv v G a) := by
  exact Flick.Ty.asRow_obv t v a G

/-- What a call of an arrow at `R r e` with the qualifier `c` needs live, `R⟨Live c + r; e⟩`
holds by its row: elision moves the qualifier of a later stage into its row. -/
theorem trStage_use_live (r e c : Ty) :
    (Stage.use (trStage N (.R r e)) (trT N c).reach).le
      (Stage.use (trStage N (.R (.run (.cun c .cnil) r) e)) Caps.none) := by
  refine ⟨fun x hx => ?_, fun _ h => h⟩
  rcases hx with hx | hx
  · refine .inl ?_
    cases x with
    | ob n c a => exact (trT_asRow_ob _ n c a hx).elim
    | obv v G a => exact (trT_asRow_obv _ v a G hx).elim
    | atm a => exact .inr hx
    | nm n => exact .inr hx
    | rv a => exact .inr hx
  · cases x with
    | atm a => exact .inl (.inl (.inl hx))
    | _ => exact hx.elim

end Flick.Elab
