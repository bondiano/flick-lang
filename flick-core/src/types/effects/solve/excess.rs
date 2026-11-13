use super::Excess;

impl Excess {
    pub(super) fn merge(&mut self, other: Self) {
        self.taken |= other.taken;
        for (into, from) in [
            (&mut self.services, other.services),
            (&mut self.errors, other.errors),
        ] {
            for name in from {
                if !into.contains(&name) {
                    into.push(name);
                }
            }
        }
        for label in other.labels {
            if !self.labels.contains(&label) {
                self.labels.push(label);
            }
        }
        if self.bound.0.is_none() {
            self.bound = other.bound;
        }
    }
}
