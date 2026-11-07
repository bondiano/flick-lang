import Formal.Elab.Sugar

/-!
# Elaboration: the pieces of the sugar elaborate (elaboration.md §3)

Constants and methods at an instance of their schemes, `λp₁ … pₙ`, the tuple of a group
(`Sugar.tuple`) and `unc n`. A lambda of the sugar takes as its qualifier a row that covers
its whole context (`Cov`, `ctxRow`).
-/

namespace Flick.Elab

/-- A monomorphic binder `x : a`. -/
abbrev Scheme.mono (a : Ty) : Scheme := ⟨0, [], a, .cnil⟩

/-- `q` covers what every entry of `Γ` reaches. -/
def Cov (Γ : List Scheme) (q : Ty) : Prop := ∀ s ∈ Γ, ∀ n, s.reach n → q.reach n

/-- `⌈s⌉` as a row: the bound variables become `cnil`. -/
def Scheme.row (s : Scheme) : Ty :=
  (Ty.cun s.ty s.q).subst fun m => if m < s.k then .cnil else .var (m - s.k)

/-- A row that covers the context. -/
def ctxRow (Γ : List Scheme) : Ty := Γ.foldr (fun s q => .cun s.row q) .cnil

theorem Ty.occurs_subst {σ : Nat → Ty} {m n : Nat} {t : Ty} (ht : t.Occurs m)
    (hs : (σ m).Occurs n) : (t.subst σ).Occurs n :=
  (Ty.occurs_subst_iff σ t n).mpr ⟨m, ht, hs⟩

theorem cov_ctxRow (Γ : List Scheme) : Cov Γ (ctxRow Γ) := by
  induction Γ with
  | nil => intro s hs; cases hs
  | cons s Γ ih =>
    intro s' hs n hn
    rcases List.mem_cons.1 hs with rfl | hs
    · refine .inl (Ty.occurs_subst (m := n + s'.k) ?_ ?_)
      · rcases hn with h | h
        · exact .inl h
        · exact .inr h.2
      · rw [if_neg (by omega)]; show n + s'.k - s'.k = n; omega
    · exact .inr (ih s' hs n hn)

/-- A lambda in `Γ` holds no more than a row covering `Γ`. -/
theorem lam_cov {Γ : List Scheme} {q : Ty} (h : Cov Γ q) {φ a t} {b : Exp} :
    ∀ n, heldS Γ (.lam b) n → (Ty.arr φ a t q).reach n := by
  rintro n ⟨x, s, -, hs, hr⟩
  exact .inr (.inr (.inl (h s (List.mem_of_getElem? hs) n hr)))

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

/-- `λp₁ … pₙ. b`, each arrow with a qualifier `q` that covers the innermost context. -/
theorem elab_lams {Δ r b b' q} : ∀ {as : List Ty} {Γ : List Scheme} {ψ : Stage},
    as ≠ [] → Cov (as.reverse.map Scheme.mono ++ Γ) q →
    Elab P (as.reverse.map Scheme.mono ++ Γ) Δ b r .P b' →
    ∃ e', Elab P Γ Δ (lams as.length b) (curry q as r) ψ e'
  | [], _, _, h, _, _ => absurd rfl h
  | [a], Γ, ψ, _, hq, hb =>
    ⟨_, .lam (φ := .P) hb (lam_cov fun s hs => hq s (List.mem_append_right _ hs))⟩
  | a :: a' :: as, Γ, ψ, _, hq, hb => by
    obtain ⟨e', he⟩ := elab_lams (as := a' :: as) (Γ := Scheme.mono a :: Γ) (ψ := .P)
      (List.cons_ne_nil _ _) (by simpa using hq) (by simpa using hb)
    exact ⟨_, .lam (φ := .P) he (lam_cov fun s hs => hq s (List.mem_append_right _ hs))⟩

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
