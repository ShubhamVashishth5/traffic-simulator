//! The committed data dictionary matches the output schemas (CLAUDE.md rule 10).

#[test]
fn data_dictionary_is_up_to_date() {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/data-dictionary.md");
    let committed = std::fs::read_to_string(&path).expect("docs/data-dictionary.md exists");
    assert_eq!(
        committed,
        marg_io::outputs::data_dictionary(),
        "regenerate with `cargo run -p marg-cli -- data-dictionary`"
    );
}
