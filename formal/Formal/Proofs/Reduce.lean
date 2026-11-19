import Formal.Proofs.Fundamental

/-!
# Reduction of open terms

`Step`: one reduction anywhere in a term, under binders too: β, a split
of a pair, a case of an injection, `let`, and a global unfolded. In a
pure, total calculus each is an equality (`Step.sound`), the argument of β
need not be a value, and a step keeps a term typed (`Step.typed`).

`norm` reduces with a budget of steps, so `refl` compares normal forms.
-/

namespace Flick.PT

theorem Tm.inst_subst (b v : Tm) (γ : Nat → Tm) :
    (b.inst v).subst γ = b.subst (scons (v.subst γ) γ) := by
  unfold Tm.inst; rw [Tm.subst_subst]; apply Tm.subst_ext; intro n; cases n <;> rfl

theorem Tm.inst2_subst (c v w : Tm) (γ : Nat → Tm) :
    (c.inst2 v w).subst γ = c.subst (scons (w.subst γ) (scons (v.subst γ) γ)) := by
  unfold Tm.inst2; rw [Tm.subst_subst]; apply Tm.subst_ext; intro n
  match n with
  | 0 => rfl
  | 1 => rfl
  | _ + 2 => rfl

inductive Step (G : List Tm) : Tm → Tm → Prop where
  | beta {a : Ty} {b x : Tm} : Step G (.app (.lam a b) x) (b.inst x)
  | split {x y c : Tm} : Step G (.split (.pair x y) c) (c.inst2 x y)
  | caseL {x l r : Tm} {b : Ty} : Step G (.case (.inl x b) l r) (l.inst x)
  | caseR {x l r : Tm} {a : Ty} : Step G (.case (.inr a x) l r) (r.inst x)
  | let_ {e c : Tm} : Step G (.let_ e c) (c.inst e)
  | glob {g : Nat} {l : List Ty} {t : Tm} : G[g]? = some t → Step G (.glob g l) t
  | lam {a : Ty} {b b₁ : Tm} : Step G b b₁ → Step G (.lam a b) (.lam a b₁)
  | appL {f f₁ x : Tm} : Step G f f₁ → Step G (.app f x) (.app f₁ x)
  | appR {f x x₁ : Tm} : Step G x x₁ → Step G (.app f x) (.app f x₁)
  | pairL {x x₁ y : Tm} : Step G x x₁ → Step G (.pair x y) (.pair x₁ y)
  | pairR {x y y₁ : Tm} : Step G y y₁ → Step G (.pair x y) (.pair x y₁)
  | splitE {e e₁ c : Tm} : Step G e e₁ → Step G (.split e c) (.split e₁ c)
  | splitC {e c c₁ : Tm} : Step G c c₁ → Step G (.split e c) (.split e c₁)
  | inl {x x₁ : Tm} {b : Ty} : Step G x x₁ → Step G (.inl x b) (.inl x₁ b)
  | inr {x x₁ : Tm} {a : Ty} : Step G x x₁ → Step G (.inr a x) (.inr a x₁)
  | caseE {e e₁ l r : Tm} : Step G e e₁ → Step G (.case e l r) (.case e₁ l r)
  | caseL' {e l l₁ r : Tm} : Step G l l₁ → Step G (.case e l r) (.case e l₁ r)
  | caseR' {e l r r₁ : Tm} : Step G r r₁ → Step G (.case e l r) (.case e l r₁)
  | letE {e e₁ c : Tm} : Step G e e₁ → Step G (.let_ e c) (.let_ e₁ c)
  | letC {e c c₁ : Tm} : Step G c c₁ → Step G (.let_ e c) (.let_ e c₁)

theorem Prog.body_of {P : Prog} {g : Nat} {t : Tm} (h : P.bodies[g]? = some t) :
    ∃ hg : g < P.length, P[g].1 = t := by
  simp only [Prog.bodies, List.getElem?_map, Option.map_eq_some_iff] at h
  obtain ⟨p, hp, rfl⟩ := h
  obtain ⟨hg, hp⟩ := List.getElem?_eq_some_iff.mp hp
  exact ⟨hg, by rw [hp]⟩

