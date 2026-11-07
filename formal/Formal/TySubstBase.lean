import Formal.RowSubst
import Formal.DictOps

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
  | .dfs fs => .dfs (fs.map fun f => (f.1, f.2.substT σ))
  | .harms E t φ s => .harms E (t.substT σ) (φ.substT σ) (s.substT σ)

theorem Fits.sub {σ : Atom → Ty} {e e' : Tm} (h : Fits σ e) (hs : ∀ s, e'.Has s → e.Has s) :
    Fits σ e' :=
  ⟨fun p hp => h.1 p (hs _ hp), fun hi => h.2.1 (hs _ hi), fun hi => h.2.2.1 (hs _ hi),
    fun hi => h.2.2.2 (hs _ hi)⟩

theorem Fits.noIns {σ : Atom → Ty} {e : Tm} (hn : e.NoIns) (hr : ∀ p, σ (.r p) = .rg p) :
    Fits σ e :=
  ⟨fun p _ => hr p, fun hi => (hn hi).elim, fun hi => (hn hi).elim, fun hi => (hn hi).elim⟩

theorem ident_noNames : NoNames Ty.ident := by
  intro b N h; cases b <;> simp [Ty.ident, Ty.asRow_tvar, Ty.asRow_rg] at h

/-- Shifting brings no name into a row. -/
theorem NoNames.up {σ : Atom → Ty} (h : NoNames σ) (k : Nat) : NoNames fun b => (σ b).up k := by
  intro b N hb
  simp only [Ty.up_def, Ty.asRow_substT, Row.substT] at hb
  rcases hb with hb | ⟨c, _, hc⟩
  · exact h b N hb
  · cases c <;> simp [shiftV, Ty.asRow_tvar, Ty.asRow_rg] at hc

theorem NoNames.lift {σ : Atom → Ty} (h : NoNames σ) (k : Nat) : NoNames (liftT k σ) := by
  intro b N hb
  cases b with
  | v i =>
    by_cases hik : i < k
    · simp [liftT, hik, Ty.asRow_tvar] at hb
    · simp only [liftT, hik, if_false] at hb; exact h.up k _ N hb
  | r q => exact h.up k _ N hb

theorem shift_lift (k : Nat) (σ : Atom → Ty) (a : Atom) :
    (shiftV k a).substT (liftT k σ) = (σ a).up k := by
  cases a with
  | v i => have : ¬ i + k < k := by omega
           simp [shiftV, Ty.substT, liftT, this]
  | r p => simp [shiftV, Ty.substT, liftT]

theorem Atom.ty_up (a : Atom) (k : Nat) : a.ty.up k = (a.up k).ty := by
  cases a <;> simp [Atom.ty, Atom.up, Ty.up_def, Ty.substT, shiftV]

