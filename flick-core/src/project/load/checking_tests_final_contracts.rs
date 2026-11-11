#[cfg(feature = "ide")]
#[test]
fn parameter_callees_wait_for_their_prefix_arguments() {
    for member in ["answer", "extra"] {
        let mut files = vec![
            ("Contracts".to_owned(), "module App.Contracts exposing (..)\ntype module Input =\n  seed : Int\ntype module Output =\n  answer : Int\ntype module Rich =\n  answer : Int\n  extra : Int\ntype module Factory = (K : Input) -> Output\n".to_owned()),
            ("Argument".into(), "module App.Argument exposing (seed)\nseed : Int\nlet seed = 41\n".into()),
            ("Make".into(), "module App.Make(K : App.Contracts.Input) : App.Contracts.Rich\nanswer : Int\nlet answer = K.seed + 1\nextra : Int\nlet extra = 99\n".into()),
            ("Slow".into(), "module App.Slow exposing (seed)\nimport App.Delay9 as D\nseed : Int\nlet seed = D.value\n".into()),
            ("Apply".into(), "private module App.Apply(A : App.Contracts.Input, F : App.Contracts.Factory) = F(App.Argument)\n".into()),
            ("Root".into(), format!("module App.Root exposing (answer)\nmodule Built = App.Apply(App.Slow, App.Make)\nanswer : Int\nlet answer = Built.{member}\n")),
            ("Delay0".into(), "module App.Delay0 exposing (value)\nvalue : Int\nlet value = 1\n".into()),
        ];
        for index in 1..10 {
            files.push((format!("Delay{index}"), format!("module App.Delay{index} exposing (value)\nimport App.Delay{} as D\nvalue : Int\nlet value = D.value\n", index - 1)));
        }
        let sources: Vec<_> = files
            .iter()
            .map(|(name, source)| (name.as_str(), source.as_str()))
            .collect();
        let (_, analysis) = hom_boundary_analysis(&sources, "Root");
        if member == "answer" {
            assert!(
                analysis.diagnostics.is_empty(),
                "{:?}",
                analysis.diagnostics
            );
        } else {
            assert!(
                analysis
                    .diagnostics
                    .iter()
                    .any(|d| d.code == codes::R004 && d.message.contains("extra")),
                "{:?}",
                analysis.diagnostics
            );
        }
    }
}