theorem Prog.scheme_of {P : Prog} {g : Nat} {σ : Ty} (h : P.schemes[g]? = some σ) :
    ∃ hg : g < P.length, P[g].2 = σ := by
  simp only [Prog.schemes, List.getElem?_map, Option.map_eq_some_iff] at h
  obtain ⟨p, hp, rfl⟩ := h
  obtain ⟨hg, hp⟩ := List.getElem?_eq_some_iff.mp hp
  exact ⟨hg, by rw [hp]⟩

/-- The body of a global, typed at its scheme with all the globals. -/
theorem Prog.WF.body {P : Prog} (hP : P.WF) {g : Nat} (hg : g < P.length) :
    Typed P.schemes [] P[g].1 P[g].2 :=
  (hP g hg).weaken_globals fun g' σ h => by
    have hlt : g' < g := by
      have := (List.getElem?_eq_some_iff.mp h).1
      simp at this; omega
    rwa [List.getElem?_take_of_lt hlt] at h

theorem Step.typed {P : Prog} (hP : P.WF) {e e' : Tm} (h : Step P.bodies e e') :
    ∀ {Γ : List Ty} {t : Ty}, Typed P.schemes Γ e t → Typed P.schemes Γ e' t := by
  induction h with
  | beta => intro Γ t ht; cases ht with | app hf hx => cases hf with | lam hb => exact hb.inst hx
  | split =>
    intro Γ t ht; cases ht with | split he hc => cases he with | pair hx hy => exact hc.inst2 hx hy
  | caseL => intro Γ t ht; cases ht with | case he hl _ => cases he with | inl hx => exact hl.inst hx
  | caseR => intro Γ t ht; cases ht with | case he _ hr => cases he with | inr hx => exact hr.inst hx
  | let_ => intro Γ t ht; cases ht with | let_ he hc => exact hc.inst he
  | glob hg =>
    intro Γ t ht
    cases ht with
    | glob hσ =>
      rename_i σ θ
      obtain ⟨hlen, rfl⟩ := Prog.body_of hg
      obtain ⟨_, hσ'⟩ := Prog.scheme_of hσ
      have hb := (hP.body hlen).tsubst θ
      rw [hσ'] at hb
      exact hb.weaken_closed Γ
  | lam _ ih => intro Γ t ht; cases ht with | lam hb => exact .lam (ih hb)
  | appL _ ih => intro Γ t ht; cases ht with | app hf hx => exact .app (ih hf) hx
  | appR _ ih => intro Γ t ht; cases ht with | app hf hx => exact .app hf (ih hx)
  | pairL _ ih => intro Γ t ht; cases ht with | pair hx hy => exact .pair (ih hx) hy
  | pairR _ ih => intro Γ t ht; cases ht with | pair hx hy => exact .pair hx (ih hy)
  | splitE _ ih => intro Γ t ht; cases ht with | split he hc => exact .split (ih he) hc
  | splitC _ ih => intro Γ t ht; cases ht with | split he hc => exact .split he (ih hc)
  | inl _ ih => intro Γ t ht; cases ht with | inl hx => exact .inl (ih hx)
  | inr _ ih => intro Γ t ht; cases ht with | inr hx => exact .inr (ih hx)
  | caseE _ ih => intro Γ t ht; cases ht with | case he hl hr => exact .case (ih he) hl hr
  | caseL' _ ih => intro Γ t ht; cases ht with | case he hl hr => exact .case he (ih hl) hr
  | caseR' _ ih => intro Γ t ht; cases ht with | case he hl hr => exact .case he hl (ih hr)
  | letE _ ih => intro Γ t ht; cases ht with | let_ he hc => exact .let_ (ih he) hc
  | letC _ ih => intro Γ t ht; cases ht with | let_ he hc => exact .let_ he (ih hc)

/-- A related term at the head of a substitution: a value is related to a
term that evaluates to a related value. -/
theorem E.of_value {G : List Tm} {t : Ty} {v x w : Tm} (ev : Eval G x w) (r : R G t v w) :
    E G t v x :=
  ⟨v, w, r.value.1.eval, ev, r⟩

/-- **Each reduction step is an equality.** -/
theorem Step.sound {P : Prog} (hP : P.WF) {e e' : Tm} (h : Step P.bodies e e') :
    ∀ {Γ : List Ty} {t : Ty}, Typed P.schemes Γ e t → Equiv P.bodies Γ e e' t := by
  have hG := hP.globOK
  induction h with
  | beta =>
    intro Γ t ht
    cases ht with
    | app hf hx =>
      cases hf with
      | lam hb =>
        intro θ γ₁ γ₂ hγ
        obtain ⟨v₁, v₂, ev₁, ev₂, rv⟩ := fundamental hG hx θ γ₁ γ₂ hγ
        obtain ⟨u₁, u₂, eu₁, eu₂, ru⟩ := fundamental hG hb θ _ _ (hγ.cons (E.of_value ev₂ rv))
        refine ⟨u₁, u₂, .app .lam ev₁ ?_, ?_, ru⟩
        · rw [Tm.subst_up_inst]; exact eu₁
        · rw [Tm.inst_subst]; exact eu₂
  | split =>
    intro Γ t ht
    cases ht with
    | split he hc =>
      cases he with
      | pair hx hy =>
        intro θ γ₁ γ₂ hγ
        obtain ⟨v₁, v₂, ev₁, ev₂, rv⟩ := fundamental hG hx θ γ₁ γ₂ hγ
        obtain ⟨w₁, w₂, ew₁, ew₂, rw⟩ := fundamental hG hy θ γ₁ γ₂ hγ
        obtain ⟨u₁, u₂, eu₁, eu₂, ru⟩ :=
          fundamental hG hc θ _ _ ((hγ.cons (E.of_value ev₂ rv)).cons (E.of_value ew₂ rw))
        refine ⟨u₁, u₂, .split (.pair ev₁ ew₁) ?_, ?_, ru⟩
        · rw [Tm.subst_up2_inst2]; exact eu₁
        · rw [Tm.inst2_subst]; exact eu₂
  | caseL =>
    intro Γ t ht
    cases ht with
    | case he hl _ =>
      cases he with
      | inl hx =>
        intro θ γ₁ γ₂ hγ
        obtain ⟨v₁, v₂, ev₁, ev₂, rv⟩ := fundamental hG hx θ γ₁ γ₂ hγ
        obtain ⟨u₁, u₂, eu₁, eu₂, ru⟩ := fundamental hG hl θ _ _ (hγ.cons (E.of_value ev₂ rv))
        refine ⟨u₁, u₂, .caseL (.inl ev₁) ?_, ?_, ru⟩
        · rw [Tm.subst_up_inst]; exact eu₁
        · rw [Tm.inst_subst]; exact eu₂
  | caseR =>
    intro Γ t ht
    cases ht with
    | case he _ hr =>
      cases he with
      | inr hx =>
        intro θ γ₁ γ₂ hγ
        obtain ⟨v₁, v₂, ev₁, ev₂, rv⟩ := fundamental hG hx θ γ₁ γ₂ hγ
        obtain ⟨u₁, u₂, eu₁, eu₂, ru⟩ := fundamental hG hr θ _ _ (hγ.cons (E.of_value ev₂ rv))
        refine ⟨u₁, u₂, .caseR (.inr ev₁) ?_, ?_, ru⟩
        · rw [Tm.subst_up_inst]; exact eu₁
        · rw [Tm.inst_subst]; exact eu₂
  | let_ =>
    intro Γ t ht
    cases ht with
    | let_ he hc =>
      intro θ γ₁ γ₂ hγ
      obtain ⟨v₁, v₂, ev₁, ev₂, rv⟩ := fundamental hG he θ γ₁ γ₂ hγ
      obtain ⟨u₁, u₂, eu₁, eu₂, ru⟩ := fundamental hG hc θ _ _ (hγ.cons (E.of_value ev₂ rv))
      refine ⟨u₁, u₂, .let_ ev₁ ?_, ?_, ru⟩
      · rw [Tm.subst_up_inst]; exact eu₁
      · rw [Tm.inst_subst]; exact eu₂
  | glob hg =>
    intro Γ t ht
    cases ht with
    | glob hσ =>
      rename_i σ θ'
      obtain ⟨hlen, rfl⟩ := Prog.body_of hg
      intro θ γ₁ γ₂ _
      obtain ⟨v₁, v₂, e₁, e₂, r⟩ := hG _ _ σ (fun n => (θ' n).subst θ) hσ
      rw [Ty.subst_subst]
      cases e₂ with
      | glob hg' eb =>
        rw [hg] at hg'; cases hg'
        exact ⟨v₁, v₂, e₁, by rw [(hP.body hlen).closed]; exact eb, r⟩
  | lam _ ih => intro Γ t ht; cases ht with | lam hb => exact compat_lam (ih hb)
  | appL _ ih =>
    intro Γ t ht; cases ht with | app hf hx => exact compat_app (ih hf) (fundamental hG hx)
  | appR _ ih =>
    intro Γ t ht; cases ht with | app hf hx => exact compat_app (fundamental hG hf) (ih hx)
  | pairL _ ih =>
    intro Γ t ht; cases ht with | pair hx hy => exact compat_pair (ih hx) (fundamental hG hy)
  | pairR _ ih =>
    intro Γ t ht; cases ht with | pair hx hy => exact compat_pair (fundamental hG hx) (ih hy)
  | splitE _ ih =>
    intro Γ t ht; cases ht with | split he hc => exact compat_split (ih he) (fundamental hG hc)
  | splitC _ ih =>
    intro Γ t ht; cases ht with | split he hc => exact compat_split (fundamental hG he) (ih hc)
  | inl _ ih => intro Γ t ht; cases ht with | inl hx => exact compat_inl (ih hx)
  | inr _ ih => intro Γ t ht; cases ht with | inr hx => exact compat_inr (ih hx)
  | caseE _ ih =>
    intro Γ t ht
    cases ht with
    | case he hl hr => exact compat_case (ih he) (fundamental hG hl) (fundamental hG hr)
  | caseL' _ ih =>
    intro Γ t ht
    cases ht with
    | case he hl hr => exact compat_case (fundamental hG he) (ih hl) (fundamental hG hr)
  | caseR' _ ih =>
    intro Γ t ht
    cases ht with
    | case he hl hr => exact compat_case (fundamental hG he) (fundamental hG hl) (ih hr)
  | letE _ ih =>
    intro Γ t ht; cases ht with | let_ he hc => exact compat_let (ih he) (fundamental hG hc)
  | letC _ ih =>
    intro Γ t ht; cases ht with | let_ he hc => exact compat_let (fundamental hG he) (ih hc)

/-! ## Annotations -/

/-- The term without its annotations: evaluation and typing read none. -/
def Tm.erase : Tm → Tm
  | .var n => .var n
  | .glob g _ => .glob g []
  | .lam _ b => .lam .unit b.erase
  | .app f x => .app f.erase x.erase
  | .unit => .unit
  | .pair x y => .pair x.erase y.erase
  | .split e c => .split e.erase c.erase
  | .inl x _ => .inl x.erase .unit
  | .inr _ x => .inr .unit x.erase
  | .case e l r => .case e.erase l.erase r.erase
  | .let_ e c => .let_ e.erase c.erase

/-- A typed term is equal to its erasure. -/
theorem Typed.erase {P : Prog} (hP : P.WF) {Γ : List Ty} {e : Tm} {t : Ty}
    (h : Typed P.schemes Γ e t) : Equiv P.bodies Γ e e.erase t := by
  have hG := hP.globOK
  induction h with
  | var hn => exact compat_var hn
  | glob hg => exact compat_glob₂ _ _ _ hG hg
  | lam _ ih => exact compat_lam ih
  | app _ _ ihf ihx => exact compat_app ihf ihx
  | unit => exact compat_unit
  | pair _ _ ihx ihy => exact compat_pair ihx ihy
  | split _ _ ihe ihc => exact compat_split ihe ihc
  | inl _ ih => exact compat_inl ih
  | inr _ ih => exact compat_inr ih
  | case _ _ _ ihe ihl ihr => exact compat_case ihe ihl ihr
  | let_ _ _ ihe ihc => exact compat_let ihe ihc

/-! ## Many steps, and the normalizer -/

inductive Steps (G : List Tm) : Tm → Tm → Prop where
  | refl {e : Tm} : Steps G e e
  | step {e e₁ e₂ : Tm} : Step G e e₁ → Steps G e₁ e₂ → Steps G e e₂

theorem Steps.sound {P : Prog} (hP : P.WF) {e e' : Tm} (h : Steps P.bodies e e') :
    ∀ {Γ : List Ty} {t : Ty}, Typed P.schemes Γ e t →
      Typed P.schemes Γ e' t ∧ Equiv P.bodies Γ e e' t := by
  induction h with
  | refl => intro Γ t ht; exact ⟨ht, fundamental hP.globOK ht⟩
  | step s _ ih =>
    intro Γ t ht
    obtain ⟨ht', hq⟩ := ih (s.typed hP ht)
    exact ⟨ht', (s.sound hP ht).trans hq⟩

/-- A step of an application, given the steps of its parts: β first. -/
def appStep (sf sx : Option Tm) (f x : Tm) : Option Tm :=
  match f with
  | .lam _ b => some (b.inst x)
  | _ =>
    match sf with
    | some f' => some (.app f' x)
    | none => sx.map (.app f)

def pairStep (sx sy : Option Tm) (x y : Tm) : Option Tm :=
  match sx with
  | some x' => some (.pair x' y)
  | none => sy.map (.pair x)

def splitStep (se sc : Option Tm) (e c : Tm) : Option Tm :=
  match e with
  | .pair x y => some (c.inst2 x y)
  | _ =>
    match se with
    | some e' => some (.split e' c)
    | none => sc.map (.split e)

