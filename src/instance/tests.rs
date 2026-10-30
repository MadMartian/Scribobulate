use super::*;

fn argv(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

fn name(raw: &str) -> InstanceName {
    InstanceName::parse(raw).expect("valid name")
}

/// TDD 8.10: every refusal names its reason, and nothing is repaired into a valid name.
#[test]
fn invalid_names_are_refused_with_their_reason() {
    assert_eq!(InstanceName::parse(""), Err(NameError::Empty));
    assert_eq!(InstanceName::parse("9lives"), Err(NameError::LeadingDigit));
    assert_eq!(
        InstanceName::parse("my.work"),
        Err(NameError::Disallowed('.'))
    );
    assert_eq!(InstanceName::parse("a/b"), Err(NameError::Disallowed('/')));
    assert_eq!(InstanceName::parse("a b"), Err(NameError::Disallowed(' ')));
    assert_eq!(InstanceName::parse("a-b"), Err(NameError::Disallowed('-')));
    assert_eq!(InstanceName::parse("café"), Err(NameError::Disallowed('é')));
    let long = "a".repeat(MAX_NAME_LEN + 1);
    assert_eq!(
        InstanceName::parse(&long),
        Err(NameError::TooLong(MAX_NAME_LEN + 1))
    );
}

#[test]
fn names_at_the_limits_are_accepted() {
    assert_eq!(name("a").key(), "a");
    let longest = format!("w{}", "1".repeat(MAX_NAME_LEN - 1));
    assert_eq!(name(&longest).key(), longest);
}

/// TDD 8.11: identity is lower case; the spelling survives only for display.
#[test]
fn identity_is_lower_case_and_the_spelling_is_kept_for_display() {
    let typed = name("WorK2");
    assert_eq!(typed.key(), "work2");
    assert_eq!(typed.shown(), "WorK2");
    assert_eq!(name("WORK").key(), name("work").key());
}

#[test]
fn argv_without_the_switch_is_the_default_instance_unchanged() {
    let args = argv(&["scribobulate", "-n", "a.md"]);
    assert_eq!(instance_argv(args.clone()), Ok((None, args)));
}

#[test]
fn both_spellings_are_taken_out_of_argv() {
    let (got, rest) = instance_argv(argv(&["prog", "--instance", "Work", "a.md"])).unwrap();
    assert_eq!(got, Some(name("Work")));
    assert_eq!(rest, argv(&["prog", "a.md"]));

    let (got, rest) = instance_argv(argv(&["prog", "a.md", "--instance=work"])).unwrap();
    assert_eq!(got, Some(name("work")));
    assert_eq!(rest, argv(&["prog", "a.md"]));
}

#[test]
fn a_switch_after_a_double_dash_is_a_file_name() {
    let args = argv(&["prog", "--", "--instance", "work"]);
    assert_eq!(instance_argv(args.clone()), Ok((None, args)));
}

#[test]
fn the_program_name_is_never_read_as_the_switch() {
    let args = argv(&["--instance", "a.md"]);
    assert_eq!(instance_argv(args.clone()), Ok((None, args)));
}

#[test]
fn a_missing_repeated_or_invalid_name_is_refused() {
    assert_eq!(
        instance_argv(argv(&["prog", "--instance"])),
        Err(ArgError::MissingName)
    );
    assert_eq!(
        instance_argv(argv(&["prog", "--instance", "a", "--instance=a"])),
        Err(ArgError::Repeated)
    );
    assert_eq!(
        instance_argv(argv(&["prog", "--instance="])),
        Err(ArgError::Invalid {
            raw: String::new(),
            why: NameError::Empty
        })
    );
    let refused = instance_argv(argv(&["prog", "--instance", "1x"])).unwrap_err();
    let message = refused.to_string();
    assert!(
        message.contains("\"1x\"") && message.contains("digit"),
        "{message}"
    );
}

/// TDD 8.9 and 8.15: a named instance gets an ID of its own; the default keeps today's.
#[test]
fn the_application_id_carries_the_lower_cased_name() {
    assert_eq!(app_id_for("com.example.app", None), "com.example.app");
    assert_eq!(
        app_id_for("com.example.app", Some(&name("Work"))),
        "com.example.app.work"
    );
}

#[test]
fn the_program_name_is_the_programs_file_name_dash_the_name() {
    assert_eq!(program_name_for("/usr/bin/scribobulate", None), None);
    assert_eq!(
        program_name_for("/usr/bin/scribobulate", Some(&name("Work"))),
        Some("scribobulate-work".to_owned())
    );
    assert_eq!(
        program_name_for("scribobulate", Some(&name("home"))),
        Some("scribobulate-home".to_owned())
    );
}

/// TDD 8.11, 8.12 and 8.15: one folder per named instance, under `instances/`, and the
/// default instance's folder is the one it always used.
#[test]
fn state_folders_nest_named_instances_under_the_default() {
    let base = PathBuf::from("state").join("scribobulate");
    assert_eq!(state_dir_for(base.clone(), None), base);
    assert_eq!(
        state_dir_for(base.clone(), Some(&name("Work"))),
        base.join(INSTANCES_DIR).join("work")
    );
}

/// TDD 8.16.
#[test]
fn only_a_named_instance_has_a_title_suffix_spelled_as_typed() {
    assert_eq!(title_suffix(None), "");
    assert_eq!(title_suffix(Some(&name("Work"))), " | Work");
}

/// The tag must be identical in every build, so it is pinned to literal values rather
/// than recomputed by the function under test.
#[test]
fn the_short_tag_is_stable_and_case_blind() {
    // FNV-1a 32 of "a", the algorithm's published test vector (written as a number so
    // it is not mistaken for a commit hash by the reference lint).
    assert_eq!(
        u32::from_str_radix(&short_tag(&name("a")), 16),
        Ok(0xe40c_292c)
    );
    assert_eq!(short_tag(&name("work")), short_tag(&name("WORK")));
    assert_ne!(short_tag(&name("work")), short_tag(&name("home")));
    assert_eq!(short_tag(&name(&"z".repeat(MAX_NAME_LEN))).len(), 8);
}
