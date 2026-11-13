structure GenSig (S : Sig) : Prop where
  ctor : ∀ K, (genCtor K).isSome → S.ctor K = genCtor K
  res : ∀ K, (genCtor K).isSome → S.res K = genRes K
  glob : ∀ g, (genGlob g).isSome → S.glob g = genGlob g
  /-- A type of the generator has no constructors but its own. -/
  closed : ∀ K k ts T, S.ctor K = some (k, ts, T) → genAdtCtors T ≠ [] → genCtor K = some (k, ts, T)
  /-- A service of the generator has its operations and no other. -/
  op : ∀ E o, genOps E ≠ [] → S.op E o = genOp E o
  dict : ∀ c, (genDict c).isSome → S.dict c = genDict c
  /-- A type of the generator has the variances the model gives it. -/
  var : ∀ T, genAdtCtors T ≠ [] → ∀ j, S.var T j = genVar T j

theorem GenSig.var_of {S : Sig} (hS : GenSig S) {T : Name} {j : Nat} {v : Variance}
    (h : S.var T j = v) (hT : genAdtCtors T ≠ []) : genVar T j = v :=
  (hS.var T hT j).symm.trans h

macro_rules
  | `(tactic| var_absurd) => `(tactic| (
    rename_i hv
    have := GenSig.var_of ‹GenSig _› hv (by simp [genAdtCtors])
    simp [genVar] at this))

