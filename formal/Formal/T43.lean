import Formal.Regions
import Formal.Effects

/-!
# What a child is typed with reaches a handler or the root

The work of a child or a finalizer has a typing whose effects are answered by the root the
program runs with, or have an operation only the place that started it runs (`KidCov`). The
effects the child runs are those of the stage that registered it, or of a handler it keeps: the
obligation `Provided {E} ϱ` of the registration keeps out of its row an effect whose handler
is younger than its owner `ϱ` (`KidRow`, `withs_typing`). So a `tail` operation of a child is
always answered, by a kept handler or by the root.
-/

namespace Flick

variable {S : Sig}

/-- A stage of effects the root answers, or with an operation that is not `tail`. -/
def Stage.Cov (S : Sig) (root : Roots) (φ : Stage) : Prop :=
  ∀ N, φ.svc (.nm N) → (root.svc N).isSome ∨ S.HasNonTail N

/-- The children and finalizers of the regions have typings of such stages. -/
def KidCov (S : Sig) (root : Roots) (Ψ : List Ty) (Λ : List Rgn) : Prop :=
  ∀ (r : Nat) (g : Rgn), Λ[r]? = some g → ∀ k ∈ g.kids ++ g.fins,
    ∃ t φ, Typ S Ψ Facts.age [] k (.ex t φ) ∧ φ.Cov S root

theorem Stage.Cov.kill {root : Roots} {φ : Stage} (h : φ.Cov S root) (n : Nat) : (φ.substT (kill n)).Cov S root := by
  intro N hN
  cases φ with
  | pure => exact hN.elim
  | rt r e => exact h N (Row.kill_sub r n _ hN)

theorem KidCov.weaken {root Ψ Λ} (h : KidCov S root Ψ Λ) (X : List Ty) : KidCov S root (Ψ ++ X) Λ :=
  fun r g hg k hk => let ⟨t, φ, ht, hn⟩ := h r g hg k hk; ⟨t, φ, ht.weakenStore X, hn⟩

