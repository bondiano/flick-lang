import Formal.T43

/-!
# T4.3 and T1 with `op_H` (regions.md §5.3)
-/

namespace Flick

variable {S : Sig}

theorem cstep_all {root : Roots} {Ψ : List Ty} {c c' : Cfg} {t : Ty} {φ : Stage} {l : Lbl}
    {anc B : Nat → Nat → Prop} {home : Nat → Option (Nat × Nat)}
    (hroot : RootTy S root) (ht : t.YF 0) (hφ : φ.substT (kill 0) = φ)
    (hc : CfgTy S Ψ c t φ) (hn : Nest Ψ c anc B home) (hk : KidNeeds S Ψ c.rgs anc)
    (hs : CStep root l c c') :
    ∃ X anc' B' home', CfgTy S (Ψ ++ X) c' t φ ∧ Nest (Ψ ++ X) c' anc' B' home' ∧
      KidNeeds S (Ψ ++ X) c'.rgs anc' := by
  cases hs with
  | main hs' =>
    obtain ⟨X, hc', hX⟩ := cstep_typed hroot ht hφ hc (.main hs')
    obtain ⟨anc', B', home', hn', hag⟩ := hn.cstep hroot hc hc' hX (.main hs')
    exact ⟨X, anc', B', home', hc', hn',
      (KidNeeds.step (i := none) hn rfl hc.2.1 (fun N q h => (Stage.yf_need hφ h).elim)
        hc.2.2.2.2.2.1.main hk hag hs').weaken X⟩
  | @kid Λ p g j k l μ Λ' μ' k' e hg hl hj hs' =>
    obtain ⟨hμ, h, hΛ, hb, hΨ, ha, hgs, hgc, hcg⟩ := hc
    have hkm := List.mem_of_getElem? hj
    obtain ⟨t', φ', hkt, hkn⟩ := hk _ _ hg hl _ (List.mem_append_left _ hkm)
    have hkb := (hb.rgs _ _ hg).1 _ hkm
    have hkA := ha.rgs _ _ hg _ (List.mem_append_left _ hkm)
    obtain ⟨X, hμ', hk', hX⟩ := preservation hroot hkt hμ hkb hb.mem hΨ hs'
    obtain ⟨hb', hl'⟩ := hs'.below hroot ⟨hkb, hb.mem, hb.rgs⟩
    obtain ⟨ha', hka⟩ := hs'.asc hroot ⟨hkA, ha.rgs, ha.mem⟩ hkb
    obtain ⟨hgs', hgc', hgg, hkc⟩ := hs'.caught hroot
      ⟨hcg.rgs _ _ hg _ (List.mem_append_left _ hkm), hcg.rgs⟩ ⟨hkA, ha.rgs, ha.mem⟩ ⟨hkb, hb.mem, hb.rgs⟩
    have hc' : CfgTy S (Ψ ++ X) ⟨setKid Λ' p j k', μ', e⟩ t φ :=
      ⟨hμ', h.weakenStore X, ((step_rgs hΛ hkt hkb hΨ hkA hs').weaken X).setKid ⟨_, _, hk'⟩,
        CfgBelow.kid ⟨hb.main.mono hl', hb'.mem, hb'.rgs⟩ hb'.main, fun u hu => by
          rw [setKid_length]
          rcases List.mem_append.mp hu with hu | hu
          · exact (hΨ u hu).mono hl'
          · exact (hX u hu).mono hl',
        (hka _ ha.main).setKid ha', hgs', hgc', hkc.setKid hkc.main (hcg.main.ghostB hb.main hgg)⟩
    obtain ⟨anc', B', home', hn', hag⟩ :=
      hn.cstep hroot ⟨hμ, h, hΛ, hb, hΨ, ha, hgs, hgc, hcg⟩ hc' hX (.kid hg hl hj hs')
    refine ⟨X, anc', B', home', hc', hn', ?_⟩
    have hkn' := KidNeeds.step (i := some (_, _)) hn (Cfg.at_some.mpr ⟨_, hg, hl, hj⟩) hkt
      (fun N q h => hkn N q h) hkA hk hag hs'
    refine (hkn'.weaken X).setKid ⟨_, _, hk', fun N q h => ?_⟩
    rw [hag _ (Nat.ne_of_lt (getElem?_lt hg))]
    exact hkn N q (Stage.kill_need h)

theorem steps_all {root : Roots} {c c' : Cfg} (hroot : RootTy S root)
    (hs : CSteps root c c') :
    ∀ {Ψ t φ anc B home}, t.YF 0 → φ.substT (kill 0) = φ → CfgTy S Ψ c t φ →
      Nest Ψ c anc B home → KidNeeds S Ψ c.rgs anc →
      ∃ X anc' B' home', CfgTy S (Ψ ++ X) c' t φ ∧ Nest (Ψ ++ X) c' anc' B' home' ∧
        KidNeeds S (Ψ ++ X) c'.rgs anc' := by
  induction hs with
  | refl =>
    intro Ψ _ _ anc B home _ _ hc hn hk
    exact ⟨[], anc, B, home, by simpa using hc, by simpa using hn, by simpa using hk⟩
  | step h₁ _ ih =>
    intro Ψ t φ anc B home ht hφ hc hn hk
    obtain ⟨X₁, anc₁, B₁, home₁, hc₁, hn₁, hk₁⟩ := cstep_all hroot ht hφ hc hn hk h₁
    obtain ⟨X₂, anc₂, B₂, home₂, hc₂, hn₂, hk₂⟩ := ih ht hφ hc₁ hn₁ hk₁
    exact ⟨X₁ ++ X₂, anc₂, B₂, home₂, by simpa using hc₂, by simpa using hn₂, by simpa using hk₂⟩

/-- **T4.3 (regions.md §5.3).** In every configuration a program reaches, an `ask_S`
of a thread (the main thread or a child of a live region) answered by a `with` frame
of the `provide` region `q`, inherited or not, finds `q` live: no fiber uses a
service of a `provide` after its region closed. This is the premise G11 of T5 in the
form of the liveness premise of `provide` (`runtime.md`). -/
theorem T4_3 {root e₀ t φ₀ c} (hroot : RootTy S root) (h₀ : Typ S [] Facts.none [] e₀ (.ex t φ₀))
    (hp : e₀.Below 0) (ht : t.YF 0) (hφ : φ₀.substT (kill 0) = φ₀)
    (hs : CSteps root ⟨[], [], e₀⟩ c) :
    ∀ i e E N q v, c.at i = some e → OkE E → e = plug E (.app (.const (.ask N)) (.lit .unit)) →
      lookupE E N = some (q, v) → Live c.rgs q := by
  obtain ⟨X, anc, B, home, hc, hn, hk⟩ := steps_all hroot hs ht hφ (CfgTy.init h₀ hp)
    (Nest.init (Ψ := []) h₀ hp) (fun _ _ h => by simp at h)
  intro i e E N q v hi hE he hl
  have hask : ∀ {Ψ' Θ' t' φ'}, Typ S Ψ' Θ' [] (.app (.const (.ask N)) (.lit .unit)) (.ex t' φ') →
      φ'.svc (.nm N) := fun h => (ask_inv h).1
  have frame : .inside q ∈ E → Live c.rgs q := fun h => ((hn.thread i _ hi).2 q (he ▸ plug_frame h)).2
  rcases i with _ | ⟨r, j⟩
  · have hm : c.main = plug E _ := (Option.some.inj hi).trans he
    obtain ⟨_, _, _, _, hr, -⟩ := ctx_typing E hE ask_not_value (hm ▸ hc.2.1)
    rcases (need_out hask ask_not_value (ask_ok hr) E hE (hm ▸ hc.2.1)).1 q v hl with h' | h'
    · exact (Stage.yf_need hφ h').elim
    · exact frame h'
  · obtain ⟨g, hg, hgl, hj⟩ := Cfg.at_some.mp hi
    obtain ⟨t', φ', hkt, hkn⟩ := hk r g hg hgl _ (List.mem_append_left _ (List.mem_of_getElem? hj))
    obtain ⟨_, _, _, _, hr, -⟩ := ctx_typing E hE ask_not_value (he ▸ hkt)
    rcases (need_out hask ask_not_value (ask_ok hr) E hE (he ▸ hkt)).1 q v hl with h' | h'
    · exact hn.live r q ⟨g, hg, hgl⟩ (hkn N q h')
    · exact frame h'

/-- **T1 with `op_H`** (regions.md §5.3, corollary). As T1, and a thread that does not
step, is not finished and is not stuck at an `ask_S` or an operation nothing handles
waits for a child of a region it closes. -/
theorem T1_regions {root e₀ t φ₀ c} (hroot : RootTy S root)
    (hcov : ∀ N, φ₀.svc (.nm N) → (root.svc N).isSome) (h₀ : Typ S [] Facts.none [] e₀ (.ex t φ₀))
    (hp : e₀.Below 0) (ht : t.YF 0) (hφ : φ₀.substT (kill 0) = φ₀)
    (hs : CSteps root ⟨[], [], e₀⟩ c) :
    ((Value c.main ∧ ∃ Ψ, StoreTy S Ψ c.mem ∧ Typ S Ψ Facts.age [] c.main (.ex t .pure)) ∨
      (∃ v, c.main = .raise v ∧ ∃ Ψ, Typ S Ψ Facts.age [] v (.ex (.errors φ₀.err) .pure)) ∨
      (∃ l o, Step root l c.rgs c.mem c.main o) ∨ Waits c.rgs c.main) ∧
    ∀ (p : Nat) (g : Rgn), c.rgs[p]? = some g → g.live = true → ∀ k ∈ g.kids,
      Final k ∨ (∃ l o, Step root l c.rgs c.mem k o) ∨ (∃ D, AskStuck S root k D) ∨
        (∃ D, PerformStuck S k D) ∨ Waits c.rgs k := by
  obtain ⟨h₁, h₂⟩ := T1 hroot hcov h₀ hp ht hφ hs
  obtain ⟨hm, hk, -, -⟩ := T4 hroot h₀ hp ht hφ hs
  have h43 := T4_3 hroot h₀ hp ht hφ hs
  have waits : ∀ i e, c.at i = some e → (∀ E r, OkE E → e = plug E r → ¬ DeadUse c.rgs r) →
      Blocked c.rgs e → Waits c.rgs e := by
    rintro i e hi hd (⟨E, r, hE, he, hb⟩ | ⟨E, N, q, v, hE, he, hl, hq⟩)
    · exact hb.elim (fun hw => ⟨E, r, hE, he, hw⟩) fun hb' => (hd E r hE he hb').elim
    · exact (hq (h43 i e E N q v hi hE he hl)).elim
  refine ⟨h₁.imp_right (Or.imp_right (Or.imp_right (waits none _ rfl hm))), fun p g hg hl k hk' => ?_⟩
  obtain ⟨j, hj⟩ := List.mem_iff_getElem?.mp hk'
  exact (h₂ p g hg hl k hk').imp_right (Or.imp_right (Or.imp_right (Or.imp_right
    (waits (some (p, j)) k (Cfg.at_some.mpr ⟨g, hg, hl, hj⟩) (hk p g hg hl k hk')))))

end Flick
