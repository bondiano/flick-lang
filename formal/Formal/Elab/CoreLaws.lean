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
    | ob n a => rcases hx with hx | hx <;> exact hx.elim
    | obv v a => rcases hx with hx | hx <;> exact hx.elim

/-- A row `⟦t⟧` holds no obligation. -/
theorem trT_asRow_ob (t : Flick.Ty) (n : Flick.Name) (a : Atom) : ¬ t.asRow (.ob n a) := by
  exact Flick.Ty.asRow_ob t n a

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
    | ob n a => exact (trT_asRow_ob _ n a hx).elim
    | obv v G a => exact (trT_asRow_obv _ v a G hx).elim
    | atm a => exact .inr hx
    | nm n => exact .inr hx
    | rv a => exact .inr hx
  · cases x with
    | atm a => exact .inl (.inl (.inl hx))
    | _ => exact hx.elim

theorem has_var {Γ Δ x s ts φ} (hx : Γ[x]? = some s) (hk : ts.length = s.k)
    (hb : Bounds.Hold (s.bd.map (Bound.subst (inst ts)))) :
    Has S N Γ Δ (.var x) (s.ty.subst (inst ts)) φ := fun _ _ _ _ _ hc =>
  ⟨_, .var, .var (σ := trS N s) (ts := ts.map (trT N)) (by rw [hc.var, hx]; rfl)
    (by simpa [trS] using hk) (holds_trS hb)
    (by rw [Flick.Scheme.inst, trS, trTT_inst_tr]; exact .refl)⟩

/-- A lambda typed under fresh variables, by (∀I). -/
theorem has_allLam {Γ Δ k b t ψ}
    (h : Has S N (Γ.map (·.subst (up k))) (Δ.map (·.subst (up k))) (.lam b) (.mono t) .P) :
    Has S N Γ Δ (.lam b) (.mono (.all k t)) ψ := by
  intro Ψ Θ Γc γ δ hc
  obtain ⟨e', he', te⟩ := h (Ψ.map (Flick.Ty.up k)) (Θ.substT (shiftV k)) _ _ _ (hc.up N k)
  cases he' with
  | lam hb => exact ⟨_, .lam hb, .allI .lam te⟩

/-- A scheme without value bounds has no facts. -/
theorem trS_nil (k : Nat) (t : TTy) (C : Caps) (r : Flick.Ty) : ¬ (trS N ⟨k, t, []⟩).cs C r := by
  simp only [trS, trFacts]; split <;> exact id

theorem has_allVar {Γ Δ x k t ψ} (hx : Γ[x]? = some ⟨k, .mono t, []⟩) (hk : 0 < k) :
    Has S N Γ Δ (.var x) (.mono (.all k t)) ψ := fun _ _ _ _ _ hc =>
  ⟨_, .var, .allVar (σ := trS N ⟨k, .mono t, []⟩) (by rw [hc.var, hx]; rfl) hk (trS_nil _ _)⟩

