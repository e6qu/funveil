Feature: Metadata extraction and indexing
  When a file is veiled, its symbols are extracted and indexed
  so that agents can discover code structure without unveiling.

  Background:
    Given a funveil project is initialized

  Scenario: Veiling a Rust file extracts symbol metadata
    Given a file "src/auth.rs" with content:
      """
      pub fn verify_token(token: &str) -> bool { true }
      fn helper() {}
      """
    When I veil "src/auth.rs"
    Then metadata should exist for "src/auth.rs"
    And the metadata should contain symbol "verify_token"
    And the metadata should contain symbol "helper"

  Scenario: Metadata index allows symbol lookup
    Given a file "src/math.rs" with content:
      """
      pub fn add(a: i32, b: i32) -> i32 { a + b }
      """
    When I veil "src/math.rs"
    And I rebuild the metadata index
    Then the index should map "add" to "src/math.rs"

  Scenario: Manifest reflects project disclosure state
    Given a file "src/visible.rs" with content:
      """
      pub fn visible() {}
      """
    And a file "src/hidden.rs" with content:
      """
      pub fn hidden() {}
      """
    When I veil "src/hidden.rs"
    And I generate a manifest
    Then the manifest should list "src/hidden.rs" as veiled

  Scenario: Unveiling removes metadata
    Given a file "src/temp.rs" with content:
      """
      pub fn temp() {}
      """
    When I veil "src/temp.rs"
    And I unveil "src/temp.rs"
    Then metadata should not exist for "src/temp.rs"
