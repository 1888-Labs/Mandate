//! Runs the `mandate` binary through the full demo flow (BUILD_SPEC.md §29)
//! in a scratch directory, checking output and exit codes.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

const BIN: &str = env!("CARGO_BIN_EXE_mandate");

/// A fresh, empty MANDATE_HOME that is removed afterwards.
struct Home(PathBuf);

impl Home {
    fn new() -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let dir = std::env::temp_dir().join(format!(
            "mandate-cli-test-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        Home(dir)
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(BIN)
            .args(args)
            .env("MANDATE_HOME", &self.0)
            .output()
            .expect("failed to run mandate binary")
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).into_owned()
}

/// Assert exit code, printing output on failure.
fn expect(o: &Output, code: i32) -> String {
    assert_eq!(
        o.status.code(),
        Some(code),
        "stdout:\n{}\nstderr:\n{}",
        stdout(o),
        stderr(o)
    );
    stdout(o)
}

/// Principals, ownership, M1 and M2, as in the demo.
fn setup(h: &Home) {
    for name in ["alice", "finance-agent", "invoice-agent"] {
        expect(&h.run(&["principal", "create", name]), 0);
    }
    expect(
        &h.run(&[
            "resource",
            "own",
            "wallet:treasury-demo",
            "--owner",
            "alice",
        ]),
        0,
    );
    let out = expect(
        &h.run(&[
            "grant",
            "--from",
            "alice",
            "--to",
            "finance-agent",
            "--action",
            "transfer",
            "--resource",
            "wallet:treasury-demo",
            "--max-amount",
            "1000",
            "--recipient",
            "AWS",
            "--delegable",
            "--expires-in",
            "48h",
        ]),
        0,
    );
    assert!(out.contains("Mandate M1 created"), "{out}");
    let out = expect(
        &h.run(&[
            "delegate",
            "--mandate",
            "M1",
            "--from",
            "finance-agent",
            "--to",
            "invoice-agent",
            "--max-amount",
            "300",
            "--expires-in",
            "24h",
        ]),
        0,
    );
    assert!(
        out.contains("Delegation valid. Mandate M2 created"),
        "{out}"
    );
    assert!(out.contains("invoice-agent  [≤ $300 → AWS]"), "{out}");
}

fn check(h: &Home, amount: &str, recipient: &str) -> Output {
    h.run(&[
        "check",
        "--agent",
        "invoice-agent",
        "--action",
        "transfer",
        "--resource",
        "wallet:treasury-demo",
        "--amount",
        amount,
        "--recipient",
        recipient,
    ])
}

#[test]
fn full_demo_flow() {
    let h = Home::new();
    setup(&h);

    // 3. $200 → AWS: ALLOW with the full chain.
    let out = expect(&check(&h, "200", "AWS"), 0);
    assert!(out.contains("✓ AUTHORIZED"), "{out}");
    assert!(out.contains("Authority    alice"), "{out}");
    assert!(
        out.contains("alice → finance-agent → invoice-agent"),
        "{out}"
    );

    // 4. $700 → AWS: DENY AMOUNT_EXCEEDED.
    let out = expect(&check(&h, "700", "AWS"), 1);
    assert!(out.contains("✗ DENIED  AMOUNT_EXCEEDED"), "{out}");
    assert!(out.contains("Requested    $700"), "{out}");
    assert!(out.contains("Maximum      $300"), "{out}");

    // 5. $100 → RandomVendor: DENY RECIPIENT_NOT_ALLOWED.
    let out = expect(&check(&h, "100", "RandomVendor"), 1);
    assert!(out.contains("✗ DENIED  RECIPIENT_NOT_ALLOWED"), "{out}");

    // 6. Alice revokes M1; M2 is listed as cut off.
    let out = expect(&h.run(&["revoke", "M1"]), 0);
    assert!(out.contains("Mandate M1 revoked"), "{out}");
    assert!(out.contains("M2  finance-agent → invoice-agent"), "{out}");

    // 7. $100 → AWS: DENY ANCESTOR_REVOKED, naming M1.
    let out = expect(&check(&h, "100", "AWS"), 1);
    assert!(out.contains("✗ DENIED  ANCESTOR_REVOKED"), "{out}");
    assert!(out.contains("M1 by alice (an ancestor)"), "{out}");
    assert!(
        out.contains("alice → finance-agent → invoice-agent"),
        "{out}"
    );

    // 8. The chain explains which link broke.
    let out = expect(&h.run(&["chain", "M2"]), 0);
    assert!(out.contains("✗ revoked by alice"), "{out}");
    assert!(out.contains("chain broken: M1 is not usable"), "{out}");

    let out = expect(&h.run(&["inspect", "M2"]), 0);
    assert!(out.contains("✓ valid, signed by finance-agent"), "{out}");
    assert!(out.contains("Status       ✗ revoked by alice"), "{out}");
}

#[test]
fn escalating_delegation_is_rejected_and_not_stored() {
    let h = Home::new();
    setup(&h);
    let out = expect(
        &h.run(&[
            "delegate",
            "--mandate",
            "M1",
            "--from",
            "finance-agent",
            "--to",
            "invoice-agent",
            "--max-amount",
            "5000",
        ]),
        1,
    );
    assert!(
        out.contains("✗ DELEGATION REJECTED  AMOUNT_ESCALATION"),
        "{out}"
    );
    assert!(out.contains("Nothing was stored"), "{out}");
    assert!(h.run(&["inspect", "M3"]).status.code() == Some(2));

    let out = expect(
        &h.run(&[
            "delegate",
            "--mandate",
            "M1",
            "--from",
            "finance-agent",
            "--to",
            "invoice-agent",
            "--recipient",
            "Coinbase",
        ]),
        1,
    );
    assert!(out.contains("RECIPIENT_ESCALATION"), "{out}");

    let out = expect(
        &h.run(&[
            "delegate",
            "--mandate",
            "M1",
            "--from",
            "finance-agent",
            "--to",
            "invoice-agent",
            "--expires-in",
            "30d",
        ]),
        1,
    );
    assert!(out.contains("EXPIRATION_ESCALATION"), "{out}");

    // InvoiceAgent's M2 is not delegable.
    let out = expect(
        &h.run(&[
            "delegate",
            "--mandate",
            "M2",
            "--from",
            "invoice-agent",
            "--to",
            "alice",
            "--max-amount",
            "1",
        ]),
        1,
    );
    assert!(out.contains("PARENT_NOT_DELEGABLE"), "{out}");
}

#[test]
fn only_owners_can_grant() {
    let h = Home::new();
    for name in ["alice", "mallory"] {
        expect(&h.run(&["principal", "create", name]), 0);
    }
    expect(
        &h.run(&[
            "resource",
            "own",
            "wallet:treasury-demo",
            "--owner",
            "alice",
        ]),
        0,
    );
    let o = h.run(&[
        "grant",
        "--from",
        "mallory",
        "--to",
        "mallory",
        "--action",
        "transfer",
        "--resource",
        "wallet:treasury-demo",
        "--expires-in",
        "1h",
    ]);
    assert_eq!(o.status.code(), Some(2));
    assert!(stderr(&o).contains("mallory is not an owner of wallet:treasury-demo"));
}

#[test]
fn cannot_delegate_from_revoked_mandate() {
    let h = Home::new();
    setup(&h);
    expect(&h.run(&["revoke", "M1"]), 0);
    let o = h.run(&[
        "delegate",
        "--mandate",
        "M1",
        "--from",
        "finance-agent",
        "--to",
        "invoice-agent",
        "--max-amount",
        "10",
    ]);
    assert_eq!(o.status.code(), Some(2));
    assert!(stderr(&o).contains("cannot delegate from M1: it is revoked"));
}

#[test]
fn tampered_local_state_cannot_grant_authority() {
    // Editing state.json to raise M2's limit breaks its ID and signature:
    // the store can never invent authority.
    let h = Home::new();
    setup(&h);
    let path = h.path().join("state.json");
    let state = std::fs::read_to_string(&path).unwrap();
    let tampered = state.replacen("\"maxAmount\": 300", "\"maxAmount\": 5000", 1);
    assert_ne!(state, tampered, "expected to find M2's limit in state.json");
    std::fs::write(&path, tampered).unwrap();

    let out = expect(&check(&h, "700", "AWS"), 1);
    assert!(out.contains("✗ DENIED  INVALID_SIGNATURE"), "{out}");
}

#[test]
fn keys_are_private_and_separate_from_state() {
    let h = Home::new();
    expect(&h.run(&["principal", "create", "alice"]), 0);
    let keys = std::fs::read_to_string(h.path().join("keys.json")).unwrap();
    let state = std::fs::read_to_string(h.path().join("state.json")).unwrap();
    let key_hex = keys
        .split('"')
        .find(|s| s.starts_with("0x") && s.len() == 66)
        .expect("a private key in keys.json");
    assert!(
        !state.contains(&key_hex[2..]),
        "private key leaked into state.json"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(h.path().join("keys.json"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "keys.json must be owner-only");
    }
}

#[test]
fn usage_errors_exit_2() {
    let h = Home::new();
    let o = h.run(&["principal", "create", "Not Valid"]);
    assert_eq!(o.status.code(), Some(2));
    let o = h.run(&[
        "check",
        "--agent",
        "nobody",
        "--action",
        "x",
        "--resource",
        "a:b",
    ]);
    assert_eq!(o.status.code(), Some(2));
    assert!(stderr(&o).contains("unknown principal `nobody`"));
    expect(&h.run(&["principal", "create", "alice"]), 0);
    let o = h.run(&["principal", "create", "alice"]);
    assert_eq!(o.status.code(), Some(2));
}