theorem has_varAll {Γ Δ x k₀ t₀ ts k t us φ} (hx : Γ[x]? = some ⟨k₀, .mono t₀, []⟩)
    (hts : ts.length = k₀) (hA : t₀.subst (inst ts) = .all k t) (hus : us.length = k) :
    Has S N Γ Δ (.var x) (.mono (t.subst (inst us))) φ := fun _ _ _ _ _ hc => by
  have key : ∀ (u : Ty) (vs : List Ty), trT N (u.subst (inst vs)) = (trT N u).substT (instT (vs.map (trT N))) := by
    intro u vs; rw [trT_subst]; congr 1; funext a; exact trσ_inst N vs a
  have hA' := congrArg (trT N) hA
  rw [key] at hA'
  refine ⟨_, .var, .var (σ := trS N ⟨k₀, .mono t₀, []⟩) (ts := ts.map (trT N)) (by rw [hc.var, hx]; rfl)
    (by simpa [trS] using hts) (fun _ _ h => (trS_nil _ _ _ _ h).elim) ?_⟩
  show LeLiveI S ((trT N t₀).substT (instT (ts.map (trT N)))) (trT N (t.subst (inst us)))
  rw [hA', key]
  exact .inst (by simpa using hus) .refl

theorem has_const (hR : Real P S N) {Γ Δ c s ts φ} (hc : P.consts c = some s)
    (hk : ts.length = s.k) (hb : Bounds.Hold (s.bd.map (Bound.subst (inst ts)))) :
    Has S N Γ Δ (.const c) ((dicts .cnil s.ctx (.mono s.ty)).subst (inst ts)) φ := fun _ _ _ _ _ _ =>
  ⟨_, .const, .glob (hR.const hc) (ts := ts.map (trT N)) (by simpa using hk)
    (holds_trBd hb) (by rw [Flick.Scheme.inst, trTT_inst_tr]; exact .refl)⟩

theorem has_app {Γ Δ f x a b q ψ φ} (hf : Has S N Γ Δ f (.mono (.arr ψ a b q)) φ)
    (hx : Has S N Γ Δ x (.mono a) φ) (hψ : ψ.Applies φ q) :
    Has S N Γ Δ (.app f x) (.mono b) φ := by
  intro Ψ Θ Γc γ δ hc
  obtain ⟨f', hf', tf⟩ := hf Ψ Θ Γc γ δ hc
  obtain ⟨x', hx', tx⟩ := hx Ψ Θ Γc γ δ hc
  simp only [trTT, trT_arr] at tf
  exact ⟨_, .app hf' hx', .app tf tx (trStage_use_le hψ) .refl⟩

theorem has_dapp {Γ Δ f x a t q φ} (hf : Has S N Γ Δ f (.arr .P a t q) φ) (hx : Has S N Γ Δ x a φ) :
    Has S N Γ Δ (.app f x) t φ := by
  intro Ψ Θ Γc γ δ hc
  obtain ⟨f', hf', tf⟩ := hf Ψ Θ Γc γ δ hc
  obtain ⟨x', hx', tx⟩ := hx Ψ Θ Γc γ δ hc
  exact ⟨_, .app hf' hx', .app tf tx trivial .refl⟩

theorem has_dap {Γ Δ f x a t q φ} (hf : Has S N Γ Δ (.dict f) (.arr .P a t q) φ)
    (hx : Has S N Γ Δ (.dict x) a φ) : Has S N Γ Δ (.dict (.ap f x)) t φ := by
  intro Ψ Θ Γc γ δ hc
  obtain ⟨f', hf', tf⟩ := hf Ψ Θ Γc γ δ hc
  obtain ⟨x', hx', tx⟩ := hx Ψ Θ Γc γ δ hc
  cases hf' with | dict hf' => cases hx' with | dict hx' =>
  exact ⟨_, .dict (.ap hf' hx'), .app tf tx trivial .refl⟩

theorem has_dvar {Γ Δ k q φ} (hk : Δ[k]? = some q) : Has S N Γ Δ (.dict (.var k)) (.dict q) φ :=
  fun _ _ _ _ _ hc => ⟨_, .dict .var, .var (ts := []) (by rw [hc.dict, hk]; rfl) rfl
    (fun _ _ h => (h : False).elim) (by rw [Flick.Scheme.inst, substT_instT_nil]; exact .refl)⟩

theorem has_ite {Γ Δ c a b t φ} (hc : Has S N Γ Δ c (.mono boolTy) φ) (ha : Has S N Γ Δ a (.mono t) φ)
    (hb : Has S N Γ Δ b (.mono t) φ) : Has S N Γ Δ (.ite c a b) (.mono t) φ := by
  intro Ψ Θ Γc γ δ hC
  obtain ⟨c', hc', tc⟩ := hc Ψ Θ Γc γ δ hC
  obtain ⟨a', ha', ta⟩ := ha Ψ Θ Γc γ δ hC
  obtain ⟨b', hb', tb⟩ := hb Ψ Θ Γc γ δ hC
  have tc' : Typ S Ψ Θ Γc c' (.ex (.base (Lit.bool true).base) (trStage N φ)) := by
    have := trT_bool N; simp only [trTT] at tc; rwa [this] at tc
  exact ⟨_, .ite hc' ha' hb', .match_ tc' (.acons .lit ta (.acons .wild tb .anil))
    (.inl fun _ _ => ⟨.wild, by simp [Flick.Tm.unguarded], rfl⟩)⟩

theorem has_caseOpt (hR : Real P S N) {Γ Δ e n s a t φ} (he : Has S N Γ Δ e (.mono (optTy a)) φ)
    (hn : Has S N Γ Δ n (.mono t) φ) (hs : Has S N (⟨0, .mono a, []⟩ :: Γ) Δ s (.mono t) φ) :
    Has S N Γ Δ (.caseOpt e n s) (.mono t) φ := by
  intro Ψ Θ Γc γ δ hc
  obtain ⟨e', he', te⟩ := he Ψ Θ Γc γ δ hc
  obtain ⟨n', hn', tn⟩ := hn Ψ Θ Γc γ δ hc
  obtain ⟨s', hs', ts⟩ := hs Ψ Θ _ _ _ (hc.cons N ⟨0, .mono a, []⟩)
  have pat : PatTy S true (.app (.ctor N.some) .var) (trT N (optTy a)) [trT N a] := by
    have hh := PatTy.head_plain (S := S) hR.some hR.some_res (us := [trT N a]) rfl
    have e : Sig.ctorTy [.tvar 0] (N.con 3) [trT N a] =
        .arrow (trT N a) .pure (.tapp (.adt (N.con 3)) (trT N a)) Caps.none := by
      simp [Sig.ctorTy, Flick.Ty.arrows, Flick.Ty.tcon, Flick.Ty.substT, instT]
    rw [e] at hh
    rw [trT_opt]
    exact .adt (.app hh .var) rfl
  exact ⟨_, .caseOpt he' hn' hs', .match_ te (.acons pat ts (.acons .wild tn .anil))
    (.inl fun _ _ => ⟨.wild, by simp [Flick.Tm.unguarded], rfl⟩)⟩

/-- (Lam) by use: the body runs where the closure is called, at the stage of its arrow with
its qualifier live; what the closure holds is not asked. -/
theorem has_lam {Γ Δ b a t q φ ψ} (hb : Has S N (⟨0, .mono a, []⟩ :: Γ) Δ b (.mono t) φ) :
    Has S N Γ Δ (.lam b) (.mono (.arr φ a t q)) ψ := by
  intro Ψ Θ Γc γ δ hc
  obtain ⟨b', hb', tb⟩ := hb Ψ Θ _ _ _ (hc.cons N ⟨0, .mono a, []⟩)
  simp only [trTT, trT_arr]
  exact ⟨_, .lam hb', .lam (tb.sub ⟨.refl, Stage.le_use _ _⟩) (hb'.noIns N) .refl⟩