theorem KidCov.keep {root Ψ Λ Λ'} (hk : KidCov S root Ψ Λ)
    (h : ∀ (r : Nat) (g : Rgn), Λ'[r]? = some g → ∀ k ∈ g.kids ++ g.fins,
      (∃ g₀, Λ[r]? = some g₀ ∧ k ∈ g₀.kids ++ g₀.fins) ∨
      ∃ t φ, Typ S Ψ Facts.age [] k (.ex t φ) ∧ φ.Cov S root) : KidCov S root Ψ Λ' := by
  intro r g hg k hk'
  rcases h r g hg k hk' with ⟨g₀, h₀, hk₀⟩ | h'
  · exact hk r g₀ h₀ k hk₀
  · exact h'

theorem KidCov.setKid {root Ψ Λ p j k} (h : KidCov S root Ψ Λ)
    (hk : ∃ t φ, Typ S Ψ Facts.age [] k (.ex t φ) ∧ φ.Cov S root) : KidCov S root Ψ (setKid Λ p j k) := by
  intro q g hq k' hk'
  obtain ⟨g₀, hg₀, h₁, h₂⟩ := setKid_get hq
  by_cases hqp : q = p
  · subst hqp
    rw [h₁ rfl] at hk'
    rcases List.mem_append.mp hk' with hk' | hk'
    · obtain ⟨i, hi, rfl⟩ := List.getElem_of_mem hk'
      rw [List.getElem_set]
      split
      · exact hk
      · exact h q g₀ hg₀ _ (List.mem_append_left _ (List.getElem_mem _))
    · exact h q g₀ hg₀ _ (List.mem_append_right _ hk')
  · rw [h₂ hqp] at hk'; exact h q g₀ hg₀ k' hk'

/-- A step of a thread keeps the children and finalizers within what the root answers. -/
theorem KidCov.step {root : Roots} {Λ : List Rgn} {μ : List Tm} {e : Tm} {l : Lbl} {Λ' : List Rgn}
    {μ' : List Tm} {e' : Tm} {Ψ : List Ty} {t : Ty} {φ : Stage}
    (he : Typ S Ψ Facts.age [] e (.ex t φ)) (hφ : φ.Cov S root) (hA : e.Asc 0)
    (hk : KidCov S root Ψ Λ) (hs : Step root l Λ μ e (.cfg Λ' μ' e')) : KidCov S root Ψ Λ' := by
  have setold : ∀ {p g g'}, Λ[p]? = some g →
      (∀ k ∈ g'.kids ++ g'.fins, k ∈ g.kids ++ g.fins ∨
        ∃ t φ, Typ S Ψ Facts.age [] k (.ex t φ) ∧ φ.Cov S root) →
      KidCov S root Ψ (setRgn Λ p g') := by
    intro p g g' hg hnew
    refine hk.keep fun r g'' hr k hk' => ?_
    rcases setRgn_get hg hr with ⟨rfl, rfl⟩ | ⟨_, hr⟩
    · rcases hnew k hk' with h | h
      · exact .inl ⟨g, hg, h⟩
      · exact .inr h
    · exact .inl ⟨g'', hr, hk'⟩
  have newkid : ∀ {E c p w}, OkE E → e = plug E (.app (.app (.const c) (.own p)) w) →
      (c = .addFin ∨ c = .spawn) → Value w →
      ∃ t φ, Typ S Ψ Facts.age [] (withs p E (.await w)) (.ex t φ) ∧ φ.Cov S root := by
    intro E c p w hE he' hc hv
    subst he'
    have hnv := fin_not_value (w₁ := .own p) (w₂ := w) hc
    obtain ⟨hAE, -⟩ := asc_of_plug hnv hE hA
    obtain ⟨t', R', ε', hk', -, h2⟩ := withs_typing (o := p)
      (fun h' hΘ => fin_await h' hc hΘ hv) (fun h' => fin_not_pure h' hc) hnv _ hE he
      (Entails.refl _) hAE
    refine ⟨t', _, hk', fun N hN => ?_⟩
    rcases (h2 N hN).1 with h₁ | ⟨hn, -⟩
    · exact hφ N h₁
    · exact .inr hn
  cases hs with
  | rt hE hh =>
    cases hh with
    | scope | hdl =>
      refine hk.keep fun r g hg k hk' => ?_
      by_cases hr : r < Λ.length
      · rw [List.getElem?_append_left hr] at hg; exact .inl ⟨g, hg, hk'⟩
      · rw [List.getElem?_append_right (by omega)] at hg
        cases hj : r - Λ.length with
        | zero => simp [hj] at hg; subst hg; simp [Rgn.new] at hk'
        | succ => simp [hj] at hg
    | open_ hg =>
      refine setold hg fun k hk' => ?_
      simp only [List.mem_append] at hk'
      rcases hk' with hk' | hk' | hk'
      · exact .inl (List.mem_append_left _ hk')
      · exact .inl (List.mem_append_right _ hk')
      · simp at hk'; subst hk'
        -- `op (cap p)` at the stage of its owner: the live `op` needs its region there
        rename_i p _ _
        have hcov : ∀ q, (Stage.rt (Row.scope (.rg q)) Row.none).Cov S root := fun _ N h => by
          simp [Stage.svc, Row.scope, Row.ofCaps] at h
        exact .inr ⟨_, .rt (Row.scope (.rg p)) Row.none,
          .app (.const (ι := { (default : CArg) with s := .rg p }) trivial .refl) .cap
            (by rw [Stage.use_none]; exact Stage.le_refl _) .refl, hcov _⟩
    | finRun hg _ _ _ hfs =>
      refine setold hg fun k hk' => .inl ?_
      simp only [List.mem_append, hfs] at hk' ⊢
      rcases hk' with (hk' | hk') | hk'
      · exact .inl hk'
      · simp at hk'; subst hk'; simp
      · simp [hk']
    | close hg =>
      refine setold hg fun k hk' => .inl ?_
      simp only [List.mem_append] at hk' ⊢
      rcases hk' with hk' | hk'
      · exact .inl hk'
      · simp at hk'
    | _ => exact hk
  | addFin hE hg _ hv =>
    refine setold hg fun k hk' => ?_
    simp only [List.mem_append] at hk'
    rcases hk' with hk' | hk' | hk'
    · exact .inl (List.mem_append_left _ hk')
    · exact .inl (List.mem_append_right _ hk')
    · simp at hk'; subst hk'; exact .inr (newkid hE rfl (.inl rfl) hv)
  | spawn hE hg _ hv =>
    refine setold hg fun k hk' => ?_
    simp only [List.mem_append] at hk'
    rcases hk' with (hk' | hk') | hk'
    · exact .inl (List.mem_append_left _ hk')
    · simp at hk'; subst hk'; exact .inr (newkid hE rfl (.inr rfl) hv)
    · exact .inl (List.mem_append_right _ hk')
  | _ => exact hk

/-- A configuration step keeps a typed configuration and its children within what the root
answers; a child steps at the typing that says so. -/
theorem cstep_cov {root : Roots} {Ψ : List Ty} {c c' : Cfg} {t : Ty} {φ : Stage} {l : Lbl}
    (hroot : RootTy S root) (ht : t.YF 0) (hφ : φ.substT (kill 0) = φ) (hcov : φ.Cov S root)
    (hc : CfgTy S Ψ c t φ) (hk : KidCov S root Ψ c.rgs) (hs : CStep root l c c') :
    ∃ X, CfgTy S (Ψ ++ X) c' t φ ∧ KidCov S root (Ψ ++ X) c'.rgs := by
  cases hs with
  | main hs' =>
    obtain ⟨X, hc', -⟩ := cstep_typed hroot ht hφ hc (.main hs')
    exact ⟨X, hc', (KidCov.step hc.2.1 hcov hc.2.2.2.2.2.1.main hk hs').weaken X⟩
  | @kid Λ p g j k l μ Λ' μ' k' e hg hl hj hs' =>
    obtain ⟨hμ, h, hΛ, hb, hΨ, ha, hgs, hgc, hcg⟩ := hc
    have hkm := List.mem_of_getElem? hj
    obtain ⟨t', φ', hkt, hkn⟩ := hk _ _ hg _ (List.mem_append_left _ hkm)
    have hkb := (hb.rgs _ _ hg).1 _ hkm
    have hkA := ha.rgs _ _ hg _ (List.mem_append_left _ hkm)
    obtain ⟨X, hμ', hk', hX⟩ := preservation hroot hkt hμ hkb hb.mem hΨ hs'
    obtain ⟨hb', hl'⟩ := hs'.below hroot.below ⟨hkb, hb.mem, hb.rgs⟩
    obtain ⟨ha', hka⟩ := hs'.asc hroot ⟨hkA, ha.rgs, ha.mem⟩ hkb
    obtain ⟨m, hm, hkm'⟩ := hcg.rgs _ _ hg _ (List.mem_append_left _ hkm)
    obtain ⟨hgs', hgc', hgg, hkc⟩ := hs'.caught hroot (n := m)
      ⟨hkm', hcg.rgs⟩ ⟨hkA, ha.rgs, ha.mem⟩ ⟨hkb, hb.mem, hb.rgs⟩ hm
    refine ⟨X, ⟨hμ', h.weakenStore X, ((step_rgs hΛ hkt hkb hΨ hkA hs').weaken X).setKid ⟨_, _, hk'⟩,
        CfgBelow.kid ⟨hb.main.mono hl', hb'.mem, hb'.rgs⟩ hb'.main, fun u hu => by
          rw [setKid_length]
          rcases List.mem_append.mp hu with hu | hu
          · exact (hΨ u hu).mono hl'
          · exact (hX u hu).mono hl',
        (hka _ ha.main).setKid ha', hgs', hgc',
        hkc.setKid (Nat.le_trans hm hl') hkc.main (hcg.main.ghostB hb.main hgg)⟩, ?_⟩
    exact ((KidCov.step hkt hkn hkA hk hs').weaken X).setKid ⟨_, _, hk', Stage.Cov.kill hkn _⟩

theorem steps_cov {root : Roots} {c c' : Cfg} (hroot : RootTy S root) (hs : CSteps root c c') :
    ∀ {Ψ t φ}, t.YF 0 → φ.substT (kill 0) = φ → φ.Cov S root → CfgTy S Ψ c t φ →
      KidCov S root Ψ c.rgs → ∃ X, CfgTy S (Ψ ++ X) c' t φ ∧ KidCov S root (Ψ ++ X) c'.rgs := by
  induction hs with
  | refl => intro Ψ _ _ _ _ _ hc hk; exact ⟨[], by simpa using hc, by simpa using hk⟩
  | step h₁ _ ih =>
    intro Ψ t φ ht hφ hcov hc hk
    obtain ⟨X₁, hc₁, hk₁⟩ := cstep_cov hroot ht hφ hcov hc hk h₁
    obtain ⟨X₂, hc₂, hk₂⟩ := ih ht hφ hcov hc₁ hk₁
    exact ⟨X₁ ++ X₂, by simpa using hc₂, by simpa using hk₂⟩

end Flick
