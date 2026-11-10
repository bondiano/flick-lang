import Formal.Elab.Sugar

/-!
# Elaboration: the pieces of the sugar elaborate (elaboration.md §3)

Constants and methods at an instance of their schemes, `λp₁ … pₙ`, the tuple of a group
(`Sugar.tuple`) and `unc n`. A lambda of the sugar takes the empty qualifier: it needs nothing
live where it is called.
-/

namespace Flick.Elab

/-- A monomorphic binder `x : a`. -/
abbrev Scheme.mono (a : Ty) : Scheme := ⟨0, [], a, .cnil⟩

variable {P : Prog} (S : Sugar P)

theorem elab_const {Γ Δ c k ty q ts τ φ} (hc : P.consts c = some ⟨k, [], ty, q⟩)
    (hk : ts.length = k) (hτ : ty.subst (inst ts) = τ) : Elab P Γ Δ (.const c) τ φ (.const c) := by
  subst hτ; exact .const (σ := ⟨k, [], ty, q⟩) hc hk .nil

theorem elab_meth {Γ Δ m C k c₀ ty q ts d τ φ} (hm : P.methods m = some (C, ⟨k, [c₀], ty, q⟩))
    (hC : c₀.cls = C) (hk : ts.length = k) (hd : Ent P Δ [c₀.subst (inst ts)] [d])
    (hτ : ty.subst (inst ts) = τ) : Elab P Γ Δ (.meth m) τ φ (.meth (.dict d) m) := by
  subst hτ; exact .meth (σ := ⟨k, [c₀], ty, q⟩) (rest := []) hm rfl hC hk hd .nil

theorem Ty.subst_curry (σ : Nat → Ty) (q : Ty) (as : List Ty) (r : Ty) :
    (curry q as r).subst σ = curry (q.subst σ) (as.map (·.subst σ)) (r.subst σ) := by
  induction as with
  | nil => rfl
  | cons a as ih =>
    simp only [curry, List.foldr_cons, List.map_cons] at ih ⊢; simp [Ty.subst, Stage.subst, ih]

theorem Ty.subst_nest (σ : Nat → Ty) (tup : Nat) (t : Ty) (as : List Ty) :
    (nest tup t as).subst σ = nest tup (t.subst σ) (as.map (·.subst σ)) := by
  induction as generalizing t with
  | nil => rfl
  | cons a as ih => exact ih _

theorem vars_inst {as : List Ty} {n : Nat} (rs : List Ty) (h : as.length = n) :
    (vars n).map (·.subst (inst (as ++ rs))) = as := by
  subst h
  apply List.ext_getElem (by simp [vars])
  intro i h₁ h₂
  simp only [vars, List.getElem_map, List.getElem_range, Ty.subst, inst]
  rw [dif_pos (by simp; omega), List.getElem_append_left]

theorem elab_unc {Γ Δ φ} (a : Ty) (as : List Ty) (r q : Ty) :
    Elab P Γ Δ (.const (S.unc (as.length + 1)))
      (arrP (curry q (a :: as) r) (.arr .P (nest S.tup a as) r q)) φ
      (.const (S.unc (as.length + 1))) := by
  refine elab_const (S.hUnc _ (Nat.succ_pos _)) (ts := (a :: as) ++ [r, q]) (by simp) ?_
  have hv : (vars (as.length + 1)).map (·.subst (inst ((a :: as) ++ [r, q]))) = a :: as :=
    vars_inst [r, q] rfl
  have h0 : inst ((a :: as) ++ [r, q]) 0 = a := by simp [inst]
  have hn : inst ((a :: as) ++ [r, q]) (as.length + 1) = r := by simp [inst]
  have hq : inst ((a :: as) ++ [r, q]) (as.length + 1 + 1) = q := by simp [inst]
  simp only [Ty.subst, Stage.subst, Ty.subst_curry, Ty.subst_nest, List.map_tail, hv, h0, hn, hq,
    List.tail_cons]

/-- `λp₁ … pₙ. b`, each arrow with the qualifier `q`. -/
theorem elab_lams {Δ r b b' q} : ∀ {as : List Ty} {Γ : List Scheme} {ψ : Stage},
    as ≠ [] → Elab P (as.reverse.map Scheme.mono ++ Γ) Δ b r .P b' →
    ∃ e', Elab P Γ Δ (lams as.length b) (curry q as r) ψ e'
  | [], _, _, h, _ => absurd rfl h
  | [a], Γ, ψ, _, hb => ⟨_, .lam (φ := .P) hb⟩
  | a :: a' :: as, Γ, ψ, _, hb => by
    obtain ⟨e', he⟩ := elab_lams (as := a' :: as) (Γ := Scheme.mono a :: Γ) (ψ := .P)
      (List.cons_ne_nil _ _) (by simpa using hb)
    exact ⟨_, .lam (φ := .P) he⟩

/-- The tuple of a group: `map2 pair` at each `and`; its row is whatever `map2` shows. -/
theorem elab_tuple {Γ Δ φ m dk} (hd : Ent P Δ [⟨S.applicative, [m]⟩] [dk]) :
    ∀ {es : List Exp} {as : List Ty} {acc t d acc'},
    All2 (fun e a => ∃ d e', Elab P Γ Δ e (appQ m d a) φ e') es as →
    Elab P Γ Δ acc (appQ m d t) φ acc' →
    ∃ q e', Elab P Γ Δ (S.tuple acc es) (appQ m q (nest S.tup t as)) φ e'
  | _, _, _, _, _, _, .nil, h => ⟨_, _, h⟩
  | _, _, acc, t, d, _, .cons (a := e) (b := a) ⟨d', _, he⟩ hs, h => by
    let ta : Ty := .app (.app (.con S.tup) t) a
    have hm := elab_meth (Γ := Γ) (φ := φ) S.hMap2 rfl (ts := [m, .cnil, d, t, a, ta, .cnil, d'])
      rfl (by simpa [Cn.subst, Ty.subst, inst] using hd)
      (τ := map2Ty m .cnil d t a ta .cnil d') rfl
    have hp := elab_const (Γ := Γ) (Δ := Δ) (φ := φ) S.hPair (ts := [t, a])
      (τ := arrP t (arrP a ta)) rfl rfl
    have := elab_tuple hd hs (.app (.app (.app hm hp (.inl rfl)) h (.inl rfl)) he (.inl rfl))
    simpa only [Sugar.tuple, nest] using this

end Flick.Elab
