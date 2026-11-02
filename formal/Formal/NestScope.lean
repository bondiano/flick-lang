import Formal.NestRedex

/-!
# Creating and closing a region keeps `⊢ Λ`; L5

A new region `π` gets as ancestors the regions of the place of its `scope` and
itself; its home is the thread of the `scope`. Closing `π` drops it from what live
work may name: no live region but `π` has `π` among its ancestors (`Nest.nested`,
L5), since such a region has its `inside` form in the body of `π` or in a child
of a region inside `π`, and these are finished.
-/

namespace Flick

variable {S : Sig} {Ψ : List Ty} {B anc : Nat → Nat → Prop}

theorem Cfg.at_append {Λ : List Rgn} {μ : List Tm} {m : Tm} (i : Option (Nat × Nat)) :
    (⟨Λ ++ [Rgn.new], μ, m⟩ : Cfg).at i = (⟨Λ, μ, m⟩ : Cfg).at i := by
  rcases i with _ | ⟨r, j⟩
  · rfl
  · apply Option.ext; intro k
    rw [Cfg.at_some, Cfg.at_some]
    simp only
    by_cases hr : r < Λ.length
    · rw [List.getElem?_append_left hr]
    · rw [List.getElem?_eq_none (show Λ.length ≤ r by omega)]
      constructor
      · rintro ⟨g, hg, -, hk⟩
        rw [List.getElem?_append_right (by omega)] at hg
        cases hj : r - Λ.length with
        | zero => rw [hj] at hg; simp at hg; subst hg; simp [Rgn.new] at hk
        | succ n => rw [hj] at hg; simp at hg
      · rintro ⟨g, hg, -⟩; cases hg

