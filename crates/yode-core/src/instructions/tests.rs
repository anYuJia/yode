use std::fs;

use super::instruction_loader::load_instruction_context_with_sources_test;
use super::load_memory_context;

#[test]
fn loads_layered_instructions_in_priority_order() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    let home = temp.path().join("home");
    let admin = temp.path().join("admin");

    fs::create_dir_all(project.join(".claude").join("rules")).unwrap();
    fs::create_dir_all(home.join(".claude")).unwrap();
    fs::create_dir_all(&admin).unwrap();

    fs::write(admin.join("CLAUDE.md"), "admin rule").unwrap();
    fs::write(home.join(".claude").join("CLAUDE.md"), "user rule").unwrap();
    fs::write(project.join("CLAUDE.md"), "project rule").unwrap();
    fs::write(project.join(".claude").join("rules").join("b.md"), "rule b").unwrap();
    fs::write(project.join(".claude").join("rules").join("a.md"), "rule a").unwrap();
    fs::write(project.join("CLAUDE.local.md"), "local rule").unwrap();

    let loaded = load_instruction_context_with_sources_test(
        &project,
        Some(home.clone()),
        Some(admin.clone()),
    )
    .unwrap();

    let admin_idx = loaded.find("admin rule").unwrap();
    let user_idx = loaded.find("user rule").unwrap();
    let project_idx = loaded.find("project rule").unwrap();
    let rule_a_idx = loaded.find("rule a").unwrap();
    let rule_b_idx = loaded.find("rule b").unwrap();
    let local_idx = loaded.find("local rule").unwrap();

    assert!(admin_idx < user_idx);
    assert!(user_idx < project_idx);
    assert!(project_idx < rule_a_idx);
    assert!(rule_a_idx < rule_b_idx);
    assert!(rule_b_idx < local_idx);
}

#[test]
fn supports_include_without_expanding_code_fences() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    fs::create_dir_all(project.join("docs")).unwrap();

    fs::write(
        project.join("docs").join("shared.md"),
        "shared instructions",
    )
    .unwrap();
    fs::write(
        project.join("docs").join("ignored.md"),
        "should stay ignored",
    )
    .unwrap();
    fs::write(
        project.join("CLAUDE.md"),
        "intro\n@./docs/shared.md\n```md\n@./docs/ignored.md\n```\noutro\n",
    )
    .unwrap();

    let loaded = load_instruction_context_with_sources_test(&project, None, None).unwrap();
    assert!(loaded.contains("shared instructions"));
    assert!(loaded.contains("@./docs/ignored.md"));
    assert!(!loaded.contains("should stay ignored"));
}

#[test]
fn prevents_circular_includes() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    fs::create_dir_all(&project).unwrap();

    fs::write(project.join("A.md"), "A top\n@./B.md\n").unwrap();
    fs::write(project.join("B.md"), "B top\n@./A.md\n").unwrap();
    fs::write(project.join("CLAUDE.md"), "@./A.md\n").unwrap();

    let loaded = load_instruction_context_with_sources_test(&project, None, None).unwrap();
    assert_eq!(loaded.matches("A top").count(), 1);
    assert_eq!(loaded.matches("B top").count(), 1);
}

#[test]
fn loads_project_memory_from_supported_locations() {
    let temp = tempfile::tempdir().unwrap();
    let project = temp.path().join("project");
    fs::create_dir_all(project.join(".yode").join("memory").join("nested")).unwrap();
    fs::create_dir_all(project.join("memory")).unwrap();

    fs::write(project.join("MEMORY.md"), "root memory").unwrap();
    fs::write(
        project
            .join(".yode")
            .join("memory")
            .join("nested")
            .join("notes.md"),
        "nested memory",
    )
    .unwrap();
    fs::write(project.join("memory").join("legacy.md"), "legacy memory").unwrap();

    let loaded = load_memory_context(&project).unwrap();
    assert!(loaded.contains("root memory"));
    assert!(loaded.contains("nested memory"));
    assert!(loaded.contains("legacy memory"));
}
