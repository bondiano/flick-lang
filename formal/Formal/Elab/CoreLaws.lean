import Formal.Elab.CoreTr

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

/-- What the core's signature declares for the program. -/
structure Real : Prop where
  dict : ∀ C n, ∃ fs, S.dict (N.cls C n) = some (n, fs)
  slot : ∀ {C n fs i Sup}, S.dict (N.cls C n) = some (n, fs) → P.super ⟨C, vars n⟩ i = some Sup →
    fs.lookup (N.slot i) = some ⟨0, Facts.none, trD N Sup⟩
  meth : ∀ {m C s}, P.methods m = some (C, s) → ∃ p rest, s.ctx = ⟨C, vars p⟩ :: rest ∧
    p ≤ s.k ∧ (methTy rest s.ty).Closed s.k ∧ ∀ fs, S.dict (N.cls C p) = some (p, fs) →
      fs.lookup (N.meth m) =
        some ⟨s.k - p, Facts.none, (trTT N (methTy rest s.ty)).substT (rot p (s.k - p))⟩
  inst : ∀ {I ins}, P.insts I = some ins → ∃ k,
    S.glob (N.inst I) = some ⟨k, Facts.none, trTT N (instTy ins)⟩ ∧ (instTy ins).Closed k
  const : ∀ {c s}, P.consts c = some s →
    S.glob (N.const c) = some ⟨s.k, Facts.none, trTT N (dicts .cnil s.ctx (.mono s.ty))⟩
  strct : ∀ {C τ ts}, P.structC C = true → P.fields τ = some ts → ∃ k,
    S.glob (N.strct C τ) = some ⟨k, Facts.none, trTT N (strctTy C τ ts)⟩ ∧
      (strctTy C τ ts).Closed k
  prim : ∀ {q p}, P.primC q.cls = true → P.prims q = some p → ∃ k,
    S.glob (N.prim p q) = some ⟨k, Facts.none, trD N q⟩ ∧ (TTy.dict q).Closed k
  some : S.ctor N.some = some (1, [.tvar 0], N.con 3)

/-- `Γ, ⟦Δ⟧ ⊢⁺ e : t ! φ` in the core. -/
def Has (Γ : List TSch) (Δ : List Cn) (e : Tm) (t : TTy) (φ : Stage) : Prop :=
  ∀ Ψ Θ Γc γ δ, Corr N Γc Γ Δ γ δ →
    ∃ e', Tr N γ δ e e' ∧ Typ S Ψ Θ Γc e' (.ex (trTT N t) (trStage φ))

variable {P S N}

theorem holds_none {k : Nat} {t : Flick.Ty} {σ : Atom → Flick.Ty} {Θ : Facts} {ts : List Flick.Ty} :
    ((⟨k, Facts.none, t⟩ : Flick.Scheme).substT σ).Holds Θ ts := by
  intro C r h; simp [Flick.Scheme.substT, Facts.none_substT, Facts.none] at h

theorem substT_instT_nil (t : Flick.Ty) : t.substT (instT []) = t := by
  have := Flick.Ty.up_substT t 0 [] rfl; rwa [Flick.Ty.up_zero] at this

theorem trTT_inst_tr (t : TTy) (ts : List Ty) :
    (trTT N t).substT (instT (ts.map (trT N))) = trTT N (t.subst (inst ts)) := by
  rw [trTT_subst]; exact Flick.Ty.substT_congr fun a => (trσ_inst N ts a).symm

theorem trStage_le {ψ φ : Stage} (h : ψ = .P ∨ ψ = φ) : (trStage ψ).le (trStage φ) := by
  rcases h with rfl | rfl
  · trivial
  · cases ψ
    · trivial
    · exact ⟨fun _ h => h, fun _ h => h⟩

theorem has_var {Γ Δ x s ts φ} (hx : Γ[x]? = some s) (hk : ts.length = s.k) :
    Has S N Γ Δ (.var x) (s.ty.subst (inst ts)) φ := fun _ _ _ _ _ hc =>
  ⟨_, .var, .var (ts := ts.map (trT N)) (by rw [hc.var, hx]; rfl) (by simpa [trS] using hk)
    (fun _ _ h => (h : False).elim) (by rw [Flick.Scheme.inst, trS, trTT_inst_tr]; exact .refl)⟩

theorem has_const (hR : Real P S N) {Γ Δ c s ts φ} (hc : P.consts c = some s)
    (hk : ts.length = s.k) :
    Has S N Γ Δ (.const c) ((dicts .cnil s.ctx (.mono s.ty)).subst (inst ts)) φ := fun _ _ _ _ _ _ =>
  ⟨_, .const, .glob (hR.const hc) (ts := ts.map (trT N)) (by simpa using hk)
    (fun _ _ h => (h : False).elim) (by rw [Flick.Scheme.inst, trTT_inst_tr]; exact .refl)⟩