def caseStep (se sl sr : Option Tm) (e l r : Tm) : Option Tm :=
  match e with
  | .inl x _ => some (l.inst x)
  | .inr _ x => some (r.inst x)
  | _ =>
    match se with
    | some e' => some (.case e' l r)
    | none =>
      match sl with
      | some l' => some (.case e l' r)
      | none => sr.map (.case e l)

/-- A step of the leftmost outermost redex, where there is one. -/
def step? (G : List Tm) : Tm → Option Tm
  | .var _ | .unit => none
  | .glob g _ => G[g]?
  | .let_ e c => some (c.inst e)
  | .lam a b => (step? G b).map (.lam a)
  | .app f x => appStep (step? G f) (step? G x) f x
  | .pair x y => pairStep (step? G x) (step? G y) x y
  | .split e c => splitStep (step? G e) (step? G c) e c
  | .inl x b => (step? G x).map (.inl · b)
  | .inr a x => (step? G x).map (.inr a)
  | .case e l r => caseStep (step? G e) (step? G l) (step? G r) e l r

theorem step?_sound {G : List Tm} : ∀ {e e' : Tm}, step? G e = some e' → Step G e e' := by
  intro e
  induction e with
  | var | unit => intro e' h; simp [step?] at h
  | glob g l => intro e' h; exact .glob h
  | let_ e c _ _ => intro e' h; simp only [step?] at h; cases h; exact .let_
  | lam a b ih =>
    intro e' h; simp only [step?, Option.map_eq_some_iff] at h
    obtain ⟨b', hb, rfl⟩ := h; exact .lam (ih hb)
  | app f x ihf ihx =>
    intro e' h; simp only [step?, appStep] at h
    split at h
    · cases h; exact .beta
    · split at h
      · rename_i f' hf; cases h; exact .appL (ihf hf)
      · simp only [Option.map_eq_some_iff] at h
        obtain ⟨x', hx, rfl⟩ := h; exact .appR (ihx hx)
  | pair x y ihx ihy =>
    intro e' h; simp only [step?, pairStep] at h
    split at h
    · rename_i x' hx; cases h; exact .pairL (ihx hx)
    · simp only [Option.map_eq_some_iff] at h
      obtain ⟨y', hy, rfl⟩ := h; exact .pairR (ihy hy)
  | split e c ihe ihc =>
    intro e' h; simp only [step?, splitStep] at h
    split at h
    · cases h; exact .split
    · split at h
      · rename_i e₁ he; cases h; exact .splitE (ihe he)
      · simp only [Option.map_eq_some_iff] at h
        obtain ⟨c', hc, rfl⟩ := h; exact .splitC (ihc hc)
  | inl x b ih =>
    intro e' h; simp only [step?, Option.map_eq_some_iff] at h
    obtain ⟨x', hx, rfl⟩ := h; exact .inl (ih hx)
  | inr a x ih =>
    intro e' h; simp only [step?, Option.map_eq_some_iff] at h
    obtain ⟨x', hx, rfl⟩ := h; exact .inr (ih hx)
  | case e l r ihe ihl ihr =>
    intro e' h; simp only [step?, caseStep] at h
    split at h
    · cases h; exact .caseL
    · cases h; exact .caseR
    · split at h
      · rename_i e₁ he; cases h; exact .caseE (ihe he)
      · split at h
        · rename_i l' hl; cases h; exact .caseL' (ihl hl)
        · simp only [Option.map_eq_some_iff] at h
          obtain ⟨r', hr, rfl⟩ := h; exact .caseR' (ihr hr)

/-- At most `k` steps of the leftmost outermost redex. -/
def norm (G : List Tm) : Nat → Tm → Tm
  | 0, e => e
  | k + 1, e =>
    match step? G e with
    | some e' => norm G k e'
    | none => e

theorem norm_steps {G : List Tm} : ∀ (k : Nat) (e : Tm), Steps G e (norm G k e)
  | 0, _ => .refl
  | k + 1, e => by
    simp only [norm]
    split
    · rename_i e' h; exact .step (step?_sound h) (norm_steps k e')
    · exact .refl

