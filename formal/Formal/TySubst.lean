import Formal.RowSubst

/-!
# Type substitution preserves typing

`Γ;Θ;Ψ ⊢ e : τ ! φ` implies `Γ[σ];Θ[σ];Ψ[σ] ⊢ e : τ[σ] ! φ[σ]` for a
substitution `σ` of variables and run-time regions that keeps the regions `e`
writes down and, if `e` has an `inside` form, makes no other atom one of them
(`Fits`). Terms carry no types, so `e` does not change. Shifting, the
instantiation of a scheme, the region of a new scope and the renaming of regions
not yet created all fit.
-/

namespace Flick

variable {S : Sig}

def J.substT (σ : Atom → Ty) : J → J
  | .ex t φ => .ex (t.substT σ) (φ.substT σ)
  | .alts t t' φ => .alts (t.substT σ) (t'.substT σ) (φ.substT σ)
  | .halts ε H t φ => .halts (ε.substT σ) (H.substT σ) (t.substT σ) (φ.substT σ)

/-- A substitution that fits a term: it keeps the regions the term writes and the
`provide` regions of its `with` frames, and if the term has a run-time form, it brings
in no run-time region and sends each atom to an atom or to a type without one. -/
def Fits (σ : Atom → Ty) (e : Tm) : Prop :=
  (∀ p, (e.Has (.rgn p) ∨ e.Has (.prov p)) → σ (.r p) = .rg p) ∧
    (e.Has .ins → ∀ b p, (σ b).reach (.r p) → b = .r p) ∧
    (e.Has .ins → ∀ b a, (σ b).reach a → σ b = a.ty)

theorem Fits.sub {σ : Atom → Ty} {e e' : Tm} (h : Fits σ e) (hs : ∀ s, e'.Has s → e.Has s) :
    Fits σ e' :=
  ⟨fun p hp => h.1 p (hp.imp (hs _) (hs _)), fun hi => h.2.1 (hs _ hi), fun hi => h.2.2 (hs _ hi)⟩

theorem Fits.noIns {σ : Atom → Ty} {e : Tm} (hn : e.NoIns) (hr : ∀ p, σ (.r p) = .rg p) :
    Fits σ e :=
  ⟨fun p _ => hr p, fun hi => (hn hi).elim, fun hi => (hn hi).elim⟩

theorem shift_lift (k : Nat) (σ : Atom → Ty) (a : Atom) :
    (shiftV k a).substT (liftT k σ) = (σ a).up k := by
  cases a with
  | v i => have : ¬ i + k < k := by omega
           simp [shiftV, Ty.substT, liftT, this]
  | r p => simp [shiftV, Ty.substT, liftT]

theorem Atom.ty_up (a : Atom) (k : Nat) : a.ty.up k = (a.up k).ty := by
  cases a <;> simp [Atom.ty, Atom.up, Ty.up, Ty.substT, shiftV]

