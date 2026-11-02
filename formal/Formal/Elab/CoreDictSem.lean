import Formal.Elab.CoreDictRun

/-!
# λFlick⁺ as a `Target`: `≈` keeps the values of dictionaries

`RealRun P N root` gives the bodies of the globals that realise the program's dictionaries.
`ent_eval`: a closed derivation evaluates. `eqv_sem`: `≈`-related dictionaries have the same
values (`SemEq`), by induction on `≈`: the slot rule of an instance (and of `struct_C^τ`)
evaluates the instance applied to closed derivations to a record (`slot_body`), whose slot is
the slot's dictionary over the values of the arguments; the substitution of the arguments
themselves has the same values (`trDt_sem`).
-/

namespace Flick.Elab

/-- The bodies of the globals of dictionaries: an instance is `λ^l. drec …` with the slot
`N.slot i` the translation of `slots[i]` over the premises, `struct_C^τ` is `λ^n. drec …`
with the slot `struct_S^τ (δ̄.$slotᵢ)` (Р180), a primitive dictionary is a value. The run
erases the type of a structural and a primitive global (Р196). -/
structure RealRun (P : Prog) (N : Names) (root : Roots) : Prop where
  inst : ∀ {I ins}, P.insts I = some ins → ∃ r,
    root.glob (N.inst I) =
      some (lamsC ins.ctx.length (.drec (N.cls ins.cls ins.head.length) r)) ∧
    ∀ i s, ins.slots[i]? = some s → r.field (N.slot i) = some (trDt N (dext ins.ctx.length id) s)
  strct : ∀ {C τ ts}, P.structC C = true → P.fields τ = some ts → ∃ r,
    root.glob (N.strct C τ) = some (lamsC ts.length (.drec (N.cls C 1) r)) ∧
    ∀ i S, (P.supers C)[i]? = some S → r.field (N.slot i) = some (trDt N (dext ts.length id)
      (apps (.strct S.cls τ) ((List.range ts.length).map fun k => .slot (.var k) i)))
  strctSub : ∀ {C τ ts θ}, P.structC C = true → P.fields τ = some ts →
    root.glob (N.strct C (τ.subst θ)) = root.glob (N.strct C τ)
  prim : ∀ {q p}, P.primC q.cls = true → P.prims q = some p →
    ∃ v, root.glob (N.prim p q) = some v ∧ Value v
  primSub : ∀ {q p θ}, P.primC q.cls = true → P.prims q = some p →
    root.glob (N.prim p (q.subst θ)) = root.glob (N.prim p q)

variable {P : Prog} {N : Names} {root : Roots}

