use super::*;
#[test]
fn rejects_all_escape_hatches_and_wrong_module_formats() {
    let good = "import Target\ntheorem wishpool_solution : wishpool_target_prop := by trivial\n";
    assert_eq!(validate_solution(good), Ok(Verdict::Proved));
    assert_eq!(
        validate_solution(
            "import Mathlib\nimport Target\nlemma helper : True := by trivial\ntheorem wishpool_disproof : ¬ wishpool_target_prop := by simp [wishpool_target_prop]\n"
        ),
        Ok(Verdict::Disproved)
    );
    for construct in [
        "axiom magic : False",
        "constant magic : False",
        "opaque magic : False := by sorry",
        "sorry",
        "admit",
        "unsafe def",
        "@[implemented_by evil]",
        "@[extern evil]",
        "macro x",
        "notation x",
        "syntax x",
        "elab x",
        "initialize x",
        "@[init boot] def value := 1",
        "@[builtin_init boot] def value := 1",
        "run_meta x",
        "_root_.sorryAx",
        "set_option debug.skipKernelTC true",
        "run_cmd x",
        "#eval x",
        "run_tac x",
        "import Evil",
        "import Mathlib Target",
        "namespace Fake",
    ] {
        assert!(
            validate_solution(&format!("{construct}\n{good}")).is_err(),
            "{construct}"
        );
    }
    assert!(validate_solution(&good.replace("import Target", "import Mathlib")).is_err());
    assert!(
        validate_solution(&format!(
            "{good}\ntheorem wishpool_disproof : False := by contradiction"
        ))
        .is_err()
    );
    assert!(validate_solution(&good.repeat(2)).is_err());
}
#[test]
fn target_conversion_is_exact_and_binders_require_regeneration() {
    let legacy = "import Mathlib\ndef bound (n : Nat) := n\ntheorem wishpool_target : ∀ n : Nat, bound n = n := by sorry\n";
    let converted = convert_target(legacy).unwrap();
    assert_eq!(
        converted,
        "import Mathlib\ndef bound (n : Nat) := n\ndef wishpool_target_prop : Prop := ∀ n : Nat, bound n = n\n"
    );
    assert_eq!(convert_target(&converted), Ok(converted.clone()));
    assert!(validate_target(&converted).is_ok());
    for bad in [
        "theorem wishpool_target (n : Nat) : n = n := by sorry",
        "axiom bad : False\ntheorem wishpool_target : True := by sorry",
        "theorem wishpool_target : True := by sorry\n#eval 1",
    ] {
        assert!(convert_target(bad).is_err());
    }
}
#[tokio::test]
async fn validates_digest_before_any_workspace_or_process_access() {
    let checker = Checker {
        workspace: "/nonexistent".into(),
        scratch: std::env::temp_dir(),
        timeout: Duration::from_secs(1),
    };
    let request = Request {
        target: "import Mathlib\ndef wishpool_target_prop : Prop := True\n".into(),
        target_digest: "wrong".into(),
        toolchain: "test".into(),
        solution: "import Target\ntheorem wishpool_solution : wishpool_target_prop := by trivial"
            .into(),
    };
    let receipt = checker.verify(&request).await;
    assert_eq!(receipt.verdict, Verdict::Rejected);
    assert_eq!(receipt.reason, "target digest mismatch");
    assert_eq!(receipt.solution_digest, digest(&request.solution));
}
#[cfg(unix)]
#[tokio::test]
async fn executable_fakes_enforce_exit_status_output_limit_and_axiom_receipts() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("fake-lean");
    for (body, ok) in [
        (
            "echo \"'wishpool_solution' depends on axioms: [propext, Classical.choice, Quot.sound]\"",
            true,
        ),
        ("echo 'error: wrong target type'", false),
        ("exit 1", false),
        ("head -c 70000 /dev/zero", false),
    ] {
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        let out = capture(command(&path, dir.path())).await;
        assert_eq!(out.is_ok(), ok);
        if let Ok(text) = out {
            assert_eq!(axioms_of(&text, "wishpool_solution").unwrap().len(), 3);
        }
    }
    assert_eq!(
        axioms_of(
            "'wishpool_solution' does not depend on any axioms",
            "wishpool_solution"
        ),
        Some(vec![])
    );
    assert_eq!(
        axioms_of("'wrong' depends on axioms: [sorryAx]", "wishpool_solution"),
        None
    );
}
#[tokio::test]
async fn real_lean_target_proof_disproof_wrong_type_and_axiom() {
    let Ok(workspace) = std::env::var("WISHPOOL_TEST_LEAN_WORKSPACE") else {
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    let checker = Checker {
        workspace: workspace.into(),
        scratch: dir.path().into(),
        timeout: Duration::from_secs(600),
    };
    let toolchain = std::fs::read_to_string(checker.workspace.join("lean-toolchain"))
        .unwrap()
        .trim()
        .to_owned();
    for (prop, solution, expected) in [
        (
            "True",
            "theorem wishpool_solution : wishpool_target_prop := by trivial",
            Verdict::Proved,
        ),
        (
            "False",
            "theorem wishpool_disproof : ¬ wishpool_target_prop := by simp [wishpool_target_prop]",
            Verdict::Disproved,
        ),
        (
            "False",
            "theorem wishpool_solution : True := by trivial",
            Verdict::Rejected,
        ),
        (
            "False",
            "axiom magic : False\ntheorem wishpool_solution : wishpool_target_prop := magic",
            Verdict::Rejected,
        ),
    ] {
        let target = format!("import Mathlib\ndef wishpool_target_prop : Prop := {prop}\n");
        let request = Request {
            target_digest: digest(&target),
            target,
            toolchain: toolchain.clone(),
            solution: format!("import Target\n{solution}\n"),
        };
        let receipt = checker.verify(&request).await;
        assert_eq!(receipt.verdict, expected, "{}", receipt.reason);
    }
}

#[cfg(unix)]
#[tokio::test]
async fn full_module_pipeline_with_executable_fake_checks_names_types_and_receipt() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let workspace = dir.path().join("workspace");
    std::fs::create_dir_all(workspace.join(".lake/packages")).unwrap();
    std::fs::write(workspace.join("lean-toolchain"), "fake").unwrap();
    std::fs::write(
        workspace.join("lake-manifest.json"),
        r#"{"packages":[{"name":"mathlib","rev":"0123456789"}]}"#,
    )
    .unwrap();
    let binary = dir.path().join("fake-lean");
    std::fs::write(
        &binary,
        r##"#!/usr/bin/python3
import os, sys, json
name = sys.argv[-1]
assert sys.argv[1:3] == ['-j','2']
assert os.path.realpath(os.environ['HOME']) == os.path.realpath(os.getcwd())
setup = json.load(open(sys.argv[sys.argv.index('--setup')+1]))
if name == 'Solution.lean':
    assert 'Target' in setup['importArts']
    assert len(setup['importArts']['Target'][0]) == 3
    assert len(setup['importArts']['Target'][1]) == 2
    assert not any('TOKEN' in key or 'SECRET' in key for key in os.environ)
if name == 'Check.lean':
    assert 'Solution' in setup['importArts']
    text = open(name).read()
    solution = open('Solution.lean').read()
    assert '#check (_root_.wishpool_' in text and '_root_.wishpool_target_prop)' in text
    if 'wrong_type' in solution:
        print('error: type mismatch'); sys.exit(1)
    theorem = 'wishpool_disproof' if 'wishpool_disproof' in solution else 'wishpool_solution'
    axioms = 'customAxiom' if 'extra_axiom' in solution else 'propext, Classical.choice, Quot.sound'
    print("'%s' depends on axioms: [%s]" % (theorem, axioms))
output = sys.argv[sys.argv.index('-o')+1]
for suffix in ['', '.server', '.private']:
    open(output + suffix, 'w').close()
for suffix in ['ir.sig', 'ir']:
    open(output.removesuffix('olean') + suffix, 'w').close()
"##,
    )
    .unwrap();
    std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o700)).unwrap();
    let checker = Checker {
        workspace: workspace.clone(),
        scratch: dir.path().into(),
        timeout: Duration::from_secs(5),
    };
    for (declaration, expected) in [
        (
            "theorem wishpool_solution : wishpool_target_prop := by trivial",
            Some(Verdict::Proved),
        ),
        (
            "theorem wishpool_disproof : ¬ wishpool_target_prop := by simp [wishpool_target_prop]",
            Some(Verdict::Disproved),
        ),
        (
            "theorem wishpool_solution : True := by trivial -- wrong_type",
            None,
        ),
        (
            "theorem wishpool_solution : wishpool_target_prop := by trivial -- extra_axiom",
            None,
        ),
    ] {
        let target = "import Mathlib\ndef wishpool_target_prop : Prop := True\n".to_string();
        let request = Request {
            target_digest: digest(&target),
            target,
            toolchain: "fake".into(),
            solution: format!("import Target\n{declaration}\n"),
        };
        let result = checker.check_using(&request, Some(&binary)).await;
        assert_eq!(result.as_ref().ok().map(|r| r.0), expected, "{result:?}");
    }
    assert!(!workspace.join("Target.lean").exists());
    assert!(!workspace.join("Solution.lean").exists());
}
