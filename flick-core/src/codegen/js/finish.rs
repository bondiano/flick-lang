fn finish(
    #[cfg(feature = "native-project")] code: &str,
    #[cfg(not(feature = "native-project"))] code: String,
    #[cfg(feature = "native-project")] spans: &[Span],
    #[cfg(feature = "native-project")] names: &HashMap<usize, String>,
) -> Output {
    #[cfg(feature = "native-project")]
    {
        let mut out = String::with_capacity(code.len());
        let mut mappings = Vec::new();
        let (mut line, mut col) = (0u32, 0u32);
        let mut chars = code.chars();
        while let Some(c) = chars.next() {
            match c {
                MARK_START => {
                    let id: String = chars.by_ref().take_while(|&c| c != MARK_END).collect();
                    if let Ok(id) = id.parse::<usize>() {
                        mappings.push((line, col, spans[id], names.get(&id).cloned()));
                    }
                }
                '\n' => {
                    out.push(c);
                    line += 1;
                    col = 0;
                }
                c => {
                    out.push(c);
                    col += u32::try_from(c.len_utf16()).unwrap_or(1);
                }
            }
        }
        Output {
            code: out,
            mappings,
            imports: Vec::new(),
        }
    }
    #[cfg(not(feature = "native-project"))]
    Output {
        code,
        imports: Vec::new(),
    }
}