theorem Frame.Within.withAnc {anc' : Nat → Nat → Prop} {F : Frame} {A : Nat → Prop}
    (h : F.Within Ψ B anc A) (ha : ∀ q, F = .inside q → anc' q = anc q) : F.Within Ψ B anc' A := by
  have m : ∀ {x : Tm} {A'}, x.NoIns → x.Within Ψ B anc A' → x.Within Ψ B anc' A' :=
    fun hn hx => hx.mono (fun _ _ h => h) fun q hq => (hn.frame q hq).elim
  cases F <;> simp only [Frame.Within] at h ⊢ <;>
    first
    | exact ⟨m h.2 h.1, h.2⟩
    | exact ⟨m (h.2.noIns h.1) h.1, h.2⟩
    | exact ⟨h.1, m h.1 h.2⟩
    | trivial
    | exact ⟨h.1, (ha _ rfl).trans h.2⟩

theorem WithinE.withAnc {anc' : Nat → Nat → Prop} {A : Nat → Prop} :
    ∀ {E : List Frame}, WithinE Ψ B anc A E → (∀ q, .inside q ∈ E → anc' q = anc q) →
      WithinE Ψ B anc' A E
  | [], _, _ => trivial
  | F :: _, ⟨hE, hF⟩, ha =>
    ⟨WithinE.withAnc hE fun q hq => ha q (List.mem_cons_of_mem _ hq),
      hF.withAnc fun q hq => ha q (by rw [hq]; exact List.mem_cons_self)⟩

/-- The regions of the hole of a thread are live. -/
theorem Nest.hole_live {c : Cfg} {home i E r} (hn : Nest Ψ c anc B home) (hi : c.at i = some (plug E r))
    {x : Nat} (hx : holeE (baseOf anc i) E x) : Live c.rgs x := by
  rcases holeE_iff.mp hx with hx | hx
  · exact baseOf_live hn.live (Cfg.at_live hi) hx
  · exact ((hn.thread i _ hi).2 x (plug_frame hx)).2

/-- A redex creates the region `|Λ|`, a child of the regions of its place, whose
`inside` form holds a body without `inside` forms (`scope`, `provide`). -/
theorem Nest.fresh {c : Cfg} {home i E r b} (hn : Nest Ψ c anc B home)
    (hi : c.at i = some (plug E r)) (hE : OkE E) (hv : ¬ Value r) (hr : r.NoIns)
    (hb : ∀ q, ¬ b.Has (.frame q))
    (hbW : b.Within Ψ B anc (fun x => holeE (baseOf anc i) E x ∨ x = c.rgs.length)) :
    ∃ anc' home', Nest Ψ ((⟨c.rgs ++ [Rgn.new], c.mem, c.main⟩ : Cfg).put i
      (plug E (.inside c.rgs.length b))) anc' B home' ∧ ∀ q, q ≠ c.rgs.length → anc' q = anc q := by
  let p := c.rgs.length
  have hw := (hn.thread i _ hi).1
  obtain ⟨hWE, -⟩ := within_of_plug hv hE hw
  let A₀ := baseOf anc i
  let H := holeE A₀ E
  have Hlive : ∀ x, H x → Live c.rgs x := fun x hx => hn.hole_live hi hx
  have Hlt : ∀ x, H x → x < p := fun x hx => (Hlive x hx).lt
  have hpH : ¬ H p := fun h => Nat.lt_irrefl _ (Hlt p h)
  have livep : ∀ x, Live c.rgs x → x ≠ p := fun x hx h => by subst h; exact Nat.lt_irrefl _ hx.lt
  let anc' : Nat → Nat → Prop := fun q => if q = p then (fun x => H x ∨ x = p) else anc q
  let home' : Nat → Option (Nat × Nat) := fun q => if q = p then i else home q
  have ancq : ∀ q, q ≠ p → anc' q = anc q := fun q hq => if_neg hq
  have ancp : anc' p = fun x => H x ∨ x = p := if_pos rfl
  have homeq : ∀ q, q ≠ p → home' q = home q := fun q hq => if_neg hq
  have chainH : Chain anc H := hWE.chain hn.le (hn.base_chain hi)
  have chainH' : Chain anc' H := fun x hx => by rw [ancq x (fun h => hpH (h ▸ hx))]; exact chainH x hx
  have hbW' : b.Within Ψ B anc' (fun x => H x ∨ x = p) :=
    hbW.mono (fun _ _ h => h) fun q hq => (hb q hq).elim
  have hrW : (Tm.inside p b).Within Ψ B anc' H := ⟨hpH, ancp, hbW'⟩
  have hframes : ∀ q, .inside q ∈ E → Live c.rgs q := fun q hq => ((hn.thread i _ hi).2 q (plug_frame hq)).2
  have hWE' : WithinE Ψ B anc' A₀ E := hWE.withAnc fun q hq => ancq q (livep q (hframes q hq))
  have base' : ∀ i' e', c.at i' = some e' → baseOf anc' i' = baseOf anc i' := by
    rintro (_ | ⟨r, j⟩) e' h'
    · rfl
    · exact ancq r (livep r (Cfg.at_live h' r j rfl))
  have hc₁ : (⟨c.rgs ++ [Rgn.new], c.mem, c.main⟩ : Cfg).at i = some (plug E r) := by
    rw [Cfg.at_append]; exact hi
  have hat : ∀ i', ((⟨c.rgs ++ [Rgn.new], c.mem, c.main⟩ : Cfg).put i
      (plug E (.inside p b))).at i' = if i' = i then some (plug E (.inside p b)) else c.at i' := by
    intro i'; rw [Cfg.at_put hc₁, Cfg.at_append]
  have getA : ∀ {r g}, (c.rgs ++ [Rgn.new])[r]? = some g → (r < p ∧ c.rgs[r]? = some g) ∨ (r = p ∧ g = Rgn.new) := by
    intro r g h
    by_cases hr : r < p
    · rw [List.getElem?_append_left hr] at h; exact .inl ⟨hr, h⟩
    · rw [List.getElem?_append_right (by omega)] at h
      cases hj : r - c.rgs.length with
      | zero => simp [hj] at h; exact .inr ⟨by omega, h.symm⟩
      | succ => simp [hj] at h
  have hlive : ∀ q, Live ((⟨c.rgs ++ [Rgn.new], c.mem, c.main⟩ : Cfg).put i
      (plug E (.inside p b))).rgs q ↔ Live c.rgs q ∨ q = p := by
    intro q
    rw [Cfg.put_live]
    constructor
    · rintro ⟨g, hg, hl⟩
      rcases getA hg with ⟨_, hg⟩ | ⟨rfl, -⟩
      · exact .inl ⟨g, hg, hl⟩
      · exact .inr rfl
    · rintro (⟨g, hg, hl⟩ | rfl)
      · exact ⟨g, List.getElem?_append_left (getElem?_lt hg) ▸ hg, hl⟩
      · exact ⟨Rgn.new, by simp [p], rfl⟩
  have frE : ∀ q, (plug E (.inside p b)).Has (.frame q) → .inside q ∈ E ∨ q = p := by
    intro q hq
    rcases hWE'.frames hq with h | h
    · exact .inl h
    · rcases h with h | h | h | h
      · cases h
      · cases h
      · cases h; exact .inr rfl
      · exact (hb q h).elim
  refine ⟨anc', home', ⟨fun q => ?_, fun q x hx => ?_, fun q hq => ?_, fun q x hq hx => ?_,
    fun i' e' he' => ?_, fun q hq => ?_, fun r g hg hgl f hf => ?_, fun l v hv => ?_⟩,
    fun q hq => ancq q hq⟩
  · by_cases hq : q = p
    · subst hq; rw [ancp]; exact .inr rfl
    · rw [ancq q hq]; exact hn.self q
  · by_cases hq : q = p
    · subst hq; rw [ancp] at hx
      rcases hx with hx | rfl
      · exact Nat.le_of_lt (Hlt x hx)
      · exact Nat.le_refl _
    · rw [ancq q hq] at hx; exact hn.le q x hx
  · rcases (hlive q).mp hq with hq | rfl
    · rw [ancq q (livep q hq)]
      intro x hx
      rw [ancq x (livep x (hn.live q x hq hx))]
      exact hn.chain q hq x hx
    · rw [ancp]
      exact chainH'.inside hpH ancp fun x hx => by
        rw [ancp] at hx
        rcases hx with hx | rfl
        · exact Nat.le_of_lt (Hlt x hx)
        · exact Nat.le_refl _
  · rcases (hlive q).mp hq with hq | rfl
    · rw [ancq q (livep q hq)] at hx; exact (hlive x).mpr (.inl (hn.live q x hq hx))
    · rw [ancp] at hx
      rcases hx with hx | rfl
      · exact (hlive x).mpr (.inl (Hlive x hx))
      · exact (hlive _).mpr (.inr rfl)
  · rw [hat] at he'
    split at he'
    · subst i'; cases he'
      rw [base' i _ hi]
      refine ⟨hWE'.fill hrW, fun q hq => ?_⟩
      rcases frE q hq with hq | rfl
      · have := (hn.thread i _ hi).2 q (plug_frame hq)
        exact ⟨(homeq q (livep q this.2)).trans this.1, (hlive q).mpr (.inl this.2)⟩
      · exact ⟨if_pos rfl, (hlive _).mpr (.inr rfl)⟩
    · obtain ⟨h₁, h₂⟩ := hn.thread i' e' he'
      rw [base' i' e' he']
      refine ⟨h₁.mono (fun _ _ h => h) fun q hq => ancq q (livep q (h₂ q hq).2), fun q hq => ?_⟩
      obtain ⟨h₃, h₄⟩ := h₂ q hq
      exact ⟨(homeq q (livep q h₄)).trans h₃, (hlive q).mpr (.inl h₄)⟩
  · rcases (hlive q).mp hq with hq | rfl
    · obtain ⟨e', he', hq'⟩ := hn.home q hq
      rw [homeq q (livep q hq), hat]
      split
      · next h => rw [h, hi] at he'; cases he'
                  refine ⟨_, rfl, ?_⟩
                  rcases hWE.frames hq' with h' | h'
                  · exact plug_frame h'
                  · exact (hr.frame q h').elim
      · exact ⟨e', he', hq'⟩
    · exact ⟨plug E (.inside p b), by rw [show home' p = i from if_pos rfl, hat, if_pos rfl],
        plug_has E (by simp [Tm.Has])⟩
  · obtain ⟨g₀, h₀, h₁, h₂⟩ := Cfg.put_get hg
    rcases getA h₀ with ⟨hr, h₀⟩ | ⟨rfl, rfl⟩
    · rw [h₂] at hf
      obtain ⟨h₃, h₄⟩ := hn.fins r g₀ h₀ (h₁ ▸ hgl) f hf
      rw [ancq r (livep r ⟨g₀, h₀, h₁ ▸ hgl⟩)]
      exact ⟨h₃.mono (fun _ _ h => h) fun q hq => (h₄ q hq).elim, h₄⟩
    · rw [h₂] at hf; simp [Rgn.new] at hf
  · rw [Cfg.put_mem] at hv
    obtain ⟨h₁, h₂⟩ := hn.cells l v hv
    exact ⟨h₁.mono (fun _ _ h => h) fun q hq => (h₂.frame q hq).elim, h₂⟩

/-- A term without run-time forms moves into a new region `p` that no type names. -/
theorem Tm.Within.extend {e : Tm} {H : Nat → Prop} {p : Nat} (hn : e.NoIns) (h : e.Within Ψ B anc H)
    (hΨ : ∀ (l : Nat) (t : Ty), Ψ[l]? = some t → ¬ t.reach (.r p)) :
    e.Within Ψ B anc (fun x => H x ∨ x = p) := by
  refine Tm.Within.of_has hn (fun x hx => .inl (h.rgn0 hn hx)) fun l hl => ?_
  obtain ⟨t, ht, hB⟩ := h.loc0 hn hl
  refine ⟨t, ht, hB.trans (funext fun x => propext ⟨fun ⟨h, h'⟩ => ⟨.inl h, h'⟩, ?_⟩)⟩
  rintro ⟨h | rfl, h'⟩
  · exact ⟨h, h'⟩
  · exact absurd h' (hΨ l t ht)

/-- `scope` creates the region `|Λ|`. -/
theorem Nest.scope {c : Cfg} {home i E e₁} (hn : Nest Ψ c anc B home)
    (hi : c.at i = some (plug E (.scope e₁))) (hE : OkE E) (hΨ : ∀ u ∈ Ψ, u.YF c.rgs.length) :
    ∃ anc' home', Nest Ψ ((⟨c.rgs ++ [Rgn.new], c.mem, c.main⟩ : Cfg).put i
      (plug E (.inside c.rgs.length (e₁.subst (inst [.own c.rgs.length]))))) anc' B home' ∧
      ∀ q, q ≠ c.rgs.length → anc' q = anc q := by
  obtain ⟨-, h₁, hn₁⟩ := within_of_plug (by intro h; cases h) hE (hn.thread i _ hi).1
  have hb : (e₁.subst (inst [.own c.rgs.length])).NoIns := fun h => by
    rcases subst_inst_has h with h | ⟨w, hw, h⟩
    · exact hn₁ h
    · simp at hw; subst hw; cases h
  refine hn.fresh hi hE (by intro h; cases h) hn₁ hb.frame (Tm.Within.of_has hb (fun x hx => ?_) fun l hl => ?_)
  · rcases subst_inst_has hx with hx | ⟨w, hw, hx⟩
    · exact .inl (h₁.rgn0 hn₁ hx)
    · simp at hw; subst hw; cases hx; exact .inr rfl
  · rcases subst_inst_has hl with hl | ⟨w, hw, hl⟩
    · exact (h₁.extend hn₁ fun l' t ht => fun h => Nat.lt_irrefl _
        ((hΨ t (List.mem_of_getElem? ht)).reach h)).loc0 hn₁ hl
    · simp at hw; subst hw; cases hl

end Flick