theorem All2.map_left {α β γ : Type} {R : β → γ → Prop} {f : α → β} {l : List α} {l' : List γ}
    (h : All2 (fun a b => R (f a) b) l l') : All2 R (l.map f) l' := by
  induction h with
  | nil => exact .nil
  | cons r _ ih => exact .cons r ih

theorem All2.get {α β : Type} {R : α → β → Prop} {l : List α} {l' : List β} (h : All2 R l l') :
    ∀ k (h₁ : k < l.length) (h₂ : k < l'.length), R l[k] l'[k] := by
  induction h with
  | nil => intro k h₁; simp at h₁
  | cons r _ ih =>
    intro k h₁ h₂
    cases k with
    | zero => exact r
    | succ k => exact ih k (by simpa using h₁) (by simpa using h₂)

theorem getD_lt {α : Type} {l : List α} {k : Nat} (h : k < l.length) (d : α) : l.getD k d = l[k] := by
  simp [List.getD, List.getElem?_eq_getElem h]

/-- **A closed derivation evaluates.** -/
theorem ent_eval (hR : RealRun P N root) (δ : Nat → Nat) {qs : List Cn} {ds : List Dict}
    (h : Ent P [] qs ds) : ∃ vs, All2 (fun d v => Eval root (trDt N δ d) v) ds vs := by
  induction h with
  | nil => exact ⟨[], .nil⟩
  | cons _ _ ih₁ ih₂ =>
    obtain ⟨vs₁, a₁⟩ := ih₁
    obtain ⟨vs₂, a₂⟩ := ih₂
    cases a₁ with
    | cons r a => cases a; exact ⟨_, .cons r a₂⟩
  | given hc => exact (hc.not_nil).elim
  | prim h₁ h₂ =>
    obtain ⟨v, hg, hv⟩ := hR.prim h₁ h₂
    exact ⟨[v], .cons ⟨.step (.glob1 hg) .refl, hv⟩ .nil⟩
  | strct h₁ h₂ hds ih =>
    obtain ⟨vs, a⟩ := ih
    obtain ⟨r, hg, -⟩ := hR.strct h₁ h₂
    have ev := eval_body hg a.map_left (by simpa using hds.get.1)
    exact ⟨_, .cons (by rw [trDt_apps]; exact ev) .nil⟩
  | inst h₁ _ _ hds ih =>
    obtain ⟨vs, a⟩ := ih
    obtain ⟨r, hg, -⟩ := hR.inst h₁
    have ev := eval_body hg a.map_left (by simpa using hds.get.1)
    exact ⟨_, .cons (by rw [trDt_apps]; exact ev) .nil⟩

/-- The slot of a global `λ^l. drec …` applied to closed derivations. -/
theorem slot_body (hR : RealRun P N root) {g l c : Nat} {r : Flick.Tm} {lab : Label} {s : Dict}
    (hg : root.glob g = some (lamsC l (.drec c r)))
    (hf : r.field lab = some (trDt N (dext l id) s)) {qs : List Cn} {ds : List Dict}
    (hds : Ent P [] qs ds) (hl : ds.length = l) :
    SemEq root (.proj (appsC (.glob g) (ds.map (trDt N id))) lab) (trDt N id (s.subst (args ds))) := by
  obtain ⟨vs, a⟩ := ent_eval hR id hds
  have hvl : vs.length = l := a.length ▸ hl
  have ev := eval_body hg a.map_left (by simpa using hl)
  refine (semEq_proj (SemEq.of_eval ev) lab).trans
    ((show SemEq root _ _ from fun v => eval_proj (Tm.field_subst hf _)).trans ?_)
  rw [← trDt_subst (δ := dext l id) (σ := fun j => if j < l then trDt N id (args ds (l - 1 - j))
    else .var 0)]
  · refine trDt_sem _ (fun j => ?_) s
    simp only [Flick.inst, List.length_reverse, hvl]
    split
    · rename_i hj
      have hk : l - 1 - j < ds.length := by omega
      have hk' : l - 1 - j < vs.length := by omega
      simp only [List.getElem_reverse, args, getD_lt hk]
      have e : vs.length - 1 - j = l - 1 - j := by omega
      simp only [e]
      exact (SemEq.of_eval (a.get _ hk hk')).symm
    · exact var_sem _ _
  · intro k
    unfold dext
    split
    · rename_i hk; rw [if_pos (by omega)]; congr 2; omega
    · rename_i hk
      simp only [id, Nat.sub_add_cancel (Nat.le_of_not_lt hk), if_neg hk]
      simp [args, trDt, List.getD, List.getElem?_eq_none (by omega : ds.length ≤ k)]

/-- **`≈` keeps the values of a dictionary** (Р197). -/
theorem eqv_sem (hR : RealRun P N root) {d d' : Dict} (h : Eqv P d d') :
    SemEq root (trDt N id d) (trDt N id d') := by
  induction h with
  | refl => exact SemEq.refl _
  | symm _ ih => exact ih.symm
  | trans _ _ ih₁ ih₂ => exact ih₁.trans ih₂
  | slot _ ih => exact semEq_proj ih _
  | ap _ _ ih₁ ih₂ => exact semEq_app ih₁ ih₂
  | @instSlot I ins i s σ ds hI hsl hds =>
    obtain ⟨r, hg, hf⟩ := hR.inst hI
    simp only [trDt, trDt_apps]
    exact slot_body hR hg (hf i s hsl) hds (by simpa using hds.get.1)
  | @strctSlot C τ ts i S ds hC hS hfl hds =>
    obtain ⟨r, hg, hf⟩ := hR.strct hC hfl
    have hl : ds.length = ts.length := by simpa using hds.get.1
    have := slot_body hR hg (hf i S hS) hds hl
    simp only [trDt, trDt_apps] at this ⊢
    refine this.trans (SemEq.of_star ?_ |>.trans (SemEq.refl _))
    rw [apps_subst]
    simp only [Dict.subst, List.map_map, trDt_apps, trDt]
    have e : (List.range ts.length).map
          (trDt N id ∘ (fun x => Dict.subst (args ds) x) ∘ fun k => (Dict.var k).slot i) =
        ds.map (trDt N id ∘ fun x => x.slot i) := by
      apply List.ext_getElem (by simp [hl])
      intro k h₁ h₂
      simp only [List.getElem_map, List.getElem_range, Function.comp, Dict.subst, args]
      rw [getD_lt (by simpa [hl] using h₁)]
    rw [e]; exact .refl
  | strctSub hC hf => exact glob_sem (hR.strctSub hC hf).symm
  | primSub hq hp => exact glob_sem (hR.primSub hq hp).symm

end Flick.Elab