theorem Fits.lift {σ : Atom → Ty} {e : Tm} (h : Fits σ e) (k : Nat) : Fits (liftT k σ) e := by
  refine ⟨fun p hp => by simp [liftT, h.1 p hp, Ty.up, Ty.substT, shiftV], fun hi b p hb => ?_,
    fun hi b a hb => ?_⟩
  · have key : ∀ t : Ty, (t.up k).reach (.r p) → t.reach (.r p) := by
      intro t ht
      obtain ⟨a, ha, he⟩ := (Ty.reach_up t k _).mp ht
      cases a <;> simp [Atom.up] at he; subst he; exact ha
    cases b with
    | v i =>
      by_cases hik : i < k
      · simp [liftT, hik] at hb
      · simp only [liftT, hik, if_false] at hb
        exact absurd (h.2.1 hi _ p (key _ hb)) (by simp)
    | r q => simp only [liftT] at hb; exact h.2.1 hi _ p (key _ hb)
  · have up : ∀ b', (σ b').up k = liftT k σ b → σ b = σ b → (liftT k σ b) = a.ty := by
      intro b' hb' _
      rw [← hb'] at hb ⊢
      obtain ⟨a', ha', rfl⟩ := (Ty.reach_up _ k _).mp hb
      rw [h.2.2 hi b' a' ha', Atom.ty_up]
    cases b with
    | v i =>
      by_cases hik : i < k
      · simp only [liftT, hik, if_true] at hb ⊢
        simp [Ty.reach] at hb; subst hb; rfl
      · simp only [liftT, hik, if_false] at hb ⊢
        have := up (.v (i - k)) (by simp [liftT, hik]) rfl
        simpa [liftT, hik] using this
    | r q => exact up (.r q) (by simp [liftT]) rfl

theorem Ty.up_avoid_v0 (t : Ty) : ¬ (t.up 1).reach (.v 0) := by
  intro h
  obtain ⟨a, _, he⟩ := (Ty.reach_up t 1 _).mp h
  cases a <;> simp [Atom.up] at he

theorem ctx_substT (Δ : List Ty) (Γ : List Scheme) (σ : Atom → Ty) :
    (Δ.map Scheme.mono ++ Γ).map (Scheme.substT σ) =
      (Δ.map (Ty.substT σ)).map Scheme.mono ++ Γ.map (Scheme.substT σ) := by
  simp

theorem shift_comm (Ψ : List Ty) (k : Nat) (σ : Atom → Ty) :
    (Ψ.map (Ty.up k)).map (Ty.substT (liftT k σ)) = (Ψ.map (Ty.substT σ)).map (Ty.up k) := by
  simp [Ty.up_comm]

theorem shift_comm' (Γ : List Scheme) (k : Nat) (σ : Atom → Ty) :
    (Γ.map (Scheme.up k)).map (Scheme.substT (liftT k σ)) =
      (Γ.map (Scheme.substT σ)).map (Scheme.up k) := by
  simp [Scheme.up_comm]

theorem Facts.up_comm (Θ : Facts) (k : Nat) (σ : Atom → Ty) :
    (Θ.substT (shiftV k)).substT (liftT k σ) = (Θ.substT σ).substT (shiftV k) := by
  rw [Facts.substT_comp, Facts.substT_comp]; congr 1; funext a; exact shift_lift k σ a

theorem Row.up_comm (r : Row) (k : Nat) (σ : Atom → Ty) :
    (r.substT (shiftV k)).substT (liftT k σ) = (r.substT σ).substT (shiftV k) := by
  rw [Row.substT_comp, Row.substT_comp]; congr 1; funext a; exact shift_lift k σ a

theorem Row.ofCaps_substT (C : Caps) (σ : Atom → Ty) :
    (Row.ofCaps C).substT σ = Row.ofCaps (C.substT σ) := by
  funext x; cases x <;> first | rfl | simp [Row.substT, Row.ofCaps]

theorem Row.err_substT (E : Name) (U : Ty) (σ : Atom → Ty) :
    (Row.err E U).substT σ = Row.err E (U.substT σ) := by
  simp [Row.err, Row.ofCaps_substT, Ty.reach_caps]

/-- What a term holds is kept by a substitution that fits it. -/
theorem held_substT {Ψ : List Ty} {Γ : List Scheme} {e : Tm} {T : Ty} {σ : Atom → Ty} (hσ : Fits σ e)
    (h : Caps.Sub (held Ψ Γ e) T.reach) :
    Caps.Sub (held (Ψ.map (Ty.substT σ)) (Γ.map (Scheme.substT σ)) e) (T.substT σ).reach := by
  rintro a (⟨i, s, hi, hs, ha⟩ | ⟨p, hp, rfl⟩ | ⟨l, t, hl, ht, ha⟩)
  · simp only [List.getElem?_map, Option.map_eq_some_iff] at hs
    obtain ⟨s₀, hs₀, rfl⟩ := hs
    obtain ⟨b, hb, h'⟩ := (Scheme.reach_substT _ _ _).mp ha
    exact (Ty.reach_substT σ _ _).mpr ⟨b, h b (.inl ⟨i, s₀, hi, hs₀, hb⟩), h'⟩
  · refine (Ty.reach_substT σ _ _).mpr ⟨.r p, h _ (.inr (.inl ⟨p, hp, rfl⟩)), ?_⟩
    rw [hσ.1 p (.inl hp)]; rfl
  · simp only [List.getElem?_map, Option.map_eq_some_iff] at ht
    obtain ⟨t₀, ht₀, rfl⟩ := ht
    obtain ⟨b, hb, h'⟩ := (Ty.reach_substT σ _ _).mp ha
    exact (Ty.reach_substT σ _ _).mpr ⟨b, h b (.inr (.inr ⟨l, t₀, hl, ht₀, hb⟩)), h'⟩

theorem Tm.NoIns.of_has {e e' : Tm} (h : e.NoIns) (hs : ∀ s, e'.Has s → e.Has s) : e'.NoIns :=
  fun h' => h (hs _ h')

theorem Typ.var_substT {Ψ : List Ty} {Θ : Facts} {Γ : List Scheme} {i : Nat} {σ' : Scheme}
    {ts : List Ty} {T : Ty} {φ : Stage} (hi : Γ[i]? = some σ') (hts : ts.length = σ'.k)
    (hH : σ'.Holds Θ ts) (hT : Le (σ'.inst ts) T) (σ : Atom → Ty) :
    Typ S (Ψ.map (Ty.substT σ)) (Θ.substT σ) (Γ.map (Scheme.substT σ)) (.var i)
      (.ex (T.substT σ) (φ.substT σ)) := by
  refine .var (σ := σ'.substT σ) (ts := ts.map (Ty.substT σ)) (by simp [hi])
    (by simp [Scheme.substT, hts]) (hH.substT σ hts) ?_
  rw [Scheme.inst_substT _ _ _ hts]; exact hT.substT σ

theorem Typ.ctor_substT {Ψ : List Ty} {Θ : Facts} {Γ : List Scheme} {K : Name} {k : Nat}
    {ts us : List Ty} {T₀ : Name} {T : Ty} {φ : Stage} (hK : S.ctor K = some (k, ts, T₀))
    (hk : us.length = k) (hT : Le (Sig.ctorTy ts T₀ us) T) (σ : Atom → Ty) :
    Typ S Ψ Θ Γ (.ctor K) (.ex (T.substT σ) φ) := by
  refine .ctor (us := us.map (Ty.substT σ)) hK (by simpa using hk) ?_
  have := hT.substT σ; rwa [S.ctorTy_substT σ _ hk hK] at this

theorem Typ.letGen_substT {Ψ : List Ty} {Θ : Facts} {Γ : List Scheme} {e b : Tm} {a t : Ty}
    {φ : Stage} {k : Nat} {Φ : Facts}
    (ih₁ : ∀ σ, Fits σ e → Typ S ((Ψ.map (Ty.up k)).map (Ty.substT σ))
      ((Facts.union (Θ.substT (shiftV k)) Φ).substT σ) ((Γ.map (Scheme.up k)).map (Scheme.substT σ))
      e (J.substT σ (.ex a .pure)))
    (ih₂ : ∀ σ, Fits σ b → Typ S (Ψ.map (Ty.substT σ)) (Θ.substT σ)
      ((⟨k, Φ, a⟩ :: Γ).map (Scheme.substT σ)) b (J.substT σ (.ex t φ)))
    (σ : Atom → Ty) (hσ : Fits σ (.let_ e b)) :
    Typ S (Ψ.map (Ty.substT σ)) (Θ.substT σ) (Γ.map (Scheme.substT σ)) (.let_ e b)
      (J.substT σ (.ex t φ)) := by
  have h₁ := ih₁ (liftT k σ) ((hσ.sub fun _ h => .inl h).lift k)
  rw [shift_comm, shift_comm', Facts.union_substT, Facts.up_comm] at h₁
  exact .letGen h₁ (ih₂ σ (hσ.sub fun _ h => .inr h))

theorem Typ.substT {Ψ Θ Γ e J} (h : Typ S Ψ Θ Γ e J) :
    ∀ σ, Fits σ e →
      Typ S (Ψ.map (Ty.substT σ)) (Θ.substT σ) (Γ.map (Scheme.substT σ)) e (J.substT σ) := by
  induction h with
  | var hi hts hH hT => intro σ _; exact Typ.var_substT hi hts hH hT σ
  | lam _ hn hc hT ih =>
    intro σ hσ; have := ih σ hσ
    simp only [List.map_cons, Scheme.mono_substT, Flick.J.substT] at this
    exact .lam this hn (held_substT (e := .lam _) hσ hc) (hT.substT σ)
  | app _ _ hψ hT ih₁ ih₂ =>
    intro σ hσ
    exact .app (ih₁ σ (hσ.sub fun _ h => .inl h)) (ih₂ σ (hσ.sub fun _ h => .inr h))
      (Stage.le_substT σ hψ) (hT.substT σ)
  | let_ _ _ ih₁ ih₂ =>
    intro σ hσ; have := ih₂ σ (hσ.sub fun _ h => .inr h)
    simp only [List.map_cons, Scheme.mono_substT, Flick.J.substT] at this
    exact .let_ (ih₁ σ (hσ.sub fun _ h => .inl h)) this
  | letGen _ _ ih₁ ih₂ => exact Typ.letGen_substT ih₁ ih₂
  | letrec _ hn hc _ ih₁ ih₂ =>
    intro σ hσ
    have h₁ := ih₁ σ (hσ.sub fun _ h => .inl h)
    simp only [List.map_cons, Scheme.mono_substT, Flick.J.substT] at h₁
    have h₂ := ih₂ σ (hσ.sub fun _ h => .inr h)
    simp only [List.map_cons, Scheme.mono_substT, Flick.J.substT] at h₂
    exact .letrec h₁ hn (held_substT (e := .fix _) (hσ.sub fun _ h => .inl h) hc) h₂
  | fix _ hn hc hT ih =>
    intro σ hσ; have := ih σ hσ
    simp only [List.map_cons, Scheme.mono_substT, Flick.J.substT] at this
    exact .fix this hn (held_substT (e := .fix _) hσ hc) (hT.substT σ)
  | lit => intro _ _; exact .lit
  | loc hl => intro σ _; exact .loc (by simp [hl])
  | const hc hT =>
    intro σ _; exact .const (Const.ok_substT σ hc) (by have := hT.substT σ; rwa [Const.ty_substT] at this)
  | ctor hK hk hT => intro σ _; exact Typ.ctor_substT hK hk hT σ
  | rnil => intro _ _; exact .rnil
  | rcons hR hl _ _ ih₁ ih₂ =>
    intro σ hσ
    exact .rcons (hR.substT σ) (by rw [Ty.labels_substT σ hR]; exact hl)
      (ih₁ σ (hσ.sub fun _ h => .inl h)) (ih₂ σ (hσ.sub fun _ h => .inr h))
  | proj _ hf hT ih => intro σ hσ; exact .proj (ih σ hσ) (Ty.field_substT σ hf) (hT.substT σ)
  | update _ hf _ ih₁ ih₂ =>
    intro σ hσ
    exact .update (ih₁ σ (hσ.sub fun _ h => .inl h)) (Ty.field_substT σ hf)
      (ih₂ σ (hσ.sub fun _ h => .inr h))
  | match_ _ _ hc ih₁ ih₂ =>
    intro σ hσ
    exact .match_ (ih₁ σ (hσ.sub fun _ h => .inl h)) (ih₂ σ (hσ.sub fun _ h => .inr h))
      (hc.substT σ)
  | anil => intro _ _; exact .anil
  | acons hp _ _ ih₁ ih₂ =>
    intro σ hσ
    have := ih₁ σ (hσ.sub fun _ h => .inl h); rw [ctx_substT] at this
    exact .acons (hp.substT σ) this (ih₂ σ (hσ.sub fun _ h => .inr h))
  | aguard hp _ _ _ ih₁ ih₂ ih₃ =>
    intro σ hσ
    have g := ih₁ σ (hσ.sub fun _ h => .inl h); rw [ctx_substT] at g
    have b := ih₂ σ (hσ.sub fun _ h => .inr (.inl h)); rw [ctx_substT] at b
    exact .aguard (hp.substT σ) g b (ih₃ σ (hσ.sub fun _ h => .inr (.inr h)))
  | todo => intro _ _; exact .todo
  | task _ hψ hn hc hT ih =>
    intro σ hσ
    exact .task (ih σ hσ) (Stage.le_substT σ hψ) hn (held_substT (e := .task _) hσ hc)
      (hT.substT σ)
  | await _ hφ hT ih =>
    intro σ hσ; exact .await (ih σ hσ) (Stage.le_substT σ hφ) (hT.substT σ)
  | raise _ hφ ih =>
    intro σ hσ; exact .raise (ih σ hσ) (by simpa [Stage.substT] using Stage.le_substT σ hφ)
  | raiseAdt _ hU hφ ih =>
    intro σ hσ
    refine .raiseAdt (ih σ hσ) (Ty.hd_substT σ hU) ?_
    simpa [Stage.substT, Row.err_substT] using Stage.le_substT σ hφ
  | errVal hv _ hU hE ih =>
    intro σ hσ
    refine .errVal hv (ih σ hσ) (Ty.hd_substT σ hU) ?_
    rw [← Row.err_substT]; exact hE.substT σ
  | tryR _ _ hφ hs ih₁ ih₂ =>
    intro σ hσ
    exact .tryR (ih₁ σ (hσ.sub fun _ h => .inl h)) (ih₂ σ (hσ.sub fun _ h => .inr h))
      (Stage.le_substT σ hφ) (by simpa using hs.substT σ)
  | tryP _ _ ih₁ ih₂ =>
    intro σ hσ
    have := ih₂ σ (hσ.sub fun _ h => .inr h)
    simp only [Flick.J.substT, Row.none_substT] at this
    exact .tryP (ih₁ σ (hσ.sub fun _ h => .inl h)) this
  | handle hv _ _ hc ha hφ ih₁ ih₂ =>
    intro σ hσ
    refine .handle hv (ih₁ σ (hσ.sub fun _ h => .inl h)) (ih₂ σ (hσ.sub fun _ h => .inr h))
      ?_ ?_ (by simpa [Stage.substT] using Stage.le_substT σ hφ)
    · exact hc
    · rintro a ⟨b, hb, h'⟩; exact ⟨b, ha b hb, h'⟩
  | hnil hH =>
    intro σ _
    refine .hnil fun n hn => ?_
    cases n with
    | nm N => exact hH _ hn
    | atm a | ob _ a | need _ a => obtain ⟨b, hb, _⟩ := hn; exact hH _ hb
  | hcons hp _ _ hH ih₁ ih₂ =>
    intro σ hσ
    have := ih₁ σ (hσ.sub fun _ h => .inl h); rw [ctx_substT] at this
    exact .hcons (hp.substT σ) this (ih₂ σ (hσ.sub fun _ h => .inr h)) (Catches.substT σ hH)
  | hguard hp _ _ _ hH ih₁ ih₂ ih₃ =>
    intro σ hσ
    have g := ih₁ σ (hσ.sub fun _ h => .inl h); rw [ctx_substT] at g
    have b := ih₂ σ (hσ.sub fun _ h => .inr (.inl h)); rw [ctx_substT] at b
    exact .hguard (hp.substT σ) g b (ih₃ σ (hσ.sub fun _ h => .inr (.inr h))) (hH.substT σ)
  | provideL _ _ h₁ h₀ h₂ ih₁ ih₂ =>
    intro σ hσ
    exact .provideL (ih₁ σ (hσ.sub fun _ h => .inl h)) (ih₂ σ (hσ.sub fun _ h => .inr h))
      (by simpa [Stage.substT] using Stage.le_substT σ h₁) (ob_free_substT h₀)
      (by simpa [Stage.substT] using Stage.le_substT σ h₂)
  | provideE _ _ h₁ h₀ h₂ ih₁ ih₂ =>
    intro σ hσ
    exact .provideE (ih₁ σ (hσ.sub fun _ h => .inl h)) (ih₂ σ (hσ.sub fun _ h => .inr h))
      (by simpa [Stage.substT] using Stage.le_substT σ h₁) (ob_free_substT h₀)
      (by simpa [Stage.substT] using Stage.le_substT σ h₂)
  | with_ hv _ _ h₁ h₂ ih₁ ih₂ =>
    intro σ hσ
    have hq := hσ.1 _ (.inr (.inr (.inl rfl)))
    have hat := hσ.2.2 (.inl rfl)
    exact .with_ hv (ih₁ σ (hσ.sub fun _ h => .inr (.inr (.inl h))))
      (ih₂ σ (hσ.sub fun _ h => .inr (.inr (.inr h))))
      (Stage.le_rt_sub (by simpa [Stage.substT] using Stage.le_substT σ h₁) (Row.withP_substT hq hat))
      (by simpa [Stage.substT] using Stage.le_substT σ h₂)
  | enil hρ =>
    intro σ _
    refine .enil fun n hn => ?_
    cases n with
    | nm N => exact hρ _ hn
    | atm a | ob _ a | need _ a => obtain ⟨b, hb, _⟩ := hn; exact hρ _ hb
  | econs hv hθ _ _ hρ ih₁ ih₂ =>
    intro σ hσ
    have := ih₁ σ (hσ.sub fun _ h => .inl h); simp only [Flick.J.substT, S.impl_closed] at this
    refine .econs hv hθ this (ih₂ σ (hσ.sub fun _ h => .inr h)) fun n hn => ?_
    cases n with
    | nm N' => exact hρ _ hn
    | atm a | ob _ a | need _ a =>
      obtain ⟨b, hb, h'⟩ := hn
      rcases hρ _ hb with h | h
      · cases h
      · exact .inr ⟨b, h, h'⟩
  | bind _ hρ ih =>
    intro σ hσ
    have := ih σ hσ; simp only [Flick.J.substT, S.impl_closed] at this
    refine .bind this fun n hn => ?_
    cases n with
    | nm N' => exact hρ _ hn
    | atm a | ob _ a | need _ a => obtain ⟨b, hb, _⟩ := hn; cases hρ _ hb
  | join _ _ hρ ih₁ ih₂ =>
    intro σ hσ
    refine .join (ih₁ σ (hσ.sub fun _ h => .inl h)) (ih₂ σ (hσ.sub fun _ h => .inr h))
      fun n hn => ?_
    cases n with
    | nm N' => exact hρ _ hn
    | atm a | ob _ a | need _ a =>
      obtain ⟨b, hb, h'⟩ := hn
      rcases hρ _ hb with h | h
      · exact .inl ⟨b, h, h'⟩
      · exact .inr ⟨b, h, h'⟩
  | @scope F _ _ r ε _ _ _ _ _ _ hn hF _ hφ' hφ hT ih =>
    intro σ hσ
    have h₁ := ih (liftT 1 σ) (hσ.lift 1)
    simp only [Flick.J.substT, List.map_cons, Scheme.mono_substT, Ty.substT, liftT] at h₁
    simp only [if_true, Nat.zero_lt_one] at h₁
    rw [shift_comm, shift_comm', Facts.union_substT, Facts.up_comm, Ty.up_comm] at h₁
    refine .scope hn ?_ h₁ ?_ (Stage.le_substT σ hφ) (hT.substT σ)
    · rintro C s ⟨C₀, s₀, h₀, rfl, rfl⟩
      obtain ⟨rfl, hC⟩ := hF C₀ s₀ h₀
      refine ⟨by simp [Ty.substT, liftT], fun ⟨b, hb, hv⟩ => ?_⟩
      cases b with
      | v i =>
        cases i with
        | zero => exact hC hb
        | succ i =>
          simp only [liftT, show ¬ i + 1 < 1 by omega, if_false] at hv
          exact Ty.up_avoid_v0 _ hv
      | r q => simp only [liftT] at hv; exact Ty.up_avoid_v0 _ hv
    · have := Stage.le_substT (liftT 1 σ) hφ'
      have hd := Row.disch_substT (a := .v 0) (a' := .v 0) (r := r.substT (shiftV 1))
        (σ := liftT 1 σ) (fun x hx => by simpa [liftT, Ty.reach] using hx)
      rw [Row.up_comm] at hd
      simp only [Stage.substT, Row.union_substT, Row.scope_substT, Row.up_comm] at this
      refine Stage.le_rt_sub (r := Row.union (Row.scope (.tvar 0)) (Row.union
        ((r.substT σ).substT (shiftV 1)) ((Row.disch (.v 0) (r.substT (shiftV 1))).substT (liftT 1 σ))))
        (by simpa [Ty.substT, liftT] using this) ?_
      rintro x (hx | hx | hx)
      · exact .inl hx
      · exact .inr (.inl hx)
      · exact .inr (.inr (hd x hx))
  | inside hF _ hφ' ht hε hφ hT ih =>
    intro σ hσ
    have hp : σ (.r _) = .rg _ := hσ.1 _ (.inl (.inl rfl))
    have hav := hσ.2.1 (.inr (.inl rfl))
    have h₁ := ih σ (hσ.sub fun _ h => .inr (.inr (.inr h)))
    rw [Facts.union_substT] at h₁
    refine .inside ?_ h₁ ?_ (fun h => ht ?_) (fun ⟨b, hb, h'⟩ => hε (by rwa [hav b _ h'] at hb))
      (Stage.le_substT σ hφ) (hT.substT σ)
    · rintro C s ⟨C₀, s₀, h₀, rfl, rfl⟩
      obtain ⟨rfl, hC⟩ := hF C₀ s₀ h₀
      exact ⟨by simp [Ty.substT, hp], fun q ⟨b, hb, h'⟩ => by rw [hav b q h'] at hb; exact hC q hb⟩
    · have := Stage.le_substT σ hφ'
      simp only [Stage.substT, Row.union_substT, Row.scope_substT, Ty.substT, hp] at this
      refine Stage.le_rt_sub this ?_
      rintro x (hx | hx | hx | hx)
      · exact .inl hx
      · exact .inr (.inl hx)
      · exact .inr (.inr (.inl (Row.disch_substT (fun y hy => by rw [hp] at hy; exact hy) x hx)))
      · exact .inr (.inr (.inr (Row.needs_substT (fun y hy => by rw [hp] at hy; exact hy) x hx)))
    · obtain ⟨b, hb, h'⟩ := (Ty.reach_substT σ _ _).mp h
      rwa [hav b _ h'] at hb
  | cap =>
    intro σ hσ
    have hp := hσ.1 _ (.inl rfl)
    simp only [Flick.J.substT, Ty.substT, hp]; exact .cap
  | own =>
    intro σ hσ
    have hp := hσ.1 _ (.inl rfl)
    simp only [Flick.J.substT, Ty.substT, hp]; exact .own

end Flick