/-- `t.NoV i` of a concrete type. -/
macro "no_v" : tactic => `(tactic| (
  simp [Ty.NoV, Ty.substT, Stage.substT, dropV, Ty.ident]
  all_goals first
    | (funext x; simp [Caps.substT, dropV, Ty.ident, Ty.reach]; (try omega))
    | omega))

/-- `Ty.Pol genVar i s true f` of a field of the generator's data. -/
syntax "pol" : tactic
macro_rules
  | `(tactic| pol) => `(tactic| first
    | exact Ty.Pol.var (.inl ⟨rfl, rfl⟩)
    | exact Ty.Pol.var (.inr ⟨rfl, rfl⟩)
    | (apply Ty.Pol.none; no_v)
    | (refine Ty.Pol.rcons ?_ ?_ <;> pol)
    | (refine Ty.Pol.arrow ?_ ?_ ?_ ?_ ?_ <;> first
        | (intro h; cases h)
        | (intro _; no_v)
        | pol
        | no_v
        | (left; no_v)
        | exact .inr (.inl ⟨rfl, rfl⟩)
        | exact .inr (.inr ⟨rfl, rfl⟩))
    | (refine Ty.Pol.data ?_ ?_ ?_ ?_ ?_ (Ty.unspine_eq rfl) <;>
        intro j u hu hv <;> rcases j with $CASESj <;> simp at hu <;> (try subst hu) <;>
        first | (simp [genVar] at hv; done) | pol | (intro _; no_v) | (intro _ _; no_v) | no_v))

theorem idTs_noV {k j i : Nat} {u : Ty} (h : (idTs k)[j]? = some u) (hj : j ≠ i) : u.NoV i := by
  simp only [idTs, List.getElem?_map, Option.map_eq_some_iff] at h
  obtain ⟨n, hn, rfl⟩ := h
  obtain ⟨_, rfl⟩ := List.getElem?_eq_some_iff.mp hn
  simp [Ty.NoV, Ty.substT, dropV, hj, Ty.ident]

/-- Each constructor of the generator is ordinary, and holds a parameter only where its
variance allows. -/
theorem genVar_ok : ∀ T i, genVar T i ≠ .inv → ∀ K k ts, genCtor K = some (k, ts, T) →
    (∀ j u, (genRes K)[j]? = some u → j ≠ i → u.NoV i) ∧
    ((i < k ∧ (genRes K)[i]? = some (.tvar i) ∧ ∀ f ∈ ts, (genVar T i = .bi → f.NoV i) ∧
        (genVar T i = .pos ∨ genVar T i = .neg → Ty.Pol genVar i (genVar T i) true f)) ∨
      (genVar T i ≠ .bi ∧ (∃ c, (genRes K)[i]? = some c ∧ c.Closed) ∧ ∀ f ∈ ts, f.NoV i)) := by
  intro T i hv K k ts hK
  have hres : genRes K = idTs k := by simp [genRes, hK]
  rw [hres]
  refine ⟨fun j u hu hj => idTs_noV hu hj, .inl ?_⟩
  unfold genCtor at hK
  split at hK <;> cases hK <;> rcases i with $CASESi <;> simp only [genVar] at hv ⊢ <;>
    (try exact absurd rfl hv) <;>
    refine ⟨by decide, by simp [idTs], fun f hf => ⟨fun h => (nomatch h), fun _ => ?_⟩⟩ <;>
    simp at hf <;> (try rcases hf with rfl | rfl) <;> (try subst hf) <;> pol

theorem genAdtCtors_complete : ∀ K k ts T, genCtor K = some (k, ts, T) →
    (K, ts.length) ∈ genAdtCtors T := by
  intro K k ts T h
  unfold genCtor at h
  split at h <;> cases h <;> decide

theorem genCtor_closed : ∀ K k ts T σ, genCtor K = some (k, ts, T) →
    (∀ i, i < k → σ (.v i) = .tvar i) → ts.map (Ty.substT σ) = ts := by
  intro K k ts T σ hK hσ
  unfold genCtor at hK
  split at hK <;> cases hK <;> simp [Ty.substT, Stage.substT, hσ] <;>
    funext a <;> simp [Caps.substT, hσ]

theorem genCtor_reach : ∀ K k ts T, genCtor K = some (k, ts, T) →
    ∀ t ∈ ts, ∀ a, t.reach a → ∃ i, i < k ∧ a = .v i := by
  intro K k ts T hK t ht a ha
  unfold genCtor at hK
  split at hK <;> cases hK <;> simp at ht <;>
    (try rcases ht with rfl | rfl) <;> (try subst ht) <;>
    simp [Ty.reach, Caps.none] at ha <;>
    (try rcases ha with ha | ha) <;> (try subst ha) <;> refine ⟨_, ?_, rfl⟩ <;> decide

theorem genRes_closed : ∀ K k ts T σ, genCtor K = some (k, ts, T) →
    (∀ i, i < k → σ (.v i) = .tvar i) → (genRes K).map (Ty.substT σ) = genRes K := by
  intro K k ts T σ h hσ
  simp only [genRes, h, idTs, List.map_map]
  apply List.map_congr_left
  intro i hi
  simp only [Function.comp, Ty.substT]
  exact hσ i (List.mem_range.mp hi)

theorem genGlob_closed : ∀ g s, genGlob g = some s → s.Closed s.k := by
  intro g s h
  unfold genGlob at h
  split at h <;> cases h <;> closed_tac

theorem genOp_closed : ∀ E o c a b σ, genOp E o = some (c, a, b) →
    a.substT σ = a ∧ b.substT σ = b := by
  intro E o c a b σ h
  unfold genOp at h
  split at h <;> cases h <;> simp [Ty.substT]

theorem genOp_mem : ∀ E o x, genOp E o = some x → o ∈ genOps E := by
  intro E o x h
  unfold genOp at h
  split at h <;> cases h <;> simp [genOps]

/-- The arm of a service's one operation is all a handler of it needs. -/
theorem GenSig.complete {S : Sig} (hS : GenSig S) {E o : Name} (h : genOps E = [o]) :
    S.Complete E (recArm o) := by
  intro o' c a b hop
  rw [hS.op E o' (by simp [h])] at hop
  have hm := genOp_mem E o' _ hop
  rw [h, List.mem_singleton] at hm
  subst hm
  simp [Tm.armOf, Pat.spine, Pat.headName]

def genSig : Sig where
  ctor := genCtor
  impl _ := .errors Row.none
  ctor_closed := genCtor_closed
  ctor_reach := genCtor_reach
  res := genRes
  res_closed := genRes_closed
  impl_closed _ _ := by simp [Ty.substT, Row.none_substT]
  dict := genDict
  glob := genGlob
  dict_closed := genDict_closed
  glob_closed := genGlob_closed
  op := genOp
  op_closed := genOp_closed
  op_impl _ _ _ _ _ _ _ := rfl
  op_field _ _ _ _ _ h := (h rfl).elim
  var := genVar
  var_ok := genVar_ok

/-- The assumption of a certificate holds of a signature. -/
theorem GenSig.inhabited : ∃ S, GenSig S :=
  ⟨genSig, fun _ _ => rfl, fun _ _ => rfl, fun _ _ => rfl, fun _ _ _ _ h _ => h, fun _ _ _ => rfl,
    fun _ _ => rfl, fun _ _ _ => rfl⟩

end Flick.Cert
