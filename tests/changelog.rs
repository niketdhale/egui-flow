//! Release hygiene: the changelog must describe the version in `Cargo.toml`.

const CHANGELOG: &str = include_str!("../CHANGELOG.md");

#[test]
fn changelog_has_an_entry_for_the_current_version() {
    let version = env!("CARGO_PKG_VERSION");
    let heading = format!("## [{version}]");
    assert!(
        CHANGELOG.lines().any(|l| l.starts_with(&heading)),
        "CHANGELOG.md has no `{heading}` section; add one before bumping the version"
    );
}

#[test]
fn released_entries_have_an_iso_date_and_a_link() {
    let version = env!("CARGO_PKG_VERSION");
    let line = CHANGELOG
        .lines()
        .find(|l| l.starts_with(&format!("## [{version}]")))
        .unwrap();
    let date = line.rsplit(" - ").next().unwrap();
    let ok = date.len() == 10
        && date.split('-').map(str::len).eq([4, 2, 2])
        && date.chars().all(|c| c.is_ascii_digit() || c == '-');
    assert!(ok, "expected `## [{version}] - YYYY-MM-DD`, got `{line}`");
    assert!(
        CHANGELOG.contains(&format!("[{version}]: https://github.com/")),
        "missing the `[{version}]: ...` link reference at the bottom"
    );
}