/-- What `norm` gives is equal to the term, and typed as it is. -/
theorem norm_sound {P : Prog} (hP : P.WF) {Γ : List Ty} {e : Tm} {t : Ty}
    (ht : Typed P.schemes Γ e t) (k : Nat) :
    Typed P.schemes Γ (norm P.bodies k e) t ∧ Equiv P.bodies Γ e (norm P.bodies k e) t :=
  (norm_steps k e).sound hP ht

/-- Two typed terms with one erased normal form are equal. -/
theorem same_normal_form {P : Prog} (hP : P.WF) {Γ : List Ty} {e₁ e₂ : Tm} {t : Ty}
    (h₁ : Typed P.schemes Γ e₁ t) (h₂ : Typed P.schemes Γ e₂ t) (k : Nat)
    (h : (norm P.bodies k e₁).erase = (norm P.bodies k e₂).erase) : Equiv P.bodies Γ e₁ e₂ t := by
  obtain ⟨t₁, q₁⟩ := norm_sound hP h₁ k
  obtain ⟨t₂, q₂⟩ := norm_sound hP h₂ k
  have r₁ := q₁.trans (t₁.erase hP)
  have r₂ := q₂.trans (t₂.erase hP)
  rw [h] at r₁
  exact r₁.trans r₂.symm

end Flick.PT
