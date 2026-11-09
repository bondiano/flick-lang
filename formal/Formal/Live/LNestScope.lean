import Formal.Live.LNestGrow

/-!
# Creating a region keeps `⊢ Λ` of run-time forms

A new region `π` gets as ancestors the regions of the place of its `scope` and
itself; its home is the thread of the `scope`.
-/

namespace Flick

variable {Ψ : List Ty} {anc : Nat → Nat → Prop}

/-- A redex creates the region `|Λ|`, a child of the regions of its place, whose
`inside` form holds a body without `inside` forms (`scope`, `hdl`). -/
theorem LNest.fresh {c : Cfg} {home i E r b} (hn : LNest Ψ c anc home)
    (hi : c.at i = some (plug E r)) (hE : OkE E) (hv : ¬ Value r) (hr : r.NoIns)
    (hb : ∀ q, ¬ b.Has (.frame q))
    (hbW : ∀ anc' : Nat → Nat → Prop, anc' c.rgs.length = (fun x => holeE (baseOf anc i) E x ∨ x = c.rgs.length) →
      (∀ q, q ≠ c.rgs.length → anc' q = anc q) →
      b.Fw Ψ anc' (fun x => holeE (baseOf anc i) E x ∨ x = c.rgs.length)) :
    ∃ anc' home', LNest Ψ ((⟨c.rgs ++ [Rgn.new], c.mem, c.main⟩ : Cfg).put i
      (plug E (.inside c.rgs.length b))) anc' home' ∧ ∀ q, q ≠ c.rgs.length → anc' q = anc q := by
  let p := c.rgs.length
  have hw := (hn.thread i _ hi).1
  obtain ⟨hWE, -⟩ := fw_of_plug hv hE hw
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
  have hrW : (Tm.inside p b).Fw Ψ anc' H := ⟨hpH, ancp, hbW anc' ancp ancq⟩
  have hthr : ∀ q, (plug E r).Has (.frame q) → Live c.rgs q := fun q hq => ((hn.thread i _ hi).2 q hq).2
  have hw' : (plug E r).Fw Ψ anc' A₀ := hw.mono (fun _ _ h => h) fun q hq =>
    ancq q (livep q (hq.elim (baseOf_live hn.live (Cfg.at_live hi)) (hthr q)))
  have hd : ∀ x, (Tm.inside p b).Has (.frame x) → r.Has (.frame x) ∨ ¬ (plug E r).Has (.frame x) := by
    intro x hx
    refine .inr fun h => livep x (hthr x h) ?_
    simp only [Tm.Has] at hx
    rcases hx with hx | hx | hx | hx
    · cases hx
    · cases hx
    · cases hx; rfl
    · exact (hb x hx).elim
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
  have frE : ∀ q, (plug E (.inside p b)).Has (.frame q) → (plug E (.var 0)).Has (.frame q) ∨ q = p := by
    intro q hq
    rcases plug_has_inv (r := .var 0) E hq with h | h
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
      refine ⟨hw'.replug hv hE hrW hd, fun q hq => ?_⟩
      rcases frE q hq with hq | rfl
      · have := (hn.thread i _ hi).2 q (plug_of_hole hq)
        exact ⟨(homeq q (livep q this.2)).trans this.1, (hlive q).mpr (.inl this.2)⟩
      · exact ⟨if_pos rfl, (hlive _).mpr (.inr rfl)⟩
    · obtain ⟨h₁, h₂⟩ := hn.thread i' e' he'
      rw [base' i' e' he']
      refine ⟨h₁.mono (fun _ _ h => h) fun q hq => ancq q (livep q (hq.elim
        (baseOf_live hn.live (Cfg.at_live he')) fun h => (h₂ q h).2)), fun q hq => ?_⟩
      obtain ⟨h₃, h₄⟩ := h₂ q hq
      exact ⟨(homeq q (livep q h₄)).trans h₃, (hlive q).mpr (.inl h₄)⟩
  · rcases (hlive q).mp hq with hq | rfl
    · obtain ⟨e', he', hq'⟩ := hn.home q hq
      rw [homeq q (livep q hq), hat]
      split
      · next h => rw [h, hi] at he'; cases he'
                  refine ⟨_, rfl, ?_⟩
                  rcases plug_has_inv (r := .var 0) E hq' with h' | h'
                  · exact plug_of_hole h'
                  · exact (hr.frame q h').elim
      · exact ⟨e', he', hq'⟩
    · exact ⟨plug E (.inside p b), by rw [show home' p = i from if_pos rfl, hat, if_pos rfl],
        plug_has E (by simp [Tm.Has])⟩
  · obtain ⟨g₀, h₀, h₁, h₂⟩ := Cfg.put_get hg
    rcases getA h₀ with ⟨hr, h₀⟩ | ⟨rfl, rfl⟩
    · rw [h₂] at hf
      have hl₀ : Live c.rgs r := ⟨g₀, h₀, h₁ ▸ hgl⟩
      obtain ⟨h₃, h₄⟩ := hn.fins r g₀ h₀ (h₁ ▸ hgl) f hf
      rw [ancq r (livep r hl₀)]
      exact ⟨h₃.mono (fun _ _ h => h) fun q hq => ancq q (livep q (hq.elim (hn.live r q hl₀)
        fun h => (h₄ q h).elim)), h₄⟩
    · rw [h₂] at hf; simp [Rgn.new] at hf
  · rw [Cfg.put_mem] at hv
    exact hn.cells l v hv

/-- `scope` creates the region `|Λ|`. -/
theorem LNest.scope {c : Cfg} {home i E e₁} (hn : LNest Ψ c anc home)
    (hi : c.at i = some (plug E (.scope e₁))) (hE : OkE E) :
    ∃ anc' home', LNest Ψ ((⟨c.rgs ++ [Rgn.new], c.mem, c.main⟩ : Cfg).put i
      (plug E (.inside c.rgs.length (e₁.subst (inst [.own c.rgs.length]))))) anc' home' ∧
      ∀ q, q ≠ c.rgs.length → anc' q = anc q := by
  obtain ⟨-, -, hn₁⟩ := fw_of_plug (by intro h; cases h) hE (hn.thread i _ hi).1
  have hb : (e₁.subst (inst [.own c.rgs.length])).NoIns := fun h => by
    rcases subst_inst_has h with h | ⟨w, hw, h⟩
    · exact hn₁ h
    · simp at hw; subst hw; cases h
  exact hn.fresh hi hE (by intro h; cases h) hn₁ hb.frame fun _ _ _ => Tm.Fw.of_noIns hb

end Flick
