import Formal.PresPure
import Formal.Fresh

/-!
# The step of `scope` and the closing of a region

The scope step renames the regions not yet created out of the body of the scope
(`kill n`) and then puts the new region `n` for the variable of its region. The
body then mentions `n` only where the scope's region was, so the result type and
the errors of the body do not mention it (the freshness premises of (Inside)), and
the facts the scope gave its region hold of the order of creation. Closing a
region drops these facts: they follow from those of the context.
-/

namespace Flick

variable {S : Sig}

/-- The facts and the store typing of a step at `n` regions: the store is typed
with the facts of creation, the hole with facts that extend those of the regions
`< n` and hold of the order of creation, and no type names a region `≥ n`. -/
structure RtCtx (Ψ : List Ty) (Θ : Facts) (n : Nat) : Prop where
  inΘ : Entails (Facts.age.substT (kill n)) Θ
  outΘ : Entails Θ Facts.age
  idem : Θ.substT (kill n) = Θ
  yf : ∀ u ∈ Ψ, u.YF n

theorem Row.up_inst (r : Row) (x : Ty) : (r.substT (shiftV 1)).substT (instT [x]) = r := by
  rw [Row.substT_comp]; conv => rhs; rw [← r.substT_ident]
  congr 1; funext a
  cases a with
  | v i => simp [shiftV, Ty.substT, instT, Ty.ident]
  | r p => simp [shiftV, Ty.substT, instT, Ty.ident]

@[simp] theorem Ty.up_inst1 (t x : Ty) : (t.up 1).substT (instT [x]) = t :=
  Ty.up_substT t 1 [x] rfl

theorem Facts.up_inst1 (Θ : Facts) (x : Ty) : (Θ.substT (shiftV 1)).substT (instT [x]) = Θ :=
  Facts.up_substT Θ 1 [x] rfl

theorem Caps.kill_fixed {C : Caps} {n : Nat} (h : ∀ q, C (.r q) → q < n) : C.substT (kill n) = C := by
  funext a; apply propext
  constructor
  · rintro ⟨b, hb, hk⟩; obtain ⟨rfl, _⟩ := kill_reach hk; exact hb
  · intro ha
    refine ⟨a, ha, ?_⟩
    cases a with
    | v i => rfl
    | r q => simp [kill, h q ha]

theorem Ty.kill_fresh (t : Ty) (n : Nat) : ¬ (t.substT (kill n)).reach (.r n) :=
  fun h => Nat.lt_irrefl n (Ty.reach_kill h)

theorem Row.kill_fresh (ε : Row) (n : Nat) : ¬ (Ty.errors (ε.substT (kill n))).reach (.r n) :=
  Ty.kill_fresh (.errors ε) n

theorem instT_rg_noNames (n : Nat) : NoNames (instT [.rg n]) := by
  intro b N h
  cases b with
  | v i => cases i <;> simp [instT, Ty.asRow_rg, Ty.asRow_tvar] at h
  | r p => simp [instT, Ty.asRow_rg] at h

