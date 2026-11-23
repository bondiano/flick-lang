import Formal.Lazy

/-!
# Linear settle frames for lazy cells

In a configuration a program reaches, every `settle ℓ` frame belongs to a lazy cell `ℓ` that is
currently `Running`, and at most one `settle ℓ` frame exists for each cell `ℓ` across the main
thread, all children and finalizers, and all continuations parked in handlers (`hctl`, `hres`,
`hkarm`).

`Tm.settleCount l e` counts occurrences of `settle l` in `e` (including parked continuations).
`Tm.NoSettle e` says `e` has no `settle` frame for any cell. `Tm.SettleLin e` restricts `settle`
frames to active evaluation positions and parked continuations (never inside values, lambdas,
unexecuted branches, or the store).
-/

namespace Flick

/-- Number of `settle l` frames anywhere in `e`. -/
def Tm.settleCount (l : Nat) : Tm → Nat
  | .settle m e => (if m = l then 1 else 0) + e.settleCount l
  | .lam b | .fix b | .task b | .await b | .lazy b | .force b | .raise b | .scope b | .proj b _
  | .inside _ b | .drec _ b | .hdone _ b | .unw _ _ b => b.settleCount l
  | .app a b | .let_ a b | .letrec a b | .rcons _ a b | .update a _ b | .match_ a b
  | .acons _ a b | .try_ a b | .handle a b | .hres _ _ a b | .hkeep _ _ _ a b =>
      a.settleCount l + b.settleCount l
  | .aguard _ a b c | .hdl _ a b c | .hwith _ _ _ a b c | .hkarm _ _ _ a b c =>
      a.settleCount l + b.settleCount l + c.settleCount l
  | .hctl _ _ _ _ _ a b c d =>
      a.settleCount l + b.settleCount l + c.settleCount l + d.settleCount l
  | _ => 0

/-- `e` contains no `settle` frame for any lazy cell. -/
def Tm.NoSettle (e : Tm) : Prop := ∀ l, e.settleCount l = 0

/-- `settle` frames occur only in active evaluation positions and parked continuations, never inside
values, unevaluated code, or the bodies of signals. -/
def Tm.SettleLin : Tm → Prop
  | .var _ | .lit _ | .loc _ | .const _ | .ctor _ | .rnil | .anil | .todo
  | .cap _ | .own _ | .glob _ | .kont _ _ | .hole _ => True
  | .lam b | .fix b | .task b | .lazy b | .scope b | .drec _ b => b.NoSettle
  | .letrec f b => f.NoSettle ∧ b.NoSettle
  | .acons _ b r => b.NoSettle ∧ r.NoSettle
  | .aguard _ g b r => g.NoSettle ∧ b.NoSettle ∧ r.NoSettle
  | .handle v al => v.NoSettle ∧ al.NoSettle
  | .hdl _ al r e => al.NoSettle ∧ r.NoSettle ∧ e.NoSettle
  | .unw _ _ a => a.NoSettle
  | .app f a | .rcons _ f a | .update f _ a => f.SettleLin ∧ a.SettleLin ∧ (a.NoSettle ∨ Value f)
  | .let_ e b => e.SettleLin ∧ b.NoSettle
  | .proj e _ | .await e | .force e | .raise e | .inside _ e | .hdone _ e | .settle _ e =>
      e.SettleLin
  | .match_ e al | .try_ e al => e.SettleLin ∧ al.NoSettle
  | .hwith _ _ _ al r e => al.NoSettle ∧ r.NoSettle ∧ e.SettleLin
  | .hkeep _ _ _ al e => al.NoSettle ∧ e.SettleLin
  | .hctl _ _ _ _ _ al r a k => al.NoSettle ∧ r.NoSettle ∧ a.SettleLin ∧ k.SettleLin
  | .hkarm _ _ _ al a k => al.NoSettle ∧ a.SettleLin ∧ k.SettleLin
  | .hres _ _ a b => a.SettleLin ∧ b.SettleLin

