import Formal.CanonForms
import Formal.Substitution

/-!
# Canonical form of a dictionary (L1)

A value of type `D_C ā` is a dictionary record, and each field its class declares
is there, typed at its scheme.
-/

namespace Flick

variable {S : Sig}

theorem List.lookup_map_snd {l : Label} (g : Scheme → Scheme) : ∀ fs : List (Label × Scheme),
    (fs.map fun f => (f.1, g f.2)).lookup l = (fs.lookup l).map g
  | [] => rfl
  | (l', s) :: fs => by
    simp only [List.map, List.lookup]
    split <;> simp_all [List.lookup_map_snd g fs]

/-- L1 for dictionaries. -/
theorem canonical_dict {Ψ Θ Γ v c U φ} (hv : Value v) (h : Typ S Ψ Θ Γ v (.ex (.dict c U) φ)) :
    ∃ r k fs us, v = .drec c r ∧ U = Ty.spine us ∧ S.dict c = some (k, fs) ∧ us.length = k ∧
      Typ S Ψ Θ Γ r (.dfs (fs.map fun f => (f.1, f.2.substT (instT us)))) ∧ r.NoIns := by
  have hk := h.vkind hv rfl
  cases hv with
  | drec => cases h with | drec hc hl hr hn _ => exact ⟨_, _, _, _, rfl, rfl, hc, hl, hr, hn⟩
  | ctorApp _ hf _ => simp [VKind, Tm.head, hf, Ty.kind] at hk
  | constApp =>
    simp [VKind, Tm.head, Ty.kind] at hk
  | _ => simp [VKind, Tm.head, Ty.kind] at hk

/-- A field a dictionary record declares is there, at its scheme. -/
theorem Typ.dfs_field {Ψ Θ Γ l} : ∀ {r : Tm} {fs : List (Label × Scheme)} {s : Scheme},
    Typ S Ψ Θ Γ r (.dfs fs) → fs.lookup l = some s →
    ∃ w, r.field l = some w ∧ Typ S (Ψ.map (Ty.up s.k)) (Facts.union (Θ.substT (shiftV s.k)) s.cs)
      (Γ.map (Scheme.up s.k)) w (.ex s.t .pure) := by
  intro r
  induction r with
  | rnil => intro _ _ h hl; cases h; simp [List.lookup] at hl
  | rcons l' v r _ ih =>
    intro _ _ h hl
    cases h with
    | dcons hv hr =>
      simp only [List.lookup, Tm.field] at hl ⊢
      by_cases he : l' = l
      · subst he; simp at hl; subst hl; exact ⟨_, by simp, hv⟩
      · have : (l == l') = false := by simpa using Ne.symm he
        rw [this] at hl; simp only [he, if_false]; exact ih hr hl
  | _ => intro _ _ h; cases h

theorem Tm.has_of_field {l : Label} {x : Site} : ∀ {r w : Tm}, r.field l = some w → w.Has x → r.Has x := by
  intro r
  induction r with
  | rcons l' e r _ ih =>
    intro w h hw
    simp only [Tm.field] at h
    split at h
    · cases h; exact .inl hw
    · exact .inr (ih h hw)
  | drec c r ih => intro w h hw; exact ih h hw
  | _ => intro w h; simp [Tm.field] at h

/-- The field of a dictionary record at an instance of its scheme. -/
theorem dict_field {Ψ Θ v c us φ k fs l s ts w} (hv : Value v)
    (h : Typ S Ψ Θ [] v (.ex (.dict c (Ty.spine us)) φ)) (hc : S.dict c = some (k, fs))
    (hl : fs.lookup l = some s) (hts : ts.length = s.k) (hH : (s.substT (instT us)).Holds Θ ts)
    (hw : v.field l = some w) : Typ S Ψ Θ [] w (.ex ((s.substT (instT us)).inst ts) .pure) := by
  obtain ⟨r, _, _, _, rfl, hU, hc', _, hr, hn⟩ := canonical_dict hv h
  obtain rfl := Ty.spine_inj hU
  rw [hc] at hc'; cases hc'
  obtain ⟨w', hw', ht⟩ := hr.dfs_field (by rw [List.lookup_map_snd, hl]; rfl)
  simp only [Tm.field] at hw; rw [hw] at hw'; cases hw'
  exact Typ.instantiate hts hH (fun h => hn (Tm.has_of_field hw h)) ht

end Flick