theorem Stage.YF_kill {φ φ' : Stage} {n : Nat} (h : φ.le φ') (hφ' : φ'.substT (kill n) = φ') :
    (φ.substT (kill n)).le φ' := by
  have := Stage.le_substT (kill n) h; rwa [hφ'] at this

/-- The facts of a region of the context follow from those of the order of creation. -/
theorem inside_entails {Θ F : Facts} {p n : Nat} (hp : p < n)
    (hF : ∀ C s, F C s → s = .rg p ∧ ∀ q, C (.r q) → q < p)
    (hin : Entails (Facts.age.substT (kill n)) Θ) : Entails (Facts.union Θ F) Θ := by
  intro C s hC x hx
  rcases hC x hx with h | ⟨C', h₁ | h₁, h₂⟩
  · exact .inl h
  · exact .inr ⟨C', h₁, h₂⟩
  · obtain ⟨rfl, hC'⟩ := hF C' s h₁
    have hfact : Facts.age.substT (kill n) C' (.rg p) :=
      ⟨C', .rg p, fun q p' hq hp' => by cases hp'; exact hC' q hq,
        (Caps.kill_fixed fun q hq => Nat.lt_trans (hC' q hq) hp).symm, by simp [Ty.substT, kill, hp]⟩
    exact hin C' (.rg p) (fun y hy => .inr ⟨C', hfact, hy⟩) x h₂

/-- The body of a new scope, in its run-time region `n`: the regions not yet created
are renamed out, and `n` is put for the region of the scope. -/
theorem scope_body {Ψ : List Ty} {Θ : Facts} {e : Tm} {t₀ : Ty} {φ' : Stage} {r ε : Row} {F : Facts}
    {n : Nat} (hn : e.NoIns) (hbelow : e.Below n)
    (hF : ∀ C s, F C s → s = .tvar 0 ∧ ¬ C (.v 0))
    (h : Typ S (Ψ.map (Ty.up 1)) (Facts.union (Θ.substT (shiftV 1)) F)
      (.mono (.owner (.tvar 0)) :: [].map (Scheme.up 1)) e (.ex (t₀.up 1) φ'))
    (hφ' : φ'.le (.rt (Row.union (Row.scope (.tvar 0))
      (Row.union (r.substT (shiftV 1)) (Row.disch (.v 0) (r.substT (shiftV 1))))) (ε.substT (shiftV 1))))
    (hΨ : ∀ u ∈ Ψ, u.YF n) (hidem : Θ.substT (kill n) = Θ) :
    ∃ F' φ₁, (∀ C s, F' C s → s = .rg n ∧ ∀ q, C (.r q) → q < n) ∧
      Typ S Ψ (Facts.union Θ F') [] (e.subst (inst [.own n])) (.ex (t₀.substT (kill n)) φ₁) ∧
      φ₁.le (.rt (Row.union (Row.scope (.rg n))
        (Row.union (r.substT (kill n)) (Row.disch (.r n) (r.substT (kill n))))) (ε.substT (kill n))) := by
  have hΨ' : Ψ.map (Ty.substT (kill n)) = Ψ := by
    conv => rhs; rw [← List.map_id Ψ]
    exact List.map_congr_left fun u hu => hΨ u hu
  have h₁ := h.substT (liftT 1 (kill n)) ((Fits.kill hbelow).lift 1)
  simp only [shift_comm, hΨ', Facts.union_substT, Facts.up_comm, hidem, List.map_cons, List.map_nil,
    Scheme.mono_substT, Flick.J.substT, Ty.up_comm] at h₁
  have hown : (Ty.owner (.tvar 0)).substT (liftT 1 (kill n)) = .owner (.tvar 0) := by
    simp [Ty.substT, liftT]
  rw [hown] at h₁
  have h₂ := h₁.substT (instT [.rg n]) (Fits.noIns hn fun _ => rfl)
  simp only [List.map_map, Function.comp_def, Ty.up_inst1, List.map_id_fun', id, List.map_cons,
    List.map_nil, Scheme.mono_substT, Flick.J.substT, Facts.union_substT, Facts.up_inst1] at h₂
  have hb : Typ S Ψ (Facts.union Θ ((F.substT (liftT 1 (kill n))).substT (instT [.rg n]))) []
      (e.subst (inst [.own n])) (.ex (t₀.substT (kill n))
        ((φ'.substT (liftT 1 (kill n))).substT (instT [.rg n]))) := by
    refine h₂.inst (Δ := [_]) (.cons .own ?_ .nil)
    simp [Ty.substT, instT]; exact .own
  refine ⟨_, _, ?_, hb, ?_⟩
  · rintro C s ⟨C₁, s₁, ⟨C₀, s₀, h₀, rfl, rfl⟩, rfl, rfl⟩
    obtain ⟨rfl, hv0⟩ := hF C₀ s₀ h₀
    refine ⟨by simp [Ty.substT, liftT, instT], fun q hq => ?_⟩
    obtain ⟨b₁, ⟨b, hb, hb₁⟩, hq'⟩ := hq
    cases b with
    | v i =>
      cases i with
      | zero => exact (hv0 hb).elim
      | succ i =>
        simp only [liftT, show ¬ i + 1 < 1 by omega, if_false] at hb₁
        obtain ⟨a, _, rfl⟩ := (Ty.reach_up _ _ _).mp hb₁
        cases a <;> simp [kill, Atom.up] at *
        all_goals simp_all [Ty.substT, instT]
    | r q' =>
      simp only [liftT] at hb₁
      obtain ⟨a, ha, rfl⟩ := (Ty.reach_up _ _ _).mp hb₁
      obtain ⟨rfl, hlt⟩ := kill_reach ha
      simp [Atom.up, instT] at hq'
      subst hq'; exact hlt _ rfl
  · have := Stage.le_substT (instT [.rg n]) (Stage.le_substT (liftT 1 (kill n)) hφ')
    have d₁ := Row.disch_substT_eq (a := .v 0) (a' := .v 0) (r := r.substT (shiftV 1))
      (σ := liftT 1 (kill n)) (fun x => by simp [liftT]) ((kill_noNames n).lift 1)
    have d₂ := Row.disch_substT_eq (a := .v 0) (a' := .r n)
      (r := (r.substT (kill n)).substT (shiftV 1)) (σ := instT [.rg n]) (fun x => by simp [instT])
      (instT_rg_noNames n)
    simpa [Stage.substT, Row.union_substT, Row.scope_substT, Row.up_comm, d₁, d₂, Row.up_inst,
      Ty.substT, instT, liftT] using this

end Flick
