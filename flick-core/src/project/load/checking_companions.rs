// A test companion `App.Users$test` sees all that `App.Users` declares: the
// checking of a module whose companion is loaded keeps that private view,
// which the companion reads in place of the module's public interface.

impl Loader<'_> {
    /// Keeps what a loaded test companion of module `i` sees of it, all it
    /// declares, once `output` checked it.
    fn keep_private_view(&mut self, i: usize, output: &Output) {
        let name = &self.modules[i].name;
        if COMPANIONS && self.modules.iter().any(|m| companion_of(&m.name) == Some(name)) {
            let view = private_view(&output.exported, &output.declarations);
            self.privates.insert(name.clone(), view);
        }
    }
}

/// What a test companion sees of its module: its exports and every other
/// top-level declaration, constructors of exported types included.
fn private_view(exported: &Interface, declarations: &Interface) -> Interface {
    let mut view = exported.clone();
    for export in &declarations.exports {
        if !view.exports.iter().any(|e| e.entity == export.entity) {
            view.exports.push(export.clone());
        }
    }
    view.hidden.clear();
    view
}
