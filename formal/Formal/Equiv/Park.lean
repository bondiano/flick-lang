import Formal.Equiv.Frames

/-!
# Contextual equivalence: parked bodies

A handler parks a body or an arm at its hole, and resumes it later by filling the hole. Related
parked bodies are related at `park`, and a body parked at a hole is related only to one: the same
frames, ordered on both sides, up to the frames of cells only one side has (`Sim.plug_hole`).
-/

namespace Flick.Equiv

open Flick.Elab (All2)

variable {W : World}

/-- A frame on the spine of a parked body related at `park`: the same frame with ordered parts,
the frame of a cell only the left side has, one only the right side has, or a coupled `settle` +
`let` pair of O2a. -/
theorem Sim.park_frame' {x : Pos} {F : Frame} {a t : Tm} (h : Sim W x (F.plug a) t)
    (hx : Pos.park = x) (hF : F.ok) (ha : a.Spine) :
    (∃ F' a', t = F'.plug a' ∧ FrS W F F' ∧ F'.ok ∧ Sim W .park a a') ∨
    (∃ n, F = .settle n ∧ W.J₁ n ∧ Sim W .park a t) ∨
    (∃ n t', t = .settle n t' ∧ W.J₂ n ∧ Sim W .park (F.plug a) t') ∨
    (∃ l l' F' a₀ a', a = .settle l a₀ ∧ t = F'.plug (.settle l' a') ∧
      FrL W [.settle l, F] [.settle l', F'] ∧ F'.ok ∧ Sim W .park a₀ a') := by
  have jr : ∀ {n t'}, x ≠ .top → W.J₂ n → Sim W x.fr (F.plug a) t' →
      (∃ n' t'', Tm.settle n t' = .settle n' t'' ∧ W.J₂ n' ∧ Sim W .park (F.plug a) t'') :=
    fun _ hn h' => ⟨_, _, rfl, hn, by subst hx; exact h'⟩
  cases F with
  | appL e =>
    simp only [Frame.plug] at h jr ⊢
    cases h with
    | appK _ h₁ h₂ => exact .inl ⟨_, _, rfl, .appL h₂, trivial, h₁⟩
    | appV hv _ _ => exact (ha.not_value hv).elim
    | jR hx' hn h' => exact .inr (.inr (.inl (jr hx' hn h')))
    | app hx' => exact (hx' hx.symm).elim
    | _ => cases hx
  | appR v =>
    simp only [Frame.plug] at h jr ⊢
    cases h with
    | appK hs _ _ => exact (hs.not_value hF).elim
    | appV _ h₁ h₂ => exact .inl ⟨_, _, rfl, .appR h₁.lower, h₁.top_value rfl hF, h₂⟩
    | jR hx' hn h' => exact .inr (.inr (.inl (jr hx' hn h')))
    | app hx' => exact (hx' hx.symm).elim
    | _ => cases hx
  | rconsL l r =>
    simp only [Frame.plug] at h jr ⊢
    cases h with
    | rconsK _ h₁ h₂ => exact .inl ⟨_, _, rfl, .rconsL h₂, trivial, h₁⟩
    | rconsV hv _ _ => exact (ha.not_value hv).elim
    | jR hx' hn h' => exact .inr (.inr (.inl (jr hx' hn h')))
    | rcons hx' => exact (hx' hx.symm).elim
    | _ => cases hx
  | rconsR l v =>
    simp only [Frame.plug] at h jr ⊢
    cases h with
    | rconsK hs _ _ => exact (hs.not_value hF).elim
    | rconsV _ h₁ h₂ => exact .inl ⟨_, _, rfl, .rconsR h₁.lower, h₁.top_value rfl hF, h₂⟩
    | jR hx' hn h' => exact .inr (.inr (.inl (jr hx' hn h')))
    | rcons hx' => exact (hx' hx.symm).elim
    | _ => cases hx
  | updL l e =>
    simp only [Frame.plug] at h jr ⊢
    cases h with
    | updK _ h₁ h₂ => exact .inl ⟨_, _, rfl, .updL h₂, trivial, h₁⟩
    | updV hv _ _ => exact (ha.not_value hv).elim
    | jR hx' hn h' => exact .inr (.inr (.inl (jr hx' hn h')))
    | update hx' => exact (hx' hx.symm).elim
    | _ => cases hx
  | updR v l =>
    simp only [Frame.plug] at h jr ⊢
    cases h with
    | updK hs _ _ => exact (hs.not_value hF).elim
    | updV _ h₁ h₂ => exact .inl ⟨_, _, rfl, .updR h₁.lower, h₁.top_value rfl hF, h₂⟩
    | jR hx' hn h' => exact .inr (.inr (.inl (jr hx' hn h')))
    | update hx' => exact (hx' hx.symm).elim
    | _ => cases hx
  | settle ℓ =>
    simp only [Frame.plug] at h jr ⊢
    cases h with
    | settle hl h' => exact .inl ⟨_, _, rfl, .settle hl, trivial, by subst hx; exact h'⟩
    | jL _ hn h' => exact .inr (.inl ⟨_, rfl, hn, by subst hx; exact h'⟩)
    | jR hx' hn h' => exact .inr (.inr (.inl (jr hx' hn h')))
    | _ => cases hx
  | let_ b =>
    simp only [Frame.plug] at h jr
    cases h with
    | let_ h₁ h₂ => subst hx; exact .inl ⟨.let_ _, _, rfl, .let_ h₂, trivial, h₁⟩
    | jR hx' hn h' => exact .inr (.inr (.inl (jr hx' hn h')))
    | againF2 _ hB hl ht hc hσ hσn hσv hσv' =>
      subst hx
      exact .inr (.inr (.inr ⟨_, _, .let_ _, _, _, rfl, rfl,
        .againF hB hl hc hσ hσn hσv hσv' .nil, trivial, ht⟩))
    | againR2 _ hB hl ht hc hσ hσn hσv hσv' =>
      subst hx
      exact .inr (.inr (.inr ⟨_, _, .let_ _, _, _, rfl, rfl,
        .againR hB hl hc hσ hσn hσv hσv' .nil, trivial, ht⟩))
    | _ => cases hx
  | match_ al =>
    simp only [Frame.plug] at h jr
    cases h <;> first
      | exact .inr (.inr (.inl (jr ‹_› ‹_› ‹_›)))
      | (cases hx; done)
      | (subst hx; exact .inl ⟨.match_ _, _, rfl, by constructor <;> assumption, trivial, ‹_›⟩)
  | proj l =>
    simp only [Frame.plug] at h jr
    cases h <;> first
      | exact .inr (.inr (.inl (jr ‹_› ‹_› ‹_›)))
      | (cases hx; done)
      | (subst hx; exact .inl ⟨.proj _, _, rfl, by constructor <;> assumption, trivial, ‹_›⟩)
  | await =>
    simp only [Frame.plug] at h jr
    cases h <;> first
      | exact .inr (.inr (.inl (jr ‹_› ‹_› ‹_›)))
      | (cases hx; done)
      | (subst hx; exact .inl ⟨.await, _, rfl, by constructor <;> assumption, trivial, ‹_›⟩)
  | try_ al =>
    simp only [Frame.plug] at h jr
    cases h <;> first
      | exact .inr (.inr (.inl (jr ‹_› ‹_› ‹_›)))
      | (cases hx; done)
      | (subst hx; exact .inl ⟨.try_ _, _, rfl, by constructor <;> assumption, trivial, ‹_›⟩)
  | raise =>
    simp only [Frame.plug] at h jr
    cases h <;> first
      | exact .inr (.inr (.inl (jr ‹_› ‹_› ‹_›)))
      | (cases hx; done)
      | (subst hx; exact .inl ⟨.raise, _, rfl, by constructor <;> assumption, trivial, ‹_›⟩)
  | inside p =>
    simp only [Frame.plug] at h jr
    cases h <;> first
      | exact .inr (.inr (.inl (jr ‹_› ‹_› ‹_›)))
      | (cases hx; done)
      | (subst hx; exact .inl ⟨.inside _, _, rfl, by constructor <;> assumption, trivial, ‹_›⟩)
  | hwith p ℓ E al r =>
    simp only [Frame.plug] at h jr
    cases h <;> first
      | exact .inr (.inr (.inl (jr ‹_› ‹_› ‹_›)))
      | (cases hx; done)
      | (subst hx; exact .inl ⟨.hwith _ _ _ _ _, _, rfl, by constructor <;> assumption, trivial, ‹_›⟩)
  | hctlA p ℓ κ tl E al r k =>
    simp only [Frame.plug] at h jr
    cases h <;> first
      | exact .inr (.inr (.inl (jr ‹_› ‹_› ‹_›)))
      | (cases hx; done)
      | (subst hx; exact .inl ⟨.hctlA _ _ _ _ _ _ _ _, _, rfl, by constructor <;> assumption, trivial, ‹_›⟩)
  | hresB p ℓ a₀ =>
    simp only [Frame.plug] at h jr
    cases h <;> first
      | exact .inr (.inr (.inl (jr ‹_› ‹_› ‹_›)))
      | (cases hx; done)
      | (subst hx; exact .inl ⟨.hresB _ _ _, _, rfl, by constructor <;> assumption, trivial, ‹_›⟩)
  | hdone p =>
    simp only [Frame.plug] at h jr
    cases h <;> first
      | exact .inr (.inr (.inl (jr ‹_› ‹_› ‹_›)))
      | (cases hx; done)
      | (subst hx; exact .inl ⟨.hdone _, _, rfl, by constructor <;> assumption, trivial, ‹_›⟩)
  | hkeep q ℓ E al =>
    simp only [Frame.plug] at h jr
    cases h <;> first
      | exact .inr (.inr (.inl (jr ‹_› ‹_› ‹_›)))
      | (cases hx; done)
      | (subst hx; exact .inl ⟨.hkeep _ _ _ _, _, rfl, by constructor <;> assumption, trivial, ‹_›⟩)
  | hkarmA q ℓ E al k =>
    simp only [Frame.plug] at h jr
    cases h <;> first
      | exact .inr (.inr (.inl (jr ‹_› ‹_› ‹_›)))
      | (cases hx; done)
      | (subst hx; exact .inl ⟨.hkarmA _ _ _ _ _, _, rfl, by constructor <;> assumption, trivial, ‹_›⟩)
  | force =>
    simp only [Frame.plug] at h jr
    cases h <;> first
      | exact .inr (.inr (.inl (jr ‹_› ‹_› ‹_›)))
      | (cases hx; done)
      | (subst hx; exact .inl ⟨.force, _, rfl, by constructor <;> assumption, trivial, ‹_›⟩)

theorem Sim.park_frame {F : Frame} {a t : Tm} (h : Sim W .park (F.plug a) t) (hF : F.ok)
    (ha : a.Spine) :
    (∃ F' a', t = F'.plug a' ∧ FrS W F F' ∧ F'.ok ∧ Sim W .park a a') ∨
    (∃ n, F = .settle n ∧ W.J₁ n ∧ Sim W .park a t) ∨
    (∃ n t', t = .settle n t' ∧ W.J₂ n ∧ Sim W .park (F.plug a) t') ∨
    (∃ l l' F' a₀ a', a = .settle l a₀ ∧ t = F'.plug (.settle l' a') ∧
      FrL W [.settle l, F] [.settle l', F'] ∧ F'.ok ∧ Sim W .park a₀ a') :=
  Sim.park_frame' h rfl hF ha

/-- Only the frames of right cells around a hole. -/
theorem Sim.hole_strip {p : Nat} : ∀ {k₂ : Tm}, Sim W .park (.hole p) k₂ →
    ∃ Ei₂, k₂ = plug Ei₂ (.hole p) ∧ FrL W [] Ei₂ ∧ OkE Ei₂ := by
  intro k₂
  induction k₂ with
  | hole q =>
    intro h
    cases h with
    | hole => exact ⟨[], rfl, .nil, fun _ h => by cases h⟩
    | _ => cases ‹Pos.park = Pos.any›
  | settle n t ih =>
    intro h
    generalize hx : Pos.park = x at h
    cases h with
    | jR _ hn h' =>
      subst hx
      obtain ⟨Ei₂, rfl, hfr, hok⟩ := ih h'
      refine ⟨Ei₂ ++ [.settle n], by rw [plug_snoc]; rfl, ?_, ?_⟩
      · simpa using hfr.append (FrL.junkR hn .nil)
      · intro G hG
        rcases List.mem_append.mp hG with hG | hG
        · exact hok G hG
        · simp at hG; subst hG; trivial
    | _ => cases hx
  | _ =>
    intro h
    generalize hx : Pos.park = x at h
    cases h <;> first | cases hx | (rename_i h; exact (h hx.symm).elim)

theorem okE_snoc {E : List Frame} {F : Frame} (hE : OkE E) (hF : F.ok) : OkE (E ++ [F]) := by
  intro G hG
  rcases List.mem_append.mp hG with hG | hG
  · exact hE G hG
  · simp at hG; subst hG; exact hF

/-- One frame (or a coupled `settle` + `let` pair) of a parked body, up to the frames of right cells. -/
theorem Sim.frame_strip {F : Frame} {E₀ : List Frame} {p : Nat} (hF : F.ok) (hE₀ : OkE E₀)
    (ih : ∀ t, Sim W .park (plug E₀ (.hole p)) t → ∃ Ei₂, t = plug Ei₂ (.hole p) ∧ FrL W E₀ Ei₂ ∧ OkE Ei₂)
    (ih₂ : ∀ l E₀₀, E₀ = E₀₀ ++ [.settle l] → ∀ t, Sim W .park (plug E₀₀ (.hole p)) t →
      ∃ Ei₂, t = plug Ei₂ (.hole p) ∧ FrL W E₀₀ Ei₂ ∧ OkE Ei₂) :
    ∀ {k₂ : Tm}, Sim W .park (F.plug (plug E₀ (.hole p))) k₂ →
      ∃ Ei₂, k₂ = plug Ei₂ (.hole p) ∧ FrL W (E₀ ++ [F]) Ei₂ ∧ OkE Ei₂ := by
  have ha : (plug E₀ (.hole p)).Spine := plug_spine hE₀ (a := .hole p) trivial
  have pair : ∀ {l l' : Nat} {F' : Frame} {a₀ a' : Tm},
      plug E₀ (.hole p) = .settle l a₀ → FrL W [.settle l, F] [.settle l', F'] → F'.ok →
      Sim W .park a₀ a' →
      ∃ Ei₂, F'.plug (.settle l' a') = plug Ei₂ (.hole p) ∧ FrL W (E₀ ++ [F]) Ei₂ ∧ OkE Ei₂ := by
    intro l l' F' a₀ a' heq hpair hF' ha'
    rcases snoc_or E₀ with rfl | ⟨E₀₀, G₀, rfl⟩
    · simp [plug] at heq
    · rw [plug_snoc] at heq
      cases G₀ <;> simp only [Frame.plug, reduceCtorEq, Tm.settle.injEq] at heq
      obtain ⟨rfl, rfl⟩ := heq
      obtain ⟨Ei₂, rfl, hfr, hok⟩ := ih₂ _ E₀₀ rfl a' ha'
      refine ⟨Ei₂ ++ [.settle l', F'], ?_, ?_, ?_⟩
      · rw [plug_append]; rfl
      · simpa [List.append_assoc] using hfr.append hpair
      · intro G hG
        rcases List.mem_append.mp hG with hG | hG
        · exact hok G hG
        · simp only [List.mem_cons, List.not_mem_nil, or_false] at hG
          rcases hG with rfl | rfl
          · trivial
          · exact hF'
  intro k₂
  induction k₂ with
  | settle n t ihk =>
    intro h
    rcases h.park_frame hF ha with ⟨F', a', he, hfs, hF', h'⟩ | ⟨n', rfl, hn, h'⟩ |
      ⟨n', t', he, hn, h'⟩ | ⟨l, l', F', a₀, a', he₀, he, hpair, hF', ha'⟩
    · obtain ⟨Ei₂, rfl, hfr, hok⟩ := ih a' h'
      exact ⟨Ei₂ ++ [F'], by rw [plug_snoc, he], hfr.append (.cons hfs .nil), okE_snoc hok hF'⟩
    · obtain ⟨Ei₂, he, hfr, hok⟩ := ih _ h'
      exact ⟨Ei₂, he, by simpa using hfr.append (FrL.junkL hn .nil), hok⟩
    · cases he
      obtain ⟨Ei₂, rfl, hfr, hok⟩ := ihk h'
      exact ⟨Ei₂ ++ [.settle n], by rw [plug_snoc]; rfl, by simpa using hfr.append (FrL.junkR hn .nil),
        okE_snoc (F := .settle n) hok trivial⟩
    · rw [he]; exact pair he₀ hpair hF' ha'
  | _ =>
    intro h
    rcases h.park_frame hF ha with ⟨F', a', he, hfs, hF', h'⟩ | ⟨n', rfl, hn, h'⟩ |
      ⟨n', t', he, hn, h'⟩ | ⟨l, l', F', a₀, a', he₀, he, hpair, hF', ha'⟩
    · obtain ⟨Ei₂, rfl, hfr, hok⟩ := ih a' h'
      exact ⟨Ei₂ ++ [F'], by rw [plug_snoc, he], hfr.append (.cons hfs .nil), okE_snoc hok hF'⟩
    · obtain ⟨Ei₂, he, hfr, hok⟩ := ih _ h'
      exact ⟨Ei₂, he, by simpa using hfr.append (FrL.junkL hn .nil), hok⟩
    · cases he
    · rw [he]; exact pair he₀ hpair hF' ha'

theorem Sim.plug_hole' {p : Nat} : ∀ (m : Nat) {Ei : List Frame} {k₂ : Tm}, Ei.length ≤ m → OkE Ei →
    Sim W .park (plug Ei (.hole p)) k₂ → ∃ Ei₂, k₂ = plug Ei₂ (.hole p) ∧ FrL W Ei Ei₂ ∧ OkE Ei₂ := by
  intro m
  induction m with
  | zero =>
    intro Ei k₂ hl _ h
    cases Ei with
    | nil => exact Sim.hole_strip h
    | cons => simp at hl
  | succ m ih =>
    intro Ei k₂ hl hE h
    rcases snoc_or Ei with rfl | ⟨E₀, F, rfl⟩
    · exact Sim.hole_strip h
    · rw [plug_snoc] at h
      have hE₀ : OkE E₀ := fun G hG => hE G (List.mem_append_left _ hG)
      have hF : F.ok := hE F (List.mem_append_right _ (List.mem_singleton_self _))
      exact Sim.frame_strip hF hE₀
        (fun t ht => ih (by simp at hl; omega) hE₀ ht)
        (fun l E₀₀ heq t ht => ih (by subst heq; simp at hl; omega)
          (fun G hG => hE₀ G (heq ▸ List.mem_append_left _ hG)) ht) h

/-- **A parked body** is related at `park` only to a parked body: ordered frames around the
same hole, related up to the frames of cells only one side has. -/
theorem Sim.plug_hole {p : Nat} {Ei : List Frame} {k₂ : Tm} (hE : OkE Ei)
    (h : Sim W .park (plug Ei (.hole p)) k₂) :
    ∃ Ei₂, k₂ = plug Ei₂ (.hole p) ∧ FrL W Ei Ei₂ ∧ OkE Ei₂ :=
  Sim.plug_hole' _ (Nat.le_refl _) hE h

end Flick.Equiv
