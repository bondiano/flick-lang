---
title: "FLICK-T065"
---

`instance Eq Key from equality` names a value of the module contract that
supplies the dictionary. Declare `equality : Eq.Dict Key` in the same
`type module` contract. A value of another type, or a name not declared by
the contract, cannot provide this local instance. The requirement does not
create a global instance.