theorem Fits.lift {σ : Atom → Ty} {e : Tm} (h : Fits σ e) (k : Nat) : Fits (liftT k σ) e := by
  refine ⟨fun p hp => by simp [liftT, h.1 p hp, Ty.up_def, Ty.substT, shiftV], fun hi b p hb => ?_,
    fun hi b a hb => ?_, fun hi => (h.2.2.2 hi).lift k⟩
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
      rw [h.2.2.1 hi b' a' ha', Atom.ty_up]
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
  rw [Facts.substT_comp, Facts.substT_comp]; congr 1; funext a; rw [shift_lift, Ty.up_def]

theorem Row.up_comm (r : Row) (k : Nat) (σ : Atom → Ty) :
    (r.substT (shiftV k)).substT (liftT k σ) = (r.substT σ).substT (shiftV k) := by
  rw [Row.substT_comp, Row.substT_comp]; congr 1; funext a; rw [shift_lift, Ty.up_def]

theorem Stage.up_comm (φ : Stage) (k : Nat) (σ : Atom → Ty) :
    (φ.substT (shiftV k)).substT (liftT k σ) = (φ.substT σ).substT (shiftV k) := by
  cases φ <;> simp [Stage.substT, Row.up_comm]

theorem Const.ty_substT (S : Sig) (c : Const) (ι : CArg) (σ : Atom → Ty) :
    (c.ty S ι).substT σ = c.ty S (ι.substT σ) := by
  cases c
  case scopeRun =>
    have hs : (Row.scope (.tvar 0)).substT (liftT 1 σ) = Row.scope (.tvar 0) := by
      simp [Ty.substT, liftT]
    have hC : (ι.C.substT (shiftV 1)).substT (liftT 1 σ) = (ι.C.substT σ).substT (shiftV 1) := by
      rw [Caps.substT_comp, Caps.substT_comp]; congr 1; funext a; rw [shift_lift, Ty.up_def]
    simp [Const.ty, Ty.substT, Stage.substT, CArg.substT, Row.obs_substT, Row.up_comm, Ty.up_comm, hs, hC,
      liftT]
  case addFin | spawn =>
    simp [Const.ty, Ty.substT, Stage.substT, CArg.substT, Ty.reach_caps, Row.obs_substT σ]
  case perform E o _ =>
    rcases h : S.op E o with _ | ⟨c, a, b⟩
    · simp [Const.ty, h, Ty.substT, Stage.substT]
    · obtain ⟨ha, hb⟩ := S.op_closed E o c a b σ h
      cases c <;> simp [Const.ty, h, Ty.substT, Stage.substT, CArg.substT, ha, hb]
  all_goals simp [Const.ty, Ty.substT, Stage.substT, CArg.substT]

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
    rw [hσ.1 p hp]; rfl
  · simp only [List.getElem?_map, Option.map_eq_some_iff] at ht
    obtain ⟨t₀, ht₀, rfl⟩ := ht
    obtain ⟨b, hb, h'⟩ := (Ty.reach_substT σ _ _).mp ha
    exact (Ty.reach_substT σ _ _).mpr ⟨b, h b (.inr (.inr ⟨l, t₀, hl, ht₀, hb⟩)), h'⟩

/-- An arm on a constructor of a GADT under `σ`: a unifier `θ` of the new problem,
after `σ` lifted past the constructor's variables, is a unifier of the old one, which
types the arm under the same contexts. -/
theorem Typ.aref_substT {Ψ : List Ty} {Θ : Facts} {Γ : List Scheme} {K : Name} {k : Nat}
    {ts : List Ty} {T₀ : Name} {qs : List Pat} {b r : Tm} {t t' : Ty} {φ : Stage}
    {Δ : (Atom → Ty) → List Ty} (hK : S.ctor K = some (k, ts, T₀)) (hU : t.hd = .adt T₀)
    (hn : b.NoIns) (hfr : ∀ i, b.Free i → i < (Pat.spine K qs).binds + Γ.length)
    (hlc : ∀ l, b.Has (.loc l) → l < Ψ.length)
    (hpA : ∀ θ, Fits θ b → (Ty.tcon T₀ (S.res K)).substT θ = (t.up k).substT θ →
      PatsTy S qs (ts.map (Ty.substT θ)) (Δ θ))
    (hbA : ∀ θ, Fits θ b → (Ty.tcon T₀ (S.res K)).substT θ = (t.up k).substT θ →
      Typ S ((Ψ.map (Ty.up k)).map (Ty.substT θ)) ((Θ.substT (shiftV k)).substT θ)
        ((Δ θ).map .mono ++ (Γ.map (Scheme.up k)).map (Scheme.substT θ)) b
        (.ex ((t'.up k).substT θ) ((φ.substT (shiftV k)).substT θ)))
    (σ : Atom → Ty) (hσ : Fits σ b)
    (hr : Typ S (Ψ.map (Ty.substT σ)) (Θ.substT σ) (Γ.map (Scheme.substT σ)) r
      (.alts (t.substT σ) (t'.substT σ) (φ.substT σ))) :
    Typ S (Ψ.map (Ty.substT σ)) (Θ.substT σ) (Γ.map (Scheme.substT σ))
      (.acons (Pat.spine K qs) b r) (.alts (t.substT σ) (t'.substT σ) (φ.substT σ)) := by
  have fits : ∀ θ, Fits θ b → Fits (fun a => (liftT k σ a).substT θ) b := fun θ h =>
    ⟨fun p hp => by simp [liftT, hσ.1 p hp, Ty.up_def, Ty.substT, shiftV, h.1 p hp],
      fun hi => (hn hi).elim, fun hi => (hn hi).elim, fun hi => (hn hi).elim⟩
  have cl : (Ty.tcon T₀ (S.res K)).substT (liftT k σ) = Ty.tcon T₀ (S.res K) := by
    rw [Ty.tcon_substT, S.res_closed K k ts T₀ _ hK fun i hi => by simp [liftT, hi]]
  have unif : ∀ θ, (Ty.tcon T₀ (S.res K)).substT θ = ((t.substT σ).up k).substT θ →
      (Ty.tcon T₀ (S.res K)).substT (fun a => (liftT k σ a).substT θ) =
        (t.up k).substT (fun a => (liftT k σ a).substT θ) := fun θ hu => by
    rw [← Ty.substT_comp, ← Ty.substT_comp, cl, Ty.up_comm]; exact hu
  have ets : ∀ θ, ts.map (Ty.substT (fun a => (liftT k σ a).substT θ)) = ts.map (Ty.substT θ) :=
    fun θ => by
      have := congrArg (List.map (Ty.substT θ))
        (S.ctor_closed K k ts T₀ (liftT k σ) hK fun i hi => by simp [liftT, hi])
      simpa [List.map_map, Function.comp_def, Ty.substT_comp] using this
  have eΨ : ∀ θ, ((Ψ.map (Ty.substT σ)).map (Ty.up k)).map (Ty.substT θ) =
      (Ψ.map (Ty.up k)).map (Ty.substT fun a => (liftT k σ a).substT θ) := fun θ => by
    rw [← shift_comm]; simp only [List.map_map, Function.comp_def, Ty.substT_comp]
  have eΓ : ∀ θ, ((Γ.map (Scheme.substT σ)).map (Scheme.up k)).map (Scheme.substT θ) =
      (Γ.map (Scheme.up k)).map (Scheme.substT fun a => (liftT k σ a).substT θ) := fun θ => by
    rw [← shift_comm']; simp only [List.map_map, Function.comp_def, Scheme.substT_comp]
  have eΘ : ∀ θ, ((Θ.substT σ).substT (shiftV k)).substT θ =
      (Θ.substT (shiftV k)).substT fun a => (liftT k σ a).substT θ := fun θ => by
    rw [← Facts.up_comm, Facts.substT_comp (Θ.substT (shiftV k)) (liftT k σ) θ]
  have eT : ∀ θ, ((t'.substT σ).up k).substT θ =
      (t'.up k).substT fun a => (liftT k σ a).substT θ := fun θ => by
    rw [← Ty.up_comm, Ty.substT_comp]
  have eP : ∀ θ, ((φ.substT σ).substT (shiftV k)).substT θ =
      (φ.substT (shiftV k)).substT fun a => (liftT k σ a).substT θ := fun θ => by
    simp only [Stage.substT_comp]; congr 1; funext a
    rw [← Ty.substT_comp, shift_lift, Ty.up_def]
  refine .aref (Δ := fun θ => Δ fun a => (liftT k σ a).substT θ) hK (Ty.hd_substT σ hU) hn
    (by simpa using hfr) (by simpa using hlc)
    (fun θ hθ hu => by have := hpA _ (fits θ hθ) (unif θ hu); rwa [ets] at this)
    (fun θ hθ hu => by
      have := hbA _ (fits θ hθ) (unif θ hu)
      rw [eΨ, eΘ, eΓ, eT, eP]; exact this) hr

theorem Tm.NoIns.of_has {e e' : Tm} (h : e.NoIns) (hs : ∀ s, e'.Has s → e.Has s) : e'.NoIns :=
  fun h' => h (hs _ h')

theorem LeI.substT {a b : Ty} (σ : Atom → Ty) (h : LeI a b) : LeI (a.substT σ) (b.substT σ) := by
  induction h with
  | le h => exact .le (h.substT σ)
  | @inst k t T us hus _ ih =>
    subst hus
    refine .inst (us := us.map (Ty.substT σ)) (by simp) ?_
    rw [Ty.inst_substT]; exact ih

theorem Typ.var_substT {Ψ : List Ty} {Θ : Facts} {Γ : List Scheme} {i : Nat} {σ' : Scheme}
    {ts : List Ty} {T : Ty} {φ : Stage} (hi : Γ[i]? = some σ') (hts : ts.length = σ'.k)
    (hH : σ'.Holds Θ ts) (hT : LeI (σ'.inst ts) T) (σ : Atom → Ty) :
    Typ S (Ψ.map (Ty.substT σ)) (Θ.substT σ) (Γ.map (Scheme.substT σ)) (.var i)
      (.ex (T.substT σ) (φ.substT σ)) := by
  refine .var (σ := σ'.substT σ) (ts := ts.map (Ty.substT σ)) (by simp [hi])
    (by simp [Scheme.substT, hts]) (hH.substT σ hts) ?_
  rw [Scheme.inst_substT _ _ _ hts]; exact hT.substT σ

/-- What the fields of a constructor hold stays within its value under a substitution. -/
theorem Sig.holds_substT (S : Sig) {K k ts T} (hK : S.ctor K = some (k, ts, T)) {us : List Ty}
    (hk : us.length = k) (hH : Sig.Holds ts T (S.res K) us) (σ : Atom → Ty) :
    Sig.Holds ts T (S.res K) (us.map (Ty.substT σ)) := by
  have hfs := S.fields_inst_substT σ us hk hK
  rw [List.map_map, List.map_inj_left] at hfs
  intro f hf a ha
  rw [← hfs f hf] at ha
  rw [← S.res_inst_substT σ us hk hK, ← Ty.tcon_substT, Ty.reach_substT]
  obtain ⟨b, hb, hσ⟩ := (Ty.reach_substT _ _ _).mp ha
  exact ⟨b, hH f hf b hb, hσ⟩

theorem Typ.ctor_substT {Ψ : List Ty} {Θ : Facts} {Γ : List Scheme} {K : Name} {k : Nat}
    {ts us : List Ty} {T₀ : Name} {T : Ty} {φ : Stage} (hK : S.ctor K = some (k, ts, T₀))
    (hk : us.length = k) (hH : Sig.Holds ts T₀ (S.res K) us)
    (hT : Le (Sig.ctorTyR ts T₀ (S.res K) us) T) (σ : Atom → Ty) :
    Typ S Ψ Θ Γ (.ctor K) (.ex (T.substT σ) φ) := by
  refine .ctor (us := us.map (Ty.substT σ)) hK (by simpa using hk) (S.holds_substT hK hk hH σ) ?_
  have := hT.substT σ; rwa [S.ctorTyR_substT σ _ hk hK] at this

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

theorem Typ.dcons_substT {Ψ : List Ty} {Θ : Facts} {Γ : List Scheme} {v r : Tm} {s : Scheme}
    {l : Label} {fs : List (Label × Scheme)}
    (ih₁ : ∀ σ, Fits σ v → Typ S ((Ψ.map (Ty.up s.k)).map (Ty.substT σ))
      ((Facts.union (Θ.substT (shiftV s.k)) s.cs).substT σ) ((Γ.map (Scheme.up s.k)).map (Scheme.substT σ))
      v (J.substT σ (.ex s.t .pure)))
    (ih₂ : ∀ σ, Fits σ r → Typ S (Ψ.map (Ty.substT σ)) (Θ.substT σ) (Γ.map (Scheme.substT σ)) r
      (J.substT σ (.dfs fs)))
    (σ : Atom → Ty) (hσ : Fits σ (.rcons l v r)) :
    Typ S (Ψ.map (Ty.substT σ)) (Θ.substT σ) (Γ.map (Scheme.substT σ)) (.rcons l v r)
      (J.substT σ (.dfs ((l, s) :: fs))) := by
  have h₁ := ih₁ (liftT s.k σ) ((hσ.sub fun _ h => .inl h).lift s.k)
  rw [shift_comm, shift_comm', Facts.union_substT, Facts.up_comm] at h₁
  exact .dcons (s := s.substT σ) h₁ (ih₂ σ (hσ.sub fun _ h => .inr h))

theorem Typ.drec_substT {Ψ : List Ty} {Θ : Facts} {Γ : List Scheme} {c k : Nat} {r : Tm}
    {fs : List (Label × Scheme)} {us : List Ty} {φ : Stage} (hc : S.dict c = some (k, fs))
    (hk : us.length = k)
    (ih : ∀ σ, Fits σ r → Typ S (Ψ.map (Ty.substT σ)) (Θ.substT σ) (Γ.map (Scheme.substT σ)) r
      (J.substT σ (.dfs (fs.map fun f => (f.1, f.2.substT (instT us))))))
    (hn : r.NoIns) (hh : Caps.Sub (held Ψ Γ (.drec c r)) (Ty.dict c (Ty.spine us)).reach)
    (σ : Atom → Ty) (hσ : Fits σ (.drec c r)) :
    Typ S (Ψ.map (Ty.substT σ)) (Θ.substT σ) (Γ.map (Scheme.substT σ)) (.drec c r)
      (J.substT σ (.ex (.dict c (Ty.spine us)) φ)) := by
  have hd := held_substT hσ hh
  have h₁ := ih σ hσ
  simp only [Flick.J.substT, S.fields_substT hc hk] at h₁
  simp only [Flick.J.substT, Ty.substT, Ty.spine_substT] at hd ⊢
  exact .drec hc (by simpa using hk) h₁ hn hd

/-- (Proj) of a dictionary after a substitution, in any context. -/
theorem Typ.dproj_at {Ψ : List Ty} {Θ : Facts} {Γ : List Scheme} {c k : Nat} {e : Tm}
    {l : Label} {fs : List (Label × Scheme)} {s : Scheme} {ts us : List Ty} {T : Ty} {φ : Stage}
    {σ : Atom → Ty} (h₁ : Typ S Ψ (Θ.substT σ) Γ e (.ex ((Ty.dict c (Ty.spine us)).substT σ) φ))
    (hc : S.dict c = some (k, fs)) (hk : us.length = k) (hl : fs.lookup l = some s)
    (hts : ts.length = s.k) (hH : (s.substT (instT us)).Holds Θ ts)
    (hT : Le ((s.substT (instT us)).inst ts) T) :
    Typ S Ψ (Θ.substT σ) Γ (.proj e l) (.ex (T.substT σ) φ) := by
  simp only [Ty.substT, Ty.spine_substT] at h₁
  have he := S.field_substT hc (List.lookup_mem' hl) hk σ
  have hH' := hH.substT σ hts
  have hT' := hT.substT σ
  rw [he] at hH'
  rw [← Scheme.inst_substT (s.substT (instT us)) σ ts hts, he] at hT'
  exact .dproj h₁ hc (by simpa using hk) hl (by simpa using hts) hH' hT'

theorem Typ.dproj_substT {Ψ : List Ty} {Θ : Facts} {Γ : List Scheme} {c k : Nat} {e : Tm}
    {l : Label} {fs : List (Label × Scheme)} {s : Scheme} {ts us : List Ty} {T : Ty} {φ : Stage}
    (ih : ∀ σ, Fits σ e → Typ S (Ψ.map (Ty.substT σ)) (Θ.substT σ) (Γ.map (Scheme.substT σ)) e
      (J.substT σ (.ex (.dict c (Ty.spine us)) φ)))
    (hc : S.dict c = some (k, fs)) (hk : us.length = k) (hl : fs.lookup l = some s)
    (hts : ts.length = s.k) (hH : (s.substT (instT us)).Holds Θ ts)
    (hT : Le ((s.substT (instT us)).inst ts) T) (σ : Atom → Ty) (hσ : Fits σ (.proj e l)) :
    Typ S (Ψ.map (Ty.substT σ)) (Θ.substT σ) (Γ.map (Scheme.substT σ)) (.proj e l)
      (J.substT σ (.ex T φ)) :=
  Typ.dproj_at (ih σ hσ) hc hk hl hts hH hT

theorem Typ.glob_substT {Ψ : List Ty} {Θ : Facts} {Γ : List Scheme} {g : Nat} {s : Scheme}
    {ts : List Ty} {T : Ty} {φ : Stage} (hg : S.glob g = some s) (hts : ts.length = s.k)
    (hH : s.Holds Θ ts) (hT : Le (s.inst ts) T) (σ : Atom → Ty) :
    Typ S (Ψ.map (Ty.substT σ)) (Θ.substT σ) (Γ.map (Scheme.substT σ)) (.glob g)
      (J.substT σ (.ex T φ)) := by
  have hH' := hH.substT σ hts
  have hT' := hT.substT σ
  rw [← Scheme.inst_substT _ _ _ hts, S.glob_substT hg] at hT'
  rw [S.glob_substT hg] at hH'
  exact .glob hg (by simpa using hts) hH' hT'

end Flick
