import Formal.Equiv.Ops

/-!
# Contextual equivalence: inversion at `top`

At `top` no special pair stands at the top, so a term is related only by the congruence of its
former: one lemma per former.
-/

namespace Flick.Equiv

variable {W : World}

theorem Sim.inv_lam {b : Tm} {r₂ : Tm} (h : Sim W .top (.lam b) r₂) :
    ∃ b', r₂ = (.lam b') ∧ Sim W .any b b' ∧ b'.NoIns := by
  generalize hx : Pos.top = x at h
  cases h with
  | lam =>
    subst hx; exact ⟨_, rfl, ‹_›, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_app {f : Tm} {a : Tm} {r₂ : Tm} (h : Sim W .top (.app f a) r₂) :
    ∃ f' a', r₂ = (.app f' a') ∧ Sim W .top f f' ∧ Sim W .top a a' := by
  generalize hx : Pos.top = x at h
  cases h with
  | app =>
    subst hx; exact ⟨_, _, rfl, ‹_›, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_let {e : Tm} {b : Tm} {r₂ : Tm} (h : Sim W .top (.let_ e b) r₂) :
    ∃ e' b', r₂ = (.let_ e' b') ∧ Sim W .any e e' ∧ Sim W .any b b' := by
  generalize hx : Pos.top = x at h
  cases h with
  | let_ =>
    subst hx; exact ⟨_, _, rfl, ‹_›, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_letrec {f : Tm} {b : Tm} {r₂ : Tm} (h : Sim W .top (.letrec f b) r₂) :
    ∃ f' b', r₂ = (.letrec f' b') ∧ Sim W .any f f' ∧ f'.NoIns ∧ Sim W .any b b' := by
  generalize hx : Pos.top = x at h
  cases h with
  | letrec =>
    subst hx; exact ⟨_, _, rfl, ‹_›, ‹_›, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_fix {f : Tm} {r₂ : Tm} (h : Sim W .top (.fix f) r₂) :
    ∃ f', r₂ = (.fix f') ∧ Sim W .any f f' ∧ f'.NoIns := by
  generalize hx : Pos.top = x at h
  cases h with
  | fix =>
    subst hx; exact ⟨_, rfl, ‹_›, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_rcons {l : Label} {e : Tm} {r : Tm} {r₂ : Tm} (h : Sim W .top (.rcons l e r) r₂) :
    ∃ e' r', r₂ = (.rcons l e' r') ∧ Sim W .top e e' ∧ Sim W .top r r' := by
  generalize hx : Pos.top = x at h
  cases h with
  | rcons =>
    subst hx; exact ⟨_, _, rfl, ‹_›, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_proj {e : Tm} {l : Label} {r₂ : Tm} (h : Sim W .top (.proj e l) r₂) :
    ∃ e', r₂ = (.proj e' l) ∧ Sim W .any e e' := by
  generalize hx : Pos.top = x at h
  cases h with
  | proj =>
    subst hx; exact ⟨_, rfl, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_update {e : Tm} {l : Label} {a : Tm} {r₂ : Tm} (h : Sim W .top (.update e l a) r₂) :
    ∃ e' a', r₂ = (.update e' l a') ∧ Sim W .any e e' ∧ Sim W .any a a' := by
  generalize hx : Pos.top = x at h
  cases h with
  | update =>
    subst hx; exact ⟨_, _, rfl, ‹_›, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_match {e : Tm} {al : Tm} {r₂ : Tm} (h : Sim W .top (.match_ e al) r₂) :
    ∃ e' al', r₂ = (.match_ e' al') ∧ Sim W .any e e' ∧ Sim W .top al al' := by
  generalize hx : Pos.top = x at h
  cases h with
  | match_ =>
    subst hx; exact ⟨_, _, rfl, ‹_›, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_acons {p : Pat} {b : Tm} {r : Tm} {r₂ : Tm} (h : Sim W .top (.acons p b r) r₂) :
    ∃ b' r', r₂ = (.acons p b' r') ∧ ArmR W b b' ∧ Sim W .top r r' := by
  generalize hx : Pos.top = x at h
  cases h with
  | acons =>
    subst hx; exact ⟨_, _, rfl, .inl ‹_›, ‹_›⟩
  | aconsD hd hn =>
    subst hx; exact ⟨_, _, rfl, .inr ⟨hd, hn⟩, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_aguard {p : Pat} {g : Tm} {b : Tm} {r : Tm} {r₂ : Tm} (h : Sim W .top (.aguard p g b r) r₂) :
    ∃ g' b' r', r₂ = (.aguard p g' b' r') ∧ Sim W .any g g' ∧ Sim W .any b b' ∧ Sim W .top r r' := by
  generalize hx : Pos.top = x at h
  cases h with
  | aguard =>
    subst hx; exact ⟨_, _, _, rfl, ‹_›, ‹_›, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_task {e : Tm} {r₂ : Tm} (h : Sim W .top (.task e) r₂) :
    ∃ e', r₂ = (.task e') ∧ Sim W .any e e' ∧ e'.NoIns := by
  generalize hx : Pos.top = x at h
  cases h with
  | task =>
    subst hx; exact ⟨_, rfl, ‹_›, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_await {e : Tm} {r₂ : Tm} (h : Sim W .top (.await e) r₂) :
    ∃ e', r₂ = (.await e') ∧ Sim W .any e e' := by
  generalize hx : Pos.top = x at h
  cases h with
  | await =>
    subst hx; exact ⟨_, rfl, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_lazy {e : Tm} {r₂ : Tm} (h : Sim W .top (.lazy e) r₂) :
    ∃ e', r₂ = (.lazy e') ∧ Sim W .any e e' := by
  generalize hx : Pos.top = x at h
  cases h with
  | lazy =>
    subst hx; exact ⟨_, rfl, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_force {e : Tm} {r₂ : Tm} (h : Sim W .top (.force e) r₂) :
    ∃ e', r₂ = (.force e') ∧ Sim W .any e e' := by
  generalize hx : Pos.top = x at h
  cases h with
  | force =>
    subst hx; exact ⟨_, rfl, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_raise {e : Tm} {r₂ : Tm} (h : Sim W .top (.raise e) r₂) :
    ∃ e', r₂ = (.raise e') ∧ Sim W .any e e' := by
  generalize hx : Pos.top = x at h
  cases h with
  | raise =>
    subst hx; exact ⟨_, rfl, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_try {e : Tm} {al : Tm} {r₂ : Tm} (h : Sim W .top (.try_ e al) r₂) :
    ∃ e' al', r₂ = (.try_ e' al') ∧ Sim W .any e e' ∧ Sim W .top al al' := by
  generalize hx : Pos.top = x at h
  cases h with
  | try_ =>
    subst hx; exact ⟨_, _, rfl, ‹_›, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_handle {v : Tm} {al : Tm} {r₂ : Tm} (h : Sim W .top (.handle v al) r₂) :
    ∃ v' al', r₂ = (.handle v' al') ∧ Sim W .top v v' ∧ Sim W .top al al' := by
  generalize hx : Pos.top = x at h
  cases h with
  | handle =>
    subst hx; exact ⟨_, _, rfl, ‹_›, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_scope {e : Tm} {r₂ : Tm} (h : Sim W .top (.scope e) r₂) :
    ∃ e', r₂ = (.scope e') ∧ Sim W .any e e' := by
  generalize hx : Pos.top = x at h
  cases h with
  | scope =>
    subst hx; exact ⟨_, rfl, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_inside {p : Nat} {e : Tm} {r₂ : Tm} (h : Sim W .top (.inside p e) r₂) :
    ∃ e', r₂ = (.inside p e') ∧ Sim W .any e e' := by
  generalize hx : Pos.top = x at h
  cases h with
  | inside =>
    subst hx; exact ⟨_, rfl, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_drec {c : Nat} {r : Tm} {r₂ : Tm} (h : Sim W .top (.drec c r) r₂) :
    ∃ r', r₂ = (.drec c r') ∧ Sim W .top r r' ∧ r'.NoIns := by
  generalize hx : Pos.top = x at h
  cases h with
  | drec =>
    subst hx; exact ⟨_, rfl, ‹_›, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_hdl {E : Name} {al : Tm} {r : Tm} {e : Tm} {r₂ : Tm} (h : Sim W .top (.hdl E al r e) r₂) :
    ∃ al' r' e', r₂ = (.hdl E al' r' e') ∧ Sim W .top al al' ∧ Sim W .any r r' ∧ Sim W .any e e' := by
  generalize hx : Pos.top = x at h
  cases h with
  | hdl =>
    subst hx; exact ⟨_, _, _, rfl, ‹_›, ‹_›, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_hwith {p : Nat} {ℓ : Nat} {E : Name} {al : Tm} {r : Tm} {e : Tm} {r₂ : Tm} (h : Sim W .top (.hwith p ℓ E al r e) r₂) :
    ∃ ℓ' k al' r' e', r₂ = (.hwith p ℓ' E al' r' e') ∧ W.L ℓ ℓ' k ∧ Sim W .top al al' ∧ Sim W .any r r' ∧ Sim W .any e e' := by
  generalize hx : Pos.top = x at h
  cases h with
  | hwith =>
    subst hx; exact ⟨_, _, _, _, _, rfl, ‹_›, ‹_›, ‹_›, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_hkeep {q : Nat} {ℓ : Nat} {E : Name} {al : Tm} {e : Tm} {r₂ : Tm} (h : Sim W .top (.hkeep q ℓ E al e) r₂) :
    ∃ ℓ' k al' e', r₂ = (.hkeep q ℓ' E al' e') ∧ W.L ℓ ℓ' k ∧ Sim W .top al al' ∧ Sim W .any e e' := by
  generalize hx : Pos.top = x at h
  cases h with
  | hkeep =>
    subst hx; exact ⟨_, _, _, _, rfl, ‹_›, ‹_›, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_hkarm {q : Nat} {ℓ : Nat} {E : Name} {al : Tm} {a : Tm} {c : Tm} {r₂ : Tm} (h : Sim W .top (.hkarm q ℓ E al a c) r₂) :
    ∃ ℓ' k al' a' c', r₂ = (.hkarm q ℓ' E al' a' c') ∧ W.L ℓ ℓ' k ∧ Sim W .top al al' ∧ Sim W .any a a' ∧ Sim W .park c c' := by
  generalize hx : Pos.top = x at h
  cases h with
  | hkarm =>
    subst hx; exact ⟨_, _, _, _, _, rfl, ‹_›, ‹_›, ‹_›, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_hctl {p : Nat} {ℓ : Nat} {κ : Nat} {tl : Bool} {E : Name} {al : Tm} {r : Tm} {a : Tm} {c : Tm} {r₂ : Tm} (h : Sim W .top (.hctl p ℓ κ tl E al r a c) r₂) :
    ∃ ℓ' k κ' al' r' a' c', r₂ = (.hctl p ℓ' κ' tl E al' r' a' c') ∧ W.L ℓ ℓ' k ∧ KapR W tl κ κ' ∧ Sim W .top al al' ∧ Sim W .any r r' ∧ Sim W .any a a' ∧ Sim W .park c c' := by
  generalize hx : Pos.top = x at h
  cases h with
  | hctl =>
    subst hx; exact ⟨_, _, _, _, _, _, _, rfl, ‹_›, ‹_›, ‹_›, ‹_›, ‹_›, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_hres {p : Nat} {ℓ : Nat} {a : Tm} {b : Tm} {r₂ : Tm} (h : Sim W .top (.hres p ℓ a b) r₂) :
    ∃ ℓ' k a' b', r₂ = (.hres p ℓ' a' b') ∧ W.L ℓ ℓ' k ∧ Sim W .park a a' ∧ Sim W .any b b' := by
  generalize hx : Pos.top = x at h
  cases h with
  | hres =>
    subst hx; exact ⟨_, _, _, _, rfl, ‹_›, ‹_›, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_hdone {p : Nat} {a : Tm} {r₂ : Tm} (h : Sim W .top (.hdone p a) r₂) :
    ∃ a', r₂ = (.hdone p a') ∧ Sim W .any a a' := by
  generalize hx : Pos.top = x at h
  cases h with
  | hdone =>
    subst hx; exact ⟨_, rfl, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_unw {p : Nat} {ℓ : Nat} {a : Tm} {r₂ : Tm} (h : Sim W .top (.unw p ℓ a) r₂) :
    ∃ ℓ' k a', r₂ = (.unw p ℓ' a') ∧ W.L ℓ ℓ' k ∧ Sim W .any a a' := by
  generalize hx : Pos.top = x at h
  cases h with
  | unw =>
    subst hx; exact ⟨_, _, _, rfl, ‹_›, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_kont {p : Nat} {ℓ : Nat} {r₂ : Tm} (h : Sim W .top (.kont p ℓ) r₂) :
    ∃ ℓ' k, r₂ = (.kont p ℓ') ∧ W.L ℓ ℓ' k := by
  generalize hx : Pos.top = x at h
  cases h with
  | kont =>
    subst hx; exact ⟨_, _, rfl, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_loc {l : Nat} {r₂ : Tm} (h : Sim W .top (.loc l) r₂) :
    ∃ l' k, r₂ = (.loc l') ∧ W.L l l' k := by
  generalize hx : Pos.top = x at h
  cases h with
  | loc =>
    subst hx; exact ⟨_, _, rfl, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_settle {ℓ : Nat} {e : Tm} {r₂ : Tm} (h : Sim W .top (.settle ℓ e) r₂) :
    ∃ ℓ' e', r₂ = (.settle ℓ' e') ∧ W.L ℓ ℓ' .sym ∧ Sim W .any e e' := by
  generalize hx : Pos.top = x at h
  cases h with
  | settle =>
    subst hx; exact ⟨_, _, rfl, ‹_›, ‹_›⟩
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_lit {l : Lit} {r₂ : Tm} (h : Sim W .top (.lit l) r₂) : r₂ = (.lit l) := by
  generalize hx : Pos.top = x at h
  cases h with
  | lit => rfl
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_const {c : Const} {r₂ : Tm} (h : Sim W .top (.const c) r₂) : r₂ = (.const c) := by
  generalize hx : Pos.top = x at h
  cases h with
  | const => rfl
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_ctor {K : Name} {r₂ : Tm} (h : Sim W .top (.ctor K) r₂) : r₂ = (.ctor K) := by
  generalize hx : Pos.top = x at h
  cases h with
  | ctor => rfl
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_cap {p : Nat} {r₂ : Tm} (h : Sim W .top (.cap p) r₂) : r₂ = (.cap p) := by
  generalize hx : Pos.top = x at h
  cases h with
  | cap => rfl
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_own {p : Nat} {r₂ : Tm} (h : Sim W .top (.own p) r₂) : r₂ = (.own p) := by
  generalize hx : Pos.top = x at h
  cases h with
  | own => rfl
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_glob {g : Nat} {r₂ : Tm} (h : Sim W .top (.glob g) r₂) : r₂ = (.glob g) := by
  generalize hx : Pos.top = x at h
  cases h with
  | glob => rfl
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_hole {p : Nat} {r₂ : Tm} (h : Sim W .top (.hole p) r₂) : r₂ = (.hole p) := by
  generalize hx : Pos.top = x at h
  cases h with
  | hole => rfl
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_todo  {r₂ : Tm} (h : Sim W .top .todo r₂) : r₂ = .todo := by
  generalize hx : Pos.top = x at h
  cases h with
  | todo => rfl
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_rnil  {r₂ : Tm} (h : Sim W .top .rnil r₂) : r₂ = .rnil := by
  generalize hx : Pos.top = x at h
  cases h with
  | rnil => rfl
  | _ => first | (cases hx; done) | (subst hx; contradiction)

theorem Sim.inv_anil  {r₂ : Tm} (h : Sim W .top .anil r₂) : r₂ = .anil := by
  generalize hx : Pos.top = x at h
  cases h with
  | anil => rfl
  | _ => first | (cases hx; done) | (subst hx; contradiction)

end Flick.Equiv
