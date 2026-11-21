import Formal.Elab.CoreLawsBase

/-!
# Derived typing rules and `Ty.Le` / `Cn.Serves` translation (`Formal.Elab.CoreLaws`)
-/

namespace Flick.Elab

variable {P : Prog} {S : Sig} {N : Names}

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

/-- Elaboration subtyping translates to core `LeLive`. -/
theorem Ty.Le.tr (hR : Real P S N) {a b : Ty} (h : Ty.Le P.dataVar a b) :
    LeLive S (trT N a) (trT N b) := by
  induction h with
  | refl => exact .refl
  | @arr φ₁ φ₂ a₁ a₂ b₁ b₂ q₁ q₂ _ hst _ hsh iha ihb =>
    rw [trT_arr, trT_arr]
    refine .arrow iha (hst.tr N) ihb fun x hx => ?_
    rw [← trT_arr] at hx ⊢
    obtain ⟨n, rfl, hn⟩ := (trT_reach N _ x).1 hx
    exact trT_reach_v N (hsh n hn)
  | @data T us us' h2 h4 hlen hsh hpos hneg hinv ihpos ihneg =>
    refine .data (T := N.con T) (us := us.map (trT N)) (us' := us'.map (trT N))
      (by simp [hlen]) ?_ ?_ ?_ ?_ (trT_tcon h2 h4 us) (trT_tcon h2 h4 us')
    · intro j x x' hx hx' a ha
      simp only [List.getElem?_map, Option.map_eq_some_iff] at hx hx'
      obtain ⟨u, hu, rfl⟩ := hx; obtain ⟨u', hu', rfl⟩ := hx'
      obtain ⟨n, rfl, hn⟩ := (trT_reach N u a).1 ha
      exact trT_reach_v N (hsh j u u' hu hu' n hn)
    · intro j x x' hx hx' _ hsv
      simp only [List.getElem?_map, Option.map_eq_some_iff] at hx hx'
      obtain ⟨u, hu, rfl⟩ := hx; obtain ⟨u', hu', rfl⟩ := hx'
      cases hv : P.dataVar T j with
      | pos => exact ihpos j u u' hu hu' hv
      | inv => rw [hinv j u u' hu hu' hv]; exact .refl
      | neg =>
        have := hR.dataVar T j (by simp [hv])
        rw [hv] at this; simp [trVar] at this; rw [this] at hsv; cases hsv
      | bi =>
        have := hR.dataVar T j (by simp [hv])
        rw [hv] at this; simp [trVar] at this; rw [this] at hsv; cases hsv
    · intro j x x' hx hx' _ hsv
      simp only [List.getElem?_map, Option.map_eq_some_iff] at hx hx'
      obtain ⟨u, hu, rfl⟩ := hx; obtain ⟨u', hu', rfl⟩ := hx'
      cases hv : P.dataVar T j with
      | neg => exact ihneg j u u' hu hu' hv
      | inv => rw [hinv j u u' hu hu' hv]; exact .refl
      | pos =>
        have := hR.dataVar T j (by simp [hv])
        rw [hv] at this; simp [trVar] at this; rw [this] at hsv; cases hsv
      | bi =>
        have := hR.dataVar T j (by simp [hv])
        rw [hv] at this; simp [trVar] at this; rw [this] at hsv; cases hsv
    · intro j x x' hx hx' _ hsv
      simp only [List.getElem?_map, Option.map_eq_some_iff] at hx hx'
      obtain ⟨u, hu, rfl⟩ := hx; obtain ⟨u', hu', rfl⟩ := hx'
      cases hv : P.dataVar T j with
      | inv => rw [hinv j u u' hu hu' hv]; exact .refl
      | pos =>
        have := hR.dataVar T j (by simp [hv])
        rw [hv] at this; simp [trVar] at this; rw [this] at hsv; cases hsv
      | neg =>
        have := hR.dataVar T j (by simp [hv])
        rw [hv] at this; simp [trVar] at this; rw [this] at hsv; cases hsv
      | bi =>
        have := hR.dataVar T j (by simp [hv])
        rw [hv] at this; simp [trVar] at this; rw [this] at hsv; cases hsv

/-- Variance-directed class service `H` serves `Q` translates to core dictionary subtyping
`LeLive S ⟦D_H⟧ ⟦D_Q⟧`. -/
theorem Cn.Serves.tr (hR : Real P S N) {Q H : Cn} (h : Cn.Serves P Q H) :
    LeLive S (trD N H) (trD N Q) := by
  obtain ⟨hcls, hlen, hpos, hneg, hinv⟩ := h
  simp only [trD, hcls, ← hlen]
  refine .dict (by simp [hlen]) ?_ ?_ ?_ ?_
  · intro j x x' hx hx' hsv
    simp only [List.getElem?_map, Option.map_eq_some_iff] at hx hx'
    obtain ⟨uH, huH, rfl⟩ := hx; obtain ⟨uQ, huQ, rfl⟩ := hx'
    cases hv : P.clsVar Q.cls j with
    | pos => exact (hpos j uQ uH huQ huH hv).tr hR
    | inv => rw [hinv j uQ uH huQ huH (.inl hv)]; exact .refl
    | bi => rw [hinv j uQ uH huQ huH (.inr hv)]; exact .refl
    | neg =>
      have := hR.clsVar Q.cls Q.args.length j (by simp [hv])
      rw [hv] at this; simp [trVar] at this; rw [this] at hsv; cases hsv
  · intro j x x' hx hx' hsv
    simp only [List.getElem?_map, Option.map_eq_some_iff] at hx hx'
    obtain ⟨uH, huH, rfl⟩ := hx; obtain ⟨uQ, huQ, rfl⟩ := hx'
    cases hv : P.clsVar Q.cls j with
    | neg => exact (hneg j uQ uH huQ huH hv).1.tr hR
    | inv => rw [hinv j uQ uH huQ huH (.inl hv)]; exact .refl
    | bi => rw [hinv j uQ uH huQ huH (.inr hv)]; exact .refl
    | pos =>
      have := hR.clsVar Q.cls Q.args.length j (by simp [hv])
      rw [hv] at this; simp [trVar] at this; rw [this] at hsv; cases hsv
  · intro j x x' hx hx' hsv a ha
    simp only [List.getElem?_map, Option.map_eq_some_iff] at hx hx'
    obtain ⟨uH, huH, rfl⟩ := hx; obtain ⟨uQ, huQ, rfl⟩ := hx'
    cases hv : P.clsVar Q.cls j with
    | neg =>
      obtain ⟨n, rfl, hn⟩ := (trT_reach N uH a).1 ha
      exact trT_reach_v N ((hneg j uQ uH huQ huH hv).2 n hn)
    | inv => rw [hinv j uQ uH huQ huH (.inl hv)]; exact ha
    | bi => rw [hinv j uQ uH huQ huH (.inr hv)]; exact ha
    | pos =>
      have := hR.clsVar Q.cls Q.args.length j (by simp [hv])
      rw [hv] at this; simp [trVar] at this; rw [this] at hsv; cases hsv
  · intro j x x' hx hx' hsv
    simp only [List.getElem?_map, Option.map_eq_some_iff] at hx hx'
    obtain ⟨uH, huH, rfl⟩ := hx; obtain ⟨uQ, huQ, rfl⟩ := hx'
    cases hv : P.clsVar Q.cls j with
    | inv => rw [hinv j uQ uH huQ huH (.inl hv)]; exact .refl
    | bi => rw [hinv j uQ uH huQ huH (.inr hv)]; exact .refl
    | pos =>
      have := hR.clsVar Q.cls Q.args.length j (by simp [hv])
      rw [hv] at this; simp [trVar] at this
      rcases hsv with hsv | hsv <;> (rw [this] at hsv; cases hsv)
    | neg =>
      have := hR.clsVar Q.cls Q.args.length j (by simp [hv])
      rw [hv] at this; simp [trVar] at this
      rcases hsv with hsv | hsv <;> (rw [this] at hsv; cases hsv)

end Flick.Elab