/-- (Let-Sig) by (Let-Gen), its right-hand side typed under the facts of its value bounds;
by (Let-Mono) when it has no variables. -/
theorem has_let {Γ Δ e b k t bd u φ₁ φ}
    (he : Has S N (Γ.map (·.subst (up k))) (Δ.map (·.subst (up k))) e t φ₁)
    (hk : k ≠ 0 → φ₁ = .P) (hφ : φ₁ = .P ∨ φ₁ = φ)
    (hb : Has S N (⟨k, t, bd⟩ :: Γ) Δ b u φ) :
    Has S N Γ Δ (.let_ e b) u φ := by
  intro Ψ Θ Γc γ δ hc
  obtain ⟨b', hb', tb⟩ := hb Ψ Θ _ _ _ (hc.cons N ⟨k, t, bd⟩)
  have gen : φ₁ = .P → ∃ e', Tr N γ δ (.let_ e b) e' ∧
      Typ S Ψ Θ Γc e' (.ex (trTT N u) (trStage N φ)) := fun h => by
    subst h
    obtain ⟨e', he', te⟩ :=
      he (Ψ.map (Flick.Ty.up k)) (Facts.union (Θ.substT (shiftV k)) (trFacts N k bd)) _ _ _
        (hc.up N k)
    exact ⟨_, .let_ he' hb', .letGen te tb⟩
  rcases hφ with h | h
  · exact gen h
  · by_cases hk0 : k = 0
    · subst hk0; subst h
      have hc0 := hc.up N 0
      simp only [Flick.List.map_sup_zero] at hc0
      obtain ⟨e', he', te⟩ := he Ψ Θ _ _ _ hc0
      exact ⟨_, .let_ he' hb', .let_ te tb⟩
    · exact gen (hk hk0)

end Flick.Elab
