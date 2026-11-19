/// The current label of a row entry: a capture row entry, and a type
/// variable of an error row, follow their element.
fn relabel(label: &Elem, t: &Type) -> Elem {
    let current = match label {
        Elem::Capture(_) => super::ty::capture_label(t),
        Elem::Error(_) => super::ty::error_label(t),
        Elem::Service(_) => super::ty::service_label(t),
        Elem::Provided(inner) => Some(Elem::Provided(Box::new(relabel(inner, t)))),
        Elem::Named(_) | Elem::Live => None,
    };
    current.unwrap_or_else(|| label.clone())
}

