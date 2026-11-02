import Formal.TyOps

/-!
# Dictionary records and globals of λFlick⁺: substitution

`D_C ā` has its arguments in a spine; the schemes of its fields mention only the
class parameters below their own binders, and the scheme of a global is closed, so
a substitution acts on a field through the arguments and leaves a global alone.
-/

namespace Flick

theorem Ty.spine_substT (σ : Atom → Ty) : ∀ us : List Ty,
    (Ty.spine us).substT σ = Ty.spine (us.map (Ty.substT σ))
  | [] => rfl
  | u :: us => by simp [Ty.spine, Ty.substT, Ty.spine_substT σ us]

theorem Ty.spine_inj : ∀ {us us' : List Ty}, Ty.spine us = Ty.spine us' → us = us'
  | [], [], _ => rfl
  | [], _ :: _, h => by cases h
  | _ :: _, [], h => by cases h
  | u :: us, u' :: us', h => by
    simp only [Ty.spine, Ty.tapp.injEq] at h
    rw [h.1, Ty.spine_inj h.2]

theorem Scheme.Closed.substT {n : Nat} {s : Scheme} (h : s.Closed (s.k + n)) {σ σ' : Atom → Ty}
    (he : ∀ i, i < n → σ (.v i) = σ' (.v i)) : s.substT σ = s.substT σ' := by
  have := h (liftT s.k σ) (liftT s.k σ') fun i hi => by
    by_cases hk : i < s.k
    · simp [liftT, hk]
    · simp only [liftT, hk, if_false]; rw [he _ (by omega)]
  simp [Scheme.substT, this.1, this.2]

theorem Sig.glob_substT (S : Sig) {g s} (h : S.glob g = some s) (σ : Atom → Ty) :
    s.substT σ = s := by
  have hc : s.Closed (s.k + 0) := S.glob_closed g s h
  rw [hc.substT (σ := σ) (σ' := Ty.ident) fun i hi => absurd hi (by omega), Scheme.substT_ident]

theorem Sig.field_substT (S : Sig) {c k fs l s} (h : S.dict c = some (k, fs)) (hl : (l, s) ∈ fs)
    {us : List Ty} (hk : us.length = k) (σ : Atom → Ty) :
    (s.substT (instT us)).substT σ = s.substT (instT (us.map (Ty.substT σ))) := by
  rw [Scheme.substT_comp]
  apply (S.dict_closed c k fs h l s hl).substT
  intro i hi
  simp [instT, hk, hi]

theorem Sig.fields_substT (S : Sig) {c k fs} (h : S.dict c = some (k, fs))
    {us : List Ty} (hk : us.length = k) (σ : Atom → Ty) :
    (fs.map fun f => (f.1, f.2.substT (instT us))).map (fun f => (f.1, f.2.substT σ)) =
      fs.map fun f => (f.1, f.2.substT (instT (us.map (Ty.substT σ)))) := by
  simp only [List.map_map]
  apply List.map_congr_left
  intro f hf
  simp [S.field_substT h hf hk σ]

/-- The scheme of field `l` of a dictionary. -/
theorem List.lookup_mem' {l : Label} : ∀ {fs : List (Label × Scheme)} {s : Scheme},
    fs.lookup l = some s → (l, s) ∈ fs
  | [], _, h => by cases h
  | (l', s') :: fs, s, h => by
    simp only [List.lookup] at h
    split at h
    · cases h; rename_i he; simp at he; subst he; exact List.mem_cons_self
    · exact List.mem_cons_of_mem _ (List.lookup_mem' h)

end Flick