theorem Tm.SettleLin.of_noSettle : ∀ {e : Tm}, e.NoSettle → e.SettleLin := by
  intro e
  induction e <;> intro h <;> simp only [Tm.SettleLin]
  all_goals first
    | trivial
    | (have h' : ∀ l, _ := fun l => by have := h l; simp only [Tm.settleCount] at this; omega
       exact h')
    | (refine ⟨?_, ?_⟩
       · first | (intro l; have := h l; simp only [Tm.settleCount] at this; omega)
               | (apply_assumption; intro l; have := h l; simp only [Tm.settleCount] at this; omega)
       · first | (intro l; have := h l; simp only [Tm.settleCount] at this; omega)
               | (apply_assumption; intro l; have := h l; simp only [Tm.settleCount] at this; omega))
    | (refine ⟨?_, ?_, ?_⟩
       · first | (intro l; have := h l; simp only [Tm.settleCount] at this; omega)
               | (apply_assumption; intro l; have := h l; simp only [Tm.settleCount] at this; omega)
       · first | (intro l; have := h l; simp only [Tm.settleCount] at this; omega)
               | (apply_assumption; intro l; have := h l; simp only [Tm.settleCount] at this; omega)
       · first | exact .inl (fun l => by have := h l; simp only [Tm.settleCount] at this; omega)
               | (intro l; have := h l; simp only [Tm.settleCount] at this; omega)
               | (apply_assumption; intro l; have := h l; simp only [Tm.settleCount] at this; omega))
    | (refine ⟨?_, ?_, ?_, ?_⟩
       · intro l; have := h l; simp only [Tm.settleCount] at this; omega
       · intro l; have := h l; simp only [Tm.settleCount] at this; omega
       · apply_assumption; intro l; have := h l; simp only [Tm.settleCount] at this; omega
       · apply_assumption; intro l; have := h l; simp only [Tm.settleCount] at this; omega)
    | (apply_assumption; intro l; have := h l; simp only [Tm.settleCount] at this; omega)

theorem Value.noSettle {e : Tm} (hv : Value e) (h : e.SettleLin) : e.NoSettle := by
  induction hv with
  | lam | fix | task | drec => exact h
  | rcons _ _ ih₁ ih₂ =>
    intro l; simp only [Tm.settleCount, ih₁ h.1 l, ih₂ h.2.1 l]
  | ctorApp _ _ _ ih₁ ih₂ =>
    intro l; simp only [Tm.settleCount, ih₁ h.1 l, ih₂ h.2.1 l]
  | constApp _ _ ih =>
    intro l; simp only [Tm.settleCount, ih h.2.1 l]
  | _ => intro l; rfl

theorem Tm.settleCount_rename (l : Nat) :
    ∀ (e : Tm) (ξ : Nat → Nat), (e.rename ξ).settleCount l = e.settleCount l := by
  intro e
  induction e <;> intro ξ <;> simp_all [Tm.rename, Tm.settleCount]

theorem Tm.NoSettle.rename {e : Tm} (h : e.NoSettle) (ξ : Nat → Nat) : (e.rename ξ).NoSettle :=
  fun l => by rw [Tm.settleCount_rename]; exact h l

theorem Tm.NoSettle.subst_at (l : Nat) :
    ∀ {e : Tm}, e.settleCount l = 0 → ∀ {σ : Nat → Tm}, (∀ i, (σ i).settleCount l = 0) →
      (e.subst σ).settleCount l = 0 := by
  have lift : ∀ {n : Nat} {σ : Nat → Tm}, (∀ i, (σ i).settleCount l = 0) →
      ∀ i, (liftS n σ i).settleCount l = 0 := by
    intro n σ h i; unfold liftS; split
    · rfl
    · rw [Tm.settleCount_rename]; exact h _
  intro e
  induction e <;> intro h σ hσ <;> simp only [Tm.subst, Tm.settleCount] at h ⊢ <;>
    grind

theorem Tm.NoSettle.subst {e : Tm} (he : e.NoSettle) {σ : Nat → Tm} (hσ : ∀ i, (σ i).NoSettle) :
    (e.subst σ).NoSettle :=
  fun l => Tm.NoSettle.subst_at l (he l) (fun i => hσ i l)

theorem Tm.NoSettle.subst_inst {e : Tm} (he : e.NoSettle) {δ : List Tm}
    (hδ : ∀ w ∈ δ, w.NoSettle) : (e.subst (inst δ)).NoSettle :=
  he.subst fun i => by
    unfold inst; split
    · exact hδ _ (List.getElem_mem _)
    · intro l; rfl

theorem Tm.NoSettle.field : ∀ {v x : Tm} {f}, v.NoSettle → v.field f = some x → x.NoSettle := by
  intro v
  induction v with
  | rcons l' e r _ ih =>
    intro x f hv hf
    simp only [Tm.field] at hf
    split at hf
    · cases hf; intro l; have := hv l; simp only [Tm.settleCount] at this; omega
    · exact ih (fun l => by have := hv l; simp only [Tm.settleCount] at this; omega) hf
  | drec _ r ih =>
    intro x f hv hf
    exact ih (fun l => by have := hv l; simp only [Tm.settleCount] at this; exact this) hf
  | _ => intro x f _ hf; simp [Tm.field] at hf

theorem Tm.NoSettle.setField : ∀ {v w v' : Tm} {f}, v.NoSettle → w.NoSettle →
    v.setField f w = some v' → v'.NoSettle := by
  intro v
  induction v with
  | rcons l' e r _ ih =>
    intro w v' f hv hw hf
    simp only [Tm.setField] at hf
    split at hf
    · cases hf
      intro l; have hvl := hv l; simp only [Tm.settleCount] at hvl ⊢; have := hw l; omega
    · obtain ⟨r', hr', rfl⟩ := Option.map_eq_some_iff.mp hf
      have hr : r'.NoSettle :=
        ih (fun l => by have := hv l; simp only [Tm.settleCount] at this; omega) hw hr'
      intro l; have hvl := hv l; have hrl := hr l; simp only [Tm.settleCount] at hvl ⊢; omega
  | _ => intro _ _ _ _ _ hf; simp [Tm.setField] at hf

theorem Tm.NoSettle.of_matchP : ∀ (p : Pat) {v : Tm} {δ : List Tm}, v.NoSettle →
    matchP p v = some δ → ∀ w ∈ δ, w.NoSettle
  | .wild, _, _, _, h => by simp [matchP] at h; subst h; simp
  | .var, _, _, hv, h => by simp [matchP] at h; subst h; simpa
  | .lit _, v, _, _, h => by cases v <;> simp [matchP] at h; obtain ⟨_, rfl⟩ := h; simp
  | .ctor _, v, _, _, h => by cases v <;> simp [matchP] at h; obtain ⟨_, rfl⟩ := h; simp
  | .app p q, v, _, hv, h => by
    cases v with
    | app f e =>
      obtain ⟨δ₁, δ₂, h1, h2, rfl⟩ := matchP_app.mp h
      intro w hw
      rcases List.mem_append.mp hw with hw | hw
      · exact Tm.NoSettle.of_matchP p (fun l => by have := hv l; simp only [Tm.settleCount] at this; omega) h1 w hw
      · exact Tm.NoSettle.of_matchP q (fun l => by have := hv l; simp only [Tm.settleCount] at this; omega) h2 w hw
    | _ => simp [matchP] at h
  | .rany, v, _, _, h => by cases v <;> simp [matchP] at h <;> subst h <;> simp
  | .rfield f p r, v, _, hv, h => by
    obtain ⟨x, δ₁, δ₂, hx, h1, h2, rfl⟩ := matchP_rfield.mp h
    intro w hw
    rcases List.mem_append.mp hw with hw | hw
    · exact Tm.NoSettle.of_matchP p (hv.field hx) h1 w hw
    · exact Tm.NoSettle.of_matchP r hv h2 w hw
  | .or p q, v, _, hv, h => by
    rcases matchP_or.mp h with h | ⟨_, h⟩
    · exact Tm.NoSettle.of_matchP p hv h
    · exact Tm.NoSettle.of_matchP q hv h

theorem Tm.NoSettle.armOf {o : Name} {b : Tm} :
    ∀ {al : Tm}, al.NoSettle → al.armOf o = some b → b.NoSettle := by
  intro al
  induction al with
  | acons p b' r _ ih =>
    intro h ha
    simp only [Tm.armOf] at ha
    split at ha
    · cases ha; intro l; have := h l; simp only [Tm.settleCount] at this; omega
    · exact ih (fun l => by have := h l; simp only [Tm.settleCount] at this; omega) ha
  | _ => intro _ ha; simp [Tm.armOf] at ha

/-- `settle l` occurrences owned by a single frame (excluding its hole). -/
def Frame.settleCount (l : Nat) : Frame → Nat
  | .appL a | .appR a | .let_ a | .match_ a | .rconsL _ a | .rconsR _ a
  | .updL _ a | .updR a _ | .try_ a | .hresB _ _ a => a.settleCount l
  | .hwith _ _ _ al r => al.settleCount l + r.settleCount l
  | .hkeep _ _ _ al => al.settleCount l
  | .hkarmA _ _ _ al k => al.settleCount l + k.settleCount l
  | .hctlA _ _ _ _ _ al r k => al.settleCount l + r.settleCount l + k.settleCount l
  | .settle m => if m = l then 1 else 0
  | _ => 0

theorem Frame.settleCount_plug (l : Nat) (F : Frame) (e : Tm) :
    (F.plug e).settleCount l = F.settleCount l + e.settleCount l := by
  cases F <;> simp only [Frame.plug, Frame.settleCount, Tm.settleCount] <;> omega

def settleCountE (l : Nat) : List Frame → Nat
  | [] => 0
  | F :: E => F.settleCount l + settleCountE l E

theorem plug_settleCount (l : Nat) :
    ∀ (E : List Frame) (e : Tm), (plug E e).settleCount l = settleCountE l E + e.settleCount l
  | [], e => by simp [plug, settleCountE]
  | F :: E, e => by
    simp only [plug, settleCountE]
    rw [plug_settleCount l E (F.plug e), F.settleCount_plug l e]
    omega

/-- Linearity of `settle` frames in a single evaluation frame. -/
def Frame.SettleLin : Frame → Prop
  | .appL a | .let_ a | .match_ a | .rconsL _ a | .updL _ a | .try_ a => a.NoSettle
  | .appR v | .rconsR _ v | .updR v _ => v.SettleLin ∧ Value v
  | .hwith _ _ _ al r => al.NoSettle ∧ r.NoSettle
  | .hctlA _ _ _ _ _ al r k => al.NoSettle ∧ r.NoSettle ∧ k.SettleLin
  | .hkeep _ _ _ al => al.NoSettle
  | .hkarmA _ _ _ al k => al.NoSettle ∧ k.SettleLin
  | .hresB _ _ a => a.SettleLin
  | _ => True

theorem Frame.SettleLin.plug {F : Frame} {r : Tm} (hF : F.SettleLin) (hr : r.SettleLin) :
    (F.plug r).SettleLin := by
  cases F <;> simp only [Frame.SettleLin, Frame.plug, Tm.SettleLin] at hF hr ⊢
  all_goals first
    | exact hr
    | exact ⟨hr, Tm.SettleLin.of_noSettle hF, .inl hF⟩
    | exact ⟨hF.1, hr, .inr hF.2⟩
    | exact ⟨hr, hF⟩
    | exact ⟨hF.1, hF.2, hr⟩
    | exact ⟨hF.1, hF.2.1, hr, hF.2.2⟩
    | exact ⟨hF, hr⟩
    | exact ⟨hF.1, hr, hF.2⟩

theorem Frame.settleLin_of_plug {F : Frame} {r : Tm} (hok : F.ok) (hv : ¬ Value r)
    (h : (F.plug r).SettleLin) : F.SettleLin ∧ r.SettleLin := by
  cases F <;> simp only [Frame.SettleLin, Frame.plug, Frame.ok, Tm.SettleLin] at hok h ⊢ <;> grind

/-- A non-control frame with `SettleLin` holds no `settle` frames outside its hole. -/
theorem Frame.not_isCtl_settleCount {F : Frame} (hF : F.SettleLin) (hn : ¬ F.isCtl) (l : Nat) :
    F.settleCount l = 0 := by
  cases F <;> simp only [Frame.isCtl, Frame.SettleLin, Frame.settleCount] at hF hn ⊢
  all_goals first
    | rfl
    | exact (hn trivial).elim
    | exact hF l
    | exact (hF.2.noSettle hF.1) l
    | simp [hF.1 l, hF.2 l]

def SettleLinE : List Frame → Prop
  | [] => True
  | F :: E => SettleLinE E ∧ F.SettleLin

theorem SettleLinE.fill {r : Tm} :
    ∀ {E : List Frame}, SettleLinE E → r.SettleLin → (plug E r).SettleLin
  | [], _, hr => hr
  | _ :: _, ⟨hE, hF⟩, hr => SettleLinE.fill (r := Frame.plug _ r) hE (hF.plug hr)

theorem settleLin_of_plug {r : Tm} (hv : ¬ Value r) :
    ∀ {E : List Frame}, OkE E → (plug E r).SettleLin → SettleLinE E ∧ r.SettleLin
  | [], _, h => ⟨trivial, h⟩
  | F :: E, hE, h => by
    obtain ⟨hF, hE'⟩ := hE.cons
    obtain ⟨h₁, h₂⟩ := settleLin_of_plug (r := F.plug r) (not_value_plug hv F) hE' h
    obtain ⟨h₃, h₄⟩ := Frame.settleLin_of_plug hF hv h₂
    exact ⟨⟨h₁, h₃⟩, h₄⟩

/-- `withs` wraps only handler arm lists from `E`, which contain no `settle` frames. -/
theorem withs_noSettle {x : Tm} {p : Nat} :
    ∀ {E : List Frame}, SettleLinE E → x.NoSettle → (withs p E x).NoSettle
  | [], _, hx => hx
  | F :: E, ⟨hE, hF⟩, hx => by
    simp only [withs]
    refine withs_noSettle hE ?_
    cases F with
    | hwith q ℓ Ef al rf =>
      simp only [Frame.wrap]
      split
      · intro l; simp only [Tm.settleCount, hF.1 l, hx l]
      · exact hx
    | hkeep q ℓ Ef al =>
      simp only [Frame.wrap]
      split
      · intro l; simp only [Tm.settleCount, hF l, hx l]
      · exact hx
    | _ => exact hx

/-- Total `settle l` frames across a region's children and finalizers. -/
def Rgn.settleCount (l : Nat) (g : Rgn) : Nat :=
  (g.kids.map (Tm.settleCount l)).sum + (g.fins.map (Tm.settleCount l)).sum

/-- Total `settle l` frames across all regions. -/
def rgsSettleCount (l : Nat) (Λ : List Rgn) : Nat :=
  (Λ.map (Rgn.settleCount l)).sum

/-- Total `settle l` frames in a configuration (main thread + all regions). -/
def Cfg.settleCount (l : Nat) (c : Cfg) : Nat :=
  c.main.settleCount l + rgsSettleCount l c.rgs

theorem list_sum_set_nat {α : Type} (f : α → Nat) :
    ∀ (xs : List α) (i : Nat) (a b : α), xs[i]? = some a →
      ((xs.set i b).map f).sum + f a = (xs.map f).sum + f b
  | [], _, _, _, h => by simp at h
  | x :: xs, 0, a, b, h => by
    simp only [List.getElem?_cons_zero, Option.some.injEq] at h
    subst h
    simp only [List.set_cons_zero, List.map_cons, List.sum_cons]
    omega
  | x :: xs, i + 1, a, b, h => by
    simp only [List.getElem?_cons_succ] at h
    simp only [List.set_cons_succ, List.map_cons, List.sum_cons]
    have ih := list_sum_set_nat f xs i a b h
    omega

theorem list_le_sum_of_getElem? {α : Type} (f : α → Nat) :
    ∀ {xs : List α} {i : Nat} {a : α}, xs[i]? = some a → f a ≤ (xs.map f).sum
  | [], _, _, h => by simp at h
  | x :: xs, 0, a, h => by
    simp only [List.getElem?_cons_zero, Option.some.injEq] at h
    subst h
    simp only [List.map_cons, List.sum_cons]
    omega
  | x :: xs, i + 1, a, h => by
    simp only [List.getElem?_cons_succ] at h
    simp only [List.map_cons, List.sum_cons]
    have ih := list_le_sum_of_getElem? f h
    omega

theorem rgsSettleCount_setRgn (l : Nat) {Λ : List Rgn} {p : Nat} {g g' : Rgn}
    (hg : Λ[p]? = some g) :
    rgsSettleCount l (setRgn Λ p g') + g.settleCount l =
      rgsSettleCount l Λ + g'.settleCount l :=
  list_sum_set_nat (Rgn.settleCount l) Λ p g g' hg

theorem rgsSettleCount_setKid (l : Nat) {Λ : List Rgn} {p j : Nat} {g : Rgn} {k k' : Tm}
    (hg : Λ[p]? = some g) (hk : g.kids[j]? = some k) :
    rgsSettleCount l (setKid Λ p j k') + k.settleCount l =
      rgsSettleCount l Λ + k'.settleCount l := by
  unfold setKid
  rw [hg]; dsimp only
  have h1 := rgsSettleCount_setRgn l hg (g' := ⟨g.live, g.kids.set j k', g.fins⟩)
  have h2 := list_sum_set_nat (Tm.settleCount l) g.kids j k k' hk
  simp only [Rgn.settleCount] at h1
  omega

theorem kid_settleCount_le (l : Nat) {c : Cfg} {p j : Nat} {g : Rgn} {k : Tm}
    (hg : c.rgs[p]? = some g) (hk : g.kids[j]? = some k) :
    k.settleCount l ≤ c.settleCount l := by
  have h1 := list_le_sum_of_getElem? (Rgn.settleCount l) hg
  have h2 := list_le_sum_of_getElem? (Tm.settleCount l) hk
  simp only [Cfg.settleCount, rgsSettleCount, Rgn.settleCount] at h1 ⊢
  omega

/-- Root services and globals contain no `settle` frames. -/
def Roots.NoSettle (root : Roots) : Prop :=
  (∀ Ef w, root.svc Ef = some w → w.NoSettle) ∧ (∀ g v, root.glob g = some v → v.NoSettle)

/-- **Reachability invariant of `settle` frames**:
every thread/finalizer has `settle` frames only in active/parked positions, the store has none,
and every cell `l` has at most one `settle l` frame in the entire configuration, which implies
cell `l` is `Running` in the store. -/
structure CfgSettle (c : Cfg) : Prop where
  main : c.main.SettleLin
  rgs : ∀ (p : Nat) (g : Rgn), c.rgs[p]? = some g → ∀ k ∈ g.kids ++ g.fins, k.SettleLin
  mem : ∀ (i : Nat) (v : Tm), c.mem[i]? = some v → v.NoSettle
  count : ∀ l, c.settleCount l ≤ 1 ∧ (c.settleCount l = 1 → Running c.mem l)

theorem CfgSettle.init {e : Tm} (he : e.NoSettle) : CfgSettle ⟨[], [], e⟩ :=
  ⟨Tm.SettleLin.of_noSettle he, fun _ _ h => by simp at h, fun _ _ h => by simp at h,
    fun l => by simp [Cfg.settleCount, rgsSettleCount, he l]⟩

theorem CfgSettle.running_of_main {c : Cfg} (h : CfgSettle c) {l : Nat}
    (hpos : 0 < c.main.settleCount l) : Running c.mem l := by
  obtain ⟨hle, hrun⟩ := h.count l
  apply hrun
  simp only [Cfg.settleCount] at hle ⊢
  omega

theorem CfgSettle.running_of_kid {c : Cfg} (h : CfgSettle c) {p j l : Nat} {g : Rgn} {k : Tm}
    (hg : c.rgs[p]? = some g) (hk : g.kids[j]? = some k) (hpos : 0 < k.settleCount l) :
    Running c.mem l := by
  obtain ⟨hle, hrun⟩ := h.count l
  have hle' := kid_settleCount_le l hg hk
  exact hrun (by omega)

end Flick
