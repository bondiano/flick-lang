use super::*;

impl Projection<'_> {
    /// A built-in type with a TypeScript form on the selected runtime.
    pub(super) fn native_applied(
        &self,
        item: Lang,
        args: &[&Type],
        vars: &Vars,
        name: &str,
    ) -> Result<Option<String>, String> {
        if let Some((_, ts)) = PRIMITIVES.iter().find(|&&(p, _)| p == item) {
            return Ok(Some((*ts).to_owned()));
        }
        let arg = |i: usize| self.ty(args[i], vars);
        let projected = match (item, args.len()) {
            (Lang::Stream, 3) => {
                let r = self.row(args[0], vars)?;
                let e = self.row(args[1], vars)?;
                let stream = if self.rt { "$Stream" } else { "$Stream.Stream" };
                format!("{stream}<{}, {e}, {r}>", arg(2)?)
            }
            (Lang::List, 1) => format!("ReadonlyArray<{}>", arg(0)?),
            (Lang::Ui, 0) => {
                self.ui.set(true);
                "$Ui".to_owned()
            }
            (Lang::Seq, 1) => format!("Iterable<{}>", arg(0)?),
            (Lang::Option, 1) => {
                let option = if self.rt { "$Option" } else { "$Option.Option" };
                format!("{option}<{}>", arg(0)?)
            }
            (Lang::Result, 2) => {
                let result = if self.rt { "$Result" } else { "$Result.Result" };
                format!("{result}<{}, {}>", arg(1)?, arg(0)?)
            }
            (Lang::Eff, 3) => {
                let r = self.row(args[0], vars)?;
                let e = self.row(args[1], vars)?;
                effect(&arg(2)?, &e, &r, self.rt)
            }
            (Lang::Layer, 3) => {
                let ri = self.row(args[0], vars)?;
                let e = self.row(args[1], vars)?;
                let ro = self.row(args[2], vars)?;
                let layer = if self.rt { "$Layer" } else { "$Layer.Layer" };
                format!("{layer}<{ro}, {e}, {ri}>")
            }
            (Lang::Errors, 1) => self.row(args[0], vars)?,
            (Lang::Schema, 1) => {
                let schema = if self.rt { "$Schema" } else { "$Schema.Codec" };
                format!("{schema}<{0}, {0}>", arg(0)?)
            }
            (Lang::PureCodec, 2) => {
                let schema = if self.rt { "$Schema" } else { "$Schema.Codec" };
                format!("{schema}<{}, {}>", arg(1)?, arg(0)?)
            }
            (Lang::Codec, 4) => {
                let rd = self.row(args[0], vars)?;
                let re = self.row(args[1], vars)?;
                let schema = if self.rt { "$Schema" } else { "$Schema.Codec" };
                format!("{schema}<{}, {}, {rd}, {re}>", arg(3)?, arg(2)?)
            }
            (Lang::ServiceTag | Lang::ServiceImpl | Lang::Scope, _) => {
                return Err(format!("`{name}` has no TypeScript form"));
            }
            _ => return Ok(None),
        };
        Ok(Some(projected))
    }
}