theorem has_app {Γ Δ f x a b q ψ φ} (hf : Has S N Γ Δ f (.mono (.arr ψ a b q)) φ)
    (hx : Has S N Γ Δ x (.mono a) φ) (hψ : ψ = .P ∨ ψ = φ) : Has S N Γ Δ (.app f x) (.mono b) φ := by
  intro Ψ Θ Γc γ δ hc
  obtain ⟨f', hf', tf⟩ := hf Ψ Θ Γc γ δ hc
  obtain ⟨x', hx', tx⟩ := hx Ψ Θ Γc γ δ hc
  exact ⟨_, .app hf' hx', .app tf tx (trStage_le hψ) .refl⟩

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
  have tc' : Typ S Ψ Θ Γc c' (.ex (.base (Lit.bool true).base) (trStage φ)) := by
    have := trT_bool N; simp only [trTT] at tc; rwa [this] at tc
  exact ⟨_, .ite hc' ha' hb', .match_ tc' (.acons .lit ta (.acons .wild tb .anil))
    (.inl fun _ _ => ⟨.wild, by simp [Flick.Tm.unguarded], rfl⟩)⟩

theorem has_caseOpt (hR : Real P S N) {Γ Δ e n s a t φ} (he : Has S N Γ Δ e (.mono (optTy a)) φ)
    (hn : Has S N Γ Δ n (.mono t) φ) (hs : Has S N (⟨0, .mono a⟩ :: Γ) Δ s (.mono t) φ) :
    Has S N Γ Δ (.caseOpt e n s) (.mono t) φ := by
  intro Ψ Θ Γc γ δ hc
  obtain ⟨e', he', te⟩ := he Ψ Θ Γc γ δ hc
  obtain ⟨n', hn', tn⟩ := hn Ψ Θ Γc γ δ hc
  obtain ⟨s', hs', ts⟩ := hs Ψ Θ _ _ _ (hc.cons N ⟨0, .mono a⟩)
  have pat : PatTy S true (.app (.ctor N.some) .var) (trT N (optTy a)) [trT N a] := by
    have hh := PatTy.head (S := S) hR.some (us := [trT N a]) rfl
    have e : Sig.ctorTy [.tvar 0] (N.con 3) [trT N a] =
        .arrow (trT N a) .pure (.tapp (.adt (N.con 3)) (trT N a)) Caps.none := by
      simp [Sig.ctorTy, Flick.Ty.arrows, Flick.Ty.tcon, Flick.Ty.substT, instT]
    rw [e] at hh
    rw [trT_opt]
    exact .adt (.app hh .var) rfl
  exact ⟨_, .caseOpt he' hn' hs', .match_ te (.acons pat ts (.acons .wild tn .anil))
    (.inl fun _ _ => ⟨.wild, by simp [Flick.Tm.unguarded], rfl⟩)⟩

theorem has_lam {Γ Δ b a t q φ ψ} (hb : Has S N (⟨0, .mono a⟩ :: Γ) Δ b (.mono t) φ)
    (hh : ∀ n, heldT Γ (.lam b) n → (Ty.arr φ a t q).reach n) :
    Has S N Γ Δ (.lam b) (.mono (.arr φ a t q)) ψ := by
  intro Ψ Θ Γc γ δ hc
  obtain ⟨b', hb', tb⟩ := hb Ψ Θ _ _ _ (hc.cons N ⟨0, .mono a⟩)
  refine ⟨_, .lam hb', .lam tb (hb'.noIns N) ?_ .refl⟩
  rintro x (⟨i, s, hi, hs, hr⟩ | ⟨⟨p, hp, -⟩ | ⟨l, u, hl, -⟩⟩)
  · obtain ⟨n, rfl, hn⟩ := corr_held N hc ((Tr.lam hb').free N hi) hs hr
    rcases hh n hn with h | h | h
    · exact .inl (trT_reach_v N h)
    · exact .inr (.inl (trT_reach_v N h))
    · exact .inr (.inr (trT_reach_v N h))
  · exact absurd hp ((Tr.lam hb').has N _)
  · exact absurd hl ((Tr.lam hb').has N _)

theorem has_let {Γ Δ e b k t u φ₁ φ}
    (he : Has S N (Γ.map (·.subst (up k))) (Δ.map (·.subst (up k))) e t φ₁)
    (hk : k ≠ 0 → φ₁ = .P) (hφ : φ₁ = .P ∨ φ₁ = φ) (hb : Has S N (⟨k, t⟩ :: Γ) Δ b u φ) :
    Has S N Γ Δ (.let_ e b) u φ := by
  intro Ψ Θ Γc γ δ hc
  obtain ⟨b', hb', tb⟩ := hb Ψ Θ _ _ _ (hc.cons N ⟨k, t⟩)
  have gen : φ₁ = .P → ∃ e', Tr N γ δ (.let_ e b) e' ∧
      Typ S Ψ Θ Γc e' (.ex (trTT N u) (trStage φ)) := fun h => by
    subst h
    obtain ⟨e', he', te⟩ :=
      he (Ψ.map (Flick.Ty.up k)) (Facts.union (Θ.substT (shiftV k)) Facts.none) _ _ _ (hc.up N k)
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
