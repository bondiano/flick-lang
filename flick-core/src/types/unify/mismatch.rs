// Failure-only display observations. These never participate in inference.

#[derive(Debug, Clone, Default)]
pub struct DiagnosticViews(Vec<(Type, Type)>);

impl DiagnosticViews {
    fn apply(&self, ty: &Type) -> Type {
        if let Some((_, observed)) = self.0.iter().rev().find(|(original, _)| original == ty) {
            return observed.clone();
        }
        ty.map_children(&mut |child| self.apply(child))
    }
}

impl Mismatch {
    /// Semantic consumers must ignore inert diagnostic observations.
    pub(in crate::types) fn reason(&self) -> &Self {
        match self {
            Self::Observed { reason, .. } => reason.reason(),
            reason => reason,
        }
    }

    fn map_reason(self, change: impl FnOnce(Self) -> Self) -> Self {
        match self {
            Self::Observed { reason, views } => Self::Observed {
                reason: Box::new(change(*reason)),
                views,
            },
            reason => change(reason),
        }
    }

    pub(in crate::types) fn diagnostic_type(&self, ty: &Type) -> Type {
        match self {
            Self::Observed { reason, views } => views.apply(&reason.diagnostic_type(ty)),
            _ => ty.clone(),
        }
    }
}

impl Checker<'_> {
    /// Snapshot only the failed comparison's observed sides before its trial
    /// rolls back. Child failures can have already restored their own writes;
    /// compose their observations into the enclosing diagnostic, not the Store.
    fn observe_mismatch(&self, mismatch: Mismatch, left: &Type, right: &Type) -> Mismatch {
        let (reason, mut views) = match mismatch {
            Mismatch::Observed { reason, views } => (*reason, views),
            reason => (reason, DiagnosticViews::default()),
        };
        for original in [left, right] {
            let observed = views.apply(&self.zonk(original));
            if *original != observed {
                views.0.push((original.clone(), observed));
            }
        }
        if views.0.is_empty() {
            reason
        } else {
            Mismatch::Observed {
                reason: Box::new(reason),
                views,
            }
        }
    }
}
