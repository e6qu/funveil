Feature: Query-based unveiling
  Agents can unveil code by symbol name or call graph queries
  instead of needing to know exact file paths.

  Background:
    Given a funveil project is initialized

  Scenario: Unveil by symbol name
    Given a file "src/auth.rs" with content:
      """
      pub fn verify_token(token: &str) -> bool { true }
      """
    When I veil "src/auth.rs"
    And I unveil with symbol "verify_token"
    Then "src/auth.rs" should exist on disk

  Scenario: Veil by symbol name
    Given a file "src/utils.rs" with content:
      """
      pub fn helper_a() {}
      pub fn helper_b() {}
      """
    When I veil with symbol "helper_a"
    Then "src/utils.rs" should exist on disk

  Scenario: Unveil callers of a function
    Given a file "src/core.rs" with content:
      """
      pub fn core_fn() {}
      """
    And a file "src/caller.rs" with content:
      """
      use crate::core::core_fn;
      pub fn caller() { core_fn(); }
      """
    When I veil "src/core.rs"
    And I veil "src/caller.rs"
    And I unveil callers of "core_fn"
    Then "src/caller.rs" should exist on disk
