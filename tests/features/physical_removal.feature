Feature: Physical file removal on veil
  When a file is fully veiled, it should be physically removed from disk
  with content stored only in the content-addressable store (CAS).

  Background:
    Given a funveil project is initialized

  Scenario: Veiling a file removes it from disk
    Given a file "src/secret.rs" with content:
      """
      fn secret() { println!("hidden"); }
      """
    When I veil "src/secret.rs"
    Then "src/secret.rs" should not exist on disk
    And "src/secret.rs" should be tracked in config

  Scenario: Unveiling restores a removed file from CAS
    Given a file "src/secret.rs" with content:
      """
      fn secret() { println!("hidden"); }
      """
    When I veil "src/secret.rs"
    And I unveil "src/secret.rs"
    Then "src/secret.rs" should exist on disk
    And "src/secret.rs" should have content:
      """
      fn secret() { println!("hidden"); }
      """
    And "src/secret.rs" should not be tracked in config

  Scenario: Veiling a file preserves content in CAS for round-trip
    Given a file "lib.rs" with content:
      """
      pub fn add(a: i32, b: i32) -> i32 { a + b }
      """
    When I veil "lib.rs"
    And I unveil "lib.rs"
    Then "lib.rs" content should exactly match the original

  Scenario: Veiling an already-veiled file returns an error
    Given a file "src/twice.rs" with content:
      """
      fn twice() {}
      """
    When I veil "src/twice.rs"
    Then veiling "src/twice.rs" again should fail with "already veiled"

  Scenario: Legacy marker files are recognized
    Given a file "legacy.rs" with content "...\n" and a config entry for "legacy.rs"
    When I run doctor
    Then the doctor output should mention "Legacy marker"

  Scenario: Apply migrates legacy markers to physical removal
    Given a file "legacy.rs" with content "...\n" and a config entry for "legacy.rs"
    When I run apply
    Then "legacy.rs" should not exist on disk
